use super::{
    files,
    manifest::{self, Asset},
};
use reqwest::{
    blocking::{Client, Response},
    redirect,
};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{self, Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[cfg(feature = "update-acceptance")]
fn trusted(url: &reqwest::Url) -> bool {
    super::acceptance::trusted_url(url)
}
#[cfg(not(feature = "update-acceptance"))]
fn trusted(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some(
                "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}
pub fn client() -> io::Result<Client> {
    Client::builder()
        .user_agent("Numera-Browser-Updater/1")
        .https_only(!cfg!(feature = "update-acceptance"))
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(1800))
        .redirect(redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !trusted(attempt.url()) {
                attempt.error("Untrusted update redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|_| io::Error::other("Update HTTPS initialization failed"))
}
fn request(client: &Client, url: &str, budget: Duration) -> io::Result<Response> {
    let url = reqwest::Url::parse(url).map_err(|_| manifest::error("Invalid update URL"))?;
    if !trusted(&url) {
        return Err(manifest::error("Untrusted update URL"));
    }
    client
        .get(url)
        .timeout(budget)
        .header("Accept", "application/vnd.github+json")
        .send()
        .and_then(Response::error_for_status)
        .map_err(|_| io::Error::other("Update service unavailable, offline or rate limited"))
}
pub fn bytes(client: &Client, url: &str, limit: u64) -> io::Result<Vec<u8>> {
    bounded_bytes(client, url, limit, Duration::from_secs(20))
}
fn bounded_bytes(client: &Client, url: &str, limit: u64, budget: Duration) -> io::Result<Vec<u8>> {
    if budget.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Update discovery budget exhausted",
        ));
    }
    let response = request(client, url, budget.min(Duration::from_secs(20)))?;
    if response.content_length().is_some_and(|size| size > limit) {
        return Err(manifest::error("Update response exceeds its size limit"));
    }
    let mut bytes = Vec::new();
    response.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(manifest::error("Update response exceeds its size limit"));
    }
    Ok(bytes)
}
#[cfg(feature = "update-acceptance")]
pub fn asset_url(source: &str, name: &str) -> String {
    // Invalid fixture configuration becomes an invalid URL, never a public fallback.
    super::acceptance::asset_url(source, name).unwrap_or_default()
}
#[cfg(not(feature = "update-acceptance"))]
pub fn asset_url(source: &str, name: &str) -> String {
    format!(
        "https://github.com/{}/releases/download/build-{source}/{name}",
        manifest::REPOSITORY
    )
}
fn discovery_url(page: u32) -> io::Result<String> {
    #[cfg(feature = "update-acceptance")]
    {
        super::acceptance::discovery_url(page)
    }
    #[cfg(not(feature = "update-acceptance"))]
    {
        Ok(format!(
            "https://api.github.com/repos/{}/releases?per_page=20&page={page}",
            manifest::REPOSITORY
        ))
    }
}
pub fn manifest_urls(source: &str) -> (String, String) {
    (
        asset_url(source, "update-manifest.json"),
        asset_url(source, "update-manifest.sig"),
    )
}
pub fn stream(
    _client: &Client,
    url: &str,
    asset: &Asset,
    path: &Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> io::Result<()> {
    let url = reqwest::Url::parse(url).map_err(|_| manifest::error("Invalid update URL"))?;
    if !trusted(&url) {
        return Err(manifest::error("Untrusted update URL"));
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let client=reqwest::Client::builder().user_agent("Numera-Browser-Updater/1").https_only(!cfg!(feature = "update-acceptance")).no_proxy()
            .connect_timeout(Duration::from_secs(10)).read_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(1800))
            .redirect(redirect::Policy::custom(|attempt|if attempt.previous().len()>=5||!trusted(attempt.url()){attempt.error("Untrusted update redirect")}else{attempt.follow()}))
            .build().map_err(|_|io::Error::other("Update HTTPS initialization failed"))?;
        let download=async {
            let mut response=client.get(url).send().await.and_then(reqwest::Response::error_for_status).map_err(|_|io::Error::other("Update archive request failed"))?;
            if response.content_length().is_some_and(|size|size!=asset.size){return Err(manifest::error("Update archive size changed"));}
            let mut file=OpenOptions::new().write(true).create_new(true).open(path)?;
            let mut count=0u64;let mut digest=Sha256::new();
            while let Some(bytes)=response.chunk().await.map_err(|_|io::Error::other("Update archive stream failed"))?{
                count=count.checked_add(bytes.len() as u64).ok_or_else(||manifest::error("Update archive byte count overflow"))?;
                if count>asset.size{return Err(manifest::error("Update archive exceeds authenticated size"));}
                digest.update(&bytes);file.write_all(&bytes)?;progress(count);
            }
            file.sync_all()?;
            if count!=asset.size||manifest::hex(&digest.finalize())!=asset.sha256{return Err(manifest::error("Update archive digest does not match its signature"));}
            Ok(())
        };
        tokio::pin!(download);
        loop {tokio::select!{result=&mut download=>return result,_=tokio::time::sleep(Duration::from_millis(100))=>{
            if cancelled.load(Ordering::Acquire){return Err(io::Error::new(io::ErrorKind::ConnectionAborted,"Update cancelled"));}
        }}}
    })
}
#[derive(Clone)]
pub struct Offer {
    pub id: String,
    pub manifest: manifest::Manifest,
    pub asset: Asset,
    pub bytes: Vec<u8>,
    pub signature: Vec<u8>,
}
pub fn discover(
    current: u64,
    platform: &str,
    key: &[u8; 32],
    now: u64,
    cancelled: &AtomicBool,
) -> io::Result<Option<Offer>> {
    let client = client()?;
    discover_with(
        current,
        platform,
        key,
        now,
        cancelled,
        |url, limit, budget| bounded_bytes(&client, url, limit, budget),
    )
}
fn discover_with(
    current: u64,
    platform: &str,
    key: &[u8; 32],
    now: u64,
    cancelled: &AtomicBool,
    mut fetch: impl FnMut(&str, u64, Duration) -> io::Result<Vec<u8>>,
) -> io::Result<Option<Offer>> {
    let mut best: Option<Offer> = None;
    let mut failed = false;
    let mut complete = false;
    let end = Instant::now() + Duration::from_secs(60);
    for page in 1..=3 {
        if cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Update discovery cancelled",
            ));
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Update discovery incomplete within its budget",
            ));
        }
        let data = fetch(
            &discovery_url(page)?,
            manifest::METADATA_LIMIT,
            end.saturating_duration_since(Instant::now()),
        )?;
        let releases: Vec<serde_json::Value> = serde_json::from_slice(&data)
            .map_err(|_| manifest::error("Release list is malformed"))?;
        let count = releases.len();
        for release in releases {
            if cancelled.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Update discovery cancelled",
                ));
            }
            if Instant::now() >= end {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Update discovery incomplete within its budget",
                ));
            }
            if release["draft"] == true {
                continue;
            }
            let tag = release["tag_name"].as_str().unwrap_or("");
            let Some(source) = tag.strip_prefix("build-") else {
                continue;
            };
            if manifest::decode_hex::<20>(source).is_err() {
                continue;
            }
            let (metadata_url, signature_url) = manifest_urls(source);
            let metadata = match fetch(
                &metadata_url,
                manifest::METADATA_LIMIT,
                end.saturating_duration_since(Instant::now()),
            ) {
                Ok(value) => value,
                Err(_) => {
                    failed = true;
                    continue;
                }
            };
            if cancelled.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Update discovery cancelled",
                ));
            }
            let signature = match fetch(
                &signature_url,
                64,
                end.saturating_duration_since(Instant::now()),
            ) {
                Ok(value) => value,
                Err(_) => {
                    failed = true;
                    continue;
                }
            };
            let manifest = match manifest::authenticated_manifest(&metadata, &signature, key) {
                Ok(value) => value,
                Err(_) => {
                    failed = true;
                    continue;
                }
            };
            if manifest.source_sha != source {
                failed = true;
                continue;
            }
            if !manifest.suitable(current, now)
                || !manifest
                    .assets
                    .iter()
                    .any(|asset| asset.platform == platform)
                || best
                    .as_ref()
                    .is_some_and(|offer| offer.manifest.generation >= manifest.generation)
            {
                continue;
            }
            let asset = manifest
                .assets
                .iter()
                .find(|asset| asset.platform == platform)
                .cloned()
                .ok_or_else(|| manifest::error("Compatible asset missing"))?;
            best = Some(Offer {
                id: manifest::digest(&metadata),
                manifest,
                asset,
                bytes: metadata,
                signature,
            });
        }
        if count < 20 {
            complete = true;
            break;
        }
    }
    if cancelled.load(Ordering::Acquire) {
        return Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "Update discovery cancelled",
        ));
    }
    if failed || !complete {
        return Err(io::Error::other(
            "Update discovery incomplete; release metadata could not be authenticated or retrieved",
        ));
    }
    Ok(best)
}
#[cfg(test)]
#[path = "discovery_tests.rs"]
mod discovery_tests;

pub fn check_space(root: &Path, asset: &Asset, previous: u64) -> io::Result<()> {
    let needed = asset
        .size
        .checked_add(asset.expanded_size)
        .and_then(|size| size.checked_add(previous))
        .and_then(|size| size.checked_add(67_108_864))
        .ok_or_else(|| manifest::error("Update storage size overflow"))?;
    if files::available(root)? < needed {
        return Err(io::Error::new(
            io::ErrorKind::StorageFull,
            "Not enough free storage for update and rollback",
        ));
    }
    Ok(())
}
