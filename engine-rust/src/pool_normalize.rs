use crate::{
    conversion::{truth, value_text},
    pool::decimal_ascii,
    validation::{kind, repr},
};
use serde_json::{Value, json};
use std::io;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
fn text(value: &Value) -> Option<String> {
    if value.is_null() || value == "" {
        None
    } else {
        Some(value_text(value))
    }
}

pub fn validate_rotate_url(url: Option<&str>) -> io::Result<Option<String>> {
    let Some(url) = url.map(str::trim).filter(|url| !url.is_empty()) else {
        return Ok(None);
    };
    let scheme = url
        .split_once(':')
        .filter(|(scheme, _)| {
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        })
        .map_or("", |(scheme, _)| scheme);
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err(invalid(format!(
            "the change-IP URL must be http(s), got {}",
            repr(&Value::String(
                if scheme.is_empty() { "nothing" } else { scheme }.into()
            ))
        )));
    }
    let rest = &url[scheme.len() + 1..];
    let authority = rest
        .strip_prefix("//")
        .unwrap_or("")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, tail) = bracketed
            .split_once(']')
            .ok_or_else(|| invalid("Invalid IPv6 URL"))?;
        host.parse::<std::net::Ipv6Addr>()
            .map_err(|_| invalid("Invalid IPv6 URL"))?;
        (host, tail.strip_prefix(':'))
    } else {
        authority
            .split_once(':')
            .map_or((authority, None), |(host, port)| (host, Some(port)))
    };
    if host.is_empty() {
        return Err(invalid("the change-IP URL has no host"));
    }
    if let Some(port) = port.filter(|port| !port.is_empty())
        && (port.bytes().any(|c| !c.is_ascii_digit()) || port.parse::<u16>().is_err())
    {
        return Err(invalid(format!(
            "{} has an invalid port",
            repr(&Value::String(url.into()))
        )));
    }
    Ok(Some(url.into()))
}

pub fn normalize(entry: &Value) -> io::Result<Value> {
    if !entry.is_object() {
        return Err(invalid(format!(
            "a proxy must be an object, got {}",
            kind(entry)
        )));
    }
    let proxy_type = if truth(&entry["type"]) {
        value_text(&entry["type"])
    } else {
        "HTTP".into()
    }
    .to_uppercase();
    if !["HTTP", "HTTPS", "SOCKS4", "SOCKS5"].contains(&proxy_type.as_str()) {
        return Err(invalid(format!(
            "unsupported proxy type {}; expected one of HTTP, HTTPS, SOCKS4, SOCKS5",
            repr(&Value::String(proxy_type))
        )));
    }
    let host = if truth(&entry["host"]) {
        value_text(&entry["host"])
    } else {
        String::new()
    };
    let host = host.trim().trim_matches(['[', ']']);
    if host.is_empty() {
        return Err(invalid("a proxy needs a host"));
    }
    if host.chars().any(char::is_whitespace) {
        return Err(invalid(format!(
            "{} is not a valid host",
            repr(&Value::String(host.into()))
        )));
    }
    let raw_port = &entry["port"];
    let raw_text = value_text(raw_port);
    let raw_text = raw_text.trim();
    let (sign, magnitude) = raw_text.strip_prefix('-').map_or_else(
        || (1, raw_text.strip_prefix('+').unwrap_or(raw_text)),
        |magnitude| (-1, magnitude),
    );
    let ascii = decimal_ascii(magnitude).filter(|digits| !digits.is_empty());
    let port = ascii
        .and_then(|digits| digits.parse::<i128>().ok())
        .and_then(|port| port.checked_mul(sign));
    let port = port.ok_or_else(|| invalid(format!("{} is not a valid port", repr(raw_port))))?;
    if !(1..=65535).contains(&port) {
        return Err(invalid(format!("port {port} is outside 1-65535")));
    }
    let secret = &entry["pass"];
    if !secret.is_null() && !secret.is_string() {
        return Err(invalid(format!(
            "a proxy password must be text, got {}",
            kind(secret)
        )));
    }
    let country = text(&entry["country"])
        .unwrap_or_default()
        .trim()
        .to_uppercase();
    if !country.is_empty()
        && (country.len() != 2 || !country.bytes().all(|c| c.is_ascii_alphabetic()))
    {
        return Err(invalid(format!(
            "{} is not a two-letter country code",
            repr(&Value::String(country))
        )));
    }
    let rotate = text(&entry["rotateUrl"]);
    Ok(
        json!({"type":proxy_type,"host":host,"port":port.to_string(),"user":text(&entry["user"]),"pass":text(secret),"country":if country.is_empty(){None}else{Some(country)},"label":text(&entry["label"]),"rotateUrl":validate_rotate_url(rotate.as_deref())?}),
    )
}

pub fn normalize_many(entries: &Value) -> io::Result<Vec<Value>> {
    let entries = entries
        .as_array()
        .ok_or_else(|| invalid(format!("'proxies' must be a list, got {}", kind(entries))))?;
    let (mut out, mut problems) = (Vec::new(), Vec::new());
    for (index, entry) in entries.iter().enumerate() {
        match normalize(entry) {
            Ok(value) => out.push(value),
            Err(error) => problems.push(format!("proxies[{index}]: {error}")),
        }
    }
    if !problems.is_empty() {
        return Err(invalid(
            problems.into_iter().take(8).collect::<Vec<_>>().join("; "),
        ));
    }
    Ok(out)
}
