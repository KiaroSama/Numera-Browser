use serde_json::Value;
use std::sync::OnceLock;

static POLICY: OnceLock<Value> = OnceLock::new();

fn policy() -> Result<&'static Value, serde_json::Error> {
    if let Some(value) = POLICY.get() {
        return Ok(value);
    }
    let value = serde_json::from_str(include_str!("field_policy.json"))?;
    let _ = POLICY.set(value);
    Ok(POLICY
        .get()
        .expect("immutable parsed field policy installed"))
}
pub(crate) fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "NoneType",
        Value::Bool(_) => "bool",
        Value::String(_) => "str",
        Value::Array(_) => "list",
        Value::Object(_) => "dict",
        Value::Number(n) => {
            if n.to_string().contains(['.', 'e', 'E']) {
                "float"
            } else {
                "int"
            }
        }
    }
}
pub(crate) fn repr(value: &Value) -> String {
    match value {
        Value::Null => "None".into(),
        Value::Bool(v) => if *v { "True" } else { "False" }.into(),
        Value::String(v) => {
            let quote = if v.contains('\'') && !v.contains('"') {
                '"'
            } else {
                '\''
            };
            let body = v
                .replace('\\', "\\\\")
                .replace(quote, &format!("\\{quote}"))
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");
            format!("{quote}{body}{quote}")
        }
        Value::Array(v) => format!("[{}]", v.iter().map(repr).collect::<Vec<_>>().join(", ")),
        Value::Object(v) => format!(
            "{{{}}}",
            v.iter()
                .map(|(k, v)| format!("{}: {}", repr(&Value::String(k.clone())), repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => value.to_string(),
    }
}
fn short(value: &Value, quoted: bool) -> String {
    let text = if quoted {
        repr(value)
    } else {
        value.to_string()
    };
    text.chars().take(40).collect()
}
fn text_problem(field: &str, value: &Value, optional: bool, secret: bool) -> Option<String> {
    if value.is_null() && optional || value.is_string() {
        return None;
    }
    Some(format!(
        "profile: '{field}' must be text, not {}{}. Nothing was written.",
        kind(value),
        if secret {
            String::new()
        } else {
            format!(" ({})", short(value, true))
        }
    ))
}
fn list_problem(field: &str, value: &Value, optional: bool) -> Option<String> {
    if value.is_null() && optional {
        return None;
    }
    match value.as_array() {
        None => Some(format!(
            "profile: '{field}' must be a list, not {}. Nothing was written.",
            if value.is_null() { "None" } else { kind(value) }
        )),
        Some(values) => values.iter().find(|v| !v.is_string()).map(|v| {
            format!(
                "profile: every entry in '{field}' must be text, got {}. Nothing was written.",
                kind(v)
            )
        }),
    }
}
fn number_problem(field: &str, value: &Value, rule: &Value, optional: bool) -> Option<String> {
    if value.is_null() {
        return if optional {
            None
        } else {
            Some(format!(
                "profile: '{field}' is required. Nothing was written."
            ))
        };
    }
    let Some(number) = value.as_number() else {
        return Some(format!(
            "profile: '{field}' must be a number, not {}. Nothing was written.",
            kind(value)
        ));
    };
    let bounds = &rule["bounds"];
    let raw = number.to_string();
    if !raw.contains(['.', 'e', 'E']) {
        let in_range = raw.parse::<i128>().ok().is_some_and(|n| {
            bounds[0].as_f64().is_none_or(|low| n >= low.ceil() as i128)
                && bounds[1]
                    .as_f64()
                    .is_none_or(|high| n <= high.floor() as i128)
        });
        if !in_range {
            return Some(format!(
                "profile: '{field}' must be between {} and {}, got {}. Nothing was written.",
                bounds[0],
                bounds[1],
                short(value, false)
            ));
        }
        return None;
    }
    let Some(n) = number.as_f64() else {
        return Some(format!(
            "profile: '{field}' must be a finite number. Nothing was written."
        ));
    };
    if !n.is_finite() {
        return Some(format!(
            "profile: '{field}' must be a finite number. Nothing was written."
        ));
    }
    let fractions = rule["fractions"].as_array();
    if rule["kind"] == "int" && n.fract() != 0.0 && !fractions.is_some_and(|v| v.contains(value)) {
        let allowed = fractions
            .filter(|v| !v.is_empty())
            .map_or(String::new(), |v| {
                format!(
                    " (or one of {})",
                    v.iter()
                        .map(Value::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            });
        return Some(format!(
            "profile: '{field}' must be a whole number{allowed}, got {}. Nothing was written.",
            short(value, false)
        ));
    }
    let bounds = &rule["bounds"];
    if let (Some(low), Some(high)) = (bounds[0].as_f64(), bounds[1].as_f64())
        && (n < low || n > high)
    {
        return Some(format!(
            "profile: '{field}' must be between {} and {}, got {}. Nothing was written.",
            bounds[0],
            bounds[1],
            short(value, false)
        ));
    }
    None
}
fn bookmarks(field: &str, value: &Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let Some(values) = value.as_array() else {
        return list_problem(field, value, false);
    };
    for entry in values {
        if !entry.is_object() {
            return Some(format!(
                "profile: every entry in '{field}' must be a {{'name', 'url'}} object, got {}. Nothing was written.",
                kind(entry)
            ));
        }
        for (key, optional) in [("url", false), ("name", true)] {
            if let Some(error) =
                text_problem(&format!("{field}[].{key}"), &entry[key], optional, false)
            {
                return Some(error);
            }
        }
    }
    None
}
fn table_problems(container: &Value, table: &Value, prefix: &str, found: &mut Vec<String>) {
    if let Some(fields) = table.as_object() {
        for (field, rule) in fields {
            let label = format!("{prefix}{field}");
            let value = &container[field];
            let optional = rule["optional"].as_bool().unwrap_or(false);
            let problem = match rule["kind"].as_str().unwrap_or("") {
                "owned" | "normalized" => None,
                "text" => text_problem(
                    &label,
                    value,
                    optional,
                    field == "pass" || field == "password",
                ),
                "texts" => list_problem(&label, value, optional),
                "number" | "int" => number_problem(&label, value, rule, optional),
                "bool" => {
                    if value.is_boolean() {
                        None
                    } else {
                        Some(format!(
                            "profile: '{label}' must be true or false, not {}. Nothing was written.",
                            kind(value)
                        ))
                    }
                }
                "bookmarks" => bookmarks(&label, value),
                "fingerprint" | "proxy" => {
                    let model = if field == "fingerprint" {
                        "Fingerprint"
                    } else {
                        "Proxy"
                    };
                    if value.is_null() {
                        if !optional {
                            found.push(format!("profile: '{label}' is required and must be a {}. Nothing was written.",model.to_lowercase()));
                        }
                    } else if !value.is_object() {
                        found.push(format!("profile: '{label}' must be a {} or absent, not {}. Nothing was written.",model.to_lowercase(),kind(value)));
                    } else {
                        table_problems(
                            value,
                            &POLICY.get().expect("policy initialized")["policy"]["tables"][model],
                            &format!("{label}."),
                            found,
                        );
                    }
                    None
                }
                _ => Some(format!("Unknown field validation policy: {label}")),
            };
            if let Some(problem) = problem {
                found.push(problem);
            }
        }
    }
}

/// The dashboard store's converter boundary checks numeric fields only; multiline
/// extension text is valid there even though a partial API patch has other rules.
pub fn flat_number_problems(record: &Value) -> Result<Vec<String>, serde_json::Error> {
    let root = policy()?;
    let p = &root["policy"];
    let mut found = Vec::new();
    if !record.is_object() {
        return Ok(vec![format!(
            "profile: a dashboard profile must be a mapping, not {}. Nothing was written.",
            kind(record)
        )]);
    }
    for (field, _) in p["ranges"].as_object().into_iter().flatten() {
        if let Some(rule) = p["tables"]["Fingerprint"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, rule)| rule)
            .find(|rule| rule["dash"] == *field)
            && let Some(problem) = number_problem(field, &record[field], rule, true)
        {
            found.push(problem);
        }
    }
    Ok(found)
}

/// Validate the original uncoerced JSON before constructing typed models.
pub fn profile_problems(record: &Value, shape: &str) -> Result<Vec<String>, serde_json::Error> {
    let root = policy()?;
    let p = &root["policy"];
    let mut found = Vec::new();
    if shape == "engine record" {
        if !record.is_object() {
            return Ok(vec![format!(
                "profile: an engine profile record must be a mapping, not {}. Nothing was written.",
                kind(record)
            )]);
        }
        table_problems(record, &p["tables"]["Profile"], "", &mut found);
    } else if shape == "dashboard patch" {
        if !record.is_object() {
            return Ok(vec![format!(
                "profile: a profile must be a record or a patch, not {}. Nothing was written.",
                kind(record)
            )]);
        }
        for field in p["texts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(value) = record.get(field)
                && let Some(problem) = text_problem(field, value, field != "name", false)
            {
                found.push(problem);
            }
        }
        for field in p["text_lists"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(problem) = list_problem(field, &record[field], true) {
                found.push(problem);
            }
        }
        for (field, _) in p["ranges"].as_object().into_iter().flatten() {
            if let Some(rule) = p["tables"]["Fingerprint"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(_, rule)| rule)
                .find(|rule| rule["dash"] == *field)
                && let Some(problem) = number_problem(field, &record[field], rule, true)
            {
                found.push(problem);
            }
        }
        if let Some(value) = record.get("screen").filter(|v| !v.is_null()) {
            if let Some(problem) = text_problem("screen", value, true, false) {
                found.push(problem);
            } else if let Some(screen) = value.as_str() {
                let sides: Option<Vec<i64>> = screen
                    .to_lowercase()
                    .split('x')
                    .map(|part| crate::decimal::integer(part)?.parse::<i64>().ok())
                    .collect();
                if !sides.is_some_and(|v| v.len() == 2 && v.iter().all(|v| (1..=65536).contains(v)))
                {
                    found.push(format!("profile: 'screen' must be two whole numbers between 1 and 65536 as '<width>x<height>', got {}. Nothing was written.",repr(&Value::String(screen.chars().take(40).collect()))));
                }
            }
        }
        let proxy = &record["proxy"];
        if !proxy.is_null() {
            if !proxy.is_object() {
                found.push(format!(
                    "profile: 'proxy' must be a proxy or absent, not {}. Nothing was written.",
                    kind(proxy)
                ));
            } else {
                for name in p["proxy_texts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if let Some(problem) = text_problem(
                        &format!("proxy.{name}"),
                        &proxy[name],
                        true,
                        name == "pass" || name == "password",
                    ) {
                        found.push(problem);
                    }
                }
            }
        }
    } else {
        found.push("Unknown profile validation shape".into());
    }
    Ok(found)
}
