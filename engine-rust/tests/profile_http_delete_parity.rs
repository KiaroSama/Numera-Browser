use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::application::router;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fs, time::Duration};
use tower::ServiceExt;
mod common;
#[test]
fn profile_deletion_preserves_source_busy_refusals_and_complete_side_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/profile-http-delete.json"))?;
        let cases = fixture["cases"].as_array().ok_or("Delete cases missing")?;
        assert_eq!(cases.len(), 7);
        for case in cases {
            let temporary = common::TempDirectory::new("http-profile-delete")?;
            let root = temporary.path();
            let mut before = BTreeMap::new();
            for (path, record) in case["initial"].as_object().ok_or("Initial missing")? {
                let path = root.join(path);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(record)?;
                fs::write(&path, &bytes)?;
                before.insert(path, bytes);
            }
            fs::create_dir_all(root.join("user-data/a"))?;
            fs::write(root.join("user-data/a/state.txt"), b"fixture state")?;
            if let Some(claim) = case["claim"].as_str() {
                fs::create_dir(root.join("sessions"))?;
                fs::write(root.join("sessions/a.json"), claim.as_bytes())?;
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
            let body = if case["input"].is_null() {
                Body::empty()
            } else {
                Body::from(serde_json::to_vec(&case["input"])?)
            };
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(
                    Request::builder()
                        .method(case["method"].as_str().ok_or("Method missing")?)
                        .uri(case["url"].as_str().ok_or("URL missing")?)
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(body)?,
                ),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes)?,
                case["body"],
                "{} response",
                case["name"]
            );
            let mut written = Map::new();
            for directory in ["dashboard", "profiles"] {
                if root.join(directory).exists() {
                    for entry in fs::read_dir(root.join(directory))? {
                        let path = entry?.path();
                        if path.extension().is_some_and(|ext| ext == "json") {
                            written.insert(
                                format!(
                                    "{directory}/{}",
                                    path.file_name()
                                        .ok_or("Name missing")?
                                        .to_str()
                                        .ok_or("Name not text")?
                                ),
                                serde_json::from_slice(&fs::read(path)?)?,
                            );
                        }
                    }
                }
            }
            assert_eq!(
                Value::Object(written),
                case["written"],
                "{} copies",
                case["name"]
            );
            assert_eq!(
                root.join("user-data/a/state.txt").exists(),
                case["session_present"]
                    .as_bool()
                    .ok_or("Session flag missing")?
            );
            assert_eq!(
                root.join("sessions/a.json").exists(),
                case["claim_present"]
                    .as_bool()
                    .ok_or("Claim flag missing")?
            );
            if case["session_present"] == true {
                for (path, bytes) in before {
                    assert_eq!(fs::read(path)?, bytes, "Refusal changed bytes");
                }
            }
            temporary.cleanup()?;
        }
        Ok(())
    })
}
