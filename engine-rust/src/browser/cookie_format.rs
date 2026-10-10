use crate::conversion::truth;
use serde_json::Value;
use std::io;

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "Cookie export row is not usable",
    )
}

fn text<'a>(row: &'a Value, key: &str, default: Option<&'a str>) -> io::Result<&'a str> {
    match row.get(key) {
        Some(value) => value.as_str().ok_or_else(invalid),
        None => default.ok_or_else(invalid),
    }
}

fn expiry(value: Option<&Value>) -> io::Result<String> {
    let Some(value) = value.filter(|value| truth(value)) else {
        return Ok("-1".into());
    };
    let integer = match value {
        Value::Bool(true) => Some("1".into()),
        Value::Number(number) => {
            let raw = number.to_string();
            if !raw.contains(['.', 'e', 'E']) {
                Some(raw)
            } else {
                number
                    .as_f64()
                    .filter(|n| n.is_finite())
                    .map(|n| format!("{:.0}", n.trunc()))
            }
        }
        Value::String(raw) => crate::decimal::integer(raw),
        _ => None,
    }
    .ok_or_else(invalid)?;
    let (negative, digits) = integer
        .strip_prefix('-')
        .map_or((false, integer.as_str()), |v| (true, v));
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        Ok("0".into())
    } else {
        Ok(format!("{}{digits}", if negative { "-" } else { "" }))
    }
}

/// Export normalized rows without changing the original header, order or bytes.
/// Parsing, normalization and destination writes belong to separate boundaries.
pub fn to_netscape(cookies: &[Value]) -> io::Result<String> {
    let mut output = String::from(
        "# Netscape HTTP Cookie File\n# Exported by Numera — https://github.com/KiaroSama/Numera\n\n",
    );
    for cookie in cookies {
        if !cookie.is_object() {
            return Err(invalid());
        }
        let domain = text(cookie, "domain", None)?;
        let name = text(cookie, "name", None)?;
        let path = text(cookie, "path", Some("/"))?;
        let value = text(cookie, "value", Some(""))?;
        let expires = expiry(cookie.get("expires"))?;
        let prefix = if truth(&cookie["httpOnly"]) {
            "#HttpOnly_"
        } else {
            ""
        };
        let include = if domain.starts_with('.') {
            "TRUE"
        } else {
            "FALSE"
        };
        let secure = if truth(&cookie["secure"]) {
            "TRUE"
        } else {
            "FALSE"
        };
        output.push_str(&format!(
            "{prefix}{domain}\t{include}\t{path}\t{secure}\t{expires}\t{name}\t{value}\n"
        ));
    }
    Ok(output)
}
