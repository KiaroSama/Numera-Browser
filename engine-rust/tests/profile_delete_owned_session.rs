use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{
    http::application::{SessionApplication, router_with_sessions},
    ownership::registry::{Kind, Registry},
    storage::dashboard::DashboardStore,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    process::{Command, Stdio},
    sync::Arc,
    time::Duration,
};
use tower::ServiceExt;
mod common;
#[test]
fn http_delete_stops_only_its_actual_registered_session_before_removing_profile()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("http-owned-delete")?;
    let root = temporary.path();
    DashboardStore::new(root.to_path_buf(), None).save(
        json!({"id":"a","name":"A","seed":42}),
        Default::default(),
        None,
        None,
        1700000000.0,
    )?;
    let sessions = Arc::new(Registry::new(
        root.to_path_buf(),
        "fixture-host".into(),
        None,
    )?);
    let ready = root.join("ready.json");
    let script = "import json,os,pathlib,threading; p=pathlib.Path(os.environ['FIXTURE_READY']); t=p.with_suffix('.part'); t.write_text(json.dumps({'ok':True}),encoding='utf-8'); t.replace(p); threading.Event().wait()";
    let mut command = Command::new(python);
    command
        .args(["-u", "-c", script])
        .env("FIXTURE_READY", &ready)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    sessions.spawn(
        "a",
        &mut command,
        &ready,
        Kind::Launch,
        None,
        Duration::from_secs(5),
    )?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async {
        let app = tokio::time::timeout(
            Duration::from_secs(5),
            router_with_sessions(
                root.to_path_buf(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                SessionApplication {
                    installed: BTreeMap::new(),
                    sessions: sessions.clone(),
                },
            ),
        )
        .await??;
        let response = tokio::time::timeout(
            Duration::from_secs(15),
            app.oneshot(
                Request::delete("/api/profiles/a")
                    .header("authorization", "Bearer fixture-api")
                    .body(Body::empty())?,
            ),
        )
        .await??;
        assert_eq!(response.status().as_u16(), 200);
        let bytes =
            tokio::time::timeout(Duration::from_secs(5), to_bytes(response.into_body(), 8192))
                .await??;
        assert_eq!(serde_json::from_slice::<Value>(&bytes)?, json!({"ok":true}));
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    assert!(!sessions.owns("a")?);
    assert!(!sessions.running("a")?);
    assert!(!root.join("dashboard/a.json").exists());
    assert!(!root.join("profiles/a.json").exists());
    assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
    drop(sessions);
    temporary.cleanup()?;
    Ok(())
}
