use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::http::application::router;
use serde_json::{Map, Value};
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;
mod common;

#[test]
fn application_composes_auth_once_and_keeps_legacy_protection_after_token_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let fixture: Value =
            serde_json::from_str(include_str!("fixtures/application-routes.json"))?;
        let cases = fixture["cases"]
            .as_array()
            .ok_or("Application cases missing")?;
        assert!(!cases.is_empty());
        for case in cases {
            let temporary = common::TempDirectory::new("application-route")?;
            let root = temporary.path().to_path_buf();
            fs::write(
                root.join("tokens.json"),
                serde_json::to_vec(&case["table"])?,
            )?;
            let start = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            let app = tokio::time::timeout(
                Duration::from_secs(5),
                router(
                    root.clone(),
                    case["environment"].as_str().ok_or("Environment missing")?,
                    "https://fixture.test",
                    None,
                    std::collections::BTreeMap::from([
                        ("camoufox".into(), false),
                        ("cloak".into(), false),
                        ("patchright".into(), false),
                        ("playwright".into(), false),
                    ]),
                ),
            )
            .await??;
            let end = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64();
            if case["remove_tokens"] == true {
                fs::remove_file(root.join("tokens.json"))?;
            }
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
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{}",
                case["name"]
            );
            let cors: Map<String, Value> = response
                .headers()
                .iter()
                .filter(|(key, _)| {
                    key.as_str().starts_with("access-control-") || key.as_str() == "vary"
                })
                .map(|(key, value)| {
                    Ok((key.to_string(), Value::String(value.to_str()?.to_owned())))
                })
                .collect::<Result<_, axum::http::header::ToStrError>>()?;
            assert_eq!(Value::Object(cors), case["cors"], "{}", case["name"]);
            let json_body = response
                .headers()
                .get("content-type")
                .is_some_and(|value| value.as_bytes().starts_with(b"application/json"));
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            let mut body = if bytes.is_empty() {
                Value::Null
            } else if json_body {
                serde_json::from_slice(&bytes)?
            } else {
                Value::String(String::from_utf8(bytes.to_vec())?)
            };
            if case["name"] == "config-read" {
                assert_eq!(body["root"], root.to_string_lossy().as_ref());
                body["root"] = "<fixture-root>".into();
            }
            assert_eq!(body, case["body"], "{}", case["name"]);
            let mut policy = match fs::read(root.join("auth-policy.json")) {
                Ok(bytes) => serde_json::from_slice::<Value>(&bytes)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Null,
                Err(error) => return Err(error.into()),
            };
            if !policy.is_null() {
                let since = policy["since"].as_f64().ok_or("Policy clock missing")?;
                assert!(since >= start && since <= end);
                policy["since"] = case["policy"]["since"].clone();
            }
            assert_eq!(policy, case["policy"], "{}", case["name"]);
            temporary.cleanup()?;
        }
        Ok(())
    })
}
