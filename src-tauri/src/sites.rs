use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const KEYRING_SERVICE: &str = "vf-sitemanager";

pub const DEFAULT_EXCLUDES: &[&str] = &[".git", "node_modules", "_notes", ".DS_Store", "Thumbs.db", "desktop.ini"];

/// Additional login of a site (CMS backend, hosting panel, database …); password lives in the keyring.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Login {
    pub id: String,
    pub label: String,
    pub url: String,
    pub user: String,
    pub note: String,
    pub has_password: bool,
}

pub fn login_key(site_id: &str, login_id: &str) -> String {
    format!("{site_id}:{login_id}")
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub remote_path: String,
    pub passive: bool,
    pub exclude: Vec<String>,
    pub web_url: String,
    pub upload_on_save: bool,
    pub has_password: bool,
    pub source: String,
    pub logins: Vec<Login>,
    pub connections: u8,
    pub group: String,
}

impl Site {
    pub fn max_connections(&self) -> usize {
        if self.connections == 0 { 4 } else { self.connections.clamp(1, 10) as usize }
    }

    pub fn port_or_default(&self) -> u16 {
        if self.port == 0 { 21 } else { self.port }
    }

    pub fn local_file(&self, rel: &str) -> PathBuf {
        let mut p = PathBuf::from(&self.local_path);
        for part in rel.split(['/', '\\']).filter(|s| !s.is_empty() && *s != "." && *s != "..") {
            p.push(part);
        }
        p
    }

    pub fn is_excluded(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        DEFAULT_EXCLUDES
            .iter()
            .map(|s| s.to_string())
            .chain(self.exclude.iter().cloned())
            .map(|p| p.trim().to_lowercase())
            .filter(|p| !p.is_empty())
            .any(|p| lower == p || (p.starts_with('.') && !p[1..].contains('.') && lower.ends_with(&p)))
    }
}

pub fn normalize_path(p: &str) -> String {
    p.trim().trim_end_matches(['\\', '/']).replace('/', "\\").to_lowercase()
}

pub struct SiteStore {
    file: PathBuf,
}

impl SiteStore {
    pub fn new(config_dir: &Path) -> Self {
        Self { file: config_dir.join("sites.json") }
    }

    pub fn load(&self) -> Vec<Site> {
        fs::read_to_string(&self.file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, sites: &[Site]) -> Result<(), String> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(sites).map_err(|e| e.to_string())?;
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &self.file).map_err(|e| e.to_string())
    }
}

fn entry(site_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, site_id).map_err(|e| e.to_string())
}

pub fn set_password(site_id: &str, password: &str) -> Result<(), String> {
    entry(site_id)?.set_password(password).map_err(|e| format!("Passwort speichern fehlgeschlagen: {e}"))
}

pub fn get_password(site_id: &str) -> Option<String> {
    entry(site_id).ok()?.get_password().ok()
}

pub fn delete_password(site_id: &str) {
    if let Ok(e) = entry(site_id) {
        let _ = e.delete_credential();
    }
}
