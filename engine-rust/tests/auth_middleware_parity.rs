use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
    routing::any,
};
use numera_core::http::{middleware::protect, request_auth::RequestAuth, tokens::TokenStore};
use serde_json::Value;
use std::{fs, sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn actual_router_middleware_enforces_source_per_request_auth_verdicts()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/request-auth.json"))?;
        let cases = fixture["cases"].as_array().ok_or("Auth cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("auth-middleware")?;
            let root = temporary.path().to_path_buf();
            fs::write(
                root.join("tokens.json"),
                match case["table"].as_str() {
                    Some(raw) => raw.as_bytes().to_vec(),
                    None => serde_json::to_vec(&case["table"])?,
                },
            )?;
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
            auth.adopt(1700000000.0)?;
            let app = protect(
                Router::new().fallback(any(|| async { "downstream-fixture" })),
                auth,
            );
            let mut request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?);
            for (key, value) in case["headers"].as_object().ok_or("Headers missing")? {
                request = request.header(key, value.as_str().ok_or("Header value missing")?);
            }
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.oneshot(request.body(Body::empty())?),
            )
            .await??;
            assert_eq!(
                Value::Bool(response.status() == 401),
                case["denied"],
                "{}",
                case["name"]
            );
            let status = response.status();
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 4096))
                    .await??;
            if status == 401 {
                assert_eq!(
                    serde_json::from_slice::<Value>(&bytes)?,
                    case["denial_body"]
                );
            } else {
                assert_eq!(bytes.as_ref(), b"downstream-fixture");
            }
            temporary.cleanup()?;
        }
        Ok(())
    })
}
