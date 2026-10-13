use crate::ftp::{Ctl, Entry};
use crate::sites::Site;
use crate::sync::{self, SyncDb};
use crate::AppState;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub enum Op {
    Up { src: String, dst: String },
    Down { src: String, dst: String, entry: Option<Entry> },
    LocalCopy { src: String, dst: String },
    RemoteCopy { src: String, dst: String },
    /// Upload of a file outside the site folder.
    UpPath { src: PathBuf, dst: String },
    /// Copy of a file from outside into the local site folder.
    CopyIn { src: PathBuf, dst: String },
    /// Moves a local file to the recycle bin; `forget` drops its sync record.
    DelLocal { dst: String, forget: bool },
    DelRemote { dst: String, forget: bool },
}

impl Op {
    fn dst(&self) -> &str {
        match self {
            Op::Up { dst, .. }
            | Op::Down { dst, .. }
            | Op::LocalCopy { dst, .. }
            | Op::RemoteCopy { dst, .. }
            | Op::UpPath { dst, .. }
            | Op::CopyIn { dst, .. }
            | Op::DelLocal { dst, .. }
            | Op::DelRemote { dst, .. } => dst,
        }
    }

    fn target_side(&self) -> &'static str {
        match self {
            Op::Up { .. } | Op::RemoteCopy { .. } | Op::UpPath { .. } | Op::DelRemote { .. } => "remote",
            Op::Down { .. } | Op::LocalCopy { .. } | Op::CopyIn { .. } | Op::DelLocal { .. } => "local",
        }
    }
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobInfo {
    pub id: u64,
    pub site_id: String,
    pub label: String,
    pub state: &'static str,
    pub scanning: bool,
    pub scan_dir: String,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub bytes: u64,
    pub current: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FsChanged<'a> {
    site_id: &'a str,
    side: &'a str,
    dir: &'a str,
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    active: Mutex<HashMap<u64, Arc<Ctl>>>,
}

impl Jobs {
    pub fn cancel(&self, id: u64) {
        if let Some(ctl) = self.active.lock().unwrap().get(&id) {
            ctl.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn cancel_all(&self) {
        for ctl in self.active.lock().unwrap().values() {
            ctl.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
struct Queue {
    ops: VecDeque<Op>,
    planning: bool,
    seen: HashSet<String>,
}

/// Context handed to the planning step: queue work as it is found, report scanned folders, check for cancellation.
pub struct Plan<'a> {
    pub ctl: &'a Ctl,
    info: &'a Mutex<JobInfo>,
    queue: &'a (Mutex<Queue>, Condvar),
}

impl Plan<'_> {
    pub fn scanning(&self, dir: &str) -> bool {
        self.info.lock().unwrap().scan_dir = format!("/{dir}");
        !self.ctl.cancelled()
    }

    /// Queues an operation; workers start on it right away. Duplicate targets are ignored.
    pub fn push(&self, op: Op) {
        let (lock, cv) = self.queue;
        let mut q = lock.lock().unwrap();
        if !q.seen.insert(format!("{}:{}", op.target_side(), op.dst())) {
            return;
        }
        q.ops.push_back(op);
        drop(q);
        let mut i = self.info.lock().unwrap();
        i.total += 1;
        i.state = "running";
        drop(i);
        cv.notify_one();
    }
}

fn parent(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

fn emit_info(app: &AppHandle, info: &Mutex<JobInfo>, ctl: &Ctl) {
    let mut snapshot = info.lock().unwrap().clone();
    snapshot.bytes = ctl.bytes.load(Ordering::Relaxed);
    let _ = app.emit("job", snapshot);
}

enum Record {
    Up(String),
    Down(String, Option<Entry>),
    Forget(String),
}

/// Starts a transfer job in the background and returns its id immediately.
/// Transfers begin while `plan` is still discovering files; ops it returns are queued at the end.
pub fn start(
    app: AppHandle,
    st: AppState,
    site: Site,
    label: String,
    plan: impl FnOnce(&Plan) -> Result<Vec<Op>, String> + Send + 'static,
) -> u64 {
    let id = st.jobs.next.fetch_add(1, Ordering::Relaxed) + 1;
    let ctl = Arc::new(Ctl::default());
    st.jobs.active.lock().unwrap().insert(id, ctl.clone());
    std::thread::spawn(move || {
        let info = Mutex::new(JobInfo { id, site_id: site.id.clone(), label, state: "scan", scanning: true, ..Default::default() });
        let queue = (Mutex::new(Queue { planning: true, ..Default::default() }), Condvar::new());
        let finished = AtomicBool::new(false);
        let created = Mutex::new(HashSet::new());
        let records = Mutex::new(Vec::new());
        std::thread::scope(|s| {
            s.spawn(|| {
                while !finished.load(Ordering::Relaxed) {
                    emit_info(&app, &info, &ctl);
                    std::thread::sleep(Duration::from_millis(250));
                }
            });
            let workers: Vec<_> = (0..site.max_connections())
                .map(|_| s.spawn(|| worker(&app, &st, &site, &ctl, &info, &queue, &created, &records)))
                .collect();

            let p = Plan { ctl: &ctl, info: &info, queue: &queue };
            match plan(&p) {
                Ok(ops) => ops.into_iter().for_each(|op| p.push(op)),
                Err(e) if !ctl.cancelled() => info.lock().unwrap().errors.push(e),
                Err(_) => {}
            }
            queue.0.lock().unwrap().planning = false;
            queue.1.notify_all();
            {
                let mut i = info.lock().unwrap();
                i.scanning = false;
                i.scan_dir.clear();
            }
            for w in workers {
                let _ = w.join();
            }
            save_records(&st, &site, &info, std::mem::take(&mut *records.lock().unwrap()));
            finished.store(true, Ordering::Relaxed);
        });
        {
            let mut i = info.lock().unwrap();
            i.current.clear();
            i.state = if ctl.cancelled() {
                "cancelled"
            } else if i.failed > 0 || (i.total == 0 && !i.errors.is_empty()) {
                "error"
            } else {
                "done"
            };
        }
        emit_info(&app, &info, &ctl);
        st.jobs.active.lock().unwrap().remove(&id);
    });
    id
}

/// Takes the next op; waits while the planner may still add more.
fn next_op(ctl: &Ctl, queue: &(Mutex<Queue>, Condvar)) -> Option<Op> {
    let (lock, cv) = queue;
    let mut q = lock.lock().unwrap();
    loop {
        if ctl.cancelled() {
            return None;
        }
        if let Some(op) = q.ops.pop_front() {
            return Some(op);
        }
        if !q.planning {
            return None;
        }
        q = cv.wait_timeout(q, Duration::from_millis(200)).unwrap().0;
    }
}

#[allow(clippy::too_many_arguments)]
fn worker(
    app: &AppHandle,
    st: &AppState,
    site: &Site,
    ctl: &Ctl,
    info: &Mutex<JobInfo>,
    queue: &(Mutex<Queue>, Condvar),
    created: &Mutex<HashSet<String>>,
    records: &Mutex<Vec<Record>>,
) {
    while let Some(op) = next_op(ctl, queue) {
        let name = op.dst().to_string();
        info.lock().unwrap().current.push(name.clone());
        let res = exec(st, site, ctl, created, &op);
        {
            let mut i = info.lock().unwrap();
            if let Some(pos) = i.current.iter().position(|c| c == &name) {
                i.current.remove(pos);
            }
            match &res {
                Ok(()) => i.done += 1,
                Err(_) if ctl.cancelled() => {}
                Err(e) => {
                    i.failed += 1;
                    i.errors.push(format!("{name}: {e}"));
                }
            }
        }
        if res.is_ok() {
            let _ = app.emit("fs-changed", FsChanged { site_id: &site.id, side: op.target_side(), dir: parent(&name) });
            match op {
                Op::Up { src, dst } if src == dst => records.lock().unwrap().push(Record::Up(dst)),
                Op::Down { src, dst, entry } if src == dst => records.lock().unwrap().push(Record::Down(dst, entry)),
                Op::DelLocal { dst, forget: true } | Op::DelRemote { dst, forget: true } => records.lock().unwrap().push(Record::Forget(dst)),
                _ => {}
            }
        }
    }
}

fn save_records(st: &AppState, site: &Site, info: &Mutex<JobInfo>, records: Vec<Record>) {
    if records.is_empty() {
        return;
    }
    let _guard = st.db_lock.lock().unwrap();
    let mut db = SyncDb::load(&st.config_dir, &site.id);
    for r in records {
        match r {
            Record::Up(rel) => sync::record_upload(&mut db, site, &rel),
            Record::Down(rel, entry) => sync::record_download(&mut db, site, &rel, entry.as_ref()),
            Record::Forget(rel) => {
                db.map.remove(&rel);
            }
        }
    }
    if let Err(e) = db.save() {
        info.lock().unwrap().errors.push(format!("Sync-Historie: {e}"));
    }
}

fn upload_file(st: &AppState, site: &Site, ctl: &Ctl, created: &Mutex<HashSet<String>>, local: &Path, dst: &str) -> Result<(), String> {
    st.pool
        .with_ctl(site, Some(ctl), |c| {
            c.ensure_dirs(site, dst, &mut created.lock().unwrap());
            let abs = c.abs(site, dst);
            c.upload(&abs, local, Some(ctl))
        })
        .map(|_| ())
}

fn copy_local(ctl: &Ctl, src: &Path, target: &Path) -> Result<(), String> {
    if let Some(p) = target.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    let n = std::fs::copy(src, target).map_err(|e| e.to_string())?;
    ctl.bytes.fetch_add(n, Ordering::Relaxed);
    Ok(())
}

fn exec(st: &AppState, site: &Site, ctl: &Ctl, created: &Mutex<HashSet<String>>, op: &Op) -> Result<(), String> {
    match op {
        Op::Up { src, dst } => upload_file(st, site, ctl, created, &site.local_file(src), dst),
        Op::UpPath { src, dst } => upload_file(st, site, ctl, created, src, dst),
        Op::CopyIn { src, dst } => copy_local(ctl, src, &site.local_file(dst)),
        Op::Down { src, dst, .. } => {
            let local = site.local_file(dst);
            st.pool
                .with_ctl(site, Some(ctl), |c| {
                    let abs = c.abs(site, src);
                    c.download(&abs, &local, Some(ctl))
                })
                .map(|_| ())
        }
        Op::LocalCopy { src, dst } => copy_local(ctl, &site.local_file(src), &site.local_file(dst)),
        Op::RemoteCopy { src, dst } => {
            let tmp = std::env::temp_dir().join("vf-sitemanager-copy").join(uuid::Uuid::new_v4().to_string());
            let res = st.pool.with_ctl(site, Some(ctl), |c| {
                let a = c.abs(site, src);
                c.download(&a, &tmp, Some(ctl))?;
                c.ensure_dirs(site, dst, &mut created.lock().unwrap());
                let b = c.abs(site, dst);
                c.upload(&b, &tmp, Some(ctl))
            });
            let _ = std::fs::remove_file(&tmp);
            res.map(|_| ())
        }
        Op::DelLocal { dst, .. } => trash::delete(site.local_file(dst)).map_err(|e| e.to_string()),
        Op::DelRemote { dst, .. } => st.pool.with_ctl(site, Some(ctl), |c| {
            let abs = c.abs(site, dst);
            c.delete_recursive(&abs, false)
        }),
    }
}
