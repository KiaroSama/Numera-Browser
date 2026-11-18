#![recursion_limit = "256"]
//! Individually reported native ports of profile/write/import baseline cases.
//! Exact source IDs and native adaptations are in docs/source-contracts-test-map.json.
use numera_core::{
    conversion,
    models::Profile,
    proxy::Proxy,
    read_view,
    storage::{
        atomic::{self, Batch},
        config::ConfigStore,
        dashboard::DashboardStore,
        locks::{DataLock, SessionLease},
        repo::ProfileRepo,
    },
    write_contract::{self, Input},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Barrier, mpsc},
    thread,
    time::Duration,
};
mod common;
macro_rules! source_test {($name:ident,$source:literal,$body:block)=>{#[test] #[doc=$source] fn $name() $body};}
static PROFILE_NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn profile(name: &str) -> Profile {
    conversion::to_engine(&json!({"id":format!("contract-{}-{}",std::process::id(),PROFILE_NONCE.fetch_add(1,std::sync::atomic::Ordering::Relaxed)),"name":name,"seed":1}),1000.0).expect("valid profile")
}
fn record() -> Value {
    serde_json::to_value(profile("p")).expect("profile record")
}
fn number(text: &str) -> Value {
    Value::Number(serde_json::Number::from_string_unchecked(text.into()))
}
fn big(exponent: usize, negative: bool) -> Value {
    serde_json::from_str(&format!(
        "{}1{}",
        if negative { "-" } else { "" },
        "0".repeat(exponent)
    ))
    .expect("large JSON integer")
}
fn problems(v: &Value, kind: Option<&str>) -> Vec<String> {
    write_contract::profile_problems(Input::Json(v), "profile", kind).expect("write contract")
}
fn repo(root: &Path) -> ProfileRepo {
    ProfileRepo::new(root.to_owned(), None)
}
fn dash(root: &Path) -> DashboardStore {
    DashboardStore::new(root.to_owned(), None)
}
fn config(root: &Path) -> ConfigStore {
    ConfigStore::new(root.to_owned())
}
fn read(root: &Path, subdir: &str, id: &str) -> Value {
    serde_json::from_slice(
        &fs::read(root.join(subdir).join(format!("{id}.json"))).expect("stored bytes"),
    )
    .expect("stored JSON")
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, at: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        if !at.exists() {
            return;
        }
        for e in fs::read_dir(at).expect("snapshot directory") {
            let p = e.expect("entry").path();
            if p.file_name()
                .is_some_and(|n| n == ".locks" || n == ".transaction.lock" || n == ".lock")
            {
                continue;
            }
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(
                    p.strip_prefix(root).expect("owned path").to_owned(),
                    fs::read(p).expect("snapshot bytes"),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn ids(root: &Path, dir: &str) -> BTreeSet<String> {
    let d = root.join(dir);
    if !d.is_dir() {
        return BTreeSet::new();
    }
    fs::read_dir(d)
        .expect("record directory")
        .map(|e| e.expect("entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| p.file_stem().expect("stem").to_string_lossy().into_owned())
        .collect()
}
fn http_raw(root: &Path, method: &str, uri: &str, body: Vec<u8>) -> (u16, Value) {
    http_request(root, method, uri, body, None)
}
fn http_request(
    root: &Path,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    authorization: Option<&str>,
) -> (u16, Value) {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("HTTP runtime");
    rt.block_on(async {
        let app = numera_core::http::application::router(
            root.to_owned(),
            "",
            "",
            None,
            "source-contract-host",
            BTreeMap::from([
                ("camoufox".into(), false),
                ("patchright".into(), false),
                ("playwright".into(), false),
            ]),
        )
        .await
        .expect("native router");
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(auth) = authorization {
            request = request.header("authorization", auth);
        }
        let req = request.body(Body::from(body)).expect("HTTP request");
        let res = app.oneshot(req).await.expect("HTTP response");
        let status = res.status().as_u16();
        let json_media = res
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("application/json"));
        let bytes = to_bytes(res.into_body(), 1 << 22)
            .await
            .expect("response body");
        (
            status,
            if json_media {
                serde_json::from_slice(&bytes).expect("JSON response")
            } else {
                Value::String(String::from_utf8(bytes.to_vec()).expect("text response"))
            },
        )
    })
}
fn encode(v: &Value) -> Vec<u8> {
    match v {
        Value::Number(n) => n.to_string().into_bytes(),
        Value::Array(rows) => {
            let mut b = vec![b'['];
            for (i, v) in rows.iter().enumerate() {
                if i > 0 {
                    b.push(b',');
                }
                b.extend(encode(v));
            }
            b.push(b']');
            b
        }
        Value::Object(map) => {
            let mut b = vec![b'{'];
            for (i, (k, v)) in map.iter().enumerate() {
                if i > 0 {
                    b.push(b',');
                }
                b.extend(serde_json::to_vec(k).expect("key"));
                b.push(b':');
                b.extend(encode(v));
            }
            b.push(b'}');
            b
        }
        _ => serde_json::to_vec(v).expect("JSON scalar"),
    }
}
fn http(root: &Path, method: &str, uri: &str, body: Value) -> (u16, Value) {
    http_raw(root, method, uri, encode(&body))
}
fn made(root: &Path, body: Value) -> Value {
    let (s, v) = http(root, "POST", "/api/profiles", body);
    assert_eq!(s, 200, "{v}");
    v
}
fn put(root: &Path, id: &Value, body: Value) -> (u16, Value) {
    http(
        root,
        "PUT",
        &format!("/api/profiles/{}", id.as_str().expect("ID")),
        body,
    )
}
fn assert_refused(status: u16, value: &Value, field: &str) {
    assert!(
        (400..500).contains(&status),
        "expected field refusal: {status} {value}"
    );
    assert!(
        value.to_string().contains(field),
        "missing field {field}: {value}"
    );
    assert!(!value.to_string().contains("Traceback"));
}
fn native(profiles: Value) -> Value {
    json!({"numeraProfiles":1,"profiles":profiles})
}
fn importing(payload: Value) -> Value {
    let bytes = encode(&payload);
    json!({"text":String::from_utf8(bytes).expect("JSON text")})
}
fn import_file(
    root: &Path,
    payload: Value,
) -> Result<Profile, numera_core::storage::repo::RepoError> {
    let outside = common::TempDirectory::new("contracts-import").expect("import root");
    let path = outside.path().join("vendor.json");
    fs::write(&path, encode(&payload)).expect("vendor file");
    repo(root).import_file(&path, 1001.0)
}
fn valid_twin(root: &Path) {
    let d = ids(root, "dashboard");
    assert!(!d.is_empty());
    assert_eq!(d, ids(root, "profiles"));
    for id in d {
        let p = Profile::from_record(&read(root, "profiles", &id)).expect("engine twin");
        assert!(
            write_contract::profile_problems(Input::Profile(&p), "profile", None)
                .expect("validate twin")
                .is_empty()
        );
    }
}
#[path = "source_contracts/helpers.rs"]
mod helpers;
use helpers::*;

fn cli_import(root: &Path, payload: Value) -> numera_core::cli::profile_commands::Outcome {
    let outside = common::TempDirectory::new("contracts-cli-import").expect("import directory");
    let path = outside.path().join("vendor.json");
    fs::write(&path, encode(&payload)).expect("vendor bytes");
    let repo = repo(root);
    let config = config(root);
    let installed = BTreeMap::new();
    numera_core::cli::profile_commands::Profiles {
        repo: &repo,
        config: &config,
        installed: &installed,
        host: "source-contract-host",
    }
    .import_from(&path, 1001.0)
    .expect("CLI import result")
}
#[path = "source_contracts/processes.rs"]
mod processes;
use processes::*;
// Generated case modules.
#[path = "source_contracts/cli_import_write_contract.rs"]
mod cli_import_write_contract;
#[path = "source_contracts/profile_schema.rs"]
mod profile_schema;
#[path = "source_contracts/profile_transactions.rs"]
mod profile_transactions;
#[path = "source_contracts/public_boundaries.rs"]
mod public_boundaries;
#[path = "source_contracts/request_atomicity.rs"]
mod request_atomicity;
#[path = "source_contracts/revision_and_patch.rs"]
mod revision_and_patch;
#[path = "source_contracts/store_boundary_validation.rs"]
mod store_boundary_validation;
#[path = "source_contracts/transaction_atomicity.rs"]
mod transaction_atomicity;
#[path = "source_contracts/validation_context_regression.rs"]
mod validation_context_regression;
#[path = "source_contracts/write_contract_boundaries.rs"]
mod write_contract_boundaries;
#[path = "source_contracts/write_contract_complete_record.rs"]
mod write_contract_complete_record;
#[path = "source_contracts/write_contract_engine_shape.rs"]
mod write_contract_engine_shape;
#[path = "source_contracts/write_contract_entry_points.rs"]
mod write_contract_entry_points;
#[path = "source_contracts/write_contracts.rs"]
mod write_contracts;
