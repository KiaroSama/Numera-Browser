//! Consent-bound, native Cloak acquisition. Reading an offer never requests a
//! provider, validates a license, creates a cache, or changes launch resources.
//! Vendor protocol: CloakBrowser 0.7.2, 1a145dbb90a26d6dd223c7642cb49fb9c850f22c.
use crate::storage::{
    archive_paths::Staging,
    locks::{SessionLease, resolved},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const SDK_VERSION: &str = "0.7.2";
const PROVIDER: &str = "CloakHQ/CloakBrowser";
const BASE: &str = "https://cloakbrowser.dev";
const PUBLIC_KEY: &str = "MKFKwIhUcKWq5xTuNA0Ovg99njcDEcEJvmWYYhApvaU=";
const CONSENT: &str =
    "affirmative consent to the current exact Cloak acquisition offer is required";
const BUSY: &str = "a Cloak acquisition is already in progress for this vendor cache";
pub const METADATA_ERROR: &str = "Cloak browser metadata could not be read safely";

fn refusal(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}
pub fn version(value: &str) -> io::Result<&str> {
    let parts: Vec<_> = value.split('.').collect();
    if ![4, 5].contains(&parts.len())
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return Err(refusal("Cloak browser version metadata is invalid"));
    }
    Ok(value)
}
fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Explicit local metadata and credential boundary. Credentials are never
/// serialized into an offer, helper argv, telemetry or public error.
#[derive(Clone)]
pub struct Metadata {
    pub sdk: String,
    pub version: String,
    pub platform: String,
    pub cache: PathBuf,
    pub binary: PathBuf,
    pub license_key: Option<String>,
    pub requested: bool,
    pub custom_mirror: bool,
    pub binary_override: Option<PathBuf>,
    pub unavailable: Option<String>,
}
#[derive(Clone, Debug)]
pub struct LicenseInfo {
    pub valid: bool,
    pub plan: String,
}

pub trait Vendor: Send + Sync {
    fn metadata(&self) -> io::Result<Metadata>;
    fn license(
        &self,
        metadata: &Metadata,
        key: &str,
        cancelled: &AtomicBool,
    ) -> io::Result<Option<LicenseInfo>>;
    fn ensure_binary(&self, metadata: &Metadata, cancelled: &AtomicBool) -> io::Result<PathBuf>;
}

fn identity(metadata: &Metadata) -> io::Result<Value> {
    version(&metadata.version)?;
    Ok(
        json!({"engine":"cloak","sdk":metadata.sdk,"version":metadata.version,"platform":metadata.platform,"provider":PROVIDER,"keyed":metadata.license_key.is_some()}),
    )
}
fn offer_for(metadata: &Metadata) -> io::Result<Value> {
    let mut offer = identity(metadata)?;
    let sorted: BTreeMap<_, _> = offer
        .as_object()
        .ok_or_else(|| refusal(METADATA_ERROR))?
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let mut bytes = Vec::new();
    sorted
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut bytes,
            crate::identity_script::SourceJson,
        ))
        .map_err(io::Error::other)?;
    let mut reason = metadata.unavailable.clone();
    if metadata.custom_mirror {
        reason = Some(
            "custom vendor mirrors are not supported by consent-controlled acquisition".into(),
        );
    } else if metadata.license_key.is_some() && !metadata.requested {
        reason = Some("keyed acquisition needs an explicit exact CLOAKBROWSER_VERSION; free-key latest-only downloads use the manual vendor route".into());
    }
    let installed = if let Some(binary) = &metadata.binary_override {
        let installed = binary.is_file();
        reason = Some(
            if installed {
                "an explicit browser override is user-managed; vendor acquisition is disabled"
            } else {
                "the explicit Cloak browser binary is missing; repair or remove its override"
            }
            .into(),
        );
        installed
    } else {
        metadata.binary.is_file()
    };
    let object = offer
        .as_object_mut()
        .ok_or_else(|| refusal(METADATA_ERROR))?;
    object.insert("offer_id".into(), sha(&bytes).into());
    object.insert("installed".into(), installed.into());
    object.insert("available".into(), reason.is_none().into());
    object.insert(
        "reason".into(),
        reason.map(Value::String).unwrap_or(Value::Null),
    );
    object.insert("notice".into(), "Vendor download, not a redistributed Numera binary; exact SDK compatibility is not certified.".into());
    Ok(offer)
}
pub fn unavailable_offer() -> Value {
    json!({"engine":"cloak","available":false,"installed":false,"reason":METADATA_ERROR,"offer_id":null})
}

pub struct Acquisition {
    vendor: Arc<dyn Vendor>,
}
pub struct Launch {
    pub options: Map<String, Value>,
    pub environment: BTreeMap<String, String>,
}
pub fn selected_binary(root: &Path) -> io::Result<Launch> {
    Acquisition::native_at(root).launch_options()
}

/// Engines CLI seam shared with the actual parser and isolated source cases.
pub fn command(
    acquisition: &Acquisition,
    args: &Map<String, Value>,
    cancelled: &AtomicBool,
) -> io::Result<Option<Value>> {
    if args.get("command").and_then(Value::as_str) != Some("engines") {
        return Ok(None);
    }
    if args.get("cloak_offer") == Some(&Value::Bool(true)) {
        return Ok(Some(acquisition.offer()));
    }
    args.get("acquire_cloak")
        .and_then(Value::as_str)
        .map(|offer_id| {
            acquisition.acquire(
                offer_id,
                args.get("approve") == Some(&Value::Bool(true)),
                cancelled,
            )
        })
        .transpose()
}
impl Acquisition {
    pub fn new(vendor: Arc<dyn Vendor>) -> Self {
        Self { vendor }
    }
    pub fn native() -> Self {
        Self::new(Arc::new(NativeVendor::environment()))
    }
    pub fn native_at(root: &Path) -> Self {
        Self::new(Arc::new(NativeVendor::at_root(root)))
    }
    pub fn offer(&self) -> Value {
        self.vendor
            .metadata()
            .and_then(|metadata| offer_for(&metadata))
            .unwrap_or_else(|_| unavailable_offer())
    }
    pub fn acquire(
        &self,
        offer_id: &str,
        approved: bool,
        cancelled: &AtomicBool,
    ) -> io::Result<Value> {
        let current = self.offer();
        if !approved
            || current["available"] != true
            || current["offer_id"].as_str() != Some(offer_id)
        {
            return Err(refusal(CONSENT));
        }
        let metadata = self
            .vendor
            .metadata()
            .map_err(|_| refusal(METADATA_ERROR))?;
        no_links(&metadata.cache)?;
        let cache = resolved(&metadata.cache)?;
        let lease_path = SessionLease::path_for(&cache, "cloak-acquisition")?;
        let _lease = SessionLease::acquire(&lease_path).map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                io::Error::new(io::ErrorKind::WouldBlock, BUSY)
            } else {
                io::Error::new(
                    error.kind(),
                    "Cloak acquisition cache could not be locked safely",
                )
            }
        })?;
        // Metadata can change while waiting for the cache-wide process lease.
        let metadata = self
            .vendor
            .metadata()
            .map_err(|_| refusal(METADATA_ERROR))?;
        let current = offer_for(&metadata)?;
        if current["available"] != true
            || current["offer_id"].as_str() != Some(offer_id)
            || resolved(&metadata.cache)? != cache
        {
            return Err(refusal(
                "Cloak acquisition offer changed; review it before approving again",
            ));
        }
        check_cancelled(cancelled)?;
        if let Some(key) = &metadata.license_key {
            let license = self.vendor.license(&metadata, key, cancelled)?;
            if license.is_none_or(|license| !license.valid || license.plan == "free") {
                return Err(refusal(
                    "this vendor license cannot honor an exact pinned acquisition; use the manual vendor route",
                ));
            }
        }
        let binary = self.vendor.ensure_binary(&metadata, cancelled)?;
        if resolved(&binary)? != resolved(&metadata.binary)? || !binary.is_file() {
            return Err(refusal(
                "vendor acquisition did not produce the approved browser target",
            ));
        }
        Ok(
            json!({"engine":"cloak","installed":true,"version":metadata.version,"offer_id":offer_id}),
        )
    }
    pub fn launch_options(&self) -> io::Result<Launch> {
        let metadata = self
            .vendor
            .metadata()
            .map_err(|_| refusal(METADATA_ERROR))?;
        let mut environment = BTreeMap::from([("CLOAKBROWSER_AUTO_UPDATE".into(), "false".into())]);
        if let Some(key) = &metadata.license_key {
            environment.insert("CLOAKBROWSER_LICENSE_KEY".into(), key.clone());
        }
        if let Some(binary) = metadata.binary_override {
            if !binary.is_file() {
                return Err(refusal("the explicit Cloak browser binary is missing"));
            }
            environment.insert("CLOAKBROWSER_BINARY_PATH".into(), path_text(&binary)?);
            return Ok(Launch {
                options: Map::new(),
                environment,
            });
        }
        version(&metadata.version)?;
        if !ready(&metadata.binary) {
            return Err(refusal(
                "Cloak browser acquisition requires explicit approval in Engines before first launch",
            ));
        }
        environment.insert(
            "CLOAKBROWSER_BINARY_PATH".into(),
            path_text(&metadata.binary)?,
        );
        Ok(Launch {
            options: Map::from_iter([("browser_version".into(), metadata.version.into())]),
            environment,
        })
    }
}

fn path_text(path: &Path) -> io::Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| refusal(METADATA_ERROR))
}
fn check_cancelled(cancelled: &AtomicBool) -> io::Result<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "Cloak acquisition cancelled",
        ))
    } else {
        Ok(())
    }
}
fn ready(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}
fn no_links(path: &Path) -> io::Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        if let Ok(metadata) = fs::symlink_metadata(&current) {
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let linked = metadata.file_type().is_symlink();
            if linked {
                return Err(refusal("Cloak acquisition refuses symbolic-link resources"));
            }
        }
    }
    Ok(())
}
fn bounded(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    no_links(path)?;
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(refusal(METADATA_ERROR));
    }
    Ok(bytes)
}

pub struct NativeVendor {
    environment: BTreeMap<String, String>,
    home: Option<PathBuf>,
}
impl NativeVendor {
    pub fn environment() -> Self {
        Self {
            environment: std::env::vars_os()
                .filter_map(|(key, value)| {
                    Some((key.into_string().ok()?, value.into_string().ok()?))
                })
                .collect(),
            home: std::env::home_dir(),
        }
    }
    pub fn at_root(root: &Path) -> Self {
        let mut vendor = Self::environment();
        if vendor.nonempty("CLOAKBROWSER_CACHE_DIR").is_none() {
            vendor.environment.insert(
                "CLOAKBROWSER_CACHE_DIR".into(),
                root.join("browser-cache/cloak")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
        vendor
    }
    pub fn with_environment(environment: BTreeMap<String, String>, home: Option<PathBuf>) -> Self {
        Self { environment, home }
    }
    fn nonempty(&self, key: &str) -> Option<&str> {
        self.environment
            .get(key)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }
}
impl Vendor for NativeVendor {
    fn metadata(&self) -> io::Result<Metadata> {
        let cache = match self.nonempty("CLOAKBROWSER_CACHE_DIR") {
            Some(path) => PathBuf::from(path),
            None => self
                .home
                .as_ref()
                .ok_or_else(|| refusal(METADATA_ERROR))?
                .join(".cloakbrowser"),
        };
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => "linux-x64",
            ("linux", "aarch64") => "linux-arm64",
            ("macos", "x86_64") => "darwin-x64",
            ("macos", "aarch64") => "darwin-arm64",
            ("windows", "x86_64") => "windows-x64",
            _ => return Err(refusal(METADATA_ERROR)),
        };
        let key = self
            .nonempty("CLOAKBROWSER_LICENSE_KEY")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                bounded(&cache.join("license.key"), 16384)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty())
            });
        let requested = self
            .nonempty("CLOAKBROWSER_VERSION")
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let cached = || -> Option<String> {
            let prefix = if self
                .nonempty("CLOAKBROWSER_RELEASE_CHANNEL")
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("preview"))
            {
                "latest_pro_version_preview"
            } else {
                "latest_pro_version"
            };
            let bytes = bounded(&cache.join(format!("{prefix}_{platform}")), 4096).ok()?;
            let value = String::from_utf8(bytes).ok()?.trim().to_owned();
            version(&value).ok()?;
            Some(value)
        };
        let pinned = requested.map(str::to_owned).or_else(cached);
        // 0.7.2 removed the unkeyed public browser and fixed default version.
        // An unavailable local offer never asks the vendor to resolve latest.
        let unavailable = if pinned.is_none() {
            Some("Cloak acquisition needs an explicit exact CLOAKBROWSER_VERSION; use the manual vendor route".into())
        } else if key.is_none() {
            Some(
                "CloakBrowser 0.7.2 requires a vendor license key; use the manual vendor route"
                    .into(),
            )
        } else {
            None
        };
        let version = pinned.unwrap_or_else(|| "0.0.0.0".into());
        self::version(&version)?;
        let directory = cache.join(format!("chromium-{version}-pro"));
        let binary = directory.join(if platform.starts_with("windows") {
            "chrome.exe"
        } else if platform.starts_with("darwin") {
            "Chromium.app/Contents/MacOS/Chromium"
        } else {
            "chrome"
        });
        Ok(Metadata {
            sdk: SDK_VERSION.into(),
            version,
            platform: platform.into(),
            cache,
            binary,
            license_key: key,
            requested: requested.is_some(),
            custom_mirror: self.nonempty("CLOAKBROWSER_DOWNLOAD_URL").is_some(),
            binary_override: self.nonempty("CLOAKBROWSER_BINARY_PATH").map(PathBuf::from),
            unavailable,
        })
    }
    fn license(
        &self,
        metadata: &Metadata,
        key: &str,
        cancelled: &AtomicBool,
    ) -> io::Result<Option<LicenseInfo>> {
        let cached = read_license(metadata, key, false);
        if cached.is_some() {
            return Ok(cached);
        }
        let result = runtime()?.block_on(async {
            let client = client(Duration::from_secs(10))?;
            let request = client
                .post(format!("{BASE}/api/license/validate"))
                .header("Content-Type", "application/json")
                .body(serde_json::to_vec(&json!({"license_key":key})).map_err(io::Error::other)?);
            let bytes = response_bytes(request, 65536, cancelled).await?;
            serde_json::from_slice::<Value>(&bytes).map_err(io::Error::other)
        });
        match result {
            Ok(data) => {
                let info = LicenseInfo {
                    valid: data["valid"] == true,
                    plan: data["plan"].as_str().unwrap_or("solo").into(),
                };
                if info.valid {
                    let content = json!({"key_sha256":sha(key.as_bytes()),"valid":true,"plan":info.plan,"expires":data["expires"],"validated_at":timestamp()?});
                    crate::storage::atomic::write_json(
                        &metadata.cache.join(".license_cache"),
                        &content,
                    )?;
                }
                Ok(Some(info))
            }
            Err(_) if cancelled.load(Ordering::Acquire) => {
                check_cancelled(cancelled)?;
                Ok(None)
            }
            Err(_) => Ok(read_license(metadata, key, true)),
        }
    }
    fn ensure_binary(&self, metadata: &Metadata, cancelled: &AtomicBool) -> io::Result<PathBuf> {
        if ready(&metadata.binary) {
            return Ok(metadata.binary.clone());
        }
        let key = metadata.license_key.as_deref().ok_or_else(|| refusal("this vendor license cannot honor an exact pinned acquisition; use the manual vendor route"))?;
        no_links(&metadata.cache)?;
        fs::create_dir_all(&metadata.cache)?;
        let mut staging = Staging::new(&metadata.cache, ".cloak-acquisition-")?;
        let archive = staging
            .path()?
            .join(if metadata.platform.starts_with("windows") {
                "download.zip"
            } else {
                "download.tar.gz"
            });
        let base = format!("{BASE}/releases/pro/chromium-v{}", metadata.version);
        runtime()?.block_on(async {
            let client = client(Duration::from_secs(600))?;
            let manifest = response_bytes(
                client.get(format!("{base}/SHA256SUMS")),
                1024 * 1024,
                cancelled,
            )
            .await?;
            let signature = response_bytes(
                client.get(format!("{base}/SHA256SUMS.sig")),
                4096,
                cancelled,
            )
            .await?;
            let expected = verified_manifest(metadata, &manifest, &signature)?;
            let request = client
                .get(format!("{BASE}/api/download/{}", metadata.version))
                .bearer_auth(key)
                .header("X-Platform", &metadata.platform);
            download(request, &archive, &expected, cancelled).await
        })?;
        let payload = staging.path()?.join("payload");
        fs::create_dir(&payload)?;
        unpack(&archive, &payload, cancelled)?;
        flatten(&payload)?;
        let directory = metadata
            .cache
            .join(format!("chromium-{}-pro", metadata.version));
        no_links(&directory)?;
        let relative = metadata
            .binary
            .strip_prefix(&directory)
            .map_err(|_| refusal("vendor acquisition target escaped its cache"))?;
        let binary = payload.join(relative);
        if !binary.is_file() {
            return Err(refusal(
                "vendor acquisition did not produce the approved browser target",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&binary)?.permissions();
            permissions.set_mode(permissions.mode() | 0o755);
            fs::set_permissions(&binary, permissions)?;
        }
        check_cancelled(cancelled)?;
        let stale = if directory.exists() {
            if ready(&metadata.binary) {
                staging.cleanup()?;
                return Ok(metadata.binary.clone());
            }
            let old = staging.path()?.join("previous-incomplete");
            fs::rename(&directory, &old)?;
            Some(old)
        } else {
            None
        };
        if let Err(error) = fs::rename(&payload, &directory) {
            if let Some(old) = &stale {
                let _ = fs::rename(old, &directory);
            }
            return Err(io::Error::new(
                error.kind(),
                "Cloak browser installation could not be published",
            ));
        }
        // This selects an installed, signature-verified version. It records no
        // approval grant: every future acquisition still needs its exact offer.
        crate::storage::atomic::write_bytes(
            &metadata
                .cache
                .join(format!("latest_pro_version_{}", metadata.platform)),
            format!("{}\n", metadata.version).as_bytes(),
        )?;
        staging.cleanup()?;
        Ok(metadata.binary.clone())
    }
}

fn timestamp() -> io::Result<f64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs_f64())
        .map_err(io::Error::other)
}
fn read_license(metadata: &Metadata, key: &str, stale: bool) -> Option<LicenseInfo> {
    let value: Value =
        serde_json::from_slice(&bounded(&metadata.cache.join(".license_cache"), 65536).ok()?)
            .ok()?;
    if value["key_sha256"].as_str()? != sha(key.as_bytes()) {
        return None;
    }
    if !stale && timestamp().ok()? - value["validated_at"].as_f64()? > 86400.0 {
        return None;
    }
    let expired = value["expires"]
        .as_str()
        .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
        .is_some_and(|date| (date.timestamp() as f64) < timestamp().unwrap_or(f64::INFINITY));
    Some(LicenseInfo {
        valid: value["valid"] == true && !expired,
        plan: value["plan"].as_str().unwrap_or("solo").into(),
    })
}
fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
fn client(budget: Duration) -> io::Result<reqwest::Client> {
    crate::network::builder(budget)?
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 || attempt.url().scheme() != "https" {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| refusal("Cloak vendor connection could not be prepared"))
}
async fn controlled<T>(
    future: impl std::future::Future<Output = io::Result<T>>,
    cancelled: &AtomicBool,
) -> io::Result<T> {
    tokio::pin!(future);
    loop {
        tokio::select! { result=&mut future=>return result, _=tokio::time::sleep(Duration::from_millis(25))=>check_cancelled(cancelled)? }
    }
}
async fn response_bytes(
    request: reqwest::RequestBuilder,
    limit: usize,
    cancelled: &AtomicBool,
) -> io::Result<Vec<u8>> {
    controlled(
        async {
            let mut response = request
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|_| refusal("Cloak vendor metadata is unavailable"))?;
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| refusal("Cloak vendor metadata is unavailable"))?
            {
                if chunk.len() > limit.saturating_sub(bytes.len()) {
                    return Err(refusal("Cloak vendor metadata exceeds its bound"));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        },
        cancelled,
    )
    .await
}
fn archive_name(metadata: &Metadata) -> String {
    format!(
        "cloakbrowser-{}{}",
        metadata.platform,
        if metadata.platform.starts_with("windows") {
            ".zip"
        } else {
            ".tar.gz"
        }
    )
}
pub fn verified_manifest(
    metadata: &Metadata,
    manifest: &[u8],
    signature: &[u8],
) -> io::Result<String> {
    let key = STANDARD
        .decode(PUBLIC_KEY)
        .map_err(|_| refusal("Cloak vendor signing key is invalid"))?;
    let key = VerifyingKey::from_bytes(
        &key.try_into()
            .map_err(|_| refusal("Cloak vendor signing key is invalid"))?,
    )
    .map_err(|_| refusal("Cloak vendor signing key is invalid"))?;
    let signature = STANDARD
        .decode(signature.trim_ascii())
        .map_err(|_| refusal("Cloak browser signature verification failed"))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|_| refusal("Cloak browser signature verification failed"))?;
    key.verify_strict(manifest, &signature)
        .map_err(|_| refusal("Cloak browser signature verification failed"))?;
    let text = std::str::from_utf8(manifest)
        .map_err(|_| refusal("Cloak signed checksum metadata is invalid"))?;
    if text
        .lines()
        .find_map(|line| line.trim().strip_prefix("version=").map(str::trim))
        != Some(metadata.version.as_str())
    {
        return Err(refusal(
            "Cloak signed browser version differs from the approved exact target",
        ));
    }
    let name = archive_name(metadata);
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let filename = parts.next()?.trim_start_matches('*');
            (filename == name
                && hash.len() == 64
                && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
        })
        .next()
        .ok_or_else(|| refusal("Cloak signed checksum manifest omits the approved platform"))
}
async fn download(
    request: reqwest::RequestBuilder,
    path: &Path,
    expected: &str,
    cancelled: &AtomicBool,
) -> io::Result<()> {
    controlled(
        async {
            let mut response = request
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|_| refusal("Cloak approved browser download failed"))?;
            if response
                .content_length()
                .is_some_and(|size| size > 1024 * 1024 * 1024)
            {
                return Err(refusal("Cloak browser download exceeds its bound"));
            }
            let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
            let mut hash = Sha256::new();
            let mut size = 0u64;
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| refusal("Cloak approved browser download failed"))?
            {
                size = size.saturating_add(chunk.len() as u64);
                if size > 1024 * 1024 * 1024 {
                    return Err(refusal("Cloak browser download exceeds its bound"));
                }
                output.write_all(&chunk)?;
                hash.update(&chunk);
            }
            output.sync_all()?;
            let actual: String = hash
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            if actual != expected {
                return Err(refusal("Cloak browser checksum verification failed"));
            }
            Ok(())
        },
        cancelled,
    )
    .await
}

fn member(root: &Path, name: &Path) -> io::Result<PathBuf> {
    let mut relative = PathBuf::new();
    for component in name.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !relative.pop() {
                    return Err(refusal("Cloak archive path escapes its installation"));
                }
            }
            _ => return Err(refusal("Cloak archive has an absolute path")),
        }
    }
    Ok(root.join(relative))
}
fn unpack(archive: &Path, root: &Path, cancelled: &AtomicBool) -> io::Result<()> {
    let mut expanded = 0u64;
    let mut count = 0usize;
    if archive
        .extension()
        .is_some_and(|extension| extension == "zip")
    {
        let mut zip = zip::ZipArchive::new(File::open(archive)?).map_err(io::Error::other)?;
        for index in 0..zip.len() {
            check_cancelled(cancelled)?;
            let mut file = zip.by_index(index).map_err(io::Error::other)?;
            count += 1;
            expanded = expanded.saturating_add(file.size());
            check_archive_bounds(count, expanded)?;
            if file
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
            {
                return Err(refusal("Cloak zip contains an unchecked link"));
            }
            let name = file.name().map_err(io::Error::other)?;
            let target = member(root, Path::new(name.as_ref()))?;
            if file.is_dir() {
                fs::create_dir_all(&target)?;
                continue;
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            io::copy(&mut file, &mut output)?;
            output.sync_all()?;
        }
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?));
        let mut links = Vec::new();
        for entry in tar.entries()? {
            check_cancelled(cancelled)?;
            let mut entry = entry?;
            count += 1;
            expanded = expanded.saturating_add(entry.size());
            check_archive_bounds(count, expanded)?;
            let target = member(root, &entry.path()?)?;
            let kind = entry.header().entry_type();
            if kind.is_dir() {
                fs::create_dir_all(&target)?;
            } else if kind.is_file() {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                let mut output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&target)?;
                io::copy(&mut entry, &mut output)?;
                output.sync_all()?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        &target,
                        fs::Permissions::from_mode(entry.header().mode()? & 0o777),
                    )?;
                }
            } else if kind.is_symlink() || kind.is_hard_link() {
                let link = entry
                    .link_name()?
                    .ok_or_else(|| refusal("Cloak archive link target missing"))?
                    .into_owned();
                if kind.is_hard_link() {
                    links.push((target, link, true));
                } else {
                    links.push((target, link, false));
                }
            } else {
                return Err(refusal("Cloak archive contains a non-file resource"));
            }
        }
        for (target, link, hard) in links {
            let parent = target
                .parent()
                .ok_or_else(|| refusal("Cloak archive link parent missing"))?;
            fs::create_dir_all(parent)?;
            let base = if hard { root } else { parent };
            let relative = base
                .strip_prefix(root)
                .map_err(|_| refusal("Cloak archive link escapes its installation"))?
                .join(&link);
            let contained = member(root, &relative)?;
            if hard {
                fs::hard_link(contained, &target)?;
            } else {
                #[cfg(unix)]
                std::os::unix::fs::symlink(&link, &target)?;
                #[cfg(not(unix))]
                return Err(refusal(
                    "Cloak tar links require a native Unix installation host",
                ));
            }
        }
    }
    Ok(())
}
fn check_archive_bounds(count: usize, expanded: u64) -> io::Result<()> {
    if count > 100000 || expanded > 3 * 1024 * 1024 * 1024 {
        Err(refusal("Cloak extracted browser exceeds its bound"))
    } else {
        Ok(())
    }
}
fn flatten(root: &Path) -> io::Result<()> {
    let entries = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
    if entries.len() == 1
        && entries[0].file_type()?.is_dir()
        && !entries[0].file_name().to_string_lossy().ends_with(".app")
    {
        let inner = entries[0].path();
        for entry in fs::read_dir(&inner)? {
            let entry = entry?;
            fs::rename(entry.path(), root.join(entry.file_name()))?;
        }
        fs::remove_dir(inner)?;
    }
    Ok(())
}
