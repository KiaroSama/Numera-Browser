//! Rebase only the historical owned root in immutable source observations.
//! Relative names, error ordering, payload bytes and summary fields stay exact.
use serde_json::Value;
use std::path::Path;
fn historical(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let marker = "/native-characterization-";
            let at = text.find(marker)?;
            let tail = at + marker.len();
            let end = text[tail..].find('/').map_or(text.len(), |n| tail + n);
            let start = text[..at]
                .rfind(['\'', '`', '"', ' ', '('])
                .map_or(0, |n| n + 1);
            Some(text[start..end].to_owned())
        }
        Value::Array(rows) => rows.iter().find_map(historical),
        Value::Object(rows) => rows.values().find_map(historical),
        _ => None,
    }
}
fn replace(text: &str, old: &str, new: &Path) -> String {
    let replacement = new.to_string_lossy();
    let rebased = text.replace(old, &replacement);
    #[cfg(windows)]
    let rebased = if text.contains(old) {
        rebased.replace('/', "\\")
    } else {
        rebased
    };
    rebased
}
fn rebase(value: &mut Value, old: &str, new: &Path) {
    match value {
        Value::String(text) => *text = replace(text, old, new),
        Value::Array(values) => {
            if let Some(bytes) = values
                .iter()
                .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                .collect::<Option<Vec<_>>>()
                && let Ok(text) = std::str::from_utf8(&bytes)
                && text.contains(old)
            {
                *values = replace(text, old, new)
                    .into_bytes()
                    .into_iter()
                    .map(Value::from)
                    .collect();
                return;
            }
            for value in values {
                rebase(value, old, new);
            }
        }
        Value::Object(values) => {
            for (name, value) in values.iter_mut() {
                rebase(value, old, new);
                // These fields are writer-created text markers, unlike caller
                // supplied payload bytes. Source text-mode writes use CRLF on
                // Windows; retain that platform contract after path rebasing.
                #[cfg(windows)]
                if ["marker", "retried_marker"].contains(&name.as_str())
                    && let Value::Array(bytes) = value
                    && let Some(raw) = bytes
                        .iter()
                        .map(|b| b.as_u64().and_then(|b| u8::try_from(b).ok()))
                        .collect::<Option<Vec<_>>>()
                    && let Ok(text) = std::str::from_utf8(&raw)
                {
                    *bytes = text
                        .replace("\r\n", "\n")
                        .replace('\n', "\r\n")
                        .into_bytes()
                        .into_iter()
                        .map(Value::from)
                        .collect();
                }
                #[cfg(not(windows))]
                let _ = name;
            }
        }
        _ => {}
    }
}
pub(super) fn load(text: &str, root: &Path) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut value: Value = serde_json::from_str(text)?;
    let original = historical(&value).ok_or("Frozen first-run root missing")?;
    rebase(&mut value, &original, root);
    Ok(serde_json::from_value(value)?)
}
