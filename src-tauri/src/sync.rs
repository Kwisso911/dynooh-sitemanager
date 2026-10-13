use crate::ftp::{Conn, Entry};
use crate::sites::Site;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// State of a file at the time of the last transfer, used to detect changes on either side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rec {
    pub lm: i64,
    pub ls: u64,
    pub rm: Option<i64>,
    pub rs: u64,
}

pub struct SyncDb {
    file: PathBuf,
    pub map: HashMap<String, Rec>,
}

impl SyncDb {
    pub fn load(config_dir: &Path, site_id: &str) -> Self {
        let file = config_dir.join("sync").join(format!("{site_id}.json"));
        let map = fs::read_to_string(&file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self { file, map }
    }

    pub fn save(&self) -> Result<(), String> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(&self.file, serde_json::to_string(&self.map).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    pub fn delete(config_dir: &Path, site_id: &str) {
        let _ = fs::remove_file(config_dir.join("sync").join(format!("{site_id}.json")));
    }
}

pub fn local_meta(path: &Path) -> Option<(i64, u64)> {
    let m = fs::metadata(path).ok()?;
    let mtime = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((mtime, m.len()))
}

pub fn local_list(site: &Site, rel: &str) -> Result<Vec<Entry>, String> {
    let dir = site.local_file(rel);
    let mut out = Vec::new();
    for item in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let Ok(item) = item else { continue };
        let name = item.file_name().to_string_lossy().to_string();
        let Ok(meta) = item.metadata() else { continue };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        out.push(Entry { name, is_dir: meta.is_dir(), size: if meta.is_dir() { 0 } else { meta.len() }, mtime, link: false });
    }
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// Relative paths (with `/`) of all non-excluded local files below `rel`.
pub fn local_walk(site: &Site, rel: &str) -> HashMap<String, (i64, u64)> {
    let mut out = HashMap::new();
    let start = site.local_file(rel);
    if start.is_file() {
        if let Some(m) = local_meta(&start) {
            out.insert(rel.trim_matches('/').to_string(), m);
        }
        return out;
    }
    let mut stack = vec![rel.trim_matches('/').to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(items) = fs::read_dir(site.local_file(&dir)) else { continue };
        for item in items.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            if site.is_excluded(&name) || name.ends_with(".vfpart") {
                continue;
            }
            let child = if dir.is_empty() { name } else { format!("{dir}/{name}") };
            let Ok(meta) = item.metadata() else { continue };
            if meta.is_dir() {
                stack.push(child);
            } else if let Some(m) = local_meta(&item.path()) {
                out.insert(child, m);
            }
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncItem {
    pub rel: String,
    pub status: &'static str,
    pub local_size: Option<u64>,
    pub local_mtime: Option<i64>,
    pub remote_size: Option<u64>,
    pub remote_mtime: Option<i64>,
}

pub fn compare(
    db: &mut SyncDb,
    local: &HashMap<String, (i64, u64)>,
    remote: &HashMap<String, Entry>,
) -> Vec<SyncItem> {
    let keys: BTreeSet<&String> = local.keys().chain(remote.keys()).collect();
    let mut items = Vec::new();
    for rel in keys {
        let l = local.get(rel);
        let r = remote.get(rel);
        let rec = db.map.get_mut(rel.as_str());
        let status = match (l, r, rec) {
            (Some(&(lm, ls)), Some(re), Some(rec)) => {
                if rec.rm.is_none() && re.size == rec.rs {
                    rec.rm = Some(re.mtime);
                }
                let local_changed = lm != rec.lm || ls != rec.ls;
                let remote_changed = re.size != rec.rs || rec.rm.is_some_and(|m| m != re.mtime);
                match (local_changed, remote_changed) {
                    (false, false) => "same",
                    (true, false) => "upload",
                    (false, true) => "download",
                    (true, true) => "conflict",
                }
            }
            (Some(&(_, ls)), Some(re), None) => {
                if ls == re.size { "probablySame" } else { "different" }
            }
            (Some(_), None, Some(_)) => "remoteDeleted",
            (Some(_), None, None) => "localOnly",
            (None, Some(_), Some(_)) => "localDeleted",
            (None, Some(_), None) => "remoteOnly",
            (None, None, _) => continue,
        };
        items.push(SyncItem {
            rel: rel.clone(),
            status,
            local_size: l.map(|x| x.1),
            local_mtime: l.map(|x| x.0),
            remote_size: r.map(|x| x.size),
            remote_mtime: r.map(|x| x.mtime),
        });
    }
    items
}

pub fn record_upload(db: &mut SyncDb, site: &Site, rel: &str) {
    if let Some((lm, ls)) = local_meta(&site.local_file(rel)) {
        db.map.insert(rel.to_string(), Rec { lm, ls, rm: None, rs: ls });
    }
}

pub fn record_download(db: &mut SyncDb, site: &Site, rel: &str, remote: Option<&Entry>) {
    if let Some((lm, ls)) = local_meta(&site.local_file(rel)) {
        db.map.insert(rel.to_string(), Rec { lm, ls, rm: remote.map(|e| e.mtime), rs: ls });
    }
}

pub fn record_same(db: &mut SyncDb, site: &Site, rel: &str, remote: &Entry) {
    if let Some((lm, ls)) = local_meta(&site.local_file(rel)) {
        db.map.insert(rel.to_string(), Rec { lm, ls, rm: Some(remote.mtime), rs: remote.size });
    }
}

pub fn remote_walk_map(
    conn: &mut Conn,
    site: &Site,
    rel: &str,
    on_dir: &mut dyn FnMut(&str) -> bool,
) -> Result<HashMap<String, Entry>, suppaftp::FtpError> {
    Ok(conn.walk(site, rel, on_dir)?.into_iter().collect())
}
