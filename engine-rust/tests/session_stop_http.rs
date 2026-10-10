use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{
    browser::adapter::Adapter,
    http::application::{SessionApplication, router_with_sessions},
    ownership::{claims, registry::Registry},
    storage::dashboard::DashboardStore,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command, sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn http_stop_preserves_source_refusals_and_closes_only_its_native_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let source_root = common::TempDirectory::new("http-stop-source")?;
    let module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/migration");
    let script = r#"
import json, os, sys
request = json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1, 'id':request['id'], 'ok':True, 'result':{}}), flush=True)
request = json.loads(sys.stdin.readline())
from session_stop_cases import cases
print(json.dumps({'protocol':1, 'id':request['id'], 'ok':True, 'result':{'cases':cases(os.environ['FIXTURE_ROOT'])}}), flush=True)
"#;
    let mut command = Command::new(&python);
    command
        .args(["-u", "-c", script])
        .env("PYTHONPATH", &module)
        .env("FIXTURE_ROOT", source_root.path().join("cases"))
        .env("PYTHONUTF8", "1");
    let mut source = Adapter::spawn(
        source_root.path(),
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call("observe-stop", Default::default(), Duration::from_secs(30))?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    source_root.cleanup()?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Stop source cases missing")?;
    assert_eq!(cases.len(), 4);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for case in cases {
        let temporary = common::TempDirectory::new("http-stop-parity")?;
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
            std::fs::write(root.join("sessions/a.json"), claim.as_bytes())?;
        }
        let before = std::fs::read(root.join("dashboard/a.json")).ok();
        let sessions = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
        let body = runtime.block_on(async {
            let app = router_with_sessions(
                root.into(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                SessionApplication {
                    installed: BTreeMap::new(),
                    sessions: sessions.clone(),
                },
            )
            .await?;
            let mut request = Request::post("/api/profiles/a/stop");
            if case["authorized"] == true {
                request = request.header("authorization", "Bearer fixture-api");
            }
            let response = tokio::time::timeout(
                Duration::from_secs(15),
                app.oneshot(request.body(Body::empty())?),
            )
            .await??;
            assert_eq!(
                response.status().as_u16(),
                case["status"].as_u64().ok_or("Status missing")? as u16,
                "{}",
                case["name"]
            );
            let bytes =
                tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                    .await??;
            Ok::<Value, Box<dyn std::error::Error>>(serde_json::from_slice(&bytes)?)
        })?;
        assert_eq!(body, case["body"], "{}", case["name"]);
        assert_eq!(std::fs::read(root.join("dashboard/a.json")).ok(), before);
        assert!(!root.join("profiles/a.json").exists());
        assert!(!sessions.owns("a")?);
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        drop(sessions);
        temporary.cleanup()?;
    }
    let temporary = common::TempDirectory::new("http-stop-owned")?;
    let root = temporary.path();
    let store = DashboardStore::new(root.into(), None);
    let original = store.save(
        json!({"id":"a", "name":"Fixture", "seed":42}),
        Default::default(),
        None,
        None,
        1700000000.0,
    )?;
    let sessions = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
    let script = r#"
import json, sys
for line in sys.stdin:
    request = json.loads(line)
    result = {'closing':True} if request['operation'] == 'close' else {}
    print(json.dumps({'protocol':1, 'id':request['id'], 'ok':True, 'result':result}), flush=True)
    if request['operation'] == 'close': break
"#;
    let mut command = Command::new(&python);
    command.args(["-u", "-c", script]);
    let adapter = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    sessions.retain_browser("a", adapter)?;
    let body = runtime.block_on(async {
        let app = router_with_sessions(
            root.into(),
            "fixture-api",
            "",
            None,
            "fixture-host",
            SessionApplication {
                installed: BTreeMap::new(),
                sessions: sessions.clone(),
            },
        )
        .await?;
        let response = tokio::time::timeout(
            Duration::from_secs(15),
            app.oneshot(
                Request::post("/api/profiles/a/stop")
                    .header("authorization", "Bearer fixture-api")
                    .body(Body::empty())?,
            ),
        )
        .await??;
        assert_eq!(response.status().as_u16(), 200);
        let bytes =
            tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                .await??;
        Ok::<Value, Box<dyn std::error::Error>>(serde_json::from_slice(&bytes)?)
    })?;
    let mut expected = original;
    expected["status"] = "stopped".into();
    expected["lastActive"] = "never".into();
    assert_eq!(body, expected);
    assert!(!sessions.owns("a")?);
    assert!(!sessions.running("a")?);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    assert!(
        store.get("a")?.ok_or("Stopped profile missing")?["_last_active"]
            .as_f64()
            .is_some_and(|time| time > 1700000000.0)
    );
    assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
    drop(sessions);
    temporary.cleanup()?;
    Ok(())
}
