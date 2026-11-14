use super::{BTreeMap, Body, Duration, Request, ServiceExt, Value, common, fs, to_bytes};
use numera_core::{http::application, ownership::registry::Registry};
use serde_json::json;
use std::sync::Arc;

#[test]
fn settings_http_preserves_original_text_refusals_and_short_circuits()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("strict-settings-http")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/settings_http_preserves_original_text_refusals_and_short_circuits.json"
    ))?;
    assert_eq!(cases.len(), 42);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        for (index, case) in cases.iter().enumerate() {
            let root = temporary.path().join(format!("native-{index}"));
            fs::create_dir(&root)?;
            fs::create_dir(root.join("dashboard"))?;
            let profile =
                serde_json::to_vec(&json!({"id":"fixture","name":"Fixture","engine":"camoufox"}))?;
            let stored = root.join("dashboard/fixture.json");
            fs::write(&stored, &profile)?;
            let path = root.join("config.json");
            let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
            if let Some(raw) = &raw {
                fs::write(&path, raw)?;
            }
            let sessions = Arc::new(Registry::new(root.clone(), "fixture-host".into(), None)?);
            let installed = BTreeMap::from([
                ("camoufox".into(), false),
                ("cloak".into(), false),
                ("patchright".into(), false),
                ("playwright".into(), true),
            ]);
            let app = application::router_with_sessions(
                root.clone(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                application::SessionApplication {
                    installed,
                    sessions: sessions.clone(),
                },
            )
            .await?;
            let request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["url"].as_str().ok_or("URL missing")?)
                .header("authorization", "Bearer fixture-api")
                .header("content-type", "application/json")
                .body(if case["body"].is_null() {
                    Body::empty()
                } else {
                    Body::from(serde_json::to_vec(&case["body"])?)
                })?;
            let response =
                tokio::time::timeout(Duration::from_secs(5), app.oneshot(request)).await??;
            assert_eq!(
                json!(response.status().as_u16()),
                case["status"],
                "Strict settings HTTP status differs: case={index} {}",
                case["url"]
            );
            let bytes = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 16384),
            )
            .await??;
            let mut value = serde_json::from_slice::<Value>(&bytes)
                .unwrap_or(Value::String(String::from_utf8(bytes.to_vec())?));
            if value.get("root").is_some() {
                assert_eq!(value["root"], json!(root));
                value["root"] = "fixture-root".into();
            }
            assert_eq!(
                value, case["result"],
                "HTTP body differs: case={index} {}",
                case["url"]
            );
            assert_eq!(fs::read(path).ok(), raw, "HTTP changed settings bytes");
            assert_eq!(fs::read(stored)?, profile, "HTTP changed profile bytes");
            assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    temporary.cleanup()?;
    Ok(())
}
