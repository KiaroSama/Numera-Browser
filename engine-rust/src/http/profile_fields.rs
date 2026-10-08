use crate::{conversion, decimal, validation};
use serde_json::{Map, Value};
use std::sync::OnceLock;

static SCHEMA: OnceLock<Value> = OnceLock::new();
fn schema() -> Result<&'static Value, serde_json::Error> {
    if let Some(schema) = SCHEMA.get() {
        return Ok(schema);
    }
    let value = serde_json::from_str(include_str!("field_schema.json"))?;
    let _ = SCHEMA.set(value);
    Ok(SCHEMA
        .get()
        .expect("immutable parsed HTTP field schema installed"))
}
pub fn strip_server_owned(
    record: &mut Map<String, Value>,
) -> Result<Vec<String>, serde_json::Error> {
    let schema = schema()?;
    let mut removed = record
        .keys()
        .filter(|key| {
            (key.starts_with('_') && key.as_str() != "_")
                || schema["server_owned"]
                    .as_array()
                    .is_some_and(|fields| fields.iter().any(|field| field == key.as_str()))
        })
        .cloned()
        .collect::<Vec<_>>();
    removed.sort();
    for key in &removed {
        record.remove(key);
    }
    Ok(removed)
}
fn matches(value: &Value, names: &[Value]) -> bool {
    names
        .iter()
        .filter_map(Value::as_str)
        .any(|name| validation::kind(value) == name || name == "float" && value.is_number())
}
fn names(values: &[Value]) -> String {
    values
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" or ")
}
fn dimension(value: &str) -> bool {
    decimal::integer(value)
        .and_then(|value| value.parse::<i64>().ok())
        .is_some_and(|number| (1..=65536).contains(&number))
}
pub fn check(
    record: &Value,
    where_: &str,
    extra: &[&str],
) -> Result<Vec<String>, serde_json::Error> {
    let schema = schema()?;
    let Some(record) = record.as_object() else {
        return Ok(vec![format!("{where_} must be an object")]);
    };
    let fields = schema["fields"]
        .as_object()
        .expect("source schema fields are an object");
    let client = schema["client_fields"]
        .as_array()
        .expect("source client fields are an array");
    let mut unknown = record
        .keys()
        .filter(|key| {
            !extra.contains(&key.as_str()) && !client.iter().any(|field| field == key.as_str())
        })
        .collect::<Vec<_>>();
    unknown.sort();
    let mut found = unknown
        .into_iter()
        .map(|key| {
            format!(
                "{where_}: unknown field {}",
                validation::repr(&Value::String(key.clone()))
            )
        })
        .collect::<Vec<_>>();
    for (field, kinds) in fields {
        let Some(value) = record.get(field).filter(|value| !value.is_null()) else {
            continue;
        };
        let kinds = kinds.as_array().expect("source field types are arrays");
        if !matches(value, kinds) {
            found.push(format!(
                "{where_}: {} must be {}, got {}",
                validation::repr(&Value::String(field.clone())),
                names(kinds),
                validation::kind(value)
            ));
            continue;
        }
        if let Some(values) = value.as_array() {
            if let Some(value) = values.iter().find(|value| !value.is_string()) {
                found.push(format!(
                    "{where_}: every entry in {} must be a string, got {}",
                    validation::repr(&Value::String(field.clone())),
                    validation::kind(value)
                ));
            }
        } else if value.is_number() {
            let one = Value::Object(Map::from_iter([(field.clone(), value.clone())]));
            found.extend(
                validation::flat_number_problems(&one)?
                    .into_iter()
                    .map(|problem| {
                        format!(
                            "{where_}{}",
                            problem.strip_prefix("profile").unwrap_or(&problem)
                        )
                    }),
            );
        } else if field == "proxy" {
            for (key, kinds) in schema["proxy_fields"]
                .as_object()
                .expect("source proxy fields are an object")
            {
                if let Some(value) = value.get(key).filter(|value| !value.is_null()) {
                    let kinds = kinds.as_array().expect("source proxy types are arrays");
                    if !matches(value, kinds) {
                        found.push(format!(
                            "{where_}: proxy {} must be {}, got {}",
                            validation::repr(&Value::String(key.clone())),
                            names(kinds),
                            validation::kind(value)
                        ));
                    }
                }
            }
        } else if field == "screen" {
            let screen = value.as_str().expect("string type checked");
            let parts = screen.split(['x', 'X']).collect::<Vec<_>>();
            if parts.len() != 2 || !parts.iter().all(|part| dimension(part)) {
                found.push(format!("{where_}: 'screen' must be two whole numbers between 1 and 65536 as '<width>x<height>', got {}. Nothing was written.",validation::repr(&Value::String(screen.chars().take(40).collect()))));
            }
        }
    }
    Ok(found)
}
pub fn complete(
    mut record: Value,
    id: &str,
    default_engine: Option<&Value>,
    now: f64,
) -> Result<Value, String> {
    let object = record.as_object_mut().ok_or("profile must be an object")?;
    object.insert("id".into(), id.into());
    if let Some(engine) = object.get("engine").filter(|value| !value.is_null())
        && !engine
            .as_str()
            .is_some_and(|name| ["playwright", "patchright", "camoufox", "cloak"].contains(&name))
    {
        return Err(format!(
            "unknown engine {}; known: camoufox, cloak, patchright, playwright",
            validation::repr(engine)
        ));
    }
    if let Some(default) = default_engine
        .filter(|_| !conversion::truth(&object.get("engine").cloned().unwrap_or(Value::Null)))
    {
        object.insert("engine".into(), default.clone());
    }
    let problems =
        validation::flat_number_problems(&record).map_err(|_| "Field policy is unreadable")?;
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }
    let profile = conversion::to_engine(&record, now)
        .map_err(|error| format!("this profile cannot be converted for the engine: {error}"))?;
    let complete = conversion::to_dashboard(&profile);
    let object = record
        .as_object_mut()
        .expect("record retains its object shape");
    for (key, value) in complete
        .as_object()
        .expect("converted profile is an object")
    {
        if object.get(key).is_none_or(Value::is_null) {
            object.insert(key.clone(), value.clone());
        }
    }
    Ok(record)
}
