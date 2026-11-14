//! Frozen immutable-source observations; path tokens belong to owned test roots.
use serde_json::Value;
use std::path::Path;
pub fn load(text: &str, root: &Path) -> serde_json::Result<Value> {
    load_with_path_spelling(text, &root.to_string_lossy(), std::path::MAIN_SEPARATOR_STR)
}
pub fn load_with_path_spelling(
    text: &str,
    root_text: &str,
    separator: &str,
) -> serde_json::Result<Value> {
    let mut value: Value = serde_json::from_str(text)?;
    const COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    let root_url = percent_encoding::utf8_percent_encode(root_text, COMPONENT).to_string();
    fn materialize(
        value: &mut Value,
        root: &str,
        encoded: &str,
        separator: &str,
        quoted_path: bool,
    ) {
        match value {
            Value::String(text) => {
                // The extension-text schema stores diagnostics as Python
                // repr literals. Escape only their inserted path tokens;
                // caller entries, native path/icon fields and URLs are raw.
                let escaped_root;
                let escaped_separator;
                let (root, separator) = if quoted_path {
                    escaped_root = root.replace('\\', "\\\\");
                    escaped_separator = separator.replace('\\', "\\\\");
                    (escaped_root.as_str(), escaped_separator.as_str())
                } else {
                    (root, separator)
                };
                *text = text
                    .replace("@ROOT_URL@", encoded)
                    .replace("@SEP@", separator)
                    .replace("@ROOT@", root);
            }
            Value::Array(values) => {
                for item in values {
                    materialize(item, root, encoded, separator, quoted_path);
                }
            }
            Value::Object(values) => {
                let diagnostic_row = values.contains_key("entries")
                    && values.get("result").is_some_and(Value::is_array);
                for (field, item) in values {
                    materialize(
                        item,
                        root,
                        encoded,
                        separator,
                        diagnostic_row && field == "result",
                    );
                }
            }
            _ => {}
        }
    }
    materialize(&mut value, root_text, &root_url, separator, false);
    Ok(value)
}
