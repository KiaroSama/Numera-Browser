//! Native ports of the original source tests. Every source_test! is one reported
//! Rust case, including each original pytest parameter expansion. The exact ID
//! is attached as documentation and recorded in docs/source-domain-test-map.json.
use numera_core::{
    conversion,
    fingerprint::{self, GenerateOptions},
    identity, importers,
    models::{Fingerprint, Profile},
    proxy::Proxy,
    read_view,
    storage::{config::ConfigStore, repo::ProfileRepo},
    validation,
    write_contract::{self, Input},
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
mod common;

macro_rules! source_test {
    ($name:ident, $source:literal, $body:block) => {
        #[test]
        #[doc = $source]
        fn $name() $body
    };
}

fn generate(seed: i64, os: Option<&str>, locale: Option<&str>) -> Fingerprint {
    fingerprint::generate(GenerateOptions {
        seed,
        os,
        locale,
        ..Default::default()
    })
    .expect("generate native fingerprint")
}
fn catalog() -> Value {
    serde_json::from_str::<Value>(include_str!("../src/device_catalog.json"))
        .expect("embedded device catalog")["catalog"]
        .clone()
}
fn policy() -> Value {
    serde_json::from_str::<Value>(include_str!("../src/field_policy.json"))
        .expect("embedded field policy")["policy"]
        .clone()
}
fn convert(record: Value) -> Profile {
    conversion::to_engine(&record, 1000.0).expect("convert dashboard record")
}
fn profile(name: &str, seed: i64, os: &str) -> Profile {
    let mut p = convert(json!({"id":format!("source-{seed}"),"name":name}));
    p.fingerprint = generate(seed, Some(os), Some("en-US"));
    p
}
fn problems(record: &Value) -> Vec<String> {
    write_contract::profile_problems(Input::Json(record), "profile", None)
        .expect("evaluate write contract")
}
// serde_json's arbitrary precision test constructor preserves the nonfinite
// in-memory values the Python cases inject. They never pass through JSON text.
fn nonfinite(text: &str) -> Value {
    Value::Number(serde_json::Number::from_string_unchecked(text.into()))
}
fn numeric_record(field: &str, value: Value) -> Value {
    // Insert directly: json! serializes Values and intentionally rejects these
    // in-memory nonfinite test values before the domain API can see them.
    let mut record = json!({"name":"n", "os":"windows"});
    record[field] = value;
    record
}
fn huge(exponent: usize, negative: bool) -> Value {
    serde_json::from_str(&format!(
        "{}1{}",
        if negative { "-" } else { "" },
        "0".repeat(exponent)
    ))
    .expect("arbitrary precision integer")
}
fn native_number(profile: &Profile, field: &str) -> Value {
    let value = serde_json::to_value(&profile.fingerprint).expect("serialize fingerprint");
    value[match field {
        "cores" => "hardware_concurrency",
        "memory" => "device_memory",
        "seed" => "seed",
        "mediaVideo" => "cameras",
        "mediaAudio" => "microphones",
        "mediaSpeakers" => "speakers",
        "maxTouchPoints" => "max_touch_points",
        "devicePixelRatio" => "device_pixel_ratio",
        _ => panic!("unknown numeric dashboard field {field}"),
    }]
    .clone()
}
fn assert_number_eq(actual: &Value, expected: &Value) {
    assert_eq!(
        actual.as_f64(),
        expected.as_f64(),
        "numeric value changed: {actual} vs {expected}"
    );
}
fn names(repo: &ProfileRepo) -> Vec<String> {
    let mut names = repo
        .list(1001.0)
        .expect("list profiles")
        .into_iter()
        .map(|p| p.name.as_str().expect("name is text").to_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}
fn json_files(root: &std::path::Path, subdir: &str) -> Vec<std::path::PathBuf> {
    let path = root.join(subdir);
    if !path.exists() {
        return vec![];
    }
    let mut entries = fs::read_dir(path)
        .expect("read stored directory")
        .map(|e| e.expect("directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
fn http(root: &std::path::Path, method: &str, uri: &str, body: Value) -> (u16, Value) {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("HTTP test runtime");
    runtime.block_on(async {
        let app = numera_core::http::application::router(
            root.to_owned(),
            "",
            "",
            None,
            "source-domain-host",
            BTreeMap::new(),
        )
        .await
        .expect("native app");
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(if body.is_null() {
                Body::empty()
            } else {
                Body::from(serde_json::to_vec(&body).expect("HTTP request JSON"))
            })
            .expect("request");
        let response = app.oneshot(request).await.expect("native response");
        let status = response.status().as_u16();
        let bytes = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("HTTP response bytes");
        let value = serde_json::from_slice(&bytes).expect("HTTP response JSON");
        (status, value)
    })
}
#[path = "source_domain/alignment.rs"]
mod alignment_cases;
#[path = "source_domain/context.rs"]
mod context_cases;
#[path = "source_domain/core.rs"]
mod core_cases;
#[path = "source_domain/fingerprint.rs"]
mod fingerprint_cases;
#[path = "source_domain/flat_numeric.rs"]
mod flat_numeric_cases;
#[path = "source_domain/importers.rs"]
mod importer_cases;
#[path = "source_domain/model_policy.rs"]
mod model_policy_cases;
#[path = "source_domain/name_contract.rs"]
mod name_contract_cases;
