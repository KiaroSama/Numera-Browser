use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::{diagnostics::router, request_auth::RequestAuth, tokens::TokenStore};
use serde_json::Value;
use std::{fs, sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn public_ready_and_protected_health_preserve_wire_and_method_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/diagnostic-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Diagnostic cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("diagnostic-route")?;
            let root = temporary.path().to_path_buf();
            if !case["policy"].is_null() {
                fs::write(
                    root.join("auth-policy.json"),
                    serde_json::to_vec(&case["policy"])?,
                )?;
            }
            let auth = Arc::new(RequestAuth::new(
                TokenStore::new(root),
                case["environment"].as_str().ok_or("Env missing")?,
            ));
            let app = router(auth);
            let mut request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?);
            for (key, value) in case["headers"].as_object().ok_or("Headers missing")? {
                request = request.header(key, value.as_str().ok_or("Header missing")?);
            }
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(request.body(Body::empty())?),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            assert_eq!(
                response
                    .headers()
                    .get("content-type")
                    .and_then(|value| value.to_str().ok()),
                case["content_type"].as_str()
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 4096))
                    .await??;
            let observed = if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes)?
            };
            assert_eq!(observed, case["body"], "{}", case["name"]);
            temporary.cleanup()?;
        }
        Ok(())
    })
}
