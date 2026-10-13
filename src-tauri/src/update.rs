use serde::{Deserialize, Serialize};
use std::time::Duration;

const MANIFEST_URL: &str = "https://apps.dynooh.com/downloads/sitemanager.json";

#[derive(Deserialize)]
struct Manifest {
    version: String,
    url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub url: String,
    pub newer: bool,
}

fn parts(v: &str) -> [u64; 3] {
    let mut out = [0; 3];
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("");
    for (slot, p) in out.iter_mut().zip(core.split('.')) {
        *slot = p.parse().unwrap_or(0);
    }
    out
}

fn is_newer(latest: &str, current: &str) -> bool {
    parts(latest) > parts(current)
}

pub fn check(current: &str) -> Result<UpdateInfo, String> {
    let m: Manifest = ureq::get(MANIFEST_URL)
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    if !m.url.starts_with("https://") {
        return Err("Ungültige Download-Adresse".into());
    }
    Ok(UpdateInfo { current: current.into(), newer: is_newer(&m.version, current), latest: m.version, url: m.url })
}

#[cfg(test)]
mod tests {
    use super::is_newer;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(is_newer("v0.1.1", "0.1.0"));
    }
}
