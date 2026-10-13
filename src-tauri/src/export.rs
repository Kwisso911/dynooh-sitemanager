use crate::import::ImportResult;
use crate::sites::{self, Site, normalize_path};
use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const FORMAT: &str = "vf-sitemanager-export";

/// Encrypted container written to disk.
#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    kdf: Kdf,
    nonce: String,
    data: String,
}

#[derive(Serialize, Deserialize)]
struct Kdf {
    alg: String,
    salt: String,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
}

/// Decrypted payload: site settings plus all passwords.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    exported_at: String,
    sites: Vec<ExportSite>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportSite {
    site: Site,
    password: Option<String>,
    login_passwords: HashMap<String, String>,
}

fn derive_key(password: &str, kdf: &Kdf) -> Result<[u8; 32], String> {
    let salt = B64.decode(&kdf.salt).map_err(|_| "Exportdatei beschädigt")?;
    let params = Params::new(kdf.m_cost, kdf.t_cost, kdf.p_cost, Some(32)).map_err(|e| e.to_string())?;
    let mut key = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), &salt, &mut key)
        .map_err(|e| e.to_string())?;
    Ok(key)
}

pub fn export_sites(sites: &[Site], password: &str) -> Result<String, String> {
    if password.chars().count() < 8 {
        return Err("Das Export-Passwort muss mindestens 8 Zeichen lang sein".into());
    }
    let payload = Payload {
        exported_at: chrono::Local::now().to_rfc3339(),
        sites: sites
            .iter()
            .map(|s| ExportSite {
                site: s.clone(),
                password: sites::get_password(&s.id),
                login_passwords: s
                    .logins
                    .iter()
                    .filter_map(|l| sites::get_password(&sites::login_key(&s.id, &l.id)).map(|p| (l.id.clone(), p)))
                    .collect(),
            })
            .collect(),
    };
    let plain = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;

    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    let kdf = Kdf { alg: "argon2id".into(), salt: B64.encode(salt), m_cost: 64 * 1024, t_cost: 3, p_cost: 1 };
    let key = derive_key(password, &kdf)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let data = cipher.encrypt(&nonce, plain.as_ref()).map_err(|_| "Verschlüsselung fehlgeschlagen")?;

    let env = Envelope { format: FORMAT.into(), version: 1, kdf, nonce: B64.encode(nonce), data: B64.encode(data) };
    serde_json::to_string_pretty(&env).map_err(|e| e.to_string())
}

/// Adds or updates sites from an export; matching by id, then by local folder.
pub fn import_sites(list: &mut Vec<Site>, content: &str, password: &str) -> Result<ImportResult, String> {
    let env: Envelope = serde_json::from_str(content).map_err(|_| "Keine gültige SiteManager-Exportdatei")?;
    if env.format != FORMAT || env.kdf.alg != "argon2id" {
        return Err("Keine gültige SiteManager-Exportdatei".into());
    }
    if env.version > 1 {
        return Err("Die Exportdatei stammt aus einer neueren Version".into());
    }
    let key = derive_key(password, &env.kdf)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| e.to_string())?;
    let nonce_bytes = B64.decode(&env.nonce).map_err(|_| "Exportdatei beschädigt")?;
    let data = B64.decode(&env.data).map_err(|_| "Exportdatei beschädigt")?;
    let plain = cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), data.as_ref())
        .map_err(|_| "Falsches Passwort oder beschädigte Datei")?;
    let payload: Payload = serde_json::from_slice(&plain).map_err(|_| "Exportdatei beschädigt")?;

    let mut res = ImportResult::default();
    for item in payload.sites {
        let mut site = item.site;
        let pos = list.iter().position(|s| s.id == site.id).or_else(|| {
            let key = normalize_path(&site.local_path);
            list.iter().position(|s| !key.is_empty() && normalize_path(&s.local_path) == key)
        });
        if let Some(i) = pos {
            site.id = list[i].id.clone();
            res.updated += 1;
        } else {
            res.added += 1;
        }
        if let Some(pw) = item.password.filter(|p| !p.is_empty()) {
            sites::set_password(&site.id, &pw)?;
            site.has_password = true;
        }
        for login in site.logins.iter_mut() {
            if let Some(pw) = item.login_passwords.get(&login.id) {
                sites::set_password(&sites::login_key(&site.id, &login.id), pw)?;
                login.has_password = true;
            }
        }
        match pos {
            Some(i) => list[i] = site,
            None => list.push(site),
        }
    }
    list.sort_by_key(|s| s.name.to_lowercase());
    Ok(res)
}
