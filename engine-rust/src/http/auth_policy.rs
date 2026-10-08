use serde_json::Value;
use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthPolicy {
    Open,
    Protected,
    Absent,
    Ambiguous,
}

pub fn read(root: &Path) -> AuthPolicy {
    let bytes = match fs::read(root.join("auth-policy.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return AuthPolicy::Absent,
        Err(_) => return AuthPolicy::Ambiguous,
    };
    let document: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return AuthPolicy::Ambiguous,
    };
    match document.get("protected") {
        Some(Value::Bool(false)) => AuthPolicy::Open,
        Some(Value::Bool(true)) => AuthPolicy::Protected,
        _ => AuthPolicy::Ambiguous,
    }
}
