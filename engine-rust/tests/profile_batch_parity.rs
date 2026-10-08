use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::application::router;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;

fn now() -> Result<f64, Box<dyn std::error::Error>> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64())
}
#[test]
fn profile_batches_preserve_source_atomic_copies_and_patch_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-batch.json"))?;
        let cases = fixture["cases"].as_array().ok_or("Batch cases missing")?;
        assert_eq!(cases.len(), 21);
        for case in cases {
            let temporary = common::TempDirectory::new("profile-batch")?;
            let root = temporary.path();
            let initial = case["initial"].as_object().ok_or("Initial missing")?;
            let mut before = BTreeMap::new();
            for (path, record) in initial {
                let path = root.join(path);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(record)?;
                fs::write(&path, &bytes)?;
                before.insert(path, bytes);
            }
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
            let start = now()?;
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::post(format!(
                        "/api/profiles/{}",
                        case["route"].as_str().ok_or("Route missing")?
                    ))
                    .header("authorization", "Bearer fixture-api")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_vec(&case["input"])?))?,
                ),
            )
            .await??;
            let end = now()?;
            let status = response.status().as_u16();
            assert_eq!(
                u64::from(status),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 32768),
            )
            .await??;
            let mut body: Value = serde_json::from_slice(&bytes)?;
            let mut ids = BTreeMap::new();
            if status == 200 && case["route"] == "batch" {
                let actual = body["ids"].as_array().ok_or("IDs missing")?;
                let expected = case["body"]["ids"].as_array().ok_or("Source IDs missing")?;
                assert_eq!(actual.len(), expected.len());
                for (actual, expected) in actual.iter().zip(expected) {
                    let id = actual.as_str().ok_or("ID not text")?;
                    assert_eq!(id.len(), 12);
                    assert!(
                        id.bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    );
                    assert!(
                        ids.insert(
                            id.to_owned(),
                            expected.as_str().ok_or("Source ID not text")?.to_owned()
                        )
                        .is_none()
                    );
                }
                body["ids"] = case["body"]["ids"].clone();
            }
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
                    let mut record: Value = serde_json::from_slice(&fs::read(&path)?)?;
                    let id = record["id"]
                        .as_str()
                        .ok_or("Written ID missing")?
                        .to_owned();
                    let expected_id = ids.get(&id).map_or(id.as_str(), String::as_str);
                    let key = format!("{directory}/{expected_id}.json");
                    if status == 200 {
                        let changed = case["written"][&key] != case["initial"][&key];
                        let clocks: &[&str] = if !changed {
                            &[]
                        } else if directory == "profiles" {
                            &["created_at", "updated_at"]
                        } else if case["route"] == "batch" {
                            &["_created"]
                        } else {
                            &[]
                        };
                        for field in clocks {
                            let value = record[field].as_f64().ok_or("Clock missing")?;
                            assert!(
                                value >= start && value <= end,
                                "{field} outside request bounds"
                            );
                            record[field] = case["written"][&key][field].clone();
                        }
                        record["id"] = expected_id.into();
                    }
                    written.insert(key, record);
                }
            }
            if status != 200 {
                for (path, bytes) in before {
                    assert_eq!(fs::read(path)?, bytes, "Refusal changed stored bytes");
                }
            }
            assert_eq!(body, case["body"], "{} response", case["name"]);
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
        }
        Ok(())
    })
}
