// SPDX-License-Identifier: MIT
//! Native launch preparation derived from CloakBrowser 0.7.2, upstream
//! 1a145dbb90a26d6dd223c7642cb49fb9c850f22c (config/browser/license/widevine).
//! The owner must provision the binary and driver. This module never acquires
//! resources, validates a license over HTTP, or reads/writes an implicit home.
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Read, Write},
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

const SEED_FILE: &str = ".cloakbrowser-seed";
const LICENSE_KEY: &str = "CLOAKBROWSER_LICENSE_KEY";
const STATUS_FILE: &str = "CLOAKBROWSER_LICENSE_STATUS_FILE";
const SEED_MIN: u32 = 10000;
const SEED_MAX: u32 = 99999;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn environment() -> BTreeMap<String, String> {
    std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .filter(|(key, _)| !matches!(key.as_str(), "NUMERA_PASSWORD" | "NUMERA_API_TOKEN"))
        .collect()
}
fn boolean(options: &Map<String, Value>, key: &str, default: bool) -> io::Result<bool> {
    options.get(key).map_or(Ok(default), |value| {
        value
            .as_bool()
            .ok_or_else(|| invalid("Cloak boolean option required"))
    })
}
fn string<'a>(options: &'a Map<String, Value>, key: &str) -> io::Result<Option<&'a str>> {
    match options.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(Some)
            .ok_or_else(|| invalid("Cloak string option required")),
    }
}
fn strings(options: &Map<String, Value>, key: &str) -> io::Result<Vec<String>> {
    match options.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid("Cloak argument must be a string"))
            })
            .collect(),
        _ => Err(invalid("Cloak argument array required")),
    }
}
fn nonce() -> io::Result<String> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
fn random_seed() -> io::Result<u32> {
    // Rejection sampling retains random.randint's inclusive, uniform range.
    let range = SEED_MAX - SEED_MIN + 1;
    let limit = u32::MAX - (u32::MAX % range);
    loop {
        let mut bytes = [0; 4];
        getrandom::fill(&mut bytes).map_err(io::Error::other)?;
        let value = u32::from_ne_bytes(bytes);
        if value < limit {
            return Ok(SEED_MIN + value % range);
        }
    }
}
fn no_links(path: &Path) -> io::Result<()> {
    let mut current = PathBuf::new();
    for part in path.components() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    metadata.file_attributes() & 0x400 != 0
                };
                #[cfg(not(windows))]
                let linked = metadata.file_type().is_symlink();
                if linked {
                    return Err(invalid("Cloak preparation refuses symbolic-link resources"));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
fn read_bounded(path: &Path, limit: usize) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut data)?;
    if data.len() > limit {
        return Err(invalid("Cloak resource exceeds size limit"));
    }
    Ok(data)
}
fn read_seed(path: &Path) -> io::Result<Option<u32>> {
    no_links(path)?;
    let read = || match read_bounded(path, 4096) {
        Ok(data) => Ok(Some(data)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) if error.kind() == io::ErrorKind::InvalidInput => Ok(Some(vec![b'x'])),
        Err(error) => Err(error),
    };
    let Some(mut bytes) = read()? else {
        return Ok(None);
    };
    // Exclusive-create fallback has a short empty-file window. Do not rotate
    // the identity while the first writer is still writing and syncing it.
    for _ in 0..20 {
        if !bytes.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
        let Some(data) = read()? else {
            return Ok(None);
        };
        bytes = data;
    }
    let data = bytes.trim_ascii();
    if data.iter().all(u8::is_ascii_digit)
        && !data.is_empty()
        && let Ok(text) = std::str::from_utf8(data)
        && let Ok(seed) = text.parse::<u32>()
        && (SEED_MIN..=SEED_MAX).contains(&seed)
    {
        return Ok(Some(seed));
    }
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(None)
}
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()
}
/// Persist one shared-wrapper-compatible decimal seed using an atomic,
/// non-replacing hard link, or synced exclusive create on filesystems without it.
fn persistent_seed(profile: &Path, stealth: bool, args: &[String]) -> io::Result<Vec<String>> {
    if !stealth
        || profile.as_os_str().is_empty()
        || args
            .iter()
            .any(|arg| arg.split('=').next() == Some("--fingerprint"))
    {
        return Ok(args.to_vec());
    }
    let path = profile.join(SEED_FILE);
    let seed = match read_seed(&path)? {
        Some(seed) => seed,
        None => {
            no_links(profile)?;
            fs::create_dir_all(profile)?;
            let temporary = profile.join(format!("{SEED_FILE}.{}.tmp", nonce()?));
            let content = format!("{}\n", random_seed()?);
            // An incomplete temporary belongs to this call too. The path is
            // random and create_new prevents replacing a different writer.
            let written = write_synced(&temporary, content.as_bytes());
            if let Err(error) = written {
                if error.kind() != io::ErrorKind::AlreadyExists {
                    let _ = fs::remove_file(&temporary);
                }
                return Err(error);
            }
            let result = match fs::hard_link(&temporary, &path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                Err(_) => match write_synced(&path, content.as_bytes()) {
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
                    result => result,
                },
            };
            let cleanup = fs::remove_file(&temporary);
            result?;
            cleanup?;
            read_seed(&path)?
                .ok_or_else(|| io::Error::other("Could not create Cloak fingerprint seed"))?
        }
    };
    Ok(std::iter::once(format!("--fingerprint={seed}"))
        .chain(args.iter().cloned())
        .collect())
}

fn put_arg(args: &mut Vec<String>, value: String) {
    let key = value.split('=').next().unwrap_or("");
    if let Some(index) = args
        .iter()
        .position(|arg| arg.split('=').next() == Some(key))
    {
        args[index] = value;
    } else {
        args.push(value);
    }
}
fn flag<'a>(args: &'a [String], keys: &[&str]) -> Option<&'a str> {
    args.iter().find_map(|arg| {
        let (key, value) = arg.split_once('=')?;
        keys.contains(&key).then_some(value)
    })
}
/// Source priority and insertion order: defaults < caller args < named values.
fn build_args(
    stealth: bool,
    extras: &[String],
    timezone: Option<&str>,
    locale: Option<&str>,
    headless: bool,
) -> io::Result<Vec<String>> {
    let mut args = Vec::new();
    if stealth {
        args.extend([
            "--no-sandbox".into(),
            format!("--fingerprint={}", random_seed()?),
            format!(
                "--fingerprint-platform={}",
                if cfg!(target_os = "macos") {
                    "macos"
                } else {
                    "windows"
                }
            ),
        ]);
    }
    if !headless || cfg!(windows) {
        put_arg(&mut args, "--ignore-gpu-blocklist".into());
    }
    for arg in extras {
        put_arg(&mut args, arg.clone());
    }
    if let Some(timezone) = timezone.filter(|value| !value.is_empty()) {
        put_arg(&mut args, format!("--fingerprint-timezone={timezone}"));
    }
    if let Some(locale) = locale.filter(|value| !value.is_empty()) {
        for key in ["--lang", "--fingerprint-locale"] {
            put_arg(&mut args, format!("{key}={locale}"));
        }
    }
    Ok(args)
}

fn quote(value: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(value).decode_utf8_lossy();
    let mut result = String::new();
    for byte in decoded.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'~') {
            result.push(*byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}
fn proxy_url(server: &str, credentials: Option<(&str, &str)>, socks: bool) -> String {
    let normalized = if server.contains("://") {
        server.to_owned()
    } else {
        format!("http://{server}")
    };
    let original = if socks { server } else { &normalized };
    let Ok(parsed) = reqwest::Url::parse(&normalized) else {
        return original.to_owned();
    };
    let Some((scheme, rest)) = normalized.split_once("://") else {
        return original.to_owned();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, suffix) = rest.split_at(authority_end);
    let (raw_credentials, host_port) = authority
        .rsplit_once('@')
        .map_or((None, authority), |(user, host)| (Some(user), host));
    let userinfo = match credentials {
        Some((user, password)) => {
            // Dict credentials are literal, whereas URL credentials are decoded
            // and re-encoded. Protect literal percent signs before quote().
            let user = quote(&user.replace('%', "%25"));
            if password.is_empty() {
                format!("{user}@")
            } else {
                format!("{user}:{}@", quote(&password.replace('%', "%25")))
            }
        }
        None => match raw_credentials {
            Some(value) => match value.split_once(':') {
                Some((user, password)) => format!("{}:{}@", quote(user), quote(password)),
                None => format!("{}@", quote(value)),
            },
            None => return original.to_owned(),
        },
    };
    let Some(host) = parsed.host_str() else {
        return original.to_owned();
    };
    // urllib lowercases hostnames for every scheme. URL's opaque SOCKS host
    // representation otherwise preserves the caller's case.
    let host = host.to_ascii_lowercase();
    // URL parsers erase default ports; urllib preserves an explicitly given one.
    let port = if host_port.starts_with('[') {
        host_port
            .split_once(']')
            .and_then(|(_, suffix)| suffix.strip_prefix(':'))
    } else {
        host_port.rsplit_once(':').map(|(_, port)| port)
    };
    let port = port
        .filter(|port| !port.is_empty())
        .map_or(String::new(), |port| format!(":{port}"));
    let suffix = if credentials.is_some() {
        suffix
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
    } else {
        suffix
    };
    format!("{}://{userinfo}{host}{port}{suffix}", scheme.to_lowercase())
}
fn proxy_config(proxy: Option<&Value>) -> io::Result<(Option<Value>, Vec<String>, Option<String>)> {
    let Some(proxy) = proxy.filter(|value| !value.is_null()) else {
        return Ok((None, Vec::new(), None));
    };
    let (server, username, password, bypass) = if let Some(server) = proxy.as_str() {
        (server, None, "", None)
    } else {
        let map = proxy
            .as_object()
            .ok_or_else(|| invalid("Cloak proxy object or URL required"))?;
        (
            string(map, "server")?.unwrap_or(""),
            string(map, "username")?.filter(|value| !value.is_empty()),
            string(map, "password")?.unwrap_or(""),
            string(map, "bypass")?,
        )
    };
    let socks = server.to_ascii_lowercase().starts_with("socks5://")
        || server.to_ascii_lowercase().starts_with("socks5h://");
    let credentials = username.map(|username| (username, password));
    let url = if proxy.is_object() && socks && credentials.is_none() {
        server.to_owned()
    } else {
        proxy_url(server, credentials, socks)
    };
    if socks || username.is_some() || (proxy.is_string() && server.contains('@')) {
        let mut args = vec![format!("--proxy-server={url}")];
        if let Some(bypass) = bypass.filter(|value| !value.is_empty()) {
            args.push(format!("--proxy-bypass-list={bypass}"));
        }
        Ok((None, args, (!server.is_empty()).then_some(url)))
    } else {
        let value = if proxy.is_string() {
            serde_json::json!({"server":server})
        } else {
            proxy.clone()
        };
        let url = if server.contains("://") {
            server.to_owned()
        } else {
            format!("http://{server}")
        };
        Ok((Some(value), Vec::new(), (!server.is_empty()).then_some(url)))
    }
}

/// The original wrapper probes only when GeoIP or an explicit WebRTC `auto`
/// flag asks for an exit IP. Requests are through the selected proxy, with a
/// shared deadline; ordinary echo failure returns None as upstream does.
async fn resolve_exit_ip(options: &Map<String, Value>) -> io::Result<Option<String>> {
    if let Some(ip) = string(options, "resolved_exit_ip")? {
        return ip
            .parse::<IpAddr>()
            .map(|_| Some(ip.to_owned()))
            .map_err(|_| invalid("Cloak resolved exit IP is invalid"));
    }
    let args = strings(options, "args")?;
    let geoip = boolean(options, "geoip", false)?;
    let auto = args.iter().any(|arg| arg == "--fingerprint-webrtc-ip=auto");
    let (_, _, proxy) = proxy_config(options.get("proxy"))?;
    let timezone = string(options, "timezone")?
        .or(string(options, "timezone_id")?)
        .or_else(|| flag(&args, &["--fingerprint-timezone"]));
    let locale =
        string(options, "locale")?.or_else(|| flag(&args, &["--lang", "--fingerprint-locale"]));
    if (!geoip && !auto)
        || (proxy.is_none() && (!geoip || (timezone.is_some() && locale.is_some())))
    {
        return Ok(None);
    }
    let seconds = std::env::var("CLOAKBROWSER_GEOIP_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(20.0);
    // Upstream accepts zero as no overall deadline. Three ten-second requests
    // still impose a bounded maximum in that case.
    let budget = if seconds <= 0.0 {
        Duration::from_secs(30)
    } else {
        Duration::from_secs_f64(seconds.min(30.0))
    };
    let deadline = Instant::now() + budget;
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none());
    if let Some(proxy) = &proxy {
        let Ok(proxy) = reqwest::Proxy::all(proxy) else {
            return Ok(None);
        };
        builder = builder.proxy(proxy);
    }
    let Ok(client) = builder.build() else {
        return Ok(None);
    };
    for url in [
        "https://api.ipify.org",
        "https://checkip.amazonaws.com",
        "https://ifconfig.me/ip",
    ] {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let response = client
            .get(url)
            .timeout(remaining.min(Duration::from_secs(10)))
            .send()
            .await;
        if let Ok(response) = response
            && response.status().is_success()
            && response
                .content_length()
                .is_none_or(|length| length <= 4096)
        {
            // Stream a bounded response rather than trust Content-Length.
            let mut response = response;
            let mut bytes = Vec::new();
            let complete = loop {
                match response.chunk().await {
                    Ok(Some(chunk)) if bytes.len() + chunk.len() <= 4096 => {
                        bytes.extend_from_slice(&chunk)
                    }
                    Ok(None) => break true,
                    _ => break false,
                }
            };
            if complete && let Ok(text) = std::str::from_utf8(&bytes) {
                let ip = text.trim();
                if ip.parse::<IpAddr>().is_ok() {
                    return Ok(Some(ip.to_owned()));
                }
            }
        }
    }
    Ok(None)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LicenseDenial {
    pub code: u8,
}
impl LicenseDenial {
    fn message(self) -> &'static str {
        match self.code {
            76 => {
                "CloakBrowser Pro: session limit reached for your plan. Close another running session or upgrade your plan. Always close with browser.close() (e.g. in a finally block). A session that crashed or was killed frees its seat automatically within 4 min (free/trial/solo), 15 min (team) or 30 min (business/scale)."
            }
            77 => {
                "CloakBrowser Pro: license key is invalid, expired, or missing. Check CLOAKBROWSER_LICENSE_KEY."
            }
            78 => {
                "CloakBrowser Pro: couldn't verify your license (license server unreachable or a connection problem)."
            }
            _ => {
                "CloakBrowser Pro: local configuration problem. Either ~/.cloakbrowser is not writable, or the --license-route value is invalid (use auto, direct, or proxy with a proxy set)."
            }
        }
    }
    pub fn into_error(self) -> io::Error {
        io::Error::new(io::ErrorKind::PermissionDenied, self)
    }
}
impl std::fmt::Display for LicenseDenial {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(self.message())
    }
}
impl std::error::Error for LicenseDenial {}
fn denial_code(value: &Value) -> Option<u8> {
    let number = value
        .as_i64()
        .or_else(|| value.as_f64().map(|value| value.trunc() as i64))
        .or_else(|| value.as_str()?.trim().parse().ok())?;
    (76..=79).contains(&number).then_some(number as u8)
}
pub struct Prepared {
    pub options: Value,
    denial_path: Option<PathBuf>,
    observed: Mutex<Option<LicenseDenial>>,
}
impl Prepared {
    /// Check after every successful or failed SDK call, excluding teardown.
    /// The observed code survives destructive consumption for later calls.
    pub fn denial(&self) -> Option<LicenseDenial> {
        let mut observed = self.observed.lock().ok()?;
        if observed.is_some() {
            return *observed;
        }
        let path = self.denial_path.as_ref()?;
        if !path.exists() {
            return None;
        }
        let result = read_bounded(path, 4096)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|value| denial_code(&value));
        let _ = fs::remove_file(path);
        *observed = result.map(|code| LicenseDenial { code });
        *observed
    }
    /// Only the anchored Playwright process-exit record is a license signal.
    pub fn launch_error(&self, text: &str) -> Option<LicenseDenial> {
        let pattern = regex::Regex::new(r"process did exit:\s*exitCode=(\d+)").ok()?;
        pattern
            .captures(text)
            .and_then(|matched| matched.get(1))
            .and_then(|value| value.as_str().parse::<u8>().ok())
            .filter(|code| (76..=79).contains(code))
            .map(|code| LicenseDenial { code })
            .or_else(|| self.denial())
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if let Some(path) = &self.denial_path {
            let _ = fs::remove_file(path);
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
    }
}

fn seed_widevine(profile: &Path, binary: &Path, env: &BTreeMap<String, String>) {
    if !cfg!(target_os = "linux")
        || env.get("CLOAKBROWSER_WIDEVINE").is_some_and(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "off" | "no"
            )
        })
    {
        return;
    }
    let result = (|| -> io::Result<()> {
        let custom = env.get("CLOAKBROWSER_WIDEVINE_CDM");
        let candidates = if let Some(custom) = custom {
            if custom.trim().is_empty() {
                return Ok(());
            }
            vec![PathBuf::from(custom)]
        } else {
            let mut paths = vec![
                binary
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("WidevineCdm"),
            ];
            if let Some(cache) = env
                .get("CLOAKBROWSER_CACHE_DIR")
                .filter(|value| !value.is_empty())
            {
                paths.push(PathBuf::from(cache).join("WidevineCdm"));
            }
            paths
        };
        let Some(cdm) = candidates
            .into_iter()
            .find(|path| path.join("manifest.json").is_file())
        else {
            return Ok(());
        };
        let cdm = fs::canonicalize(cdm)?;
        let directory = profile.join("WidevineCdm");
        let hint = directory.join("latest-component-updated-widevine-cdm");
        no_links(&hint)?;
        fs::create_dir_all(directory)?;
        let content = serde_json::to_vec(&serde_json::json!({"Path":cdm.to_str().ok_or_else(|| invalid("Widevine path must be Unicode"))?})).map_err(io::Error::other)?;
        if read_bounded(&hint, 65536).is_ok_and(|bytes| bytes == content) {
            return Ok(());
        }
        crate::storage::atomic::write_bytes(&hint, &content)
    })();
    // This optional local hint must never prevent a browser launch.
    let _ = result;
}

/// Prepare using explicitly selected environment resources. Exposed so native
/// callers and fixtures can avoid mutating process-global environment.
pub fn prepare_with_environment(
    options: &Map<String, Value>,
    env: &BTreeMap<String, String>,
) -> io::Result<Prepared> {
    let profile = string(options, "user_data_dir")?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid("Persistent profile directory required"))?;
    let profile = Path::new(profile);
    let binary = string(options, "executable_path")?
        .filter(|value| !value.is_empty())
        .or_else(|| {
            env.get("CLOAKBROWSER_BINARY_PATH")
                .map(String::as_str)
                .filter(|value| !value.is_empty())
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Cloak requires an existing native-selected browser binary",
            )
        })?;
    if !Path::new(binary).is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Cloak selected browser binary is missing",
        ));
    }
    if options.contains_key("backend") {
        return Err(invalid("Cloak backend parameter has been removed"));
    }
    if boolean(options, "humanize", false)? {
        return Err(invalid(
            "Cloak humanize is not used by the native SDK boundary",
        ));
    }
    let stealth = boolean(options, "stealth_args", true)?;
    let headless = boolean(options, "headless", true)?;
    let mut extras = strings(options, "args")?;
    let geoip = boolean(options, "geoip", false)?;
    let timezone = string(options, "timezone")?
        .or(string(options, "timezone_id")?)
        .or_else(|| {
            if geoip {
                flag(&extras, &["--fingerprint-timezone"])
            } else {
                None
            }
        })
        .map(str::to_owned);
    let locale = string(options, "locale")?
        .or_else(|| {
            if geoip {
                flag(&extras, &["--lang", "--fingerprint-locale"])
            } else {
                None
            }
        })
        .map(str::to_owned);
    if geoip && (timezone.is_none() || locale.is_none()) {
        return Err(invalid(
            "Cloak GeoIP requires native-resolved timezone and locale",
        ));
    }
    let exit_ip = string(options, "resolved_exit_ip")?;
    if exit_ip.is_some_and(|value| value.parse::<IpAddr>().is_err()) {
        return Err(invalid("Cloak resolved exit IP is invalid"));
    }
    let (proxy, proxy_args, proxy_url) = proxy_config(options.get("proxy"))?;
    if let Some(index) = extras
        .iter()
        .position(|arg| arg == "--fingerprint-webrtc-ip=auto")
    {
        if let Some(ip) = exit_ip.filter(|_| proxy_url.is_some()) {
            extras[index] = format!("--fingerprint-webrtc-ip={ip}");
        } else {
            extras.remove(index);
        }
    }
    if geoip
        && let Some(ip) = exit_ip
        && !extras
            .iter()
            .any(|arg| arg.starts_with("--fingerprint-webrtc-ip"))
    {
        extras.push(format!("--fingerprint-webrtc-ip={ip}"));
    }
    extras.extend(proxy_args);
    extras = persistent_seed(profile, stealth, &extras)?;
    let mut args = build_args(
        stealth,
        &extras,
        timezone.as_deref(),
        locale.as_deref(),
        headless,
    )?;
    let extensions = strings(options, "extension_paths")?;
    if !extensions.is_empty() {
        let paths = extensions
            .iter()
            .map(|path| {
                std::path::absolute(path).and_then(|path| {
                    path.to_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("Cloak extension path must be Unicode"))
                })
            })
            .collect::<io::Result<Vec<_>>>()?
            .join(",");
        put_arg(&mut args, format!("--load-extension={paths}"));
        put_arg(&mut args, format!("--disable-extensions-except={paths}"));
    }
    if !options.contains_key("viewport")
        && !options.contains_key("no_viewport")
        && !args.iter().any(|arg| {
            matches!(
                arg.split('=').next(),
                Some("--start-maximized" | "--window-size" | "--window-position")
            )
        })
    {
        args.push("--start-maximized".into());
    }
    let wrapper_keys = [
        "timezone",
        "timezone_id",
        "locale",
        "geoip",
        "stealth_args",
        "humanize",
        "human_preset",
        "human_config",
        "extension_paths",
        "license_key",
        "browser_version",
        "release_channel",
        "resolved_exit_ip",
        "no_viewport",
    ];
    let mut translated: Map<String, Value> = options
        .iter()
        .filter(|(key, _)| !wrapper_keys.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    translated.insert("executable_path".into(), binary.into());
    translated.insert("headless".into(), headless.into());
    translated.insert("args".into(), args.into());
    translated
        .entry("ignore_default_args".to_owned())
        .or_insert_with(|| {
            serde_json::json!(["--enable-automation", "--enable-unsafe-swiftshader"])
        });
    if let Some(proxy) = proxy {
        translated.insert("proxy".into(), proxy);
    } else {
        translated.remove("proxy");
    }
    let no_viewport = options
        .get("no_viewport")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| invalid("Cloak no_viewport must be boolean"))
        })
        .transpose()?;
    if no_viewport.is_some() {
        translated.remove("viewport");
    } else if !options.contains_key("viewport") {
        translated.insert("viewport".into(), Value::Null);
    }
    for key in ["user_agent", "color_scheme"] {
        if translated
            .get(key)
            .is_some_and(|value| value.is_null() || value.as_str() == Some(""))
        {
            translated.remove(key);
        }
    }
    let explicit_key = string(options, "license_key")?
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let env_key = env
        .get(LICENSE_KEY)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let file_key = if explicit_key.is_none() && env_key.is_none() {
        env.get("CLOAKBROWSER_CACHE_DIR")
            .filter(|path| !path.is_empty())
            .and_then(|cache| read_bounded(&Path::new(cache).join("license.key"), 65536).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    } else {
        None
    };
    let key = explicit_key.or(env_key).or(file_key.as_deref());
    let mut user_env = match translated.remove("env") {
        None | Some(Value::Null) => None,
        Some(Value::Object(values)) => Some(
            values
                .into_iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| {
                    value
                        .as_str()
                        .map(|value| (key, value.to_owned()))
                        .ok_or_else(|| invalid("Cloak environment values must be strings"))
                })
                .collect::<io::Result<BTreeMap<_, _>>>()?,
        ),
        _ => return Err(invalid("Cloak environment object required")),
    };
    let denial_path = if key.is_some() {
        let directory = profile.join(".cloakbrowser-denials");
        // Upstream's optional status-file feature never makes a launch fail
        // when its directory is unavailable. Process-exit codes still work.
        (|| -> io::Result<PathBuf> {
            no_links(&directory)?;
            fs::create_dir_all(&directory)?;
            Ok(directory.join(format!("{}.json", nonce()?)))
        })()
        .ok()
    } else {
        None
    };
    if key.is_some() && (explicit_key.is_some() || file_key.is_some() || user_env.is_some()) {
        user_env
            .get_or_insert_with(|| env.clone())
            .insert(LICENSE_KEY.into(), key.unwrap_or("").into());
    }
    if let Some(path) = &denial_path {
        user_env.get_or_insert_with(|| env.clone()).insert(
            STATUS_FILE.into(),
            path.to_str()
                .ok_or_else(|| invalid("Cloak denial path must be Unicode"))?
                .into(),
        );
    }
    if let Some(mut env) = user_env {
        env.remove("NUMERA_PASSWORD");
        env.remove("NUMERA_API_TOKEN");
        translated.insert(
            "env".into(),
            serde_json::to_value(env).map_err(io::Error::other)?,
        );
    }
    let mut prepared = Prepared {
        options: Value::Null,
        denial_path,
        observed: Mutex::new(None),
    };
    let mut wire = super::options::persistent(&translated)?;
    if let Some(value) = no_viewport {
        wire["noDefaultViewport"] = value.into();
    }
    seed_widevine(profile, Path::new(binary), env);
    prepared.options = wire;
    Ok(prepared)
}

pub async fn prepare_async(options: &Map<String, Value>) -> io::Result<Prepared> {
    let mut options = options.clone();
    if let Some(ip) = resolve_exit_ip(&options).await? {
        options.insert("resolved_exit_ip".into(), ip.into());
    }
    prepare_with_environment(&options, &environment())
}
