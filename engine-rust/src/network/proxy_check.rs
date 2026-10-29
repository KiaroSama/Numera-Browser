use super::{builder, failure, get};
use crate::{
    conversion::{truth, value_text},
    pool,
};
use reqwest::{
    Client, Url,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};
use serde_json::{Value, json};
use std::{
    io,
    time::{Duration, Instant},
};

const ECHO: [&str; 3] = [
    "https://ipapi.co/json/",
    "https://ipwho.is/",
    "https://api.ipify.org/?format=json",
];

fn proxy_client(proxy: &Value, budget: Duration) -> io::Result<Client> {
    let kind = proxy
        .get("type")
        .map(value_text)
        .unwrap_or_else(|| "HTTP".into())
        .to_uppercase();
    let user = proxy["user"].as_str().filter(|user| !user.is_empty());
    // Reqwest SOCKS4 does not carry USERID. Do not silently discard credentials.
    if kind == "SOCKS4" && user.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Authenticated SOCKS4 requires the native proxy relay",
        ));
    }
    let host = proxy["host"]
        .as_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Proxy host is not text"))?;
    let port = value_text(&proxy["port"]);
    let scheme = match kind.as_str() {
        "SOCKS4" => "socks4a",
        "SOCKS5" => "socks5h",
        // Original urllib check uses HTTP transport for both pool HTTP kinds.
        "HTTP" | "HTTPS" | "" => "http",
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsupported proxy transport",
            ));
        }
    };
    let address =
        pool::server_url("HTTP", host, &port)?.replacen("http://", &format!("{scheme}://"), 1);
    let mut transport = reqwest::Proxy::all(address).map_err(failure)?;
    if let Some(user) = user {
        transport = transport.basic_auth(user, proxy["pass"].as_str().unwrap_or(""));
    }
    builder(budget)?.proxy(transport).build().map_err(failure)
}

fn decode(bytes: &[u8]) -> Option<Value> {
    let text = String::from_utf8_lossy(bytes);
    let text = pool::trim(&text);
    let data: Value = serde_json::from_str(text).unwrap_or_else(|_| json!({"ip":text}));
    let object = data.as_object()?;
    let choose = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| object.get(*key).filter(|value| truth(value)))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let ip = choose(&["ip", "query", "IPv4"]);
    if !truth(&ip) {
        return None;
    }
    Some(
        json!({"ok":true,"ip":ip,"country":choose(&["country_code", "country", "countryCode"]),"city":object.get("city").unwrap_or(&Value::Null)}),
    )
}

pub async fn check(proxy: &Value, budget: Duration) -> Value {
    check_at(proxy, budget, &ECHO).await
}

pub(crate) async fn check_at(proxy: &Value, budget: Duration, endpoints: &[&str]) -> Value {
    let started = Instant::now();
    let relay = if proxy["type"]
        .as_str()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("SOCKS4"))
    {
        let model = crate::proxy::Proxy::from_record(
            &json!({
                "server":match pool::server_url("SOCKS4",proxy["host"].as_str().unwrap_or(""), &value_text(&proxy["port"])) {
                    Ok(server) => server,
                    Err(error) => return json!({"ok":false,"error":error.to_string()}),
                }, "username":proxy["user"],"password":proxy["pass"],"country":null
            }),
            false,
        );
        let model = match model {
            Ok(model) => model,
            Err(error) => return json!({"ok":false,"error":error.to_string()}),
        };
        match super::relay::Relay::start(&model) {
            Ok(relay) => Some(relay),
            Err(error) => return json!({"ok":false,"error":error.to_string()}),
        }
    } else {
        None
    };
    let selected = if let Some(relay) = &relay {
        let url = match Url::parse(&relay.url()) {
            Ok(url) => url,
            Err(_) => return json!({"ok":false,"error":"Native relay URL is invalid"}),
        };
        json!({"type":"HTTP","host":"127.0.0.1","port":url.port().unwrap_or(0)})
    } else {
        proxy.clone()
    };
    let result = check_client(&selected, budget, endpoints, started).await;
    if let Some(mut relay) = relay {
        let mut result = result;
        if !truth(&result["ok"])
            && let Some(error) = relay.last_error()
        {
            result["error"] = error.into();
        }
        if let Err(error) = relay.close(Duration::from_secs(5)) {
            return json!({"ok":false,"error":error.to_string()});
        }
        result
    } else {
        result
    }
}

async fn check_client(
    proxy: &Value,
    budget: Duration,
    endpoints: &[&str],
    started: Instant,
) -> Value {
    let client = match proxy_client(proxy, budget) {
        Ok(client) => client,
        Err(error) => return json!({"ok":false,"error":error.to_string()}),
    };
    let mut last = None;
    for endpoint in endpoints {
        let url = match Url::parse(endpoint) {
            Ok(url) => url,
            Err(_) => return json!({"ok":false,"error":"IP service URL is invalid"}),
        };
        let headers = HeaderMap::from_iter([(
            USER_AGENT,
            HeaderValue::from_static("numera-proxy-check/1.0"),
        )]);
        match get(&client, url, headers, 1024 * 1024).await {
            Ok(bytes) => {
                if let Some(mut result) = decode(&bytes) {
                    result["latencyMs"] = (started.elapsed().as_millis() as u64).into();
                    return result;
                }
            }
            Err(error) => {
                last = Some(
                    error
                        .to_string()
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(140)
                        .collect::<String>(),
                )
            }
        }
    }
    json!({"ok":false,"error":last.filter(|error| !error.is_empty()).unwrap_or_else(|| "no IP service answered".into())})
}

fn profile_entry(proxy: &crate::proxy::Proxy) -> io::Result<Value> {
    let server = if proxy.server.contains("://") {
        proxy.server.clone()
    } else {
        format!("http://{}", proxy.server)
    };
    let url = Url::parse(&server)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Profile proxy URL is invalid"))?;
    let kind = match url.scheme() {
        "socks" | "socks5" | "socks5h" => "SOCKS5",
        "socks4" => "SOCKS4",
        "https" => "HTTPS",
        _ => "HTTP",
    };
    let authority = server
        .split_once("://")
        .map_or("", |(_, tail)| tail)
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let explicit = if let Some(bracketed) = authority.strip_prefix('[') {
        bracketed
            .split_once(']')
            .and_then(|(_, tail)| tail.strip_prefix(':'))
    } else {
        authority.rsplit_once(':').map(|(_, port)| port)
    };
    let port = explicit
        .filter(|port| !port.is_empty())
        .map(str::parse::<u16>)
        .transpose()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Profile proxy port is invalid"))?
        .unwrap_or(0);
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    let record = proxy.to_record();
    Ok(json!({"type":kind,"host":host,"port":port,
        "user":record["username"],"pass":record["password"]}))
}

pub fn exit_country(proxy: &crate::proxy::Proxy) -> io::Result<Option<String>> {
    let result = check_sync(&profile_entry(proxy)?, Duration::from_secs(10))?;
    Ok(result["country"]
        .as_str()
        .filter(|country| !country.is_empty())
        .map(|country| country.to_uppercase().chars().take(2).collect()))
}

pub fn check_sync(proxy: &Value, budget: Duration) -> io::Result<Value> {
    let run = || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        Ok(runtime.block_on(check(proxy, budget)))
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        std::thread::scope(|scope| {
            scope
                .spawn(run)
                .join()
                .map_err(|_| io::Error::other("Native proxy worker panicked"))?
        })
    } else {
        run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profile_proxy_adaptation_retains_explicit_default_ports()
    -> Result<(), Box<dyn std::error::Error>> {
        for (server, kind, host, port) in [
            ("http://proxy.test:80", "HTTP", "proxy.test", 80),
            ("https://proxy.test:443", "HTTPS", "proxy.test", 443),
            ("proxy.test:8080", "HTTP", "proxy.test", 8080),
            ("http://proxy.test", "HTTP", "proxy.test", 0),
            ("http://[2001:db8::1]:80", "HTTP", "2001:db8::1", 80),
            ("socks5://proxy.test:1080", "SOCKS5", "proxy.test", 1080),
            ("socks4a://proxy.test:1080", "HTTP", "proxy.test", 1080),
        ] {
            let proxy = crate::proxy::Proxy::from_record(
                &json!({"server":server,"username":"user","password":"synthetic","country":null}),
                false,
            )?;
            assert_eq!(
                profile_entry(&proxy)?,
                json!({"type":kind,"host":host,"port":port,"user":"user","pass":"synthetic"})
            );
        }
        Ok(())
    }

    #[test]
    fn echo_policy_preserves_source_keys_truth_and_utf8_replacement() {
        assert_eq!(
            decode(br#"{"query":"192.0.2.1","countryCode":"DE","city":null}"#),
            Some(json!({"ok":true,"ip":"192.0.2.1","country":"DE","city":null}))
        );
        assert_eq!(
            decode(b" 192.0.2.2\n"),
            Some(json!({"ok":true,"ip":"192.0.2.2","country":null,"city":null}))
        );
        assert_eq!(decode(b"[]"), None);
        assert_eq!(decode(br#"{"ip":""}"#), None);
    }
}
