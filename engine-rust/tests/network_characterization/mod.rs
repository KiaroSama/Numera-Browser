//! Frozen immutable-source observations; path tokens belong to owned test roots.
use serde_json::Value;
use std::path::Path;
pub fn load(text: &str, root: &Path) -> serde_json::Result<Value> {
    let mut value: Value = serde_json::from_str(text)?;
    let root_text = root.to_string_lossy();
    const COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    let root_url = percent_encoding::utf8_percent_encode(&root_text, COMPONENT).to_string();
    fn materialize(value: &mut Value, root: &str, encoded: &str) {
        match value {
            Value::String(text) => {
                *text = text
                    .replace("@ROOT_URL@", encoded)
                    .replace("@SEP@", std::path::MAIN_SEPARATOR_STR)
                    .replace("@ROOT@", root);
            }
            Value::Array(values) => {
                for item in values {
                    materialize(item, root, encoded);
                }
            }
            Value::Object(values) => {
                for item in values.values_mut() {
                    materialize(item, root, encoded);
                }
            }
            _ => {}
        }
    }
    materialize(&mut value, &root_text, &root_url);
    Ok(value)
}
