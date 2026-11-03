//! Direct, validated and per-hop pinned provider control-plane requests.
use super::{builder, dns::SystemDns, failure, proxy_check, public_ip};
use crate::conversion::truth;
use reqwest::{
    Url,
    header::{LOCATION, USER_AGENT},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io,
    net::{IpAddr, SocketAddr},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn parse(text: &str) -> io::Result<Url> {
    let scheme = text.split_once(':').map_or("", |(scheme, _)| scheme);
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("the change-IP URL must be http(s), got '{}'", scheme),
        ));
    }
    if !text
        .split_once(':')
        .and_then(|(_, tail)| tail.strip_prefix("//"))
        .is_some_and(|authority| {
            !authority
                .split(['/', '?', '#'])
                .next()
                .unwrap_or("")
                .is_empty()
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the change-IP URL has no host",
        ));
    }
    Url::parse(text).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the change-IP URL is malformed",
        )
    })
}

fn pin(url: &Url, addresses: &[IpAddr]) -> io::Result<IpAddr> {
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    if addresses.is_empty() {
        return Err(io::Error::other(format!("{host} does not resolve")));
    }
    for address in addresses {
        if !public_ip::is_global(*address) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "{host} resolves to {address}, which is not a public address - a change-IP URL points at your provider, not at this machine or your network"
                ),
            ));
        }
    }
    Ok(addresses[0])
}

async fn destination(text: &str, dns: &SystemDns) -> io::Result<(Url, IpAddr)> {
    let url = parse(text)?;
    let host = url.host_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "the change-IP URL has no host")
    })?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let addresses = match host.parse::<IpAddr>() {
        Ok(address) => vec![address],
        Err(_) => dns.lookup(host).await?,
    };
    let address = pin(&url, &addresses)?;
    Ok((url, address))
}

fn cooldown(proxy: &Value, now: f64) -> io::Result<Option<String>> {
    let number = |key: &str| -> io::Result<f64> {
        let value = &proxy[key];
        if !truth(value) {
            return Ok(0.0);
        }
        if let Some(number) = value.as_f64() {
            return Ok(number);
        }
        if let Some(text) = value.as_str() {
            return text.parse().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Stored rotation timestamp is invalid",
                )
            });
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Stored rotation timestamp is invalid",
        ))
    };
    let since = now - number("rotateTriedAt")?.max(number("rotatedAt")?);
    Ok((since < 20.0).then(|| format!(
        "rotated {}s ago; most providers rate-limit this and would hand back the same IP. Wait {}s.",
        since.trunc(), (20.0 - since).trunc())))
}

fn verified(mut result: Value, previous: Value, body: &str) -> Value {
    let after = result["ip"].clone();
    let moved = truth(&after) && (!truth(&previous) || after != previous);
    result["attempted"] = true.into();
    result["rotated"] = moved.into();
    result["previousIp"] = previous.clone();
    if !moved && truth(&after) && after == previous {
        let ip = crate::conversion::value_text(&after);
        let reply = if body.is_empty() {
            String::new()
        } else {
            format!(
                " It replied: {}",
                body.chars().take(120).collect::<String>()
            )
        };
        result["error"] = format!("the endpoint answered but the exit IP is still {ip} - the provider may rotate on a timer, or the URL may be wrong.{reply}").into();
    }
    result
}

async fn control(
    url: Url,
    address: IpAddr,
    dns: &SystemDns,
    budget: Duration,
) -> io::Result<String> {
    control_with(url, address, budget, |url| async move {
        destination(url.as_str(), dns).await
    })
    .await
}

pub(crate) async fn control_with<F, Fut>(
    url: Url,
    address: IpAddr,
    budget: Duration,
    mut resolve: F,
) -> io::Result<String>
where
    F: FnMut(Url) -> Fut,
    Fut: std::future::Future<Output = io::Result<(Url, IpAddr)>>,
{
    tokio::time::timeout(budget, async {
        let (mut url, mut address) = (url, address);
        let mut visited = HashMap::<String, usize>::new();
        loop {
            let host = url
                .host_str()
                .ok_or_else(|| io::Error::other("Control-plane host missing"))?;
            let client = builder(budget)?
                .redirect(reqwest::redirect::Policy::none())
                .resolve(host, SocketAddr::new(address, 0))
                .build()
                .map_err(failure)?;
            let mut response = client
                .get(url.clone())
                .header(USER_AGENT, "numera-proxy-rotate/1.0")
                .send()
                .await
                .map_err(failure)?;
            if matches!(response.status().as_u16(), 301 | 302 | 303 | 307 | 308) {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .or_else(|| response.headers().get("uri"))
                    .ok_or_else(|| io::Error::other("Control-plane redirect has no destination"))?;
                let text = location
                    .to_str()
                    .map_err(|_| io::Error::other("Control-plane redirect is invalid"))?;
                let next = url
                    .join(text)
                    .map_err(|_| io::Error::other("Control-plane redirect is invalid"))?;
                let selected = resolve(next).await.map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!("refused to follow a redirect: {error}"),
                    )
                })?;
                let count = visited.get(selected.0.as_str()).copied().unwrap_or(0);
                if count >= 4 || visited.len() >= 10 {
                    return Err(io::Error::other("Control-plane redirect loop refused"));
                }
                visited.insert(selected.0.as_str().into(), count + 1);
                (url, address) = selected;
                continue;
            }
            response = response.error_for_status().map_err(failure)?;
            if !response.status().is_success() {
                return Err(io::Error::other(format!(
                    "Control-plane HTTP status {}",
                    response.status().as_u16()
                )));
            }
            let mut body = Vec::with_capacity(500);
            while body.len() < 500 {
                let Some(chunk) = response.chunk().await.map_err(failure)? else {
                    break;
                };
                body.extend_from_slice(&chunk[..chunk.len().min(500 - body.len())]);
            }
            return Ok(String::from_utf8_lossy(&body).trim().into());
        }
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Control-plane deadline exceeded"))?
}

pub async fn rotate(proxy: &Value, budget: Duration) -> Value {
    let text = proxy["rotateUrl"].as_str().unwrap_or("").trim();
    let refused = |error: String| json!({"ok":null,"rotated":false,"error":error});
    if text.is_empty() {
        return refused("no change-IP URL is set for this proxy".into());
    }
    let dns = match SystemDns::new(budget) {
        Ok(dns) => dns,
        Err(error) => return refused(error.to_string()),
    };
    let (url, address) = match destination(text, &dns).await {
        Ok(destination) => destination,
        Err(error) => return refused(error.to_string()),
    };
    let now = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(now) => now.as_secs_f64(),
        Err(_) => return refused("System clock precedes Unix epoch".into()),
    };
    match cooldown(proxy, now) {
        Ok(Some(error)) => return refused(error),
        Err(error) => return refused(error.to_string()),
        Ok(None) => {}
    }
    let body = match control(url, address, &dns, budget).await {
        Ok(body) => body,
        Err(error) => {
            return json!({"ok":false,"rotated":false,"attempted":true,
            "error":error.to_string().lines().next().unwrap_or("").chars().take(180).collect::<String>()});
        }
    };
    verified(
        proxy_check::check(proxy, budget).await,
        proxy["lastCheck"]["ip"].clone(),
        &body,
    )
}

pub fn rotate_sync(proxy: &Value, budget: Duration) -> io::Result<Value> {
    let run = || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(runtime.block_on(rotate(proxy, budget)))
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            scope
                .spawn(run)
                .join()
                .map_err(|_| io::Error::other("Native rotation worker panicked"))?
        })
    } else {
        run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rotation_requires_all_public_answers_cooldown_and_actual_exit_move() -> io::Result<()> {
        let url = parse("https://provider.test/rotate")?;
        let public = "8.8.8.8".parse::<IpAddr>().map_err(io::Error::other)?;
        let private = "127.0.0.1".parse::<IpAddr>().map_err(io::Error::other)?;
        assert_eq!(pin(&url, &[public])?, public);
        assert!(pin(&url, &[public, private]).is_err());
        assert!(pin(&url, &[]).is_err());
        for malformed in [
            "http:provider.test",
            "http:///rotate",
            "ftp://provider.test/rotate",
            "https://provider.test:99999/",
        ] {
            assert!(parse(malformed).is_err(), "Accepted {malformed}");
        }
        assert_eq!(cooldown(&json!({"rotatedAt":998}), 1000.0)?, Some("rotated 2s ago; most providers rate-limit this and would hand back the same IP. Wait 18s.".into()));
        assert_eq!(cooldown(&json!({"rotatedAt":980}), 1000.0)?, None);
        assert_eq!(cooldown(&json!({"rotatedAt":900,"rotateTriedAt":995}), 1000.0)?, Some("rotated 5s ago; most providers rate-limit this and would hand back the same IP. Wait 15s.".into()));
        assert_eq!(
            verified(json!({"ok":true,"ip":"9.9.9.9"}), json!("8.8.8.8"), "OK"),
            json!({"ok":true,"ip":"9.9.9.9","rotated":true,"attempted":true,"previousIp":"8.8.8.8"})
        );
        assert_eq!(
            verified(json!({"ok":true,"ip":"9.9.9.9"}), Value::Null, "OK")["rotated"],
            true
        );
        assert_eq!(
            verified(
                json!({"ok":true,"ip":"8.8.8.8"}),
                json!("8.8.8.8"),
                "queued"
            ),
            json!({"ok":true,"ip":"8.8.8.8","rotated":false,"attempted":true,"previousIp":"8.8.8.8",
                "error":"the endpoint answered but the exit IP is still 8.8.8.8 - the provider may rotate on a timer, or the URL may be wrong. It replied: queued"})
        );
        assert_eq!(
            verified(
                json!({"ok":false,"error":"fixture refusal"}),
                json!("8.8.8.8"),
                "OK"
            )["rotated"],
            false
        );
        Ok(())
    }
}
