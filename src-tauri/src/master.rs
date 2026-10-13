//! Master password that guards revealing and exporting stored passwords.
//! The Argon2 hash and the recovery e-mail live in the Windows Credential Manager.

use crate::sites;
use aes_gcm::aead::OsRng;
use aes_gcm::aead::rand_core::RngCore;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const KEY: &str = "__master__";
/// Unlock stays valid this long after the last password access.
const UNLOCK_FOR: Duration = Duration::from_secs(10 * 60);
const RESET_VALID_SECS: i64 = 30 * 60;
const RESET_MAX_TRIES: u8 = 5;
const MAIL_ENDPOINT: &str = "https://v-f.ch/software/sitemanager/reset-mail.php";
/// Shared secret of the mail endpoint, injected at build time; builds without it cannot send reset codes.
const MAIL_TOKEN: Option<&str> = option_env!("VFSM_MAIL_TOKEN");
pub const LOCKED: &str = "Master-Passwort erforderlich";

#[derive(Serialize, Deserialize, Default)]
struct Stored {
    hash: String,
    email: String,
    #[serde(default)]
    reset: Option<Reset>,
}

#[derive(Serialize, Deserialize)]
struct Reset {
    hash: String,
    expires: i64,
    tries: u8,
}

#[derive(Default)]
pub struct Session {
    until: Option<Instant>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    configured: bool,
    unlocked: bool,
    email: String,
}

fn load() -> Option<Stored> {
    serde_json::from_str(&sites::get_password(KEY)?).ok()
}

fn store(s: &Stored) -> Result<(), String> {
    sites::set_password(KEY, &serde_json::to_string(s).map_err(|e| e.to_string())?)
}

fn hash(secret: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(secret.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| e.to_string())
}

fn verify(secret: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(secret.as_bytes(), &h).is_ok())
}

fn check_new(password: &str, email: &str) -> Result<(), String> {
    if password.chars().count() < 8 {
        return Err("Das Master-Passwort muss mindestens 8 Zeichen lang sein".into());
    }
    let email = email.trim();
    let valid = email.split_once('@').is_some_and(|(u, d)| !u.is_empty() && d.contains('.') && !d.starts_with('.') && !d.ends_with('.'));
    if !valid || email.contains(char::is_whitespace) {
        return Err("Ungültige E-Mail-Adresse".into());
    }
    Ok(())
}

fn mask(email: &str) -> String {
    match email.split_once('@') {
        Some((u, d)) => {
            let first: String = u.chars().take(2).collect();
            format!("{first}{}@{d}", "•".repeat(u.chars().count().saturating_sub(2).clamp(1, 6)))
        }
        None => String::new(),
    }
}

impl Session {
    pub fn is_unlocked(&mut self) -> bool {
        match self.until {
            Some(t) if t > Instant::now() => true,
            _ => {
                self.until = None;
                false
            }
        }
    }

    fn unlock(&mut self) {
        self.until = Some(Instant::now() + UNLOCK_FOR);
    }

    /// Fails with [`LOCKED`] unless unlocked; extends the unlock on success.
    pub fn require(&mut self) -> Result<(), String> {
        if !self.is_unlocked() {
            return Err(LOCKED.into());
        }
        self.unlock();
        Ok(())
    }

    pub fn lock(&mut self) {
        self.until = None;
    }

    pub fn status(&mut self) -> Status {
        let stored = load();
        let unlocked = self.is_unlocked();
        Status {
            configured: stored.is_some(),
            unlocked,
            email: stored.map(|s| if unlocked { s.email } else { mask(&s.email) }).unwrap_or_default(),
        }
    }

    pub fn setup(&mut self, password: &str, email: &str) -> Result<(), String> {
        if load().is_some() {
            return Err("Das Master-Passwort ist bereits eingerichtet".into());
        }
        check_new(password, email)?;
        store(&Stored { hash: hash(password)?, email: email.trim().to_string(), reset: None })?;
        self.unlock();
        Ok(())
    }

    pub fn unlock_with(&mut self, password: &str) -> Result<(), String> {
        let stored = load().ok_or("Kein Master-Passwort eingerichtet")?;
        if !verify(password, &stored.hash) {
            return Err("Falsches Master-Passwort".into());
        }
        self.unlock();
        Ok(())
    }

    pub fn change(&mut self, current: &str, password: Option<&str>, email: Option<&str>) -> Result<(), String> {
        let mut stored = load().ok_or("Kein Master-Passwort eingerichtet")?;
        if !verify(current, &stored.hash) {
            return Err("Falsches Master-Passwort".into());
        }
        let password = password.filter(|p| !p.is_empty());
        let email = email.map(str::trim).filter(|e| !e.is_empty()).unwrap_or(&stored.email).to_string();
        check_new(password.unwrap_or(current), &email)?;
        if let Some(p) = password {
            stored.hash = hash(p)?;
        }
        stored.email = email;
        stored.reset = None;
        store(&stored)?;
        self.unlock();
        Ok(())
    }

    /// Generates a one-time code, sends it to the recovery address and returns the masked address.
    pub fn forgot(lang: &str) -> Result<String, String> {
        let token = MAIL_TOKEN.filter(|t| !t.is_empty()).ok_or("Passwort-Reset per E-Mail ist in diesem Build nicht verfügbar")?;
        let mut stored = load().ok_or("Kein Master-Passwort eingerichtet")?;
        let code = format!("{:06}", OsRng.next_u32() % 1_000_000);
        let resp = ureq::post(MAIL_ENDPOINT)
            .timeout(Duration::from_secs(20))
            .set("X-VF-Token", token)
            .send_json(serde_json::json!({ "email": stored.email, "code": code, "lang": lang }));
        match resp {
            Ok(_) => {}
            Err(ureq::Error::Status(429, _)) => return Err("Zu viele Anfragen, bitte später erneut versuchen".into()),
            Err(e) => return Err(format!("E-Mail konnte nicht gesendet werden: {e}")),
        }
        stored.reset = Some(Reset { hash: hash(&code)?, expires: chrono::Utc::now().timestamp() + RESET_VALID_SECS, tries: 0 });
        store(&stored)?;
        Ok(mask(&stored.email))
    }

    pub fn reset(&mut self, code: &str, password: &str) -> Result<(), String> {
        let mut stored = load().ok_or("Kein Master-Passwort eingerichtet")?;
        let Some(reset) = stored.reset.as_mut() else { return Err("Kein Rücksetz-Code angefordert".into()) };
        if chrono::Utc::now().timestamp() > reset.expires || reset.tries >= RESET_MAX_TRIES {
            stored.reset = None;
            store(&stored)?;
            return Err("Der Code ist abgelaufen, bitte neu anfordern".into());
        }
        if !verify(code.trim(), &reset.hash) {
            reset.tries += 1;
            store(&stored)?;
            return Err("Falscher Code".into());
        }
        check_new(password, &stored.email)?;
        stored.hash = hash(password)?;
        stored.reset = None;
        store(&stored)?;
        self.unlock();
        Ok(())
    }
}
