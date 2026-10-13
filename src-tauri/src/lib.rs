mod export;
mod ftp;
mod git;
mod import;
mod jobs;
mod master;
mod sites;
mod sync;
mod update;

use ftp::{Entry, Pool};
use jobs::{Jobs, Op};
use serde::Serialize;
use sites::{Site, SiteStore};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use sync::{SyncDb, SyncItem};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone)]
pub(crate) struct AppState {
    config_dir: PathBuf,
    sites: Arc<Mutex<Vec<Site>>>,
    pool: Arc<Pool>,
    jobs: Arc<Jobs>,
    db_lock: Arc<Mutex<()>>,
    master: Arc<Mutex<master::Session>>,
}

impl AppState {
    fn site(&self, id: &str) -> Result<Site, String> {
        self.sites
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| "Site nicht gefunden".to_string())
    }

    fn persist(&self) -> Result<(), String> {
        SiteStore::new(&self.config_dir).save(&self.sites.lock().unwrap())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    site_id: String,
    kind: String,
    rel: String,
    index: usize,
    total: usize,
    error: Option<String>,
}

fn emit(app: &AppHandle, site_id: &str, kind: &str, rel: &str, index: usize, total: usize, error: Option<String>) {
    let _ = app.emit(
        "progress",
        Progress { site_id: site_id.into(), kind: kind.into(), rel: rel.into(), index, total, error },
    );
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

// ---------- Sites ----------

#[tauri::command]
fn sites_list(state: State<'_, AppState>) -> Vec<Site> {
    state.sites.lock().unwrap().clone()
}

#[tauri::command]
fn site_save(state: State<'_, AppState>, mut site: Site, password: Option<String>) -> Result<Site, String> {
    if site.id.is_empty() {
        site.id = uuid::Uuid::new_v4().to_string();
    }
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        sites::set_password(&site.id, &pw)?;
        site.has_password = true;
    }
    {
        let mut list = state.sites.lock().unwrap();
        match list.iter_mut().find(|s| s.id == site.id) {
            Some(existing) => {
                site.has_password = site.has_password || existing.has_password;
                site.logins = existing.logins.clone();
                *existing = site.clone();
            }
            None => list.push(site.clone()),
        }
        list.sort_by_key(|s| s.name.to_lowercase());
    }
    state.pool.disconnect(&site.id);
    state.persist()?;
    Ok(site)
}

#[tauri::command]
fn site_set_group(state: State<'_, AppState>, ids: Vec<String>, group: String) -> Result<(), String> {
    let group = group.trim().to_string();
    {
        let mut list = state.sites.lock().unwrap();
        for s in list.iter_mut().filter(|s| ids.contains(&s.id)) {
            s.group = group.clone();
        }
    }
    state.persist()
}

#[tauri::command]
fn site_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    {
        let mut list = state.sites.lock().unwrap();
        if let Some(site) = list.iter().find(|s| s.id == id) {
            for l in &site.logins {
                sites::delete_password(&sites::login_key(&id, &l.id));
            }
        }
        list.retain(|s| s.id != id);
    }
    state.pool.disconnect(&id);
    sites::delete_password(&id);
    SyncDb::delete(&state.config_dir, &id);
    state.persist()
}

#[tauri::command]
fn login_save(
    state: State<'_, AppState>,
    site_id: String,
    mut login: sites::Login,
    password: Option<String>,
) -> Result<Site, String> {
    if login.id.is_empty() {
        login.id = uuid::Uuid::new_v4().to_string();
    }
    if let Some(pw) = password.filter(|p| !p.is_empty()) {
        sites::set_password(&sites::login_key(&site_id, &login.id), &pw)?;
        login.has_password = true;
    }
    let site = {
        let mut list = state.sites.lock().unwrap();
        let site = list.iter_mut().find(|s| s.id == site_id).ok_or("Site nicht gefunden")?;
        match site.logins.iter_mut().find(|l| l.id == login.id) {
            Some(existing) => {
                login.has_password = login.has_password || existing.has_password;
                *existing = login;
            }
            None => site.logins.push(login),
        }
        site.clone()
    };
    state.persist()?;
    Ok(site)
}

#[tauri::command]
fn login_delete(state: State<'_, AppState>, site_id: String, login_id: String) -> Result<Site, String> {
    let site = {
        let mut list = state.sites.lock().unwrap();
        let site = list.iter_mut().find(|s| s.id == site_id).ok_or("Site nicht gefunden")?;
        site.logins.retain(|l| l.id != login_id);
        site.clone()
    };
    sites::delete_password(&sites::login_key(&site_id, &login_id));
    state.persist()?;
    Ok(site)
}

#[tauri::command]
fn login_password(state: State<'_, AppState>, site_id: String, login_id: String) -> Result<String, String> {
    state.master.lock().unwrap().require()?;
    sites::get_password(&sites::login_key(&site_id, &login_id)).ok_or_else(|| "Kein Passwort gespeichert".into())
}

#[tauri::command]
fn ftp_password(state: State<'_, AppState>, site_id: String) -> Result<String, String> {
    state.master.lock().unwrap().require()?;
    sites::get_password(&site_id).ok_or_else(|| "Kein Passwort gespeichert".into())
}

// ---------- Master password ----------

#[tauri::command]
fn master_status(state: State<'_, AppState>) -> master::Status {
    state.master.lock().unwrap().status()
}

#[tauri::command]
async fn master_setup(state: State<'_, AppState>, password: String, email: String) -> Result<(), String> {
    let m = state.master.clone();
    blocking(move || m.lock().unwrap().setup(&password, &email)).await
}

#[tauri::command]
async fn master_unlock(state: State<'_, AppState>, password: String) -> Result<(), String> {
    let m = state.master.clone();
    blocking(move || m.lock().unwrap().unlock_with(&password)).await
}

#[tauri::command]
fn master_lock(state: State<'_, AppState>) {
    state.master.lock().unwrap().lock();
}

#[tauri::command]
async fn master_change(state: State<'_, AppState>, current: String, password: Option<String>, email: Option<String>) -> Result<(), String> {
    let m = state.master.clone();
    blocking(move || m.lock().unwrap().change(&current, password.as_deref(), email.as_deref())).await
}

#[tauri::command]
async fn master_forgot(lang: String) -> Result<String, String> {
    blocking(move || master::Session::forgot(&lang)).await
}

#[tauri::command]
async fn master_reset(state: State<'_, AppState>, code: String, password: String) -> Result<(), String> {
    let m = state.master.clone();
    blocking(move || m.lock().unwrap().reset(&code, &password)).await
}

#[tauri::command]
async fn site_test(site: Site, password: Option<String>) -> Result<String, String> {
    blocking(move || {
        let mut conn = ftp::connect(&site, password.as_deref())?;
        let root = conn.root(&site);
        let count = conn.list_dir(&root).map_err(|e| format!("Login ok, aber Site-Root {root} nicht lesbar: {}", ftp::describe(&e)))?.len();
        Ok(format!("Verbunden. Site-Root {root} enthält {count} Einträge."))
    })
    .await
}

#[tauri::command]
fn import_dreamweaver(state: State<'_, AppState>) -> Result<import::ImportResult, String> {
    let res = {
        let mut list = state.sites.lock().unwrap();
        let r = import::import_dreamweaver(&mut list);
        list.sort_by_key(|s| s.name.to_lowercase());
        r
    };
    state.persist()?;
    Ok(res)
}

#[tauri::command]
fn import_folders(state: State<'_, AppState>, root: String) -> Result<import::ImportResult, String> {
    let res = {
        let mut list = state.sites.lock().unwrap();
        let r = import::import_folders(&mut list, &root)?;
        list.sort_by_key(|s| s.name.to_lowercase());
        r
    };
    state.persist()?;
    Ok(res)
}

#[tauri::command]
async fn export_sites(state: State<'_, AppState>, ids: Vec<String>, path: String, password: String) -> Result<usize, String> {
    state.master.lock().unwrap().require()?;
    let chosen: Vec<Site> = state.sites.lock().unwrap().iter().filter(|s| ids.contains(&s.id)).cloned().collect();
    blocking(move || {
        let content = export::export_sites(&chosen, &password)?;
        std::fs::write(&path, content).map_err(|e| format!("{path}: {e}"))?;
        Ok(chosen.len())
    })
    .await
}

#[tauri::command]
async fn import_sites(state: State<'_, AppState>, path: String, password: String) -> Result<import::ImportResult, String> {
    let st = state.inner().clone();
    blocking(move || {
        let content = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
        let res = export::import_sites(&mut st.sites.lock().unwrap(), &content, &password)?;
        st.persist()?;
        Ok(res)
    })
    .await
}

// ---------- Language packs ----------

fn lang_dir(st: &AppState) -> PathBuf {
    st.config_dir.join("lang")
}

/// Installed language packs: JSON files `{ "code", "name", "strings": { "<German text>": "<translation>" } }`.
#[tauri::command]
fn lang_list(state: State<'_, AppState>) -> Vec<serde_json::Value> {
    let Ok(rd) = std::fs::read_dir(lang_dir(&state)) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(|v| v["code"].is_string() && v["strings"].is_object())
        .collect()
}

#[tauri::command]
fn lang_install(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let content = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&content).map_err(|e| format!("Ungültiges JSON: {e}"))?;
    let code = v["code"].as_str().filter(|c| !c.is_empty() && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-'));
    let Some(code) = code else { return Err("Sprachpaket ohne gültigen «code»".into()) };
    if !v["strings"].is_object() {
        return Err("Sprachpaket ohne «strings»".into());
    }
    let dir = lang_dir(&state);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{code}.json")), content).map_err(|e| e.to_string())?;
    Ok(v["name"].as_str().unwrap_or(code).to_string())
}

#[tauri::command]
fn lang_template(state: State<'_, AppState>, strings: Vec<String>) -> Result<String, String> {
    let dir = lang_dir(&state);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let map: serde_json::Map<String, serde_json::Value> = strings.into_iter().map(|s| (s, serde_json::Value::String(String::new()))).collect();
    let v = serde_json::json!({ "code": "xx", "name": "Neue Sprache", "strings": map });
    let path = dir.join("template.json.txt");
    std::fs::write(&path, serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

// ---------- Files ----------

#[tauri::command]
fn local_list(state: State<'_, AppState>, site_id: String, rel: String) -> Result<Vec<Entry>, String> {
    sync::local_list(&state.site(&site_id)?, &rel)
}

#[tauri::command]
async fn remote_list(state: State<'_, AppState>, site_id: String, rel: String) -> Result<Vec<Entry>, String> {
    let st = state.inner().clone();
    blocking(move || {
        let site = st.site(&site_id)?;
        st.pool.with(&site, |c| {
            let abs = c.abs(&site, &rel);
            c.list_dir(&abs)
        })
    })
    .await
}

fn job_label(verb: &str, rels: &[String]) -> String {
    match rels {
        [one] => format!("{verb}: {}", one.rsplit('/').next().unwrap_or(one)),
        _ => format!("{verb}: {} Elemente", rels.len()),
    }
}

fn plan_uploads(site: &Site, rels: &[String]) -> Vec<Op> {
    let mut files: Vec<String> = rels.iter().flat_map(|r| sync::local_walk(site, r).into_keys()).collect();
    files.sort();
    files.dedup();
    files.into_iter().map(|f| Op::Up { src: f.clone(), dst: f }).collect()
}

fn plan_downloads(st: &AppState, site: &Site, rels: &[String], plan: &jobs::Plan) -> Result<Vec<Op>, String> {
    let push = |f: String, e: Entry| plan.push(Op::Down { src: f.clone(), dst: f, entry: Some(e) });
    for rel in rels {
        if plan.ctl.cancelled() {
            break;
        }
        st.pool.with_ctl(site, Some(plan.ctl), |c| {
            let abs = c.abs(site, rel);
            match c.file_entry(&abs)? {
                Some(e) if !e.is_dir => {
                    push(rel.trim_matches('/').to_string(), e);
                    Ok(())
                }
                _ => c.walk_each(site, rel, &mut |d| plan.scanning(d), &mut |f, e| push(f, e)),
            }
        })?;
    }
    Ok(Vec::new())
}

#[tauri::command]
fn upload(app: AppHandle, state: State<'_, AppState>, site_id: String, rels: Vec<String>) -> Result<u64, String> {
    let st = state.inner().clone();
    let site = st.site(&site_id)?;
    let label = job_label("Hochladen", &rels);
    let s2 = site.clone();
    Ok(jobs::start(app, st, site, label, move |_| Ok(plan_uploads(&s2, &rels))))
}

#[tauri::command]
fn download(app: AppHandle, state: State<'_, AppState>, site_id: String, rels: Vec<String>) -> Result<u64, String> {
    let st = state.inner().clone();
    let site = st.site(&site_id)?;
    let label = job_label("Herunterladen", &rels);
    let (st2, s2) = (st.clone(), site.clone());
    Ok(jobs::start(app, st, site, label, move |p| plan_downloads(&st2, &s2, &rels, p)))
}

fn collect_external(site: &Site, src: &std::path::Path, dst: &str, out: &mut Vec<(PathBuf, String)>) {
    let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if name.is_empty() || site.is_excluded(&name) {
        return;
    }
    let target = rel_join(dst, &name);
    if src.is_dir() {
        if let Ok(rd) = std::fs::read_dir(src) {
            for e in rd.flatten() {
                collect_external(site, &e.path(), &target, out);
            }
        }
    } else if src.is_file() {
        out.push((src.to_path_buf(), target));
    }
}

/// Files/folders dropped from outside (e.g. the desktop): upload to the server or copy into the local site folder.
#[tauri::command]
fn drop_paths(
    app: AppHandle,
    state: State<'_, AppState>,
    site_id: String,
    paths: Vec<String>,
    side: String,
    dest: String,
) -> Result<u64, String> {
    let st = state.inner().clone();
    let site = st.site(&site_id)?;
    let names: Vec<String> = paths.iter().map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()).collect();
    let to_remote = side == "remote";
    let label = job_label(if to_remote { "Hochladen" } else { "Kopieren" }, &names);
    let s2 = site.clone();
    Ok(jobs::start(app, st, site, label, move |plan| {
        let mut files = Vec::new();
        for p in &paths {
            if plan.ctl.cancelled() {
                break;
            }
            let src = PathBuf::from(p);
            if !to_remote && src.starts_with(&s2.local_path) {
                return Err(format!("«{p}» liegt bereits im Site-Ordner"));
            }
            plan.scanning(p);
            collect_external(&s2, &src, &dest, &mut files);
        }
        Ok(files
            .into_iter()
            .map(|(src, dst)| if to_remote { Op::UpPath { src, dst } } else { Op::CopyIn { src, dst } })
            .collect())
    }))
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>, id: u64) {
    state.jobs.cancel(id);
}

#[tauri::command]
fn cancel_all_jobs(state: State<'_, AppState>) {
    state.jobs.cancel_all();
}

#[tauri::command]
async fn remote_delete(state: State<'_, AppState>, site_id: String, rel: String, is_dir: bool) -> Result<(), String> {
    let st = state.inner().clone();
    blocking(move || {
        let site = st.site(&site_id)?;
        st.pool.with(&site, |c| {
            let abs = c.abs(&site, &rel);
            c.delete_recursive(&abs, is_dir)
        })?;
        let _guard = st.db_lock.lock().unwrap();
        let mut db = SyncDb::load(&st.config_dir, &site.id);
        let prefix = format!("{}/", rel.trim_matches('/'));
        db.map.retain(|k, _| k != rel.trim_matches('/') && !k.starts_with(&prefix));
        db.save()
    })
    .await
}

#[tauri::command]
async fn remote_mkdir(state: State<'_, AppState>, site_id: String, rel: String) -> Result<(), String> {
    let st = state.inner().clone();
    blocking(move || {
        let site = st.site(&site_id)?;
        st.pool.with(&site, |c| {
            let abs = c.abs(&site, &rel);
            c.mkdir(&abs)
        })
    })
    .await
}

#[tauri::command]
async fn remote_rename(state: State<'_, AppState>, site_id: String, from: String, to: String) -> Result<(), String> {
    let st = state.inner().clone();
    blocking(move || {
        let site = st.site(&site_id)?;
        st.pool.with(&site, |c| {
            let (a, b) = (c.abs(&site, &from), c.abs(&site, &to));
            c.rename(&a, &b)
        })
    })
    .await
}

#[tauri::command]
fn local_mkdir(state: State<'_, AppState>, site_id: String, rel: String) -> Result<(), String> {
    std::fs::create_dir(state.site(&site_id)?.local_file(&rel)).map_err(|e| e.to_string())
}

#[tauri::command]
fn local_rename(state: State<'_, AppState>, site_id: String, from: String, to: String) -> Result<(), String> {
    let site = state.site(&site_id)?;
    let target = site.local_file(&to);
    if target.exists() {
        return Err(format!("«{to}» existiert bereits"));
    }
    std::fs::rename(site.local_file(&from), target).map_err(|e| e.to_string())
}

/// Moves local files/folders to the recycle bin.
#[tauri::command]
async fn local_delete(state: State<'_, AppState>, site_id: String, rels: Vec<String>) -> Result<(), String> {
    let st = state.inner().clone();
    blocking(move || {
        let site = st.site(&site_id)?;
        let paths: Vec<PathBuf> = rels
            .iter()
            .map(|r| r.trim_matches('/'))
            .filter(|r| !r.is_empty())
            .map(|r| site.local_file(r))
            .collect();
        if paths.len() != rels.len() {
            return Err("Der Site-Ordner selbst kann nicht gelöscht werden".into());
        }
        trash::delete_all(&paths).map_err(|e| e.to_string())
    })
    .await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopyItem {
    rel: String,
    is_dir: bool,
}

fn rel_parent(rel: &str) -> &str {
    rel.rfind('/').map(|i| &rel[..i]).unwrap_or("")
}

fn rel_join(dir: &str, name: &str) -> String {
    let dir = dir.trim_matches('/');
    if dir.is_empty() { name.to_string() } else { format!("{dir}/{name}") }
}

fn copy_name(name: &str, is_dir: bool, n: usize) -> String {
    let suffix = if n == 1 { " - Kopie".to_string() } else { format!(" - Kopie ({n})") };
    match name.rfind('.') {
        Some(i) if !is_dir && i > 0 => format!("{}{suffix}{}", &name[..i], &name[i..]),
        _ => format!("{name}{suffix}"),
    }
}

/// Copies files/folders between or within the local and remote side; cross-side copies are transfers.
#[tauri::command]
fn copy_items(
    app: AppHandle,
    state: State<'_, AppState>,
    site_id: String,
    from: String,
    items: Vec<CopyItem>,
    to: String,
    dest: String,
) -> Result<u64, String> {
    let st = state.inner().clone();
    let site = st.site(&site_id)?;
    let rels: Vec<String> = items.iter().map(|i| i.rel.clone()).collect();
    let verb = match (from.as_str(), to.as_str()) {
        ("local", "remote") => "Hochladen",
        ("remote", "local") => "Herunterladen",
        _ => "Kopieren",
    };
    let label = job_label(verb, &rels);
    let (st2, site2) = (st.clone(), site.clone());
    Ok(jobs::start(app, st, site, label, move |plan| plan_copy(&st2, &site2, &from, &items, &to, &dest, plan)))
}

fn plan_copy(
    st: &AppState,
    site: &Site,
    from: &str,
    items: &[CopyItem],
    to: &str,
    dest: &str,
    plan: &jobs::Plan,
) -> Result<Vec<Op>, String> {
    let (from_remote, to_remote) = (from == "remote", to == "remote");
    let dest = dest.trim_matches('/').to_string();

    let mut taken: HashSet<String> = if to_remote {
        st.pool
            .with(site, |c| {
                let abs = c.abs(site, &dest);
                c.list_dir(&abs)
            })?
            .into_iter()
            .map(|e| e.name)
            .collect()
    } else {
        sync::local_list(site, &dest)?.into_iter().map(|e| e.name).collect()
    };

    let mut ops = Vec::new();
    for it in items {
        if plan.ctl.cancelled() {
            break;
        }
        let rel = it.rel.trim_matches('/').to_string();
        let base = rel.rsplit('/').next().unwrap_or(&rel).to_string();
        let mut name = base.clone();
        if from == to && rel_parent(&rel) == dest {
            let mut n = 1;
            while taken.contains(&copy_name(&base, it.is_dir, n)) {
                n += 1;
            }
            name = copy_name(&base, it.is_dir, n);
        }
        taken.insert(name.clone());
        let target = rel_join(&dest, &name);
        if from == to && it.is_dir && (target == rel || target.starts_with(&format!("{rel}/"))) {
            return Err(format!("«{rel}» kann nicht in sich selbst kopiert werden"));
        }
        let files: Vec<String> = if !it.is_dir {
            vec![rel.clone()]
        } else if from_remote {
            st.pool.with(site, |c| c.walk(site, &rel, &mut |d| plan.scanning(d)))?.into_iter().map(|(k, _)| k).collect()
        } else {
            sync::local_walk(site, &rel).into_keys().collect()
        };
        if it.is_dir {
            if to_remote {
                let _ = st.pool.with(site, |c| {
                    let abs = c.abs(site, &target);
                    c.mkdir(&abs)
                });
            } else {
                std::fs::create_dir_all(site.local_file(&target)).map_err(|e| e.to_string())?;
            }
        }
        for src in files {
            let dst = format!("{target}{}", &src[rel.len()..]);
            ops.push(match (from_remote, to_remote) {
                (false, false) => Op::LocalCopy { src, dst },
                (false, true) => Op::Up { src, dst },
                (true, false) => Op::Down { src, dst, entry: None },
                (true, true) => Op::RemoteCopy { src, dst },
            });
        }
    }
    Ok(ops)
}

/// Nearest existing local path for `rel`, used to reveal remote items in the Explorer.
#[tauri::command]
fn reveal_path(state: State<'_, AppState>, site_id: String, rel: String) -> Result<String, String> {
    let site = state.site(&site_id)?;
    let mut rel = rel.trim_matches('/').to_string();
    loop {
        let p = site.local_file(&rel);
        if p.exists() || rel.is_empty() {
            return Ok(p.to_string_lossy().to_string());
        }
        rel = rel_parent(&rel).to_string();
    }
}

#[tauri::command]
fn open_folder(state: State<'_, AppState>, site_id: String) -> Result<(), String> {
    let path = state.site(&site_id)?.local_path;
    if !std::path::Path::new(&path).is_dir() {
        return Err(format!("Ordner nicht gefunden: {path}"));
    }
    std::process::Command::new("explorer").arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn disconnect(state: State<'_, AppState>, site_id: String) {
    state.pool.disconnect(&site_id);
}

// ---------- Sync ----------

/// Deepest folder containing all given paths ("" = site root).
fn common_dir(rels: &[String]) -> String {
    let mut iter = rels.iter().map(|r| rel_parent(r.trim_matches('/')));
    let Some(first) = iter.next() else { return String::new() };
    let mut parts: Vec<&str> = first.split('/').filter(|p| !p.is_empty()).collect();
    for dir in iter {
        let other: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
        let n = parts.iter().zip(&other).take_while(|(a, b)| a == b).count();
        parts.truncate(n);
    }
    parts.join("/")
}

#[tauri::command]
async fn sync_compare(app: AppHandle, state: State<'_, AppState>, site_id: String, rel: Option<String>) -> Result<Vec<SyncItem>, String> {
    let st = state.inner().clone();
    blocking(move || {
        let site = st.site(&site_id)?;
        let rel = rel.unwrap_or_default();
        let local = sync::local_walk(&site, &rel);
        let remote = st.pool.with(&site, |c| {
            sync::remote_walk_map(c, &site, &rel, &mut |d| {
                emit(&app, &site.id, "scan", d, 0, 0, None);
                true
            })
        })?;
        let _guard = st.db_lock.lock().unwrap();
        let mut db = SyncDb::load(&st.config_dir, &site.id);
        let items = sync::compare(&mut db, &local, &remote);
        db.save()?;
        emit(&app, &site.id, "done", "", 0, 0, None);
        Ok(items)
    })
    .await
}

#[derive(serde::Deserialize)]
struct SyncDelete {
    rel: String,
    side: String,
    forget: bool,
}

#[tauri::command]
fn sync_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    site_id: String,
    uploads: Vec<String>,
    downloads: Vec<String>,
    deletes: Option<Vec<SyncDelete>>,
) -> Result<u64, String> {
    let st = state.inner().clone();
    let site = st.site(&site_id)?;
    let deletes = deletes.unwrap_or_default();
    let mut label = format!("Synchronisieren: {} ↑ · {} ↓", uploads.len(), downloads.len());
    if !deletes.is_empty() {
        label.push_str(&format!(" · {} ✕", deletes.len()));
    }
    let (st2, s2) = (st.clone(), site.clone());
    Ok(jobs::start(app, st, site, label, move |plan| {
        for d in deletes {
            plan.push(match d.side.as_str() {
                "local" => Op::DelLocal { dst: d.rel, forget: d.forget },
                _ => Op::DelRemote { dst: d.rel, forget: d.forget },
            });
        }
        for f in uploads {
            plan.push(Op::Up { src: f.clone(), dst: f });
        }
        if !downloads.is_empty() {
            let wanted: HashSet<String> = downloads.into_iter().collect();
            st2.pool.with_ctl(&s2, Some(plan.ctl), |c| {
                c.walk_each(&s2, &common_dir(&wanted.iter().cloned().collect::<Vec<_>>()), &mut |d| plan.scanning(d), &mut |k, e| {
                    if wanted.contains(&k) {
                        plan.push(Op::Down { src: k.clone(), dst: k, entry: Some(e) });
                    }
                })
            })?;
        }
        Ok(Vec::new())
    }))
}

#[tauri::command]
async fn sync_mark_same(state: State<'_, AppState>, site_id: String, rels: Vec<String>) -> Result<usize, String> {
    let st = state.inner().clone();
    blocking(move || {
        let site = st.site(&site_id)?;
        let base = common_dir(&rels);
        let remote = st.pool.with(&site, |c| sync::remote_walk_map(c, &site, &base, &mut |_| true))?;
        let _guard = st.db_lock.lock().unwrap();
        let mut db = SyncDb::load(&st.config_dir, &site.id);
        let mut n = 0;
        for rel in &rels {
            if let Some(e) = remote.get(rel) {
                sync::record_same(&mut db, &site, rel, e);
                n += 1;
            }
        }
        db.save()?;
        Ok(n)
    })
    .await
}

// ---------- Editor ----------

#[tauri::command]
fn file_read(state: State<'_, AppState>, site_id: String, rel: String) -> Result<String, String> {
    let path = state.site(&site_id)?.local_file(&rel);
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "Datei ist nicht UTF-8-kodiert und kann nicht bearbeitet werden".into())
}

#[tauri::command]
fn file_write(state: State<'_, AppState>, site_id: String, rel: String, content: String) -> Result<(), String> {
    let path = state.site(&site_id)?.local_file(&rel);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

#[tauri::command]
fn local_path(state: State<'_, AppState>, site_id: String, rel: String) -> Result<String, String> {
    Ok(state.site(&site_id)?.local_file(&rel).to_string_lossy().to_string())
}

// ---------- Git ----------

#[tauri::command]
async fn git_status(state: State<'_, AppState>, site_id: String) -> Result<git::GitStatus, String> {
    let dir = state.site(&site_id)?.local_path;
    blocking(move || Ok(git::status(&dir))).await
}

#[tauri::command]
async fn git_commit(state: State<'_, AppState>, site_id: String, message: String) -> Result<String, String> {
    let dir = state.site(&site_id)?.local_path;
    blocking(move || git::commit(&dir, &message)).await
}

#[tauri::command]
async fn git_push(state: State<'_, AppState>, site_id: String) -> Result<String, String> {
    let dir = state.site(&site_id)?.local_path;
    blocking(move || git::run(&dir, &["push"])).await
}

#[tauri::command]
async fn git_pull(state: State<'_, AppState>, site_id: String) -> Result<String, String> {
    let dir = state.site(&site_id)?.local_path;
    blocking(move || git::run(&dir, &["pull", "--ff-only"])).await
}

#[tauri::command]
async fn git_init(state: State<'_, AppState>, site_id: String) -> Result<String, String> {
    let dir = state.site(&site_id)?.local_path;
    blocking(move || git::run(&dir, &["init"])).await
}

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
const SPLASH_MIN: std::time::Duration = std::time::Duration::from_millis(1600);

#[tauri::command]
async fn check_update(app: AppHandle) -> Result<update::UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    blocking(move || update::check(&current)).await
}

/// Shows the main window once the UI has rendered and closes the start screen, which stays up at least `SPLASH_MIN`.
#[tauri::command]
fn app_ready(app: AppHandle) {
    let wait = STARTED.get().map(|t| SPLASH_MIN.saturating_sub(t.elapsed())).unwrap_or_default();
    std::thread::spawn(move || {
        std::thread::sleep(wait);
        let a = app.clone();
        let _ = app.run_on_main_thread(move || show_main(&a));
    });
}

fn show_main(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.show();
        let _ = main.set_focus();
    }
    if let Some(splash) = app.get_webview_window("splash") {
        let _ = splash.close();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri_plugin_window_state::StateFlags;
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .with_denylist(&["splash"])
                .build(),
        )
        .setup(|app| {
            STARTED.get_or_init(std::time::Instant::now);
            if let Some(splash) = app.get_webview_window("splash") {
                let _ = splash.center();
                let _ = splash.show();
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(10));
                app_ready(handle);
            });
            let config_dir = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config_dir)?;
            let sites = SiteStore::new(&config_dir).load();
            app.manage(AppState {
                config_dir,
                sites: Arc::new(Mutex::new(sites)),
                pool: Arc::new(Pool::default()),
                jobs: Arc::new(Jobs::default()),
                db_lock: Arc::new(Mutex::new(())),
                master: Arc::new(Mutex::new(master::Session::default())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_ready,
            check_update,
            master_status,
            master_setup,
            master_unlock,
            master_lock,
            master_change,
            master_forgot,
            master_reset,
            sites_list,
            site_save,
            site_delete,
            site_test,
            login_save,
            login_delete,
            login_password,
            ftp_password,
            import_dreamweaver,
            import_folders,
            local_list,
            remote_list,
            upload,
            download,
            cancel_job,
            cancel_all_jobs,
            drop_paths,
            site_set_group,
            export_sites,
            import_sites,
            lang_list,
            lang_install,
            lang_template,
            remote_delete,
            local_delete,
            remote_mkdir,
            remote_rename,
            local_mkdir,
            local_rename,
            copy_items,
            reveal_path,
            open_folder,
            disconnect,
            sync_compare,
            sync_apply,
            sync_mark_same,
            file_read,
            file_write,
            local_path,
            git_status,
            git_commit,
            git_push,
            git_pull,
            git_init,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
