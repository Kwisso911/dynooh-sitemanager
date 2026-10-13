use crate::sites::{normalize_path, Site};
use serde::Serialize;
use std::fs;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
}

fn reg_str(key: &RegKey, name: &str) -> String {
    key.get_value::<String, _>(name)
        .or_else(|_| key.get_value::<u32, _>(name).map(|v| v.to_string()))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Reads site definitions of all Dreamweaver versions from HKCU\Software\Adobe\Common\<ver>\Sites.
fn dreamweaver_sites() -> Vec<Site> {
    let mut out: Vec<Site> = Vec::new();
    let Ok(common) = RegKey::predef(HKEY_CURRENT_USER).open_subkey("Software\\Adobe\\Common") else {
        return out;
    };
    for ver in common.enum_keys().flatten() {
        let Ok(sites) = common.open_subkey(format!("{ver}\\Sites")) else { continue };
        for name in sites.enum_keys().flatten() {
            let Ok(k) = sites.open_subkey(&name) else { continue };
            let local = reg_str(&k, "Local Directory");
            if local.is_empty() || out.iter().any(|s| normalize_path(&s.local_path) == normalize_path(&local)) {
                continue;
            }
            let host_raw = reg_str(&k, "SiteServer 0\\Host");
            let (host, port) = match host_raw.rsplit_once(':') {
                Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().unwrap()),
                _ => (host_raw.clone(), 21),
            };
            let mut exclude: Vec<String> =
                reg_str(&k, "Cloaked Patterns").split_whitespace().map(String::from).collect();
            let cloaked_dirs: usize = reg_str(&k, "Cloaked Dirs").parse().unwrap_or(0);
            for i in 0..cloaked_dirs {
                let d = reg_str(&k, &format!("Cloaked Dir {i}"));
                if !d.is_empty() {
                    exclude.push(d.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(&d).to_string());
                }
            }
            out.push(Site {
                id: new_id(),
                name: reg_str(&k, "Site Name"),
                local_path: local.trim_end_matches('\\').to_string(),
                host,
                port,
                user: reg_str(&k, "SiteServer 0\\User"),
                remote_path: reg_str(&k, "SiteServer 0\\Remote Directory"),
                passive: reg_str(&k, "SiteServer 0\\PASV FTP") != "0",
                exclude,
                web_url: reg_str(&k, "Root URL"),
                upload_on_save: reg_str(&k, "SiteServer 0\\Site Server Auto Upload") == "1",
                has_password: false,
                source: "dreamweaver".into(),
                ..Default::default()
            });
        }
    }
    out
}

pub fn import_dreamweaver(sites: &mut Vec<Site>) -> ImportResult {
    let mut res = ImportResult::default();
    for dw in dreamweaver_sites() {
        let key = normalize_path(&dw.local_path);
        match sites.iter_mut().find(|s| normalize_path(&s.local_path) == key) {
            Some(existing) => {
                let mut changed = false;
                let mut fill = |target: &mut String, value: &str| {
                    if target.is_empty() && !value.is_empty() {
                        *target = value.to_string();
                        changed = true;
                    }
                };
                fill(&mut existing.host, &dw.host);
                fill(&mut existing.user, &dw.user);
                fill(&mut existing.remote_path, &dw.remote_path);
                fill(&mut existing.web_url, &dw.web_url);
                if existing.exclude.is_empty() && !dw.exclude.is_empty() {
                    existing.exclude = dw.exclude.clone();
                    changed = true;
                }
                if changed {
                    existing.port = dw.port;
                    res.updated += 1;
                } else {
                    res.skipped += 1;
                }
            }
            None => {
                sites.push(dw);
                res.added += 1;
            }
        }
    }
    res
}

pub fn import_folders(sites: &mut Vec<Site>, root: &str) -> Result<ImportResult, String> {
    let mut res = ImportResult::default();
    let mut dirs: Vec<_> = fs::read_dir(root)
        .map_err(|e| format!("{root}: {e}"))?
        .flatten()
        .filter(|d| d.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    dirs.sort_by_key(|d| d.file_name());
    for d in dirs {
        let name = d.file_name().to_string_lossy().to_string();
        if name.starts_with('_') || name.starts_with('.') {
            continue;
        }
        let path = d.path().to_string_lossy().to_string();
        let key = normalize_path(&path);
        let covered = sites.iter().any(|s| {
            let sp = normalize_path(&s.local_path);
            sp == key || sp.starts_with(&format!("{key}\\"))
        });
        if covered {
            res.skipped += 1;
            continue;
        }
        sites.push(Site {
            id: new_id(),
            name: name.clone(),
            local_path: path,
            host: name,
            port: 21,
            passive: true,
            source: "folder".into(),
            ..Default::default()
        });
        res.added += 1;
    }
    Ok(res)
}
