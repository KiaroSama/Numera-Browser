use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::token_routes::router;
use serde_json::Value;
use std::{fs, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn token_http_routes_filter_hashes_and_preserve_revoke_verdicts()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value = serde_json::from_str(include_str!("fixtures/token-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Token route cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("token-route")?;
            let root = temporary.path().to_path_buf();
            let path = root.join("tokens.json");
            let before = match case["table"].as_str() {
                Some(raw) => raw.as_bytes().to_vec(),
                None => serde_json::to_vec(&case["table"])?,
            };
            fs::write(&path, &before)?;
            let app = router(
                root,
                case["environment"].as_str().ok_or("Environment missing")?,
            );
            let mut request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?);
            for (key, value) in case["headers"].as_object().ok_or("Headers missing")? {
                request = request.header(key, value.as_str().ok_or("Header missing")?);
            }
            let body = if let Some(bytes) = case["wire_bytes"].as_array() {
                Body::from(
                    bytes
                        .iter()
                        .map(|byte| {
                            byte.as_u64()
                                .and_then(|byte| u8::try_from(byte).ok())
                                .ok_or("Fixture byte invalid")
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
            } else if let Some(wire_body) = case["wire_body"].as_str() {
                Body::from(wire_body.to_owned())
            } else if case["method"] == "POST" {
                request = request.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&case["input"])?)
            } else {
                Body::empty()
            };
            let response =
                tokio::time::timeout(Duration::from_secs(5), app.oneshot(request.body(body)?))
                    .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let body =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            assert_eq!(
                serde_json::from_slice::<Value>(&body)?,
                case["body"],
                "{}",
                case["name"]
            );
            let bytes = fs::read(path)?;
            let written = serde_json::from_slice::<Value>(&bytes).unwrap_or_else(|_| {
                Value::String(String::from_utf8(bytes.clone()).expect("Fixture is UTF-8"))
            });
            assert_eq!(written, case["written"]);
            assert_eq!(Value::Bool(bytes == before), case["unchanged"]);
            temporary.cleanup()?;
        }
        let temporary = common::TempDirectory::new("token-route-mint")?;
        let root = temporary.path().to_path_buf();
        let app = router(root.clone(), "");
        let request = Request::post("/api/tokens")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"name":"Client"}"#))?;
        let response =
            tokio::time::timeout(Duration::from_secs(5), app.clone().oneshot(request)).await??;
        assert_eq!(response.status(), 200);
        let bytes =
            tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                .await??;
        let mut minted: Value = serde_json::from_slice(&bytes)?;
        let token = minted["token"]
            .as_str()
            .ok_or("Minted token missing")?
            .to_owned();
        assert!(token.starts_with("nmr_"));
        assert_eq!(token.len(), 47);
        let id = minted["id"].as_str().ok_or("Minted id missing")?.to_owned();
        assert_eq!(id.len(), 12);
        assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let disk = fs::read(root.join("tokens.json"))?;
        assert!(
            !disk
                .windows(token.len())
                .any(|window| window == token.as_bytes()),
            "Token plaintext persisted"
        );
        let original: Value = serde_json::from_str(include_str!("fixtures/token-create.json"))?;
        let expected = &original["cases"][0]["result"];
        for field in ["id", "hash", "created", "token"] {
            minted[field] = expected[field].clone();
        }
        assert_eq!(minted, *expected);
        let denied = tokio::time::timeout(
            Duration::from_secs(5),
            app.clone()
                .oneshot(Request::get("/api/tokens").body(Body::empty())?),
        )
        .await??;
        assert_eq!(denied.status(), 401);
        let authenticated = tokio::time::timeout(
            Duration::from_secs(5),
            app.oneshot(
                Request::get("/api/tokens")
                    .header("authorization", format!("Bearer {token}"))
                    .body(Body::empty())?,
            ),
        )
        .await??;
        assert_eq!(authenticated.status(), 200);
        let bytes = tokio::time::timeout(
            Duration::from_secs(5),
            to_bytes(authenticated.into_body(), 8192),
        )
        .await??;
        let listed: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(listed["tokens"][0]["id"], id);
        assert!(listed["tokens"][0].get("hash").is_none());
        assert!(listed["tokens"][0].get("token").is_none());
        temporary.cleanup()?;
        Ok(())
    })
}
