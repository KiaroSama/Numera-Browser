use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::config_routes::router;
use serde_json::Value;
use std::{collections::BTreeMap, fs, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn settings_http_validates_every_field_before_one_write_and_reports_stored_pool()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/config-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Settings cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("config-route")?;
            let root = temporary.path().to_path_buf();
            fs::create_dir(root.join("dashboard"))?;
            for (filename, profile) in case["profiles"].as_object().ok_or("Profiles missing")? {
                fs::write(
                    root.join("dashboard").join(filename),
                    serde_json::to_vec(profile)?,
                )?;
            }
            fs::write(
                root.join("dashboard-config.json"),
                serde_json::to_vec(&case["metadata"])?,
            )?;
            fs::write(
                root.join("proxies.json"),
                serde_json::to_vec(&case["proxies"])?,
            )?;
            let path = root.join("config.json");
            let before = serde_json::to_vec(&case["config"])?;
            fs::write(&path, &before)?;
            let installed = case["installed"]
                .as_object()
                .ok_or("Backend discovery missing")?
                .iter()
                .map(|(name, available)| {
                    Ok((
                        name.clone(),
                        available.as_bool().ok_or("Backend availability missing")?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>, &str>>()?;
            let app = router(root.clone(), "fixture-api", installed);
            let request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?)
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
            let mut body: Value = if json_body {
                serde_json::from_slice(&bytes)?
            } else {
                Value::String(String::from_utf8(bytes.to_vec())?)
            };
            if case["status"] == 200 && case["path"] == "/api/config" {
                assert_eq!(body["root"], root.to_string_lossy().as_ref());
                body["root"] = "<fixture-root>".into();
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            let bytes = fs::read(path)?;
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes)?,
                case["written"],
                "{}",
                case["name"]
            );
            assert_eq!(
                Value::Bool(bytes == before),
                case["unchanged"],
                "{}",
                case["name"]
            );
            temporary.cleanup()?;
        }
        Ok(())
    })
}
