//! Read-only repair of legacy JSON values; stored records remain unchanged.
use serde_json::Value;

const TEXT_FIELDS: &[&str] = &["name", "notes", "engine", "dns"];
const LIST_FIELDS: &[&str] = &[
    "tags",
    "startup_urls",
    "extensions",
    "launch_args",
    "ip_based",
    "clear_on_start",
    "switched_off",
];

fn value_with(field: &str, value: &Value, texts: &[&str], lists: &[&str]) -> Value {
    if texts.contains(&field) {
        match value {
            Value::String(_) => value.clone(),
            Value::Bool(value) => value.to_string().into(),
            Value::Number(number) => {
                let text = number.to_string();
                if text.contains(['.', 'e', 'E']) {
                    number
                        .as_f64()
                        .filter(|value| value.is_finite())
                        .map(crate::decimal::general_number)
                        .map_or_else(|| "".into(), Value::String)
                } else {
                    Value::String(text)
                }
            }
            _ => "".into(),
        }
    } else if lists.contains(&field) {
        Value::Array(
            value
                .as_array()
                .into_iter()
                .flatten()
                .filter(|v| v.is_string())
                .cloned()
                .collect(),
        )
    } else {
        value.clone()
    }
}

pub fn readable_value(field: &str, value: &Value) -> Value {
    value_with(field, value, TEXT_FIELDS, LIST_FIELDS)
}

pub fn readable_names(pairs: &[(String, Value)]) -> Vec<(String, Value)> {
    pairs
        .iter()
        .map(|(id, name)| (id.clone(), readable_value("name", name)))
        .collect()
}

pub fn readable_with(record: &Value, texts: &[&str], lists: &[&str]) -> Value {
    let mut result = record.clone();
    if let Some(fields) = result.as_object_mut() {
        for (field, value) in fields.iter_mut() {
            *value = value_with(field, value, texts, lists);
        }
        if texts.contains(&"name") {
            fields.entry("name").or_insert("".into());
        }
    }
    result
}

pub fn readable(record: &Value) -> Value {
    readable_with(record, TEXT_FIELDS, LIST_FIELDS)
}
