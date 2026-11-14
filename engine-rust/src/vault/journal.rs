use base64::{
    Engine, alphabet,
    engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig},
};
use serde_json::Value;
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalState {
    Absent,
    Valid,
    Unreadable,
    Unsupported,
}

pub fn usable_descriptor(value: &Value) -> bool {
    if !value.is_object() {
        return false;
    }
    let Some(salt) = value["salt"]
        .as_str()
        .filter(|v| !crate::pool::trim(v).is_empty())
    else {
        return false;
    };
    if value["verifier"]
        .as_str()
        .is_none_or(|v| crate::pool::trim(v).is_empty())
    {
        return false;
    }
    GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    )
    .decode(salt)
    .is_ok()
}

pub fn descriptors(record: &Value) -> (Value, Value) {
    let new = record
        .get("new")
        .filter(|v| crate::conversion::truth(v))
        .cloned()
        .unwrap_or_else(
            || serde_json::json!({"salt":record["salt"],"verifier":record["verifier"]}),
        );
    (new, record["old"].clone())
}

/// Read-only classification must never erase the descriptor needed for recovery.
pub fn classify(root: &Path) -> (JournalState, Option<Value>) {
    let bytes = match fs::read(root.join("vault-migration.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (JournalState::Absent, None);
        }
        Err(_) => return (JournalState::Unreadable, None),
    };
    let record: Value = match serde_json::from_slice(&bytes) {
        Ok(v) => v,
        Err(_) => return (JournalState::Unreadable, None),
    };
    if record.as_object().is_none_or(|v| v.is_empty()) {
        return (JournalState::Unreadable, None);
    }
    let version = record.get("version").cloned().unwrap_or(Value::from(1));
    // Python accepts 1.0 but never bool, despite bool being an int subclass.
    let operation = record.get("op").cloned().unwrap_or(Value::from("enable"));
    if version.as_f64() != Some(1.0) || !matches!(operation.as_str(), Some("enable" | "rotate")) {
        return (JournalState::Unsupported, Some(record));
    }
    let (new, old) = descriptors(&record);
    if !usable_descriptor(&new) || operation == "rotate" && !usable_descriptor(&old) {
        return (JournalState::Unreadable, Some(record));
    }
    (JournalState::Valid, Some(record))
}
