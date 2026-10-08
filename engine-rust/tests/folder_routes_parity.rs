use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::folder_routes::router;
use serde_json::Value;
use std::{fs, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn folder_http_transactions_match_original_and_never_delete_profiles()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/folder-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Folder HTTP cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("folder-route")?;
            let root = temporary.path().to_path_buf();
            fs::create_dir(root.join("dashboard"))?;
            let profiles = case["profiles"].as_object().ok_or("Profiles missing")?;
            let mut originals = Vec::new();
            for (filename, profile) in profiles {
                let bytes = serde_json::to_vec(profile)?;
                fs::write(root.join("dashboard").join(filename), &bytes)?;
                originals.push((filename, bytes));
            }
            fs::write(
                root.join("dashboard-config.json"),
                serde_json::to_vec(&case["metadata"])?,
            )?;
            let app = router(root.clone(), "fixture-api");
            let request = Request::post(case["path"].as_str().ok_or("Path missing")?)
                .header("authorization", "Bearer fixture-api")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&case["input"])?))?;
            let response =
                tokio::time::timeout(Duration::from_secs(5), app.oneshot(request)).await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let json_body = response
                .headers()
                .get("content-type")
                .is_some_and(|value| value.as_bytes().starts_with(b"application/json"));
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let body = if json_body {
                serde_json::from_slice::<Value>(&bytes)?
            } else {
                Value::String(String::from_utf8(bytes.to_vec())?)
            };
            assert_eq!(body, case["body"], "{}", case["name"]);
            for (filename, before) in originals {
                let bytes = fs::read(root.join("dashboard").join(filename))?;
                assert_eq!(
                    serde_json::from_slice::<Value>(&bytes)?,
                    case["written"][filename],
                    "{}",
                    case["name"]
                );
                assert_eq!(
                    Value::Bool(before == bytes),
                    case["unchanged"][filename],
                    "{}",
                    case["name"]
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(root.join("dashboard-config.json"))?)?,
                case["written_meta"],
                "{}",
                case["name"]
            );
            temporary.cleanup()?;
        }
        Ok(())
    })
}
