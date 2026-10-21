use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{http::application::router, importers};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;
#[path = "profile_http_import_parity/lexical.rs"]
mod lexical;
#[path = "profile_http_import_parity/tags.rs"]
mod tags;

#[test]
fn vendor_conversion_matches_independent_original_seeded_records()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-http-import.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Import cases missing")?;
    let mut count = 0;
    for case in cases.iter().filter(|case| {
        case["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("vendor-"))
            && case["status"] == 200
    }) {
        let raw: Value =
            serde_json::from_str(case["input"]["text"].as_str().ok_or("Text missing")?)?;
        let raw = raw
            .get("profiles")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .unwrap_or(&raw);
        let (mut actual, issues) = importers::profile(raw, 42, 1700000000.0)?;
        let mut expected = case["written"]["dashboard/000000000001.json"].clone();
        for key in ["id", "_created", "_rev"] {
            expected
                .as_object_mut()
                .ok_or("Expected missing")?
                .remove(key);
            actual.as_object_mut().ok_or("Actual missing")?.remove(key);
        }
        if let Some(engine) = raw["engine"].as_str() {
            actual["engine"] = engine.into();
        }
        assert_eq!(actual, expected, "{} vendor fields", case["name"]);
        assert_eq!(
            serde_json::to_value(issues)?,
            case["body"]["results"][0]["issues"],
            "{} issues",
            case["name"]
        );
        count += 1;
    }
    assert_eq!(count, 5);
    Ok(())
}
#[test]
fn native_http_import_preserves_both_copies_and_refusal_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/profile-http-import.json"))?;
        let cases = fixture["cases"].as_array().ok_or("Import cases missing")?;
        assert_eq!(cases.len(), 21);
        let mut count = 0;
        // Vendor entropy is exercised separately at its explicit-seed seam;
        // native import and refusals have deterministic public HTTP contracts.
        for case in cases
            .iter()
            .filter(|case| case["body"]["format"] != "vendor")
        {
            let temporary = common::TempDirectory::new("http-profile-import")?;
            let root = temporary.path();
            let app = tokio::time::timeout(
                Duration::from_secs(5),
                router(
                    root.to_path_buf(),
                    "fixture-api",
                    "",
                    None,
                    "fixture-host",
                    BTreeMap::new(),
                ),
            )
            .await??;
            let start = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::post("/api/profiles/import")
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&case["input"])?))?,
                ),
            )
            .await??;
            let end = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let body: Value = serde_json::from_slice(&bytes)?;
            assert_eq!(body, case["body"], "{} response", case["name"]);
            let mut written = Map::new();
            for directory in ["dashboard", "profiles"] {
                if !root.join(directory).exists() {
                    continue;
                }
                for entry in fs::read_dir(root.join(directory))? {
                    let path = entry?.path();
                    if path.extension().is_none_or(|extension| extension != "json") {
                        continue;
                    }
                    let mut record: Value = serde_json::from_slice(&fs::read(path)?)?;
                    let id = record["id"].as_str().ok_or("ID missing")?;
                    assert_eq!(id.len(), 12);
                    assert!(
                        id.bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    );
                    let key = format!("{directory}/000000000001.json");
                    let clocks: &[&str] = if directory == "dashboard" {
                        &["_created"]
                    } else {
                        &["created_at", "updated_at"]
                    };
                    for field in clocks {
                        let clock = record[field].as_f64().ok_or("Clock missing")?;
                        assert!(clock >= start && clock <= end);
                        record[field] = case["written"][&key][field].clone();
                    }
                    record["id"] = "000000000001".into();
                    written.insert(key, record);
                }
            }
            assert_eq!(
                Value::Object(written),
                case["written"],
                "{} copies",
                case["name"]
            );
            let config: Value = if root.join("config.json").exists() {
                serde_json::from_slice(&fs::read(root.join("config.json"))?)?
            } else {
                Value::Object(Map::new())
            };
            assert_eq!(config, case["after_config"], "{} settings", case["name"]);
            temporary.cleanup()?;
            count += 1;
        }
        assert_eq!(count, 16);
        Ok(())
    })
}
