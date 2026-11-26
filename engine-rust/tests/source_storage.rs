//! Individually reported native ports of the original storage source cases.
//! Each pytest parameter expansion retains its exact ID in the companion map.
use numera_core::{
    bulk as bulk_core, collision as collision_core, conversion,
    fingerprint::{self, GenerateOptions},
    identity,
    models::Profile,
    ownership::{capture, identity as process_identity, registry::Registry},
    proxy::Proxy,
    storage::{
        archive_paths,
        atomic::write_json,
        config::ConfigStore,
        dashboard::DashboardStore,
        paths,
        pool::ProxyPool,
        repo::ProfileRepo,
        schedules::{self, ScheduleStore},
    },
    vault::testing::{self, Action, Event},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Barrier, Mutex, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};
mod common;
mod process {
    include!("source_storage/process.rs");
}
mod lifecycle {
    include!("source_storage/lifecycle.rs");
}
macro_rules! source_test {
    ($name:ident,$source:literal,$body:block) => {
        #[test]
        #[doc=$source]
        fn $name() {
            process::worker();
            $body
        }
    };
}
fn temp() -> common::TempDirectory {
    common::TempDirectory::new("source-storage").expect("owned temporary root")
}
fn profile(name: &str, seed: i64) -> Profile {
    let mut p = conversion::to_engine(
        &json!({"id":format!("source-{seed}"),"name":name,"os":"Windows","seed":seed}),
        1000.0,
    )
    .expect("profile conversion");
    p.fingerprint = fingerprint::generate(GenerateOptions {
        seed,
        os: Some("windows"),
        locale: Some("en-US"),
        ..Default::default()
    })
    .expect("native fingerprint");
    p
}
fn store(root: &Path) -> ProfileRepo {
    ProfileRepo::new(root.to_owned(), None)
}
fn dash(root: &Path) -> DashboardStore {
    DashboardStore::new(root.to_owned(), None)
}
fn pool(root: &Path) -> ProxyPool {
    ProxyPool::new(root.to_owned(), None)
}
fn saved(root: &Path, name: &str, seed: i64) -> Profile {
    store(root)
        .save(profile(name, seed), 1000.0)
        .expect("save profile")
}
fn raw(root: &Path, path: &str) -> Value {
    serde_json::from_slice(&fs::read(root.join(path)).expect("stored bytes")).expect("stored JSON")
}
fn write(root: &Path, path: &str, value: Value) {
    write_json(&root.join(path), &value).expect("write owned record");
}
fn pair(root: &Path, id: &str) -> (Value, Value) {
    (
        raw(root, &format!("profiles/{id}.json")),
        raw(root, &format!("dashboard/{id}.json")),
    )
}
fn names(root: &Path) -> Vec<String> {
    let mut names = store(root)
        .list(1001.0)
        .expect("list profiles")
        .into_iter()
        .map(|p| p.name.as_str().expect("name").to_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}
fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn scan(root: &Path, at: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        if !at.exists() {
            return;
        }
        for entry in fs::read_dir(at).expect("snapshot dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                scan(root, &path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.insert(
                    path.strip_prefix(root)
                        .expect("owned path")
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(&path).expect("snapshot bytes"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    scan(root, root, &mut out);
    out
}
fn http(root: &Path, method: &str, uri: &str, body: Value) -> (u16, Value) {
    http_password(root, method, uri, body, None)
}
fn http_password(
    root: &Path,
    method: &str,
    uri: &str,
    body: Value,
    password: Option<String>,
) -> (u16, Value) {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(async {
            let app = numera_core::http::application::router(
                root.to_owned(),
                "",
                "",
                password,
                "source-storage-host",
                ["playwright", "patchright", "camoufox", "cloak"]
                    .into_iter()
                    .map(|name| (name.to_owned(), false))
                    .collect(),
            )
            .await
            .expect("router");
            let response = app
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(uri)
                        .header("content-type", "application/json")
                        .body(if body.is_null() {
                            Body::empty()
                        } else {
                            Body::from(serde_json::to_vec(&body).expect("body JSON"))
                        })
                        .expect("request"),
                )
                .await
                .expect("response");
            let status = response.status().as_u16();
            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .expect("response bytes");
            (
                status,
                serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                    Value::String(String::from_utf8_lossy(&bytes).into_owned())
                }),
            )
        })
}
fn create(root: &Path, name: &str) -> Value {
    let (status, value) = http(root, "POST", "/api/profiles", json!({"name":name}));
    assert_eq!(status, 200, "HTTP response status mismatch");
    value
}
fn archive(path: &Path, record: Option<Value>, members: &[(&str, &[u8])]) {
    let mut zip = zip::ZipWriter::new(fs::File::create(path).expect("zip file"));
    if let Some(record) = record {
        zip.start_file("profile.json", zip::write::SimpleFileOptions::default())
            .expect("profile entry");
        zip.write_all(&serde_json::to_vec(&record).expect("profile JSON"))
            .expect("profile bytes");
    }
    for (name, bytes) in members {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .expect("zip entry");
        zip.write_all(bytes).expect("member bytes");
    }
    zip.finish().expect("finish zip");
}
fn archived_profile(path: &Path, members: &[(&str, &[u8])]) -> Profile {
    let p = profile("restored", 1);
    archive(
        path,
        Some(serde_json::to_value(&p).expect("profile JSON")),
        members,
    );
    p
}
fn error_at(
    operation: &'static str,
    filename: &'static str,
    code: i32,
) -> impl FnMut(&Event) -> io::Result<Action> + Send {
    let mut fired = false;
    move |event| {
        if !fired
            && event.operation == operation
            && (filename.is_empty() || event.path.file_name().is_some_and(|p| p == filename))
        {
            fired = true;
            return Err(io::Error::from_raw_os_error(code));
        }
        Ok(Action::Continue)
    }
}

struct BatchRequest {
    oses: Vec<String>,
    locales: Vec<String>,
    versions: Vec<String>,
    engine: String,
    prefix: String,
    start: i64,
    pad: usize,
    tags: Vec<String>,
    seed: i64,
}
impl Default for BatchRequest {
    fn default() -> Self {
        Self {
            oses: vec![],
            locales: vec![],
            versions: vec![],
            engine: "playwright".into(),
            prefix: "Profile".into(),
            start: 1,
            pad: 2,
            tags: vec![],
            seed: 99,
        }
    }
}
fn batch(count: usize, options: BatchRequest) -> Vec<Profile> {
    let mut nonce = 0;
    bulk_core::generate(
        bulk_core::GenerateBatch {
            count,
            seed: options.seed,
            oses: &options.oses,
            locales: &options.locales,
            chrome_versions: &options.versions,
            engine: &options.engine,
            prefix: &options.prefix,
            start: options.start,
            pad: options.pad,
            tags: &options.tags,
            mobile: false,
        },
        || {
            nonce += 1;
            Ok(format!("batch-{nonce}"))
        },
        1000.0,
    )
    .expect("native batch generation")
}
fn three() -> Vec<Profile> {
    let mut a = profile("a", 1);
    a.tags = vec!["prod".into()];
    let mut b = profile("b", 2);
    b.tags = vec!["prod".into(), "eu".into()];
    vec![a, b, profile("c", 3)]
}

fn json_files(root: &Path, subdir: &str) -> Vec<String> {
    let folder = root.join(subdir);
    if !folder.is_dir() {
        return Vec::new();
    }
    let mut files = fs::read_dir(folder)
        .expect("files")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| p.file_name().expect("name").to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    files.sort();
    files
}
fn vendor_export() -> Value {
    json!({"name":"imported","navigator":{"userAgent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36","resolution":"1920x1080"}})
}
fn nth_failure(count: usize) -> impl FnMut(&Event) -> io::Result<Action> + Send {
    let mut calls = 0;
    move |event| {
        if event.operation == "before-atomic-write" {
            calls += 1;
            if calls == count {
                return Err(io::Error::other("disk full"));
            }
        }
        Ok(Action::Continue)
    }
}
fn cli<R>(
    root: &Path,
    action: impl FnOnce(&numera_core::cli::profile_commands::Profiles<'_>) -> R,
) -> R {
    let repository = store(root);
    let config = ConfigStore::new(root.to_owned());
    let installed = BTreeMap::new();
    action(&numera_core::cli::profile_commands::Profiles {
        repo: &repository,
        config: &config,
        installed: &installed,
        host: "source-storage-host",
    })
}
fn settings<R>(
    root: &Path,
    action: impl FnOnce(&numera_core::cli::settings_commands::Settings<'_>) -> R,
) -> R {
    let repository = store(root);
    let installed = BTreeMap::new();
    action(&numera_core::cli::settings_commands::Settings {
        root,
        repo: &repository,
        installed: &installed,
        environment_token: false,
    })
}
fn ds(root: &Path, record: Value) -> Value {
    dash(root)
        .save(record, Default::default(), None, None, 1000.0)
        .expect("dashboard save")
}
fn edit_note(root: &Path, id: &str, note: &str) {
    store(root)
        .edit(
            id,
            |p| {
                p.notes = note.into();
                Ok(None)
            },
            1001.0,
        )
        .expect("edit note");
}
fn sessions(root: &Path, id: &str) -> PathBuf {
    let ud = store(root).user_data_path(id).expect("user data");
    fs::write(ud.join("Cookies"), b"the live session").expect("cookies");
    ud
}
fn restored_fixture(root: &Path) -> (Profile, PathBuf, PathBuf) {
    let p = saved(root, "alpha", 1);
    let ud = sessions(root, &p.id);
    let zip = root.join("backup.zip");
    store(root)
        .backup(&p.id, &zip, "source-storage-host")
        .expect("backup");
    (p, ud, zip)
}

fn legacy(root: &Path) -> PathBuf {
    let src = root.join("old");
    fs::create_dir_all(src.join("profiles")).expect("legacy profiles");
    fs::create_dir_all(src.join("user-data/abc123")).expect("legacy session");
    fs::write(src.join("profiles/a.json"), br#"{"id":"a"}"#).expect("legacy a");
    fs::write(src.join("profiles/b.json"), br#"{"id":"b"}"#).expect("legacy b");
    fs::write(src.join("user-data/abc123/Cookies"), b"session").expect("session");
    fs::write(src.join("proxies.json"), br#"[{"host":"h","port":"1"}]"#).expect("pool");
    fs::write(src.join("config.json"), br#"{"default_engine":"camoufox"}"#).expect("config");
    src
}
fn directories(root: &Path, contains: &str) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    fs::read_dir(root)
        .expect("dirs")
        .map(|e| e.expect("entry").path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .contains(contains)
        })
        .collect()
}

fn secret_profile(root: &Path, name: &str, seed: i64, secret: Option<&str>) -> Profile {
    let mut p = profile(name, seed);
    if let Some(secret) = secret {
        p.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://1.2.3.4:8080","username":"u","password":secret}),
                false,
            )
            .expect("proxy"),
        );
    }
    store(root).save(p, 1000.0).expect("secret profile")
}
fn vaulted(root: &Path) -> Profile {
    let p = secret_profile(root, "p1", 1, Some("PROXY-PASSWORD"));
    pool(root).add_many(vec![json!({"type":"HTTP","host":"5.6.7.8","port":"8080","user":"u","pass":"PROXY-PASSWORD"})]).expect("pool");
    store(root).enable_vault("master").expect("enable vault");
    p
}
fn public_destination(url: &str) -> io::Result<(reqwest::Url, std::net::IpAddr)> {
    let parsed = reqwest::Url::parse(url).map_err(io::Error::other)?;
    let host = parsed.host_str().unwrap_or("").trim_matches(['[', ']']);
    let address = host.parse().unwrap_or_else(|_| {
        if host == "localhost" {
            "127.0.0.1".parse().expect("loopback")
        } else {
            "93.184.216.34".parse().expect("public fixture")
        }
    });
    numera_core::network::proxy_rotate::testing::public_destination(url, &[address])
}

fn new_salt() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("salt randomness");
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn diverged(root: &Path, name: &str, seed: i64) -> Profile {
    let p = secret_profile(root, name, seed, Some("engine"));
    let mut d = raw(root, &format!("dashboard/{}.json", p.id));
    d["proxy"]["pass"] = "dashboard".into();
    write(root, &format!("dashboard/{}.json", p.id), d);
    p
}
fn one_sided(root: &Path, engine: Option<&str>, dashboard: &str) -> Profile {
    let p = secret_profile(root, "alpha", 1, Some(engine.unwrap_or("engine")));
    let mut d = raw(root, &format!("dashboard/{}.json", p.id));
    d["proxy"]["pass"] = dashboard.into();
    write(root, &format!("dashboard/{}.json", p.id), d);
    if engine.is_none() {
        let mut e = raw(root, &format!("profiles/{}.json", p.id));
        e["proxy"]["password"] = "".into();
        write(root, &format!("profiles/{}.json", p.id), e);
    }
    p
}
fn assert_secrets(root: &Path, id: &str, engine: &str, dashboard: &str) {
    let (e, d) = pair(root, id);
    assert_eq!(e["proxy"]["password"], engine);
    assert_eq!(d["proxy"]["pass"], dashboard);
}

fn wait_job(jobs: &numera_core::jobs::JobStore, id: &str) {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let row = jobs
            .get(id, numera_core::native_entry::now().expect("clock"))
            .expect("job observation")
            .expect("job");
        if row["running"] == false {
            assert_eq!(row["exit"], 0, "detached job failed");
            return;
        }
        assert!(Instant::now() < end, "detached job did not settle");
        thread::sleep(Duration::from_millis(10));
    }
}

#[path = "source_storage/archive_safety.rs"]
mod archive_safety;

#[path = "source_storage/store.rs"]
mod store;

#[path = "source_storage/store_backup.rs"]
mod store_backup;

#[path = "source_storage/bulk.rs"]
mod bulk;

#[path = "source_storage/proxypool.rs"]
mod proxypool;

#[path = "source_storage/collision.rs"]
mod collision;

#[path = "source_storage/scheduler.rs"]
mod scheduler;

#[path = "source_storage/store_correctness.rs"]
mod store_correctness;

#[path = "source_storage/transactions_and_proxy_identity.rs"]
mod transactions_and_proxy_identity;

#[path = "source_storage/portable_root.rs"]
mod portable_root;

#[path = "source_storage/resilient_and_guarded.rs"]
mod resilient_and_guarded;

#[path = "source_storage/closure_pass.rs"]
mod closure_pass;

#[path = "source_storage/failure_atomicity.rs"]
mod failure_atomicity;

#[path = "source_storage/two_store_sync.rs"]
mod two_store_sync;

#[path = "source_storage/compound_atomicity.rs"]
mod compound_atomicity;

#[path = "source_storage/two_store_convergence.rs"]
mod two_store_convergence;

#[path = "source_storage/repo_authority.rs"]
mod repo_authority;

#[path = "source_storage/delete_rollback.rs"]
mod delete_rollback;

#[path = "source_storage/restore_race.rs"]
mod restore_race;

#[path = "source_storage/jobs.rs"]
mod jobs;

#[path = "source_storage/installation_lease.rs"]
mod installation_lease;
