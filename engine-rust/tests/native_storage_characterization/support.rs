//! Rebase captured fixture paths without changing JSON payload or byte policy.
use serde_json::Value;
use std::path::Path;

fn text(value: &str, old: &str, root: &Path) -> String {
    if let Some(suffix) = value.strip_prefix(old) {
        let suffix = suffix.trim_start_matches('/');
        return root.join(suffix).to_string_lossy().into_owned();
    }
    value.replace(old, &root.to_string_lossy())
}
fn visit(value: &mut Value, old: &str, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    match value {
        Value::String(value) => *value = text(value, old, root),
        Value::Array(values) => {
            // Embedded raw settings bytes keep their spelling and whitespace.
            // A path inside valid JSON is replaced as a JSON string fragment,
            // preserving escapes (including native Windows separators).
            if let Some(bytes) = values
                .iter()
                .map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                .collect::<Option<Vec<_>>>()
                && let Ok(raw) = std::str::from_utf8(&bytes)
                && raw.contains(old)
            {
                let parsed: Value = serde_json::from_str(raw)?;
                let mut replacements = Vec::new();
                paths(&parsed, old, root, &mut replacements)?;
                let mut replaced = raw.to_owned();
                for (before, after) in replacements {
                    replaced = replaced.replace(&before, &after);
                }
                *values = replaced.bytes().map(Value::from).collect();
            } else {
                for value in values {
                    visit(value, old, root)?;
                }
            }
        }
        Value::Object(values) => {
            for (name, value) in values {
                if name == "input"
                    && let Some(encoded) = value.as_str()
                    && encoded.len() % 2 == 0
                    && encoded.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    let bytes = (0..encoded.len())
                        .step_by(2)
                        .map(|n| u8::from_str_radix(&encoded[n..n + 2], 16))
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut raw = Value::Array(bytes.into_iter().map(Value::from).collect());
                    visit(&mut raw, old, root)?;
                    let bytes: Vec<u8> = serde_json::from_value(raw)?;
                    *value = bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                        .into();
                } else {
                    visit(value, old, root)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}
fn paths(
    value: &Value,
    old: &str,
    root: &Path,
    out: &mut Vec<(String, String)>,
) -> Result<(), serde_json::Error> {
    match value {
        Value::String(value) if value.contains(old) => out.push((
            serde_json::to_string(value)?,
            serde_json::to_string(&text(value, old, root))?,
        )),
        Value::Array(values) => {
            for value in values {
                paths(value, old, root, out)?;
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                paths(value, old, root, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn load(
    document: &str,
    old: &str,
    root: &Path,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut document: Value = serde_json::from_str(document)?;
    visit(&mut document, old, root)?;
    Ok(serde_json::from_value(document)?)
}
