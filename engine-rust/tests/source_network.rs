//! Native network/extension source cases; every macro expands one reported test.
//! The exact frozen source IDs are recorded in docs/source-network-test-map.json.
use numera_core::{
    conversion,
    fingerprint::{self, GenerateOptions},
    identity_script,
    models::{Fingerprint, Profile},
    network::{self, proxy_check, proxy_rotate as rotation, relay::Relay},
    pool,
    proxy::Proxy,
    storage::{
        config::ConfigStore, extension_install, extension_package, extensions as extension_catalog,
        pool::ProxyPool, repo::ProfileRepo,
    },
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
mod common;
macro_rules! source_test {($name:ident,$source:literal,$body:block)=>{#[test] #[doc=$source] fn $name() $body};}
fn generated() -> Fingerprint {
    fingerprint::generate(GenerateOptions {
        seed: 1,
        ..Default::default()
    })
    .expect("native generated fingerprint")
}
fn profile(name: &str) -> Profile {
    let mut p = conversion::to_engine(
        &json!({"id":"network-profile","name":name,"seed":1}),
        1000.0,
    )
    .expect("profile");
    p.fingerprint = generated();
    p
}
fn repository(root: &Path) -> ProfileRepo {
    ProfileRepo::new(root.to_owned(), None)
}
fn config(root: &Path) -> ConfigStore {
    ConfigStore::new(root.to_owned())
}
fn runtime<T>(operation: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("native runtime")
        .block_on(operation)
}
fn commands<T>(
    root: &Path,
    action: impl FnOnce(&numera_core::cli::profile_commands::Profiles<'_>) -> T,
) -> T {
    let repo = repository(root);
    let config = config(root);
    let installed = BTreeMap::from([
        ("camoufox".into(), false),
        ("playwright".into(), false),
        ("patchright".into(), false),
    ]);
    action(&numera_core::cli::profile_commands::Profiles {
        repo: &repo,
        config: &config,
        installed: &installed,
        host: "source-network-host",
    })
}
fn unpacked(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    write_ext(
        &p,
        json!({"manifest_version":3,"name":name,"version":"1.0"}),
        &[],
    );
    p
}
fn write_ext(path: &Path, manifest: Value, files: &[(&str, &[u8])]) {
    fs::create_dir_all(path).expect("extension folder");
    fs::write(
        path.join("manifest.json"),
        serde_json::to_vec(&manifest).expect("manifest JSON"),
    )
    .expect("manifest bytes");
    for (name, bytes) in files {
        let p = path.join(name);
        fs::create_dir_all(p.parent().expect("member parent")).expect("member directory");
        fs::write(p, bytes).expect("member bytes");
    }
}
fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("zip member");
        zip.write_all(bytes).expect("zip bytes");
    }
    zip.finish().expect("zip complete").into_inner()
}
fn crx3(zip: &[u8]) -> Vec<u8> {
    let mut bytes = b"Cr24".to_vec();
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(b"sig");
    bytes.extend_from_slice(zip);
    bytes
}
fn args(profile: &Profile, root: &Path) -> Vec<String> {
    let paths = numera_core::browser::extensions::paths(&profile.extensions, root, root)
        .expect("launch extension resolution");
    numera_core::browser::options::build(
        profile,
        "playwright",
        "/profile",
        false,
        &[],
        &paths,
        None,
    )
    .expect("launch options")["args"]
        .as_array()
        .expect("arguments")
        .iter()
        .filter_map(Value::as_str)
        .filter(|a| {
            a.starts_with("--disable-extensions-except=") || a.starts_with("--load-extension=")
        })
        .map(str::to_owned)
        .collect()
}
struct Response {
    status: u16,
    body: Vec<u8>,
    json: Option<Value>,
}
fn http(root: &Path, password: Option<&str>, method: &str, path: &str, body: Value) -> Response {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    runtime(async {
        let app = numera_core::http::application::router(
            root.to_owned(),
            "",
            "",
            password.map(str::to_owned),
            "source-network-host",
            BTreeMap::from([
                ("camoufox".into(), false),
                ("playwright".into(), false),
                ("patchright".into(), false),
            ]),
        )
        .await
        .expect("native router");
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .body(if body.is_null() {
                Body::empty()
            } else {
                Body::from(serde_json::to_vec(&body).expect("request JSON"))
            })
            .expect("request");
        let res = app.oneshot(req).await.expect("response");
        let status = res.status().as_u16();
        let bytes = to_bytes(res.into_body(), 1 << 22)
            .await
            .expect("body")
            .to_vec();
        let json = serde_json::from_slice(&bytes).ok();
        Response {
            status,
            body: bytes,
            json,
        }
    })
}
fn json_http(root: &Path, method: &str, path: &str, body: Value) -> (u16, Value) {
    let response = http(root, None, method, path, body);
    (response.status, response.json.expect("JSON response"))
}
fn icon_url(path: &Path) -> String {
    let mut url = reqwest::Url::parse("http://local/api/extensions/icon").expect("URL");
    url.query_pairs_mut()
        .append_pair("path", path.to_str().expect("path"));
    format!("{}?{}", url.path(), url.query().expect("query"))
}
fn put_defaults(root: &Path, paths: &[PathBuf]) -> (u16, Value) {
    json_http(
        root,
        "PUT",
        "/api/config",
        json!({"default_extensions":paths}),
    )
}
fn stored_defaults(root: &Path) -> Vec<Value> {
    config(root).default_extensions().expect("stored defaults")
}
#[path = "source_network/servers.rs"]
mod servers;
use servers::*;
#[path = "source_network/helpers.rs"]
mod helpers;
use helpers::*;
// Generated network case modules.
#[path = "source_network/bulk_proxy_check_skips.rs"]
mod bulk_proxy_check_skips;
#[path = "source_network/dns.rs"]
mod dns;
#[path = "source_network/extension_validation.rs"]
mod extension_validation;
#[path = "source_network/extensions.rs"]
mod extensions;
#[path = "source_network/proxy_check_socks.rs"]
mod proxy_check_socks;
#[path = "source_network/proxy_rotate.rs"]
mod proxy_rotate;
#[path = "source_network/relay.rs"]
mod relay;
#[path = "source_network/relay_socks4.rs"]
mod relay_socks4;
#[path = "source_network/rotate_legacy_endpoint.rs"]
mod rotate_legacy_endpoint;
#[path = "source_network/rotate_transport.rs"]
mod rotate_transport;
#[path = "source_network/webrtc.rs"]
mod webrtc;
#[path = "source_network/webstore.rs"]
mod webstore;
