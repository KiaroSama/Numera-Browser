use crate::validation::repr;
use serde_json::Value;
use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct InvalidId(String);
impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for InvalidId {}

/// IDs are portable opaque tokens, never caller-controlled path fragments.
pub fn safe_id(value: &str) -> Result<&str, InvalidId> {
    // The original regex's $ also accepts one final newline. That is a source
    // contract to preserve until an explicit validation-change decision, not an
    // invitation for a new caller to construct IDs instead of minting them.
    let matched = value.strip_suffix('\n').unwrap_or(value);
    let shown = repr(&Value::String(value.into()));
    if matched.is_empty()
        || matched.len() > 64
        || !matched
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err(InvalidId(format!(
            "id {shown} is not a valid profile id: expected 1-64 characters from A-Z, a-z, 0-9, '_' or '-'"
        )));
    }
    let upper = value.to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&upper.as_str())
        || upper
            .strip_prefix("COM")
            .or_else(|| upper.strip_prefix("LPT"))
            .is_some_and(|tail| tail.len() == 1 && tail.as_bytes()[0].is_ascii_digit());
    if reserved {
        return Err(InvalidId(format!(
            "id {shown} is not a valid profile id: {upper} is a reserved device name on Windows"
        )));
    }
    Ok(value)
}

pub fn id_path(base: &Path, value: &str, suffix: &str) -> Result<PathBuf, InvalidId> {
    Ok(base.join(format!("{}{suffix}", safe_id(value)?)))
}
