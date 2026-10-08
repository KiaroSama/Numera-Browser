use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::proxy_routes::router;
use serde_json::Value;
use std::{fs, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn proxy_http_preserves_credential_intent_and_resets_changed_address_health()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/proxy-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Proxy HTTP cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("proxy-route")?;
            let root = temporary.path().to_path_buf();
            fs::create_dir(root.join("dashboard"))?;
            for (filename, profile) in case["profiles"].as_object().ok_or("Profiles missing")? {
                fs::write(
                    root.join("dashboard").join(filename),
                    serde_json::to_vec(profile)?,
                )?;
            }
            if !case["config"].is_null() {
                fs::write(
                    root.join("config.json"),
                    serde_json::to_vec(&case["config"])?,
                )?;
            }
            let path = root.join("proxies.json");
            let before = serde_json::to_vec(&case["rows"])?;
            fs::write(&path, &before)?;
            let app = router(root, "fixture-api");
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
            let bytes = fs::read(path)?;
            let mut written: Value = serde_json::from_slice(&bytes)?;
            if case["name"] == "add-new" {
                let id = body["proxies"][0]["id"]
                    .as_str()
                    .ok_or("Added proxy ID missing")?
                    .to_owned();
                assert_eq!(id.len(), 12);
                assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
                let last = written
                    .as_array_mut()
                    .ok_or("Written proxies missing")?
                    .last_mut()
                    .ok_or("Added row missing")?;
                assert_eq!(last["id"], id);
                let expected = case["body"]["proxies"][0]["id"].clone();
                body["proxies"][0]["id"] = expected.clone();
                last["id"] = expected;
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            assert_eq!(written, case["written"], "{}", case["name"]);
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
