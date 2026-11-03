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
fn documentation_routes_match_source_without_sdk_or_api_unlock()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture::{self, Limits};
    let temporary = common::TempDirectory::new("documentation routes")?;
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut source = std::process::Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Source interpreter required")?,
    );
    source.args(["-c", "import json,os; from pathlib import Path; from documentation_cases import cases; print(json.dumps(cases(Path(os.environ['FIXTURE_ROOT']))))"])
        .env("FIXTURE_ROOT", temporary.path())
        .env("PYTHONPATH", std::env::join_paths([
            repository.join("scripts/migration"), repository.join("engine")
        ])?)
        .env("PYTHONUTF8", "1");
    let captured = capture::run(
        &mut source,
        Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(captured.exit, 0);
    let cases: Vec<Value> = serde_json::from_slice(&captured.stdout)?;
    assert_eq!(cases.len(), 12);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let app = tokio::time::timeout(
            Duration::from_secs(5),
            router(
                temporary.path().join("native"),
                "documentation-fixture-only",
                "",
                None,
                "fixture-host",
                Default::default(),
            ),
        )
        .await??;
        for case in &cases {
            let response = tokio::time::timeout(
                Duration::from_secs(5),
                app.clone().oneshot(
                    Request::builder()
                        .method(case["method"].as_str().ok_or("Method missing")?)
                        .uri(case["path"].as_str().ok_or("Path missing")?)
                        .body(Body::empty())?,
                ),
            )
            .await??;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Status missing")?,
                "{} {}",
                case["method"],
                case["path"]
            );
            assert_eq!(
                response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok()),
                case["content_type"].as_str(),
                "{}",
                case["path"]
            );
            let allow = response
                .headers()
                .get("allow")
                .and_then(|v| v.to_str().ok());
            let methods = |value: Option<&str>| {
                value.map(|value| {
                    value
                        .split(',')
                        .map(|method| method.trim().to_owned())
                        .collect::<std::collections::BTreeSet<_>>()
                })
            };
            assert_eq!(
                methods(allow),
                methods(case["allow"].as_str()),
                "{} {} allowed methods",
                case["method"],
                case["path"]
            );
            assert_eq!(
                response
                    .headers()
                    .get("content-length")
                    .and_then(|v| v.to_str().ok()),
                case["content_length"].as_str(),
                "{} {} content length",
                case["method"],
                case["path"]
            );
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 65536),
            )
            .await??;
            let text = std::str::from_utf8(&bytes)?;
            let expected = case["body"].as_str().ok_or("Body missing")?;
            if case["name"] == "openapi.json" && case["method"] == "GET" {
                assert_eq!(
                    serde_json::from_str::<Value>(text)?,
                    serde_json::from_str::<Value>(expected)?
                );
            } else {
                assert_eq!(text, expected, "{} {}", case["method"], case["path"]);
            }
            if case["method"] == "GET" {
                let asset = repository
                    .join("engine-rust/src/http/documentation-assets")
                    .join(case["name"].as_str().ok_or("Asset name missing")?);
                assert_eq!(
                    fs::read_to_string(asset)?,
                    expected,
                    "Generated documentation asset drift"
                );
            }
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}

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
                    "fixture-host",
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
