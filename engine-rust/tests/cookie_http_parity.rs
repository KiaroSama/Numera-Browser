use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{
    browser::runtime::Runtime,
    http::application::{SessionApplication, router_with_browser},
    ownership::registry::Registry,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;
use numera_core as core;
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "common/http_fixture.rs"]
mod http_fixture;

#[test]
fn cookie_http_uses_native_format_policy_and_owned_sdk() -> Result<(), Box<dyn std::error::Error>> {
    let document = http_fixture::oracle("cookie-http")?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Cookie HTTP cases missing")?;
    assert_eq!(cases.len(), 33);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for case in cases {
        let temporary = common::TempDirectory::new("cookie-http-native")?;
        let root = temporary.path();
        if !case["record"].is_null() {
            std::fs::create_dir_all(root.join("dashboard"))?;
            std::fs::write(
                root.join("dashboard/a.json"),
                serde_json::to_vec(&case["record"])?,
            )?;
        }
        if let Some(claim) = case["claim"].as_str() {
            std::fs::create_dir_all(root.join("sessions"))?;
            std::fs::write(root.join("sessions/a.json"), claim)?;
        }
        let mut probe = json!({"observation":case["probe"],"alignment":case["alignment"]});
        if !case["network"].is_null() {
            probe["observations"] = case["network"].clone();
        }
        let adapter = fixture::executable(
            &root.join("sdk"),
            "cli-probe",
            json!({
                "case":probe,"store":root,"profile_id":"a","seen_style":"operations",
                "cookies":[{"name":"fixture","value":"synthetic","domain":".fixture.test"}]
            }),
        )?;
        let sessions = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
        let browser = Runtime {
            adapter,
            cwd: root.into(),
            home: root.into(),
            logs: root.join("logs"),
            country: None,
            relay: None,
            binary_options: Default::default(),
            registry: None,
            deployment: None,
        };
        runtime.block_on(async {
            let app = router_with_browser(
                root.into(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                SessionApplication {
                    installed: BTreeMap::new(),
                    sessions: sessions.clone(),
                },
                Some(browser),
            )
            .await?;
            let mut request = Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Source route path missing")?);
            if case["authorized"] == true {
                request = request.header("authorization", "Bearer fixture-api");
            }
            let body = if case["request"].is_null() {
                Body::empty()
            } else {
                request = request.header("content-type", "application/json");
                Body::from(serde_json::to_vec(&case["request"])?)
            };
            let response =
                tokio::time::timeout(Duration::from_secs(20), app.oneshot(request.body(body)?))
                    .await??;
            assert_eq!(
                response.status().as_u16(),
                case["status"].as_u64().ok_or("Status missing")? as u16,
                "{}",
                case["name"]
            );
            let body = tokio::time::timeout(
                Duration::from_secs(5),
                to_bytes(response.into_body(), 65536),
            )
            .await??;
            let mut actual: Value = serde_json::from_slice(&body)?;
            if case["name"]
                .as_str()
                .is_some_and(|name| matches!(name, "proxy-healthy" | "proxy-unhealthy"))
            {
                assert!(actual["latencyMs"].as_u64().is_some());
                actual["latencyMs"] = json!(125);
            }
            if matches!(
                case["name"].as_str(),
                Some("empty" | "empty-clear" | "invalid-clear")
            ) {
                let expected = case["body"]["detail"]
                    .as_str()
                    .ok_or("Source empty-jar refusal missing")?;
                let actual = actual["detail"]
                    .as_str()
                    .ok_or("Native empty-jar refusal missing")?;
                for detail in [expected, actual] {
                    assert!(
                        detail.starts_with("No usable cookies found in "),
                        "{detail}"
                    );
                    assert!(detail.ends_with("cookies.json."), "{detail}");
                    let path = detail
                        .strip_prefix("No usable cookies found in ")
                        .and_then(|s| s.strip_suffix('.'))
                        .ok_or("Temporary path missing")?;
                    assert!(
                        !std::path::Path::new(path)
                            .parent()
                            .ok_or("Temporary parent missing")?
                            .exists(),
                        "Empty-jar scratch leaked"
                    );
                }
            } else if case["name"] == "align-success" {
                let mut expected = case["body"].clone();
                for profile in [&mut actual["profile"], &mut expected["profile"]] {
                    let object = profile.as_object_mut().ok_or("Aligned profile missing")?;
                    for key in ["_created", "_updated", "_rev"] {
                        object.remove(key);
                    }
                    assert!(object["_trust"]["at"].as_f64().is_some());
                    object
                        .get_mut("_trust")
                        .ok_or("Alignment cache missing")?
                        .as_object_mut()
                        .ok_or("Alignment cache invalid")?
                        .remove("at");
                }
                assert_eq!(actual, expected, "Aligned HTTP output");
            } else {
                assert_eq!(actual, case["body"], "{}", case["name"]);
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        })?;
        let seen = root.join("sdk.seen");
        let activated = seen.exists();
        assert_eq!(
            activated,
            matches!(
                case["name"].as_str(),
                Some(
                    "rows"
                        | "text-replace"
                        | "export"
                        | "tls-healthy"
                        | "tls-unhealthy"
                        | "dns-healthy"
                        | "dns-unhealthy"
                        | "proxy-healthy"
                        | "proxy-unhealthy"
                        | "trust-healthy"
                        | "trust-unhealthy"
                        | "align-success"
                )
            )
        );
        if activated {
            let calls: Value = serde_json::from_slice(&std::fs::read(seen)?)?;
            assert_eq!(
                calls.as_array().and_then(|v| v.last()),
                Some(&json!("close"))
            );
            if case["name"] == "text-replace" {
                assert_eq!(calls[2], "cookies-clear");
                assert_eq!(calls[3], "cookies-add");
            }
        }
        if case["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("trust-"))
            && !case["record"].is_null()
        {
            let stored: Value =
                serde_json::from_slice(&std::fs::read(root.join("dashboard/a.json"))?)?;
            if case["trust"].is_null() {
                assert!(stored["_trust"].is_null());
            } else {
                assert_eq!(stored["_trust"]["grade"], case["trust"]["grade"]);
                assert_eq!(stored["_trust"]["score"], case["trust"]["score"]);
                assert!(stored["_trust"]["at"].as_f64().is_some());
            }
        }
        assert!(!sessions.owns("a")?);
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        drop(sessions);
        temporary.cleanup()?;
    }
    Ok(())
}
