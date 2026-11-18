//! Compile-only hosted acceptance hooks. This module is absent from production.
use super::manifest;
use serde_json::{json, Value};
use std::{
    io,
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn origin() -> io::Result<reqwest::Url> {
    let value = std::env::var("NUMERA_ACCEPTANCE_FEED_ORIGIN")
        .map_err(|_| manifest::error("Acceptance fixture origin is missing"))?;
    let url =
        reqwest::Url::parse(&value).map_err(|_| manifest::error("Invalid acceptance origin"))?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(manifest::error(
            "Acceptance transport must use an isolated IPv4 loopback origin",
        ));
    }
    Ok(url)
}
pub fn trusted_url(url: &reqwest::Url) -> bool {
    origin().is_ok_and(|origin| {
        url.origin() == origin.origin() && url.username().is_empty() && url.password().is_none()
    })
}
pub fn discovery_url(page: u32) -> io::Result<String> {
    if !(1..=3).contains(&page) {
        return Err(manifest::error("Invalid acceptance release page"));
    }
    Ok(origin()?
        .join(&format!("releases?page={page}"))
        .map_err(io::Error::other)?
        .into())
}
pub fn asset_url(source: &str, name: &str) -> io::Result<String> {
    manifest::decode_hex::<20>(source)?;
    if name.is_empty()
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-._".contains(&byte)
        })
    {
        return Err(manifest::error("Unsafe acceptance asset name"));
    }
    Ok(origin()?
        .join(&format!("build-{source}/{name}"))
        .map_err(io::Error::other)?
        .into())
}
fn client() -> io::Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(io::Error::other)
}
fn report(value: Value) -> io::Result<()> {
    let token = std::env::var("NUMERA_ACCEPTANCE_REPORT_TOKEN")
        .map_err(|_| manifest::error("Acceptance report capability is missing"))?;
    manifest::decode_hex::<32>(&token)?;
    let bytes = serde_json::to_vec(&value).map_err(io::Error::other)?;
    if bytes.len() > 16_384 {
        return Err(manifest::error("Acceptance report exceeds limit"));
    }
    let response = client()?
        .post(origin()?.join("report").map_err(io::Error::other)?)
        .bearer_auth(token)
        .header("content-type", "application/json")
        .body(bytes)
        .send()
        .map_err(|_| io::Error::other("Acceptance report failed"))?;
    if !response.status().is_success() {
        return Err(io::Error::other("Acceptance report refused"));
    }
    Ok(())
}
fn identity() -> Value {
    json!({"pid": std::process::id(), "generation": env!("NUMERA_BUILD_GENERATION").parse::<u64>().unwrap_or(0),
        "sourceSha": env!("NUMERA_BUILD_SOURCE_SHA"), "acceptance": true})
}
pub fn report_bridge(
    root: &Path,
    engine_port: u16,
    endpoint: &str,
    bridge_token: &str,
) -> io::Result<()> {
    let mut value = identity();
    value["event"] = "bridge".into();
    value["root"] = root.to_string_lossy().into_owned().into();
    value["engineOrigin"] = format!("http://127.0.0.1:{engine_port}").into();
    value["endpoint"] = endpoint.into();
    value["token"] = bridge_token.into();
    report(value)
}
pub fn report_ready(root: &Path) -> io::Result<()> {
    let mut value = identity();
    value["event"] = "ready".into();
    value["root"] = root.to_string_lossy().into_owned().into();
    report(value)
}
pub fn report_webview(root: &Path) -> io::Result<()> {
    let mut value = identity();
    value["event"] = "webview".into();
    value["root"] = root.to_string_lossy().into_owned().into();
    report(value)
}
pub fn checkpoint(name: &str) -> io::Result<()> {
    if std::env::var("NUMERA_ACCEPTANCE_CHECKPOINT").as_deref() != Ok(name) {
        return Ok(());
    }
    if !["quiesced", "first-file-moved"].contains(&name) {
        return Err(manifest::error("Unknown acceptance checkpoint"));
    }
    let mut value = identity();
    value["event"] = "checkpoint".into();
    value["checkpoint"] = name.into();
    report(value)?;
    let token = std::env::var("NUMERA_ACCEPTANCE_REPORT_TOKEN")
        .map_err(|_| manifest::error("Acceptance checkpoint capability missing"))?;
    let url = origin()?
        .join("checkpoint/release")
        .map_err(io::Error::other)?;
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let response = client()?
            .get(url.clone())
            .bearer_auth(&token)
            .send()
            .map_err(|_| io::Error::other("Acceptance checkpoint controller unavailable"))?;
        if response.status().as_u16() == 200 {
            return Ok(());
        }
        if response.status().as_u16() != 409 {
            return Err(io::Error::other("Acceptance checkpoint release refused"));
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Acceptance checkpoint was not released",
            ));
        }
        thread::park_timeout(Duration::from_millis(20));
    }
}
