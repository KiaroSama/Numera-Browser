//! Original Fernet envelope compatibility; recovery/journal acceptance is separate.
pub mod completion;
mod durability;
pub mod journal;
pub mod migration;
pub mod policy;
pub mod recovery;
mod rotation_recovery;
pub mod transaction;
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE},
};
use fernet::Fernet;
use pbkdf2::{pbkdf2_hmac_array, sha2::Sha256};
use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug)]
pub struct VaultError(String);
impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for VaultError {}

struct CachedKey {
    password: String,
    salt: String,
    rounds: u32,
    cipher: Fernet,
}
static KEYS: OnceLock<Mutex<VecDeque<CachedKey>>> = OnceLock::new();

pub fn is_envelope(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("enc:v") else {
        return false;
    };
    let Some((version, _)) = rest.split_once(':') else {
        return false;
    };
    // Python's envelope regex uses Unicode Nd, not ASCII-only digits.
    !version.is_empty() && crate::pool::decimal_ascii(version).is_some()
}

fn cipher(password: &str, salt: &str, rounds: u32) -> Result<Fernet, VaultError> {
    let mut keys = KEYS
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .map_err(|_| VaultError("vault key cache is unavailable".into()))?;
    if let Some(index) = keys
        .iter()
        .position(|k| k.password == password && k.salt == salt && k.rounds == rounds)
    {
        let key = keys
            .remove(index)
            .ok_or_else(|| VaultError("vault key cache entry missing".into()))?;
        let cipher = key.cipher.clone();
        keys.push_back(key);
        return Ok(cipher);
    }
    let bytes = STANDARD
        .decode(salt)
        .map_err(|_| VaultError("invalid vault salt encoding".into()))?;
    let mut key = pbkdf2_hmac_array::<Sha256, 32>(password.as_bytes(), &bytes, rounds);
    let encoded = URL_SAFE.encode(key);
    key.fill(0);
    let cipher =
        Fernet::new(&encoded).ok_or_else(|| VaultError("vault derived key is invalid".into()))?;
    if keys.len() == 16 {
        keys.pop_front();
    }
    keys.push_back(CachedKey {
        password: password.into(),
        salt: salt.into(),
        rounds,
        cipher: cipher.clone(),
    });
    Ok(cipher)
}

pub fn decrypt(password: &str, salt: &str, value: &str) -> Result<String, VaultError> {
    let version = value.split(':').nth(1).unwrap_or("v2");
    let rounds = match version {
        "v1" => 200_000,
        "v2" => 600_000,
        _ => {
            return Err(VaultError(format!(
                "this secret was written by a newer version ({version}) than this engine understands"
            )));
        }
    };
    // The dependency reads wall clock even without TTL. Refuse an invalid
    // system clock before it can panic; no TTL is imposed on existing tokens.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| VaultError("system clock precedes Unix epoch".into()))?;
    let token = value
        .strip_prefix(&format!("enc:{version}:"))
        .unwrap_or(value);
    let bytes = cipher(password, salt, rounds)?
        .decrypt(token)
        .map_err(|_| VaultError("wrong master password".into()))?;
    String::from_utf8(bytes).map_err(|_| VaultError("vault secret is not valid UTF-8".into()))
}

pub fn encrypt(password: &str, salt: &str, plaintext: &str) -> Result<String, VaultError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| VaultError("system clock precedes Unix epoch".into()))?;
    Ok(format!(
        "enc:v2:{}",
        cipher(password, salt, 600_000)?.encrypt(plaintext.as_bytes())
    ))
}

pub fn check_password(password: &str, salt: &str, verifier: &str) -> bool {
    decrypt(password, salt, verifier).is_ok_and(|value| value == "numera-vault-ok")
}
