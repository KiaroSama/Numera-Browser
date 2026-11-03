use crate::conversion::{truth, value_text};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io};

fn check(name: &str, ok: bool, detail: impl Into<String>) -> Value {
    json!({"name":name,"ok":ok,"detail":detail.into()})
}
pub fn dns(result: &Value, country: Option<&str>) -> io::Result<Value> {
    let Some(rows) = result["rows"]
        .as_array()
        .filter(|_| !truth(&result["error"]))
    else {
        let error = if truth(&result["error"]) {
            result["error"].clone()
        } else {
            "no DNS result".into()
        };
        let detail = if truth(&result["error"]) {
            value_text(&result["error"])
        } else {
            "bash.ws unreachable".into()
        };
        return Ok(
            json!({"ok":false,"error":error,"checks":[check("DNS leak test reachable",false,detail)]}),
        );
    };
    let mut resolvers = Vec::new();
    for row in rows {
        if !row.is_object() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "DNS result row is not an object",
            ));
        }
        let kind = if truth(&row["type"]) {
            row["type"]
                .as_str()
                .ok_or_else(|| io::Error::other("DNS row type is not text"))?
        } else {
            ""
        };
        if kind.to_lowercase() == "dns" {
            resolvers.push(row);
        }
    }
    let countries = resolvers
        .iter()
        .map(|row| {
            let country = if truth(&row["country_code"]) {
                &row["country_code"]
            } else if truth(&row["country"]) {
                &row["country"]
            } else {
                return Ok("?".into());
            };
            country
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| io::Error::other("DNS resolver country is not text"))
        })
        .collect::<io::Result<BTreeSet<String>>>()?
        .into_iter()
        .collect::<Vec<_>>();
    let asns = resolvers
        .iter()
        .filter(|row| truth(&row["asn"]))
        .map(|row| value_text(&row["asn"]))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let countries_text = countries.join(", ");
    let mut checks = vec![check(
        "DNS resolution was observed",
        !resolvers.is_empty(),
        format!(
            "{} resolver(s) seen{}",
            resolvers.len(),
            if countries.is_empty() {
                String::new()
            } else {
                format!(" in {countries_text}")
            }
        ),
    )];
    let want = country.unwrap_or("").to_uppercase();
    if !resolvers.is_empty() {
        if !want.is_empty() {
            let mut strays = false;
            for row in &resolvers {
                if truth(&row["country_code"]) {
                    let code = row["country_code"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("DNS country code is not text"))?
                        .to_uppercase();
                    strays |= !code.is_empty() && code != want;
                }
            }
            checks.push(check(
                "DNS stays inside the proxy country (no ISP leak)",
                !strays,
                if strays {
                    format!("expected all in {want}, but saw {countries_text}")
                } else {
                    format!("all resolvers in {want}")
                },
            ));
        } else {
            checks.push(check(
                "DNS resolvers (baseline, no proxy country set)",
                true,
                if countries_text.is_empty() {
                    "unknown".into()
                } else {
                    countries_text
                },
            ));
        }
    }
    Ok(json!({"ok":checks.iter().all(|check| check["ok"] == true),
        "resolvers":resolvers.iter().map(|row| json!({"ip":row["ip"],"country":row["country"],"countryCode":row["country_code"],"asn":row["asn"]})).collect::<Vec<_>>(),
        "countries":countries,"asns":asns,"checks":checks}))
}
pub fn tls(info: &Value) -> io::Result<Value> {
    if !truth(info) {
        return Ok(json!({"ok":false,"error":"no TLS echo service answered",
            "checks":[check("TLS fingerprint readable",false,"tls.peet.ws / browserleaks unreachable")]}));
    }
    if !info.is_object() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "TLS observation is not an object",
        ));
    }
    let ja4 = if truth(&info["ja4"]) {
        info["ja4"]
            .as_str()
            .ok_or_else(|| io::Error::other("TLS JA4 is not text"))?
    } else {
        ""
    };
    let hash = if truth(&info["ja3Hash"]) {
        value_text(&info["ja3Hash"])
    } else {
        "?".into()
    };
    let mut checks = vec![check(
        "TLS handshake reaches a real endpoint",
        true,
        format!(
            "JA3 {hash}{}",
            if ja4.is_empty() {
                String::new()
            } else {
                format!(" · JA4 {ja4}")
            }
        ),
    )];
    if !ja4.is_empty() {
        checks.push(check(
            "Negotiates TLS 1.3 (browser-grade)",
            ja4.starts_with("t13"),
            format!(
                "JA4 starts '{}', not 't13'",
                ja4.chars().take(4).collect::<String>()
            ),
        ));
        checks.push(check(
            "Speaks HTTP/2 like a browser",
            ja4.split('_').next().unwrap_or("").contains("h2"),
            "no h2 in the ALPN — browsers negotiate it, most scripts don't",
        ));
    }
    if truth(&info["greaseKnown"]) {
        checks.push(check(
            "Sends GREASE like a real Chrome",
            truth(&info["grease"]),
            "no GREASE values — this isn't a stock Chromium ClientHello",
        ));
    }
    if truth(&info["cipherCount"]) {
        let count = info["cipherCount"]
            .as_f64()
            .ok_or_else(|| io::Error::other("TLS cipher count is not numeric"))?;
        checks.push(check(
            "Cipher suite list is browser-shaped",
            count >= 10.0,
            format!(
                "only {} suite(s); real Chrome offers ~15",
                value_text(&info["cipherCount"])
            ),
        ));
    }
    Ok(
        json!({"ok":checks.iter().all(|check| check["ok"] == true),"ja3":info["ja3"],"ja3Hash":info["ja3Hash"],"ja4":info["ja4"],"http2":info["http2"],
        "grease":if truth(&info["greaseKnown"]) {info["grease"].clone()} else {Value::Null},"checks":checks}),
    )
}
