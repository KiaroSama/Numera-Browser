use super::{
    auth_policy::{self, AuthPolicy},
    token_errors::TokenError,
};
use crate::{
    conversion::{truth, value_text},
    storage::{atomic::write_json, locks::DataLock},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io, path::PathBuf, time::Duration};
use subtle::ConstantTimeEq;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenState {
    Empty,
    Tokens,
    Unreadable,
}
impl TokenState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Tokens => "tokens",
            Self::Unreadable => "unreadable",
        }
    }
}

pub struct TokenStore {
    root: PathBuf,
}
impl TokenStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn path(&self) -> PathBuf {
        self.root.join("tokens.json")
    }
    fn table(&self) -> (TokenState, Vec<Value>) {
        let bytes = match fs::read(self.path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return (TokenState::Empty, Vec::new());
            }
            Err(_) => return (TokenState::Unreadable, Vec::new()),
        };
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Array(rows)) => (
                if rows.is_empty() {
                    TokenState::Empty
                } else {
                    TokenState::Tokens
                },
                rows,
            ),
            _ => (TokenState::Unreadable, Vec::new()),
        }
    }
    pub fn state(&self) -> TokenState {
        self.table().0
    }
    pub fn list(&self) -> Vec<Value> {
        self.table().1
    }
    pub fn protected(&self) -> bool {
        matches!(self.state(), TokenState::Tokens | TokenState::Unreadable)
            || matches!(
                auth_policy::read(&self.root),
                AuthPolicy::Protected | AuthPolicy::Ambiguous
            )
    }
    pub fn set_protected(&self, on: bool, now: f64) -> io::Result<()> {
        if !now.is_finite() {
            return Err(io::Error::other("Auth policy timestamp must be finite"));
        }
        let path = self.root.join("auth-policy.json");
        write_json(&path, &json!({"protected":on,"since":now}))
    }
    pub fn adopt(&self, now: f64) -> io::Result<bool> {
        let lock = DataLock::for_file(&self.path())?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        if auth_policy::read(&self.root) != AuthPolicy::Absent
            || !matches!(self.state(), TokenState::Tokens | TokenState::Unreadable)
        {
            return Ok(false);
        }
        self.set_protected(true, now)?;
        Ok(true)
    }
    pub fn create(&self, name: &str, now: f64) -> Result<Value, TokenError> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        if !now.is_finite() {
            return Err(io::Error::other("Token creation timestamp must be finite").into());
        }
        let wanted = if name.trim().is_empty() {
            "unnamed"
        } else {
            name.trim()
        };
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|_| io::Error::other("Token randomness unavailable"))?;
        let token = format!("nmr_{}", URL_SAFE_NO_PAD.encode(bytes));
        let mut id_bytes = [0u8; 6];
        getrandom::fill(&mut id_bytes)
            .map_err(|_| io::Error::other("Token ID randomness unavailable"))?;
        let id: String = id_bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let entry =
            json!({"id":id,"name":wanted,"hash":Self::hash(&token),"created":now,"lastUsed":null});
        let lock = DataLock::for_file(&self.path())?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let mut rows = self.list();
        if rows.iter().any(|row| !row.is_object()) {
            return Err(io::Error::other(
                "Stored token entry is not an object; nothing was written",
            )
            .into());
        }
        if rows
            .iter()
            .any(|row| row.get("name").map(value_text).unwrap_or_default().trim() == wanted)
        {
            return Err(TokenError::DuplicateName(wanted.into()));
        }
        if rows.iter().any(|row| row["id"] == id) {
            return Err(
                io::Error::other("Generated token ID collided; nothing was written").into(),
            );
        }
        rows.push(entry.clone());
        write_json(&self.path(), &Value::Array(rows))?;
        self.set_protected(true, now)?;
        let mut response = entry;
        response["token"] = token.into();
        Ok(response)
    }
    pub fn hash(token: &str) -> String {
        Sha256::digest(token.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    pub fn revoke(&self, id: &str, refuse_lockout: bool) -> Result<bool, TokenError> {
        let lock = DataLock::for_file(&self.path())?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let rows = self.list();
        if rows.iter().any(|row| !row.is_object()) {
            return Err(io::Error::other(
                "Stored token entry is not an object; nothing was written",
            )
            .into());
        }
        let kept: Vec<_> = rows.iter().filter(|row| row["id"] != id).cloned().collect();
        if rows.len() == kept.len() {
            return Ok(false);
        }
        if refuse_lockout && kept.is_empty() {
            return Err(TokenError::Lockout);
        }
        write_json(&self.path(), &Value::Array(kept))?;
        Ok(true)
    }
    pub fn verify(&self, token: &str, now: f64) -> io::Result<Option<Value>> {
        if token.is_empty() {
            return Ok(None);
        }
        if !now.is_finite() {
            return Err(io::Error::other(
                "Token verification timestamp must be finite",
            ));
        }
        let digest = Self::hash(token);
        let lock = DataLock::for_file(&self.path())?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let mut rows = self.list();
        for index in 0..rows.len() {
            let row = rows[index]
                .as_object()
                .ok_or_else(|| io::Error::other("Stored token entry is not an object"))?;
            let stored = row.get("hash").map(value_text).unwrap_or_default();
            if stored.as_bytes().ct_eq(digest.as_bytes()).unwrap_u8() != 1 {
                continue;
            }
            let last = row.get("lastUsed").filter(|value| truth(value));
            let last = match last {
                None => 0.0,
                Some(Value::Bool(value)) => {
                    if *value {
                        1.0
                    } else {
                        0.0
                    }
                }
                Some(Value::Number(value)) => value.as_f64().ok_or_else(|| {
                    io::Error::other("Stored token last-used timestamp is not representable")
                })?,
                Some(Value::String(value)) => value
                    .parse::<f64>()
                    .map_err(|_| io::Error::other("Stored token last-used timestamp is invalid"))?,
                Some(_) => {
                    return Err(io::Error::other(
                        "Stored token last-used timestamp is invalid",
                    ));
                }
            };
            if now - last >= 60.0 {
                rows[index]["lastUsed"] = now.into();
                write_json(&self.path(), &Value::Array(rows.clone()))?;
            }
            return Ok(Some(rows[index].clone()));
        }
        Ok(None)
    }
}
