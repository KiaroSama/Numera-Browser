use crate::{
    conversion::{truth, value_text},
    devices::catalog,
    models::Profile,
};
use serde_json::{Value, json};
use std::io;

fn check(name: &str, ok: bool, detail: String) -> Value {
    json!({"name":name,"ok":ok,"detail":detail})
}
fn private(ip: &str) -> io::Result<bool> {
    if ip.contains(':') {
        let ip = ip.to_lowercase();
        return Ok(["fc", "fd", "fe8", "fe9", "fea", "feb", "::1"]
            .iter()
            .any(|prefix| ip.starts_with(prefix)));
    }
    let parts = ip.split('.').collect::<Vec<_>>();
    if parts.len() != 4
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.chars().all(crate::pool::is_digit))
    {
        return Ok(true);
    }
    let number = |part: &str| -> io::Result<u32> {
        let ascii = crate::pool::decimal_ascii(part)
            .ok_or_else(|| io::Error::other("WebRTC address digit is not decimal"))?;
        // Only equality/ranges below 256 matter; a larger decimal stays larger.
        let significant = ascii.trim_start_matches('0');
        Ok(if significant.is_empty() {
            0
        } else {
            significant.parse::<u32>().unwrap_or(u32::MAX)
        })
    };
    let (a, b) = (number(parts[0])?, number(parts[1])?);
    Ok(a == 10
        || a == 127
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 169 && b == 254))
}
pub fn leaks(observation: &Value, exit_ip: Option<&str>) -> io::Result<Value> {
    let ips = observation
        .get("ips")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut exposed = Vec::new();
    for ip in &ips {
        let ip = ip
            .as_str()
            .ok_or_else(|| io::Error::other("WebRTC address is not text"))?;
        if !private(ip)? && exit_ip.is_none_or(|exit| exit.is_empty() || exit != ip) {
            exposed.push(ip.to_owned());
        }
    }
    let detail = if exposed.is_empty() {
        "no public address exposed".into()
    } else {
        exposed.join(", ")
    };
    Ok(
        json!({"webrtcIps":ips,"exposed":exposed,"checks":[check("WebRTC does not leak your real IP",exposed.is_empty(),detail)]}),
    )
}
pub(super) fn zone_matches(left: &str, right: &str) -> io::Result<bool> {
    let (left, right) = (left.trim().to_lowercase(), right.trim().to_lowercase());
    if left.is_empty() || right.is_empty() {
        return Ok(false);
    }
    Ok(left == right
        || catalog().map_err(io::Error::other)?["catalog"]["TZ_ALIASES"][&left]
            .as_str()
            .is_some_and(|alias| alias.to_lowercase() == right))
}
pub fn proxy(
    profile: &Profile,
    info: &Value,
    leaks: &Value,
    dns: &Value,
    tls: &Value,
    latency: u64,
) -> io::Result<Value> {
    let mut checks = Vec::new();
    let proxy = profile.proxy.as_ref().map(|proxy| proxy.server.as_str());
    if truth(info) {
        let ip = info["ip"]
            .as_str()
            .ok_or_else(|| io::Error::other("Exit IP is not text"))?;
        let city = if truth(&info["city"]) {
            format!(" · {}", value_text(&info["city"]))
        } else {
            String::new()
        };
        checks.push(check(
            "Traffic reaches the internet",
            true,
            format!("exit IP {ip}{city}"),
        ));
        let want = profile
            .proxy
            .as_ref()
            .and_then(|proxy| proxy.country.as_deref())
            .unwrap_or("")
            .to_uppercase();
        let got = info["country"].as_str().unwrap_or("").to_uppercase();
        if !want.is_empty() {
            checks.push(check(
                "Exit country matches the proxy setting",
                got == want,
                format!(
                    "expected {want}, got {}",
                    if got.is_empty() { "unknown" } else { &got }
                ),
            ));
        } else if !got.is_empty() {
            checks.push(check("Exit country", true, got.clone()));
        }
        if truth(&info["timezone"]) {
            let zone = info["timezone"]
                .as_str()
                .ok_or_else(|| io::Error::other("Exit timezone is not text"))?;
            checks.push(check(
                "Timezone matches the exit location",
                zone_matches(zone, &profile.fingerprint.timezone)?,
                format!(
                    "profile says {}, the IP looks like {zone}",
                    profile.fingerprint.timezone
                ),
            ));
        }
        append(&mut checks, leaks, dns, tls)?;
        return Ok(
            json!({"ok":checks.iter().all(|check| check["ok"] == true),"proxy":proxy,"ip":ip,
            "country":if got.is_empty() {Value::Null} else {got.into()},"city":info["city"],"timezone":info["timezone"],
            "latencyMs":latency,"dnsCountries":dns["countries"],"ja3Hash":tls["ja3Hash"],"ja4":tls["ja4"],"checks":checks}),
        );
    }
    append(&mut checks, leaks, dns, tls)?;
    Ok(
        json!({"ok":false,"error":"no IP service answered — is the proxy reachable?","proxy":proxy,"latencyMs":latency,"checks":checks}),
    )
}
fn append(checks: &mut Vec<Value>, leaks: &Value, dns: &Value, tls: &Value) -> io::Result<()> {
    for result in [leaks, dns, tls] {
        checks.extend(
            result["checks"]
                .as_array()
                .ok_or_else(|| io::Error::other("Network probe checks missing"))?
                .iter()
                .cloned(),
        );
    }
    Ok(())
}
