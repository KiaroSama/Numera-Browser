use super::{
    cookie_expiry::as_epoch,
    cookie_format::{parse_netscape, whitespace},
};
use crate::{conversion::truth, validation::repr};
use serde_json::{Value, json};
use std::{io, path::Path};

fn string(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => repr(value),
        _ => crate::conversion::value_text(value),
    }
}

/// Incomplete entries are dropped; malformed expiry does not discard good cookies.
pub fn normalize(raw: &Value) -> io::Result<Option<Value>> {
    let Some(raw) = raw.as_object() else {
        return Ok(None);
    };
    let absent = Value::Null;
    let get = |key: &str| raw.get(key).unwrap_or(&absent);
    let name = get("name");
    let domain = if truth(get("domain")) {
        get("domain")
    } else {
        get("host")
    };
    if !truth(name) || !truth(domain) {
        return Ok(None);
    }
    let expires = raw.get("expires").unwrap_or_else(|| get("expirationDate"));
    let expires = if truth(get("session"))
        || expires.is_null()
        || expires == ""
        || expires.as_f64() == Some(0.0)
        || expires == &Value::Bool(false)
    {
        -1.0
    } else {
        as_epoch(expires)?
    };
    let secure = truth(get("secure"));
    let same_site = match string(raw.get("sameSite").unwrap_or(&Value::String(String::new())))
        .to_lowercase()
        .as_str()
    {
        "no_restriction" | "none" if secure => "None",
        "strict" => "Strict",
        _ => "Lax",
    };
    Ok(Some(json!({
        "name": string(name),
        "value": string(raw.get("value").unwrap_or(&Value::String(String::new()))),
        "domain": string(domain),
        "path": if truth(get("path")) { string(get("path")) } else { "/".into() },
        "expires": expires,
        "httpOnly": truth(get("httpOnly")), "secure": secure, "sameSite": same_site
    })))
}

pub fn normalize_many(rows: &[Value]) -> io::Result<Vec<Value>> {
    let mut cookies = Vec::new();
    for row in rows {
        if let Some(cookie) = normalize(row)? {
            cookies.push(cookie);
        }
    }
    Ok(cookies)
}

pub fn parse(text: &str) -> io::Result<Vec<Value>> {
    let text = text.trim_matches(whitespace);
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let data: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return parse_netscape(text),
    };
    match data {
        Value::Array(rows) => normalize_many(&rows),
        Value::Object(mut object) => match object.remove("cookies") {
            Some(Value::Array(rows)) => normalize_many(&rows),
            Some(_) => Ok(Vec::new()),
            None => normalize_many(&[Value::Object(object)]),
        },
        _ => Ok(Vec::new()),
    }
}

pub fn read_file(path: &Path) -> io::Result<Vec<Value>> {
    parse(&std::fs::read_to_string(path)?)
}
