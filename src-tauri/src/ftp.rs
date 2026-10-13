use crate::sites::{self, Site};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};
use suppaftp::list::File as ListFile;
use suppaftp::types::FileType;
use suppaftp::{FtpError, FtpStream, Mode, Status};

const IDLE_CHECK: Duration = Duration::from_secs(45);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    pub link: bool,
}

const MAX_DEPTH: usize = 40;

/// Cancellation flag and byte counter shared by all transfers of a job.
#[derive(Default)]
pub struct Ctl {
    pub cancel: AtomicBool,
    pub bytes: AtomicU64,
}

impl Ctl {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

struct Tracked<'a, R> {
    inner: R,
    ctl: Option<&'a Ctl>,
}

impl<R: Read> Read for Tracked<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(ctl) = self.ctl {
            if ctl.cancelled() {
                return Err(io::Error::other("Abgebrochen"));
            }
        }
        let n = self.inner.read(buf)?;
        if let Some(ctl) = self.ctl {
            ctl.bytes.fetch_add(n as u64, Ordering::Relaxed);
        }
        Ok(n)
    }
}

pub struct Conn {
    stream: FtpStream,
    home: String,
    last_used: Instant,
}

pub fn join(base: &str, rel: &str) -> String {
    let rel = rel.replace('\\', "/");
    let rel = rel.trim_matches('/');
    if rel.is_empty() {
        base.to_string()
    } else if base.ends_with('/') {
        format!("{base}{rel}")
    } else {
        format!("{base}/{rel}")
    }
}

fn parent_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => "",
    }
}

impl Conn {
    pub fn root(&self, site: &Site) -> String {
        let raw = site.remote_path.trim().replace('\\', "/");
        if raw.starts_with('/') {
            let t = raw.trim_end_matches('/');
            if t.is_empty() { "/".into() } else { t.into() }
        } else {
            join(&self.home, raw.trim_matches('/'))
        }
    }

    pub fn abs(&self, site: &Site, rel: &str) -> String {
        join(&self.root(site), rel)
    }

    pub fn list_dir(&mut self, abs: &str) -> Result<Vec<Entry>, FtpError> {
        self.stream.cwd(abs)?;
        let mut lines = self.stream.list(Some("-a")).unwrap_or_default();
        if lines.is_empty() {
            lines = self.stream.list(None)?;
        }
        let mut out: Vec<Entry> = lines
            .iter()
            .filter_map(|l| ListFile::from_str(l).ok())
            .filter(|f| f.name() != "." && f.name() != "..")
            .map(|f| Entry {
                name: f.name().to_string(),
                is_dir: f.is_directory(),
                size: f.size() as u64,
                mtime: f.modified().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0),
                link: f.is_symlink(),
            })
            .collect();
        // A symlink counts as a folder if the server lets us change into it.
        for e in out.iter_mut().filter(|e| e.link) {
            e.is_dir = self.stream.cwd(join(abs, &e.name)).is_ok();
        }
        out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(out)
    }

    /// Walks the remote tree below `rel`; `on_dir` returns false to stop early.
    pub fn walk(
        &mut self,
        site: &Site,
        rel: &str,
        on_dir: &mut dyn FnMut(&str) -> bool,
    ) -> Result<Vec<(String, Entry)>, FtpError> {
        let mut files = Vec::new();
        self.walk_each(site, rel, on_dir, &mut |rel, e| files.push((rel, e)))?;
        Ok(files)
    }

    /// Like `walk`, but hands every file to `on_file` as soon as its folder is listed.
    pub fn walk_each(
        &mut self,
        site: &Site,
        rel: &str,
        on_dir: &mut dyn FnMut(&str) -> bool,
        on_file: &mut dyn FnMut(String, Entry),
    ) -> Result<(), FtpError> {
        let mut stack = vec![(rel.trim_matches('/').to_string(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            if !on_dir(&dir) {
                break;
            }
            let abs = self.abs(site, &dir);
            for e in self.list_dir(&abs)? {
                if site.is_excluded(&e.name) {
                    continue;
                }
                let child = if dir.is_empty() { e.name.clone() } else { format!("{dir}/{}", e.name) };
                if !e.is_dir {
                    on_file(child, e);
                } else if depth < MAX_DEPTH {
                    stack.push((child, depth + 1));
                }
            }
        }
        Ok(())
    }

    pub fn ensure_dirs(&mut self, site: &Site, rel_file: &str, created: &mut HashSet<String>) {
        let mut current = self.root(site);
        let parts: Vec<&str> = rel_file.split('/').filter(|s| !s.is_empty()).collect();
        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            current = join(&current, part);
            if created.insert(current.clone()) {
                let _ = self.stream.mkdir(&current);
            }
        }
    }

    pub fn upload(&mut self, abs: &str, local: &Path, ctl: Option<&Ctl>) -> Result<u64, FtpError> {
        let file = File::open(local).map_err(FtpError::ConnectionError)?;
        let mut reader = Tracked { inner: file, ctl };
        self.stream.put_file(abs, &mut reader)
    }

    pub fn download(&mut self, abs: &str, local: &Path, ctl: Option<&Ctl>) -> Result<u64, FtpError> {
        if let Some(dir) = local.parent() {
            std::fs::create_dir_all(dir).map_err(FtpError::ConnectionError)?;
        }
        let tmp = PathBuf::from(format!("{}.vfpart", local.display()));
        let mut file = File::create(&tmp).map_err(FtpError::ConnectionError)?;
        let res = self.stream.retr(abs, |r| {
            let mut reader = Tracked { inner: r, ctl };
            io::copy(&mut reader, &mut file).map_err(FtpError::ConnectionError)
        });
        drop(file);
        match res {
            Ok(n) => {
                std::fs::rename(&tmp, local).map_err(FtpError::ConnectionError)?;
                Ok(n)
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// Deletes a file or folder tree; symlinks are removed as links, never followed.
    pub fn delete_recursive(&mut self, abs: &str, is_dir: bool) -> Result<(), FtpError> {
        let link = is_dir && self.file_entry(abs)?.is_some_and(|e| e.link);
        self.delete_entry(abs, is_dir && !link)
    }

    fn delete_entry(&mut self, abs: &str, is_dir: bool) -> Result<(), FtpError> {
        if !is_dir {
            return self.stream.rm(abs);
        }
        for e in self.list_dir(abs)? {
            self.delete_entry(&join(abs, &e.name), e.is_dir && !e.link)?;
        }
        self.stream.rmdir(abs)
    }

    pub fn mkdir(&mut self, abs: &str) -> Result<(), FtpError> {
        self.stream.mkdir(abs)
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), FtpError> {
        self.stream.rename(from, to)
    }

    pub fn file_entry(&mut self, abs: &str) -> Result<Option<Entry>, FtpError> {
        let name = abs.rsplit('/').next().unwrap_or(abs).to_string();
        let dir = parent_of(abs).to_string();
        Ok(self.list_dir(&dir)?.into_iter().find(|e| e.name == name))
    }
}

fn is_connection_error(e: &FtpError) -> bool {
    match e {
        FtpError::ConnectionError(_) | FtpError::BadResponse | FtpError::DataConnectionAlreadyOpen => true,
        FtpError::UnexpectedResponse(r) => r.status == Status::NotAvailable,
        _ => false,
    }
}

fn refused(e: &FtpError) -> bool {
    match e {
        FtpError::ConnectionError(io) => io.kind() == io::ErrorKind::ConnectionRefused,
        FtpError::UnexpectedResponse(r) => r.status == Status::NotAvailable,
        _ => false,
    }
}

pub fn describe(e: &FtpError) -> String {
    match e {
        FtpError::UnexpectedResponse(r) => {
            let body = String::from_utf8_lossy(&r.body).trim().to_string();
            format!("Server meldet: {body}")
        }
        other => other.to_string(),
    }
}

pub fn connect(site: &Site, password: Option<&str>) -> Result<Conn, String> {
    if site.host.trim().is_empty() {
        return Err("Kein FTP-Host eingetragen".into());
    }
    let addr = (site.host.trim(), site.port_or_default())
        .to_socket_addrs()
        .map_err(|e| format!("Host nicht gefunden: {e}"))?
        .next()
        .ok_or("Host nicht gefunden")?;
    let mut stream = FtpStream::connect_timeout(addr, Duration::from_secs(15)).map_err(|e| describe(&e))?;
    if !site.passive {
        stream = stream.active_mode(Duration::from_secs(30));
    } else {
        stream.set_mode(Mode::Passive);
        stream.set_passive_nat_workaround(true);
    }
    let stored;
    let pw = match password {
        Some(p) if !p.is_empty() => p,
        _ => {
            stored = sites::get_password(&site.id).unwrap_or_default();
            stored.as_str()
        }
    };
    stream.login(site.user.as_str(), pw).map_err(|e| format!("Login fehlgeschlagen: {}", describe(&e)))?;
    stream.transfer_type(FileType::Binary).map_err(|e| describe(&e))?;
    let home = stream.pwd().unwrap_or_else(|_| "/".into());
    Ok(Conn { stream, home, last_used: Instant::now() })
}

#[derive(Default)]
struct Slots {
    idle: Vec<Conn>,
    total: usize,
    transfers: usize,
    /// Connection limit learned from the server refusing further connections (0 = none).
    cap: usize,
}

impl Slots {
    fn limit(&self, site: &Site) -> usize {
        let max = site.max_connections();
        if self.cap > 0 { max.min(self.cap) } else { max }
    }

    /// The server refused a connection while others were open: stay below the current count.
    fn lower_cap(&mut self) -> bool {
        let others = self.total;
        if others == 0 {
            return false;
        }
        self.cap = if self.cap > 0 { self.cap.min(others) } else { others };
        true
    }
}

type SiteSlots = Arc<(Mutex<Slots>, Condvar)>;

/// Up to `site.max_connections()` FTP connections per site, shared by listings and transfers.
#[derive(Default)]
pub struct Pool {
    sites: Mutex<HashMap<String, SiteSlots>>,
}

impl Pool {
    fn slots(&self, id: &str) -> SiteSlots {
        self.sites.lock().unwrap().entry(id.to_string()).or_default().clone()
    }

    /// Transfers may use all but one connection so listings stay responsive during long transfers.
    fn checkout(&self, site: &Site, transfer: bool) -> Result<Conn, String> {
        let slots = self.slots(&site.id);
        let (lock, cv) = &*slots;
        let mut g = lock.lock().unwrap();
        loop {
            let max = g.limit(site);
            if transfer && max > 1 && g.transfers >= max - 1 {
                g = cv.wait(g).unwrap();
                continue;
            }
            if let Some(mut c) = g.idle.pop() {
                if transfer {
                    g.transfers += 1;
                }
                drop(g);
                if c.last_used.elapsed() < IDLE_CHECK || c.stream.noop().is_ok() {
                    return Ok(c);
                }
                g = lock.lock().unwrap();
                g.total -= 1;
                if transfer {
                    g.transfers -= 1;
                }
                continue;
            }
            if g.total < max {
                g.total += 1;
                if transfer {
                    g.transfers += 1;
                }
                drop(g);
                let res = connect(site, None);
                if res.is_ok() {
                    return res;
                }
                g = lock.lock().unwrap();
                g.total -= 1;
                if transfer {
                    g.transfers -= 1;
                }
                cv.notify_all();
                let login_failed = res.as_ref().is_err_and(|e| e.starts_with("Login fehlgeschlagen") && !e.contains("421"));
                if login_failed || !g.lower_cap() {
                    return res;
                }
                continue;
            }
            g = cv.wait(g).unwrap();
        }
    }

    fn checkin(&self, site_id: &str, conn: Option<Conn>, transfer: bool) {
        let slots = self.slots(site_id);
        let (lock, cv) = &*slots;
        let mut g = lock.lock().unwrap();
        match conn {
            Some(c) => g.idle.push(c),
            None => g.total = g.total.saturating_sub(1),
        }
        if transfer {
            g.transfers = g.transfers.saturating_sub(1);
        }
        cv.notify_all();
    }

    pub fn with<R>(&self, site: &Site, f: impl FnMut(&mut Conn) -> Result<R, FtpError>) -> Result<R, String> {
        self.with_ctl(site, None, f)
    }

    /// Runs `f` on a pooled connection; retries on connection loss, drops the connection on cancel.
    pub fn with_ctl<R>(
        &self,
        site: &Site,
        ctl: Option<&Ctl>,
        mut f: impl FnMut(&mut Conn) -> Result<R, FtpError>,
    ) -> Result<R, String> {
        const ATTEMPTS: u64 = 3;
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(400 * attempt));
                if ctl.is_some_and(|c| c.cancelled()) {
                    return Err("Abgebrochen".into());
                }
            }
            let transfer = ctl.is_some();
            let mut conn = self.checkout(site, transfer)?;
            let res = f(&mut conn);
            conn.last_used = Instant::now();
            match res {
                Ok(r) => {
                    self.checkin(&site.id, Some(conn), transfer);
                    return Ok(r);
                }
                Err(_) if ctl.is_some_and(|c| c.cancelled()) => {
                    self.checkin(&site.id, None, transfer);
                    return Err("Abgebrochen".into());
                }
                Err(e) if is_connection_error(&e) => {
                    self.checkin(&site.id, None, transfer);
                    if refused(&e) {
                        self.slots(&site.id).0.lock().unwrap().lower_cap();
                    }
                    if attempt + 1 == ATTEMPTS {
                        return Err(describe(&e));
                    }
                }
                Err(e) => {
                    self.checkin(&site.id, Some(conn), transfer);
                    return Err(describe(&e));
                }
            }
        }
        Err("Verbindung zum Server verloren".into())
    }

    pub fn disconnect(&self, id: &str) {
        let slots = self.slots(id);
        let mut g = slots.0.lock().unwrap();
        let idle: Vec<Conn> = g.idle.drain(..).collect();
        g.total = g.total.saturating_sub(idle.len());
        g.cap = 0;
        drop(g);
        for mut c in idle {
            let _ = c.stream.quit();
        }
        slots.1.notify_all();
    }
}
