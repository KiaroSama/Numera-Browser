// SPDX-License-Identifier: MPL-2.0 AND MIT
// Native port of Camoufox locales.py, geolocation.py, ip.py and the geo/locale
// launch_options branches at 079a6ce8249bab98063e5ac9241e3879a3e6578d.
// Provisioning and refresh belong to the native resource owner; this reads only.
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet},
    io,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

#[derive(Deserialize)]
struct Record {
    #[serde(rename = "Subtag")]
    subtag: Option<String>,
    #[serde(rename = "Deprecated")]
    deprecated: Option<String>,
    #[serde(rename = "Suppress-Script")]
    script: Option<String>,
}
#[derive(Deserialize)]
struct Territory {
    region: String,
    population: f64,
    literacy: f64,
    languages: Vec<(String, f64)>,
}
#[derive(Deserialize)]
struct Catalog {
    subtags: HashMap<String, HashMap<String, Record>>,
    territories: Vec<Territory>,
}
fn catalog() -> &'static Catalog {
    static DATA: OnceLock<Catalog> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("camoufox-data/locale-data.json"))
            .expect("Compiled locale data is valid")
    })
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn malformed(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn random() -> io::Result<f64> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(|_| io::Error::other("Locale entropy unavailable"))?;
    Ok((u64::from_le_bytes(bytes) >> 11) as f64 / 9_007_199_254_740_992.0)
}
#[derive(Default)]
struct Locale {
    language: String,
    region: Option<String>,
    script: Option<String>,
}
impl Locale {
    fn as_string(&self) -> String {
        match &self.region {
            Some(region) => format!("{}-{region}", self.language),
            None => self.language.clone(),
        }
    }
    fn as_config(&self) -> io::Result<Map<String, Value>> {
        let region = self
            .region
            .as_ref()
            .ok_or_else(|| invalid("Locale requires a region"))?;
        let mut result = Map::from_iter([
            ("locale:language".into(), self.language.clone().into()),
            ("locale:region".into(), region.clone().into()),
        ]);
        if let Some(script) = &self.script {
            result.insert("locale:script".into(), script.clone().into());
        }
        Ok(result)
    }
}
// language-tags validates registered subtags, deprecation, multiplicity and
// order. Its singleton handling and redundant-tag validation are retained.
fn parse_locale(input: &str, require_region: bool) -> io::Result<Locale> {
    let tag = input.trim().to_ascii_lowercase();
    let data = &catalog().subtags;
    let special = data.get(&tag).and_then(|types| {
        types
            .get("grandfathered")
            .or_else(|| types.get("redundant"))
    });
    if special.is_some_and(|record| record.deprecated.is_some()) {
        return Err(invalid("Locale is deprecated"));
    }
    if data
        .get(&tag)
        .is_some_and(|types| types.contains_key("grandfathered"))
    {
        return Err(invalid("Locale has no language and region subtags"));
    }
    let mut result = Locale::default();
    let mut previous = 0;
    let mut counts = [0; 5];
    let codes: Vec<_> = tag.split('-').collect();
    for (index, code) in codes.iter().enumerate() {
        if code.len() < 2 {
            if special.is_none() && codes[index + 1..].iter().any(|tail| tail.len() > 8) {
                return Err(invalid("Locale extension is too long"));
            }
            break;
        }
        let Some(types) = data.get(*code) else {
            if special.is_none() {
                return Err(invalid("Locale contains an unknown subtag"));
            }
            continue;
        };
        let kind = if index == 0 && types.contains_key("language") {
            Some(("language", 0))
        } else {
            match code.len() {
                2 => {
                    if types.contains_key("region") {
                        Some(("region", 3))
                    } else if types.contains_key("language") {
                        Some(("language", 0))
                    } else {
                        None
                    }
                }
                3 => {
                    if types.contains_key("region") {
                        Some(("region", 3))
                    } else if types.contains_key("extlang") {
                        Some(("extlang", 1))
                    } else if types.contains_key("language") {
                        Some(("language", 0))
                    } else {
                        None
                    }
                }
                4 => {
                    if types.contains_key("variant") {
                        Some(("variant", 4))
                    } else if types.contains_key("script") {
                        Some(("script", 2))
                    } else {
                        None
                    }
                }
                _ => {
                    if types.contains_key("variant") {
                        Some(("variant", 4))
                    } else {
                        None
                    }
                }
            }
        };
        let Some((kind, priority)) = kind else {
            continue;
        };
        let record = &types[kind];
        counts[priority] += 1;
        if special.is_none()
            && (record.deprecated.is_some()
                || counts[priority] > 1
                || priority < previous
                || (result.language.is_empty() && kind != "language"))
        {
            return Err(invalid("Locale subtags are invalid or out of order"));
        }
        previous = priority;
        let subtag = record
            .subtag
            .as_deref()
            .ok_or_else(|| invalid("Locale subtag is invalid"))?;
        match kind {
            "language" if result.language.is_empty() => {
                result.language = subtag.into();
                result.script = record.script.clone();
            }
            "region" if result.region.is_none() => result.region = Some(subtag.into()),
            "script" if special.is_none() && result.script.as_deref() == Some(subtag) => {
                return Err(invalid("Locale contains its suppressed script"));
            }
            _ => {}
        }
    }
    if result.language.is_empty() || (require_region && result.region.is_none()) {
        return Err(invalid("Locale requires a registered language and region"));
    }
    Ok(result)
}
fn choose(rows: &[(String, f64)], roll: f64) -> io::Result<&str> {
    let total: f64 = rows.iter().map(|(_, w)| *w).sum();
    if !total.is_finite() || total <= 0.0 || rows.is_empty() {
        return Err(invalid("Locale population data is empty"));
    }
    let mut cumulative = 0.0;
    for (value, weight) in rows {
        cumulative += weight / total;
        if roll < cumulative {
            return Ok(value);
        }
    }
    Ok(&rows
        .last()
        .ok_or_else(|| invalid("Locale population data is empty"))?
        .0)
}
fn from_region(region: &str, rng: &mut impl FnMut() -> io::Result<f64>) -> io::Result<Locale> {
    let territory = catalog()
        .territories
        .iter()
        .find(|t| t.region == region)
        .ok_or_else(|| invalid("Unknown locale territory"))?;
    let language = choose(&territory.languages, rng()?)?.replace('_', "-");
    parse_locale(&format!("{language}-{region}"), true)
}
fn handle_locale(
    input: &str,
    ignore_region: bool,
    rng: &mut impl FnMut() -> io::Result<f64>,
) -> io::Result<Locale> {
    if input.chars().count() > 3 {
        return parse_locale(input, true);
    }
    if catalog().territories.iter().any(|t| t.region == input) {
        return from_region(input, rng);
    }
    if ignore_region {
        parse_locale(input, false)?;
        return Ok(Locale {
            language: input.into(),
            ..Locale::default()
        });
    }
    let rows: Vec<_> = catalog()
        .territories
        .iter()
        .filter_map(|t| {
            t.languages
                .iter()
                .find(|(language, _)| language == input)
                .map(|(_, percent)| {
                    (
                        t.region.clone(),
                        percent * t.literacy / 10_000.0 * t.population,
                    )
                })
        })
        .collect();
    let region = choose(&rows, rng()?)?;
    parse_locale(&format!("{input}-{region}"), true)
}
fn localize(
    locales: &Value,
    rng: &mut impl FnMut() -> io::Result<f64>,
) -> io::Result<Map<String, Value>> {
    let values: Vec<&str> = match locales {
        Value::String(text) => text.split(',').map(str::trim).collect(),
        Value::Array(items) => items
            .iter()
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| invalid("Locale list must contain text"))
            })
            .collect::<io::Result<_>>()?,
        _ => return Err(invalid("Locale must be text or a list")),
    };
    let first = values
        .first()
        .ok_or_else(|| invalid("Locale list is empty"))?;
    let mut result = handle_locale(first, false, rng)?.as_config()?;
    if values.len() > 1 {
        let mut seen = HashSet::new();
        let mut unique = Vec::new();
        for value in values {
            let value = handle_locale(value, true, rng)?.as_string();
            if seen.insert(value.clone()) {
                unique.push(value);
            }
        }
        result.insert("locale:all".into(), unique.join(", ").into());
    }
    Ok(result)
}
/// Deterministic weighted draw for parity validation and reproducible callers.
pub fn locale_config_at(locales: &Value, roll: f64) -> io::Result<Map<String, Value>> {
    if !roll.is_finite() || !(0.0..1.0).contains(&roll) {
        return Err(invalid("Locale draw must be in [0, 1)"));
    }
    localize(locales, &mut || Ok(roll))
}
fn sources() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("camoufox-data/geoip-sources.json"))
            .expect("Compiled GeoIP sources are valid")
    })
}
fn named_source(name: Option<&str>) -> io::Result<Value> {
    let name = name.unwrap_or_else(|| {
        sources()["default"]
            .as_str()
            .expect("Default GeoIP source is text")
    });
    sources()["geoip"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|r| {
                r["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
        })
        .cloned()
        .ok_or_else(|| invalid("Unknown GeoIP database source"))
}
/// Read-only resource selection shared by the SDK launcher and native callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourcePaths {
    pub root: PathBuf,
    pub databases: PathBuf,
    pub config: PathBuf,
}
pub fn resource_paths(directory: &Path) -> ResourcePaths {
    ResourcePaths {
        root: directory.to_owned(),
        databases: directory.join("mmdb"),
        config: directory.join("config.yml"),
    }
}
fn source_config(directory: &Path, name: Option<&str>) -> io::Result<Value> {
    if name.is_some() {
        return named_source(name);
    }
    let path = resource_paths(directory).config;
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return named_source(None),
        Err(e) => return Err(e),
    };
    let saved: Value =
        serde_yaml::from_slice(&bytes).map_err(|_| malformed("GeoIP config.yml is invalid"))?;
    if !saved.is_null() && !saved.is_object() {
        return Err(malformed("GeoIP config.yml must contain a mapping"));
    }
    if saved
        .get("name")
        .is_some_and(|name| !name.is_null() && !name.is_string())
    {
        return Err(malformed("GeoIP source name must be text"));
    }
    match named_source(saved.get("name").and_then(Value::as_str)) {
        Ok(config)
            if config["deprecated"].as_bool().unwrap_or(false)
                && !saved["explicit"].as_bool().unwrap_or(false) =>
        {
            named_source(None)
        }
        Ok(config) => Ok(config),
        Err(_) => Ok(saved),
    }
}
fn mmdb_path(directory: &Path, config: &Value, ip: IpAddr) -> io::Result<PathBuf> {
    let name = config
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("geolite2")
        .to_lowercase();
    if name.contains(['/', '\\']) || name.is_empty() || matches!(name.as_str(), "." | "..") {
        return Err(invalid("GeoIP source filename is invalid"));
    }
    let version = if config
        .get("urls")
        .and_then(Value::as_object)
        .is_some_and(|u| u.contains_key("combined"))
    {
        "combined"
    } else if ip.is_ipv6() {
        "ipv6"
    } else {
        "ipv4"
    };
    Ok(resource_paths(directory)
        .databases
        .join(format!("{name}-{version}.mmdb")))
}
fn find<'a>(data: &'a Value, config: &Value, name: &str) -> io::Result<&'a Value> {
    let path = config["paths"][name]
        .as_str()
        .ok_or_else(|| malformed("GeoIP source lacks a required field path"))?;
    let mut value = data;
    for part in path.split('.') {
        value = value
            .get(part)
            .ok_or_else(|| malformed("GeoIP record lacks a required field"))?;
    }
    Ok(value)
}
fn number(value: &Value) -> io::Result<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .filter(|n| n.is_finite())
        .ok_or_else(|| malformed("GeoIP coordinate is not a finite number"))
}
fn geolocate(
    directory: &Path,
    name: Option<&str>,
    ip: IpAddr,
    rng: &mut impl FnMut() -> io::Result<f64>,
) -> io::Result<Map<String, Value>> {
    let config = source_config(directory, name)?;
    let path = mmdb_path(directory, &config, ip)?;
    let bytes = std::fs::read(path)
        .map_err(|e| io::Error::new(e.kind(), "Provisioned GeoIP database is unavailable"))?;
    let reader = maxminddb::Reader::from_source(bytes)
        .map_err(|_| malformed("Provisioned GeoIP database is invalid"))?;
    let record = reader
        .lookup(ip)
        .map_err(|_| malformed("GeoIP lookup failed"))?
        .decode::<Value>()
        .map_err(|_| malformed("GeoIP record is invalid"))?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "IP address not found in GeoIP database",
            )
        })?;
    let region = find(&record, &config, "iso_code")?
        .as_str()
        .ok_or_else(|| malformed("GeoIP region is not text"))?
        .to_uppercase();
    let longitude = number(find(&record, &config, "longitude")?)?;
    let latitude = number(find(&record, &config, "latitude")?)?;
    let timezone = find(&record, &config, "timezone")?
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| malformed("GeoIP timezone is not text"))?;
    let mut result = from_region(&region, rng)?.as_config()?;
    result.extend(Map::from_iter([
        ("geolocation:longitude".into(), json!(longitude)),
        ("geolocation:latitude".into(), json!(latitude)),
        ("timezone".into(), timezone.into()),
    ]));
    Ok(result)
}
/// Read a provisioned database without changing the active source or filesystem.
pub fn geolocation_at(
    directory: &Path,
    source: Option<&str>,
    ip: &str,
    roll: f64,
) -> io::Result<Map<String, Value>> {
    if !roll.is_finite() || !(0.0..1.0).contains(&roll) {
        return Err(invalid("Locale draw must be in [0, 1)"));
    }
    let ip = ip.parse().map_err(|_| invalid("Invalid GeoIP address"))?;
    geolocate(directory, source, ip, &mut || Ok(roll))
}
const ECHO: [&str; 6] = [
    "https://api.ipify.org",
    "https://checkip.amazonaws.com",
    "https://ipinfo.io/ip",
    "https://icanhazip.com",
    "https://ifconfig.co/ip",
    "https://ipecho.net/plain",
];
const IPV6_ECHO: [&str; 2] = ["https://api6.ipify.org", "https://v6.ident.me"];
fn echo_client(proxy: Option<&Value>) -> io::Result<reqwest::Client> {
    let mut builder =
        crate::network::builder(Duration::from_secs(if proxy.is_some() { 15 } else { 5 }))?;
    if let Some(proxy) = proxy {
        let server = proxy["server"]
            .as_str()
            .ok_or_else(|| invalid("GeoIP proxy server is not text"))?;
        let server = if server.contains("://") {
            server.to_owned()
        } else {
            format!("http://{server}")
        };
        let mut transport =
            reqwest::Proxy::all(server).map_err(|_| invalid("GeoIP proxy server is invalid"))?;
        if let Some(user) = proxy["username"].as_str().filter(|s| !s.is_empty()) {
            transport = transport.basic_auth(user, proxy["password"].as_str().unwrap_or(""));
        }
        builder = builder.proxy(transport);
    }
    builder
        .build()
        .map_err(|_| io::Error::other("GeoIP HTTP client could not be created"))
}
async fn echo(client: &reqwest::Client, endpoints: &[&str], ipv6_only: bool) -> Option<IpAddr> {
    for endpoint in endpoints {
        let Ok(url) = reqwest::Url::parse(endpoint) else {
            continue;
        };
        if let Ok(bytes) =
            crate::network::get(client, url, reqwest::header::HeaderMap::new(), 1024).await
            && let Ok(text) = std::str::from_utf8(&bytes)
            && let Ok(ip) = text.trim().parse::<IpAddr>()
            && (!ipv6_only || ip.is_ipv6())
        {
            return Some(ip);
        }
    }
    None
}
async fn exit_ip(proxy: Option<&Value>) -> io::Result<IpAddr> {
    let mut relay = None;
    if let Some(proxy) = proxy {
        let server = proxy["server"]
            .as_str()
            .ok_or_else(|| invalid("GeoIP proxy server is not text"))?;
        let server = if server.contains("://") {
            server.to_owned()
        } else {
            format!("http://{server}")
        };
        let mut url =
            reqwest::Url::parse(&server).map_err(|_| invalid("GeoIP proxy server is invalid"))?;
        // Reqwest's SOCKS4 connector does not send USERID. The native relay
        // implements the source protocol and also keeps DNS behind the proxy.
        if matches!(url.scheme(), "socks4" | "socks4a") {
            if url.port().is_none() {
                url.set_port(Some(1080))
                    .map_err(|_| invalid("GeoIP proxy port is invalid"))?;
            }
            let mut record = proxy
                .as_object()
                .cloned()
                .ok_or_else(|| invalid("GeoIP proxy must be an object"))?;
            if record.get("username").is_none_or(Value::is_null) && !url.username().is_empty() {
                record.insert(
                    "username".into(),
                    percent_encoding::percent_decode_str(url.username())
                        .decode_utf8_lossy()
                        .into_owned()
                        .into(),
                );
            }
            if record.get("password").is_none_or(Value::is_null)
                && let Some(password) = url.password()
            {
                record.insert(
                    "password".into(),
                    percent_encoding::percent_decode_str(password)
                        .decode_utf8_lossy()
                        .into_owned()
                        .into(),
                );
            }
            record.insert("server".into(), url.to_string().into());
            let model = crate::proxy::Proxy::from_record(&Value::Object(record), false)
                .map_err(|_| invalid("GeoIP proxy credentials are invalid"))?;
            relay = Some(
                crate::network::relay::Relay::start(&model).map_err(|error| {
                    io::Error::new(error.kind(), "GeoIP proxy relay could not start")
                })?,
            );
        }
    }
    let local = relay.as_ref().map(|relay| json!({"server":relay.url()}));
    let result = async {
        echo(&echo_client(local.as_ref().or(proxy))?, &ECHO, false)
            .await
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::AddrNotAvailable,
                    "Could not determine the browser exit IP address",
                )
            })
    }
    .await;
    if let Some(mut relay) = relay {
        relay
            .close(Duration::from_secs(5))
            .map_err(|error| io::Error::new(error.kind(), "GeoIP proxy relay did not close"))?;
    }
    result
}
/// Resolve the source launcher's locale and GeoIP config. No resource writes or
/// downloads occur; a failed proxy exit lookup never tries the host connection.
pub async fn resolve(options: &Map<String, Value>) -> io::Result<Map<String, Value>> {
    let directory = std::env::var_os("NUMERA_ADAPTER_GEOIP_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    resolve_in(options, directory.as_deref()).await
}
/// Resolve against a resource directory supplied by the native owner.
pub async fn resolve_in(
    options: &Map<String, Value>,
    directory: Option<&Path>,
) -> io::Result<Map<String, Value>> {
    let mut config = options
        .get("config")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let requested = options
        .get("geoip")
        .filter(|v| !v.is_null() && **v != Value::Bool(false) && v.as_str() != Some(""));
    if let Some(requested) = requested {
        let directory = directory.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Native GeoIP resource directory is not configured",
            )
        })?;
        let source = options
            .get("geoip_db")
            .filter(|v| !v.is_null())
            .map(|v| {
                v.as_str()
                    .ok_or_else(|| invalid("GeoIP database source must be text"))
            })
            .transpose()?;
        let proxy = options.get("proxy").filter(|value| !value.is_null());
        if proxy.is_some_and(|value| !value.is_object()) {
            return Err(invalid("GeoIP proxy must be an object"));
        }
        let proxy =
            proxy.filter(|value| value.as_object().is_some_and(|object| !object.is_empty()));
        let ip = match requested {
            Value::String(ip) => ip.parse().map_err(|_| invalid("Invalid GeoIP address"))?,
            Value::Bool(true) => exit_ip(proxy).await?,
            _ => return Err(invalid("GeoIP must be an IP address or boolean")),
        };
        if !options
            .get("block_webrtc")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            config
                .entry(
                    if ip.is_ipv4() {
                        "webrtc:ipv4"
                    } else {
                        "webrtc:ipv6"
                    }
                    .to_owned(),
                )
                .or_insert_with(|| ip.to_string().into());
        }
        let geo = geolocate(directory, source, ip, &mut random)?;
        for (key, value) in &geo {
            if matches!(
                key.as_str(),
                "timezone" | "locale:language" | "locale:region" | "locale:script"
            ) {
                config.entry(key.clone()).or_insert(value.clone());
            } else {
                config.insert(key.clone(), value.clone());
            }
        }
        if proxy.is_none() && ip.is_ipv4() {
            let kept = if let Some(ipv6) = echo(&echo_client(None)?, &IPV6_ECHO, true).await {
                match geolocate(directory, source, ipv6, &mut random) {
                    Ok(v6) => ["timezone", "locale:region"]
                        .iter()
                        .all(|key| v6.get(*key) == geo.get(*key)),
                    Err(error) if error.kind() == io::ErrorKind::AddrNotAvailable => false,
                    Err(error) => return Err(error),
                }
            } else {
                false
            };
            if !kept {
                config.insert("network:ipv4Only".into(), true.into());
            }
        }
    }
    if let Some(locales) = options
        .get("locale")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        config.extend(localize(locales, &mut random)?);
    }
    Ok(config)
}
/// Merge these as defaults into caller Firefox preferences, after resolution.
pub fn firefox_preferences(config: &Map<String, Value>) -> Map<String, Value> {
    let mut prefs = Map::new();
    if config.contains_key("geolocation:latitude") && config.contains_key("geolocation:longitude") {
        prefs.insert("permissions.default.geo".into(), 1.into());
    }
    let requested = if config
        .get("locale:language")
        .and_then(Value::as_str)
        .is_some_and(|language| !language.is_empty())
    {
        ["locale:language", "locale:script", "locale:region"]
            .iter()
            .filter_map(|key| {
                config
                    .get(*key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
            })
            .collect::<Vec<_>>()
            .join("-")
    } else {
        "en-US".into()
    };
    prefs.insert("intl.locale.requested".into(), requested.into());
    prefs
}
