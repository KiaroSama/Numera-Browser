use axum::{
    Json, Router,
    body::{Body, to_bytes},
    http::Request,
    routing::any,
};
use numera_core::http::{
    cors::apply, middleware::protect, request_auth::RequestAuth, tokens::TokenStore,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn cors_preserves_original_status_body_headers_and_auth_layer_order()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/cors.json"))?;
        let cases = fixture["cases"].as_array().ok_or("CORS cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("cors")?;
            let auth = Arc::new(RequestAuth::new(
                TokenStore::new(temporary.path().to_path_buf()),
                case["token"].as_str().ok_or("Token missing")?,
            ));
            let router = Router::new().fallback(any(|request: Request<Body>| async move {
                if request.method() == "OPTIONS" {
                    (
                        axum::http::StatusCode::METHOD_NOT_ALLOWED,
                        Json(serde_json::json!({"detail":"Method Not Allowed"})),
                    )
                } else {
                    (
                        axum::http::StatusCode::OK,
                        Json(serde_json::json!({"fixture":true})),
                    )
                }
            }));
            let router = protect(
                apply(router, case["origins"].as_str().ok_or("Origins missing")?),
                auth,
            );
            let mut request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?);
            for (key, value) in case["headers"].as_object().ok_or("Headers missing")? {
                request = request.header(key, value.as_str().ok_or("Header missing")?);
            }
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                router.oneshot(request.body(Body::empty())?),
            )
            .await??;
            assert_eq!(
                response.status().as_u16() as u64,
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let mut observed = serde_json::Map::new();
            for (key, value) in response.headers() {
                if key.as_str().starts_with("access-control-")
                    || ["vary", "content-type"].contains(&key.as_str())
                {
                    observed.insert(key.as_str().into(), value.to_str()?.into());
                }
            }
            assert_eq!(
                Value::Object(observed),
                case["response_headers"],
                "{}",
                case["name"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 4096))
                    .await??;
            if let Some(body) = case["body"].as_str() {
                assert_eq!(bytes.as_ref(), body.as_bytes(), "{}", case["name"]);
            }
            temporary.cleanup()?;
        }
        Ok(())
    })
}
