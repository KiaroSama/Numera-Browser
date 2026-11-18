pub use crate::pool_normalize::{normalize, normalize_many, validate_rotate_url};
use serde_json::{Value, json};
use std::io;

pub(crate) fn whitespace(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\u{1c}'..='\u{1f}')
}

pub(crate) fn trim(value: &str) -> &str {
    value.trim_matches(whitespace)
}

fn country_label(value: &str) -> (Option<String>, Option<String>) {
    let value = trim(value);
    let mut letters = value.chars();
    let (Some(a), Some(b)) = (letters.next(), letters.next()) else {
        return (None, None);
    };
    if !a.is_ascii_alphabetic() || !b.is_ascii_alphabetic() {
        return (None, None);
    }
    let rest = letters.as_str();
    let country = value[..2].to_ascii_uppercase();
    if rest.is_empty() {
        return (Some(country), None);
    }
    let label = rest.trim_start_matches(|c: char| whitespace(c) || "._-".contains(c));
    if label.len() < rest.len() && !label.is_empty() {
        (Some(country), Some(value.into()))
    } else {
        (None, None)
    }
}

pub fn parse_line(line: &str) -> Option<Value> {
    let line = trim(line);
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (scheme, mut endpoint) = line
        .split_once("://")
        .map_or(("http".into(), line), |(scheme, rest)| {
            (scheme.to_lowercase(), rest)
        });
    let (mut user, mut password) = (None, None);
    if let Some((credentials, rest)) = endpoint.rsplit_once('@') {
        endpoint = rest;
        if let Some((name, secret)) = credentials.split_once(':') {
            user = Some(name);
            password = Some(secret);
        } else {
            user = Some(credentials);
        }
    }
    let parts: Vec<&str> = if let Some(rest) = endpoint.strip_prefix('[') {
        let (host, tail) = rest.split_once(']')?;
        std::iter::once(host)
            .chain(tail.strip_prefix(':').unwrap_or(tail).split(':'))
            .collect()
    } else {
        endpoint.split(':').collect()
    };
    let port = *parts.get(1)?;
    if port.is_empty() || !port.chars().all(is_digit) {
        return None;
    }
    let host = trim(parts[0]);
    if host.is_empty() {
        return None;
    }
    let (country, label) = if user.is_none() && parts.len() >= 4 {
        user = Some(parts[2]);
        password = Some(parts[3]);
        parts
            .get(4)
            .map_or((None, None), |value| country_label(value))
    } else if parts.len() >= 3 {
        country_label(parts.last()?)
    } else {
        (None, None)
    };
    let optional = |value: Option<&str>| value.filter(|v| !v.is_empty()).map(str::to_owned);
    let kind = match scheme.as_str() {
        "http" => "HTTP",
        "https" => "HTTPS",
        "socks" | "socks5" | "socks5h" => "SOCKS5",
        "socks4" => "SOCKS4",
        _ => "",
    };
    Some(
        json!({"type":if kind.is_empty(){scheme.to_uppercase()}else{kind.into()},"host":host,"port":port.trim(),"user":optional(user),"pass":optional(password),"country":country,"label":label}),
    )
}

pub(crate) fn is_digit(character: char) -> bool {
    let scalar = u32::from(character);
    // Generated from the pinned original runtime's Unicode digit predicate,
    // not Rust is_numeric (which also accepts fractions and Roman numerals).
    let ranges: &[[u32; 2]] = digit_ranges();
    ranges
        .iter()
        .any(|range| range[0] <= scalar && scalar <= range[1])
}
#[derive(serde::Deserialize)]
struct DigitCatalog {
    ranges: Vec<[u32; 2]>,
    decimal_ranges: Vec<[u32; 2]>,
}
fn digits() -> &'static DigitCatalog {
    static RANGES: std::sync::OnceLock<DigitCatalog> = std::sync::OnceLock::new();
    RANGES.get_or_init(|| {
        serde_json::from_str(include_str!("pool_digit_ranges.json"))
            .expect("Generated Unicode digit ranges must be valid JSON")
    })
}
fn digit_ranges() -> &'static [[u32; 2]] {
    &digits().ranges
}

pub(crate) fn decimal_ascii(text: &str) -> Option<String> {
    let mut out = String::new();
    for character in text.chars() {
        let scalar = u32::from(character);
        let range = digits()
            .decimal_ranges
            .iter()
            .find(|range| range[0] <= scalar && scalar <= range[1])?;
        out.push(char::from(b'0' + ((scalar - range[0]) % 10) as u8));
    }
    Some(out)
}

pub fn server_url(kind: &str, host: &str, port: &str) -> io::Result<String> {
    let scheme = scheme(kind)?;
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.into()
    };
    let tail = if port.trim().is_empty() {
        String::new()
    } else {
        format!(":{port}")
    };
    Ok(format!("{scheme}://{host}{tail}"))
}
fn scheme(kind: &str) -> io::Result<&'static str> {
    match kind.to_uppercase().as_str() {
        "HTTP" | "" => Ok("http"),
        "HTTPS" => Ok("https"),
        "SOCKS4" => Ok("socks4"),
        "SOCKS5" => Ok("socks5"),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unsupported proxy type {}; expected one of ['HTTP', 'HTTPS', 'SOCKS4', 'SOCKS5']",
                crate::validation::repr(&Value::String(kind.into()))
            ),
        )),
    }
}
pub fn to_line(proxy: &Value) -> io::Result<String> {
    // Export retains the original fallback for unknown legacy pool types.
    let scheme = scheme(proxy["type"].as_str().unwrap_or("HTTP")).unwrap_or("http");
    let mut host = proxy["host"]
        .as_str()
        .ok_or_else(|| io::Error::other("Proxy host is not text"))?
        .to_owned();
    if host.contains(':') && !host.starts_with('[') {
        host = format!("[{host}]");
    }
    let auth = proxy["user"]
        .as_str()
        .filter(|v| !v.is_empty())
        .map_or(String::new(), |user| {
            format!("{user}:{}@", proxy["pass"].as_str().unwrap_or(""))
        });
    let port = proxy["port"]
        .as_str()
        .ok_or_else(|| io::Error::other("Proxy port is not text"))?;
    let mut line = format!("{scheme}://{auth}{host}:{port}");
    if let Some(tail) = proxy["label"]
        .as_str()
        .filter(|v| !v.is_empty())
        .or_else(|| proxy["country"].as_str().filter(|v| !v.is_empty()))
    {
        line.push(':');
        line.push_str(tail);
    }
    Ok(line)
}

pub fn identity(entry: &Value) -> Value {
    let text = |key: &str, fallback: &str| {
        entry
            .get(key)
            .map_or_else(|| fallback.into(), crate::conversion::value_text)
    };
    let kept = |key: &str| {
        if crate::conversion::truth(&entry[key]) {
            entry[key].clone()
        } else {
            Value::String(String::new())
        }
    };
    json!([
        text("type", "HTTP").to_uppercase(),
        text("host", ""),
        text("port", ""),
        kept("user"),
        kept("pass")
    ])
}

pub fn parse_many(text: &str) -> Vec<Value> {
    text.replace(',', "\n")
        .split(|character| {
            matches!(
                character,
                '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                    ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
        })
        .filter_map(parse_line)
        .collect()
}
