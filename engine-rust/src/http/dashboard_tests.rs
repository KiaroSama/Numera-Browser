use super::*;
use crate::browser::adapter::Adapter;
use crate::common;
use axum::body::to_bytes;
use std::{process::Command, time::Duration};

#[test]
fn dashboard_files_and_spa_fallback_match_original_without_swallowing_api()
-> Result<(), Box<dyn std::error::Error>> {
    let root = common::TempDirectory::new("dashboard-response")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Source interpreter required")?;
    let mut command = Command::new(python);
    command.args(["-u","-c",r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from dashboard_cases import cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':cases(os.environ['FIXTURE_ROOT'])}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",root.path().join("source"))
        .env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        root.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-dashboard",
        Default::default(),
        Duration::from_secs(20),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let build = root.path().join("source/web");
    let router = mount(Router::new(), Some(&build))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let rows = document["cases"]
            .as_array()
            .ok_or("Dashboard cases missing")?;
        assert_eq!(rows.len(), 14);
        for case in rows {
            let request = axum::http::Request::builder()
                .method(case["method"].as_str().ok_or("Method missing")?)
                .uri(case["path"].as_str().ok_or("Path missing")?)
                .body(Body::empty())?;
            let response = router.clone().oneshot(request).await?;
            assert_eq!(
                u16::from(response.status()),
                case["status"].as_u64().ok_or("Status missing")? as u16,
                "{}",
                case["path"]
            );
            assert_eq!(
                response.headers()["content-type"].to_str()?,
                case["type"].as_str().ok_or("Type missing")?
            );
            let body = to_bytes(response.into_body(), 65536).await?;
            let text = String::from_utf8(body.to_vec())?;
            if case["status"] == 200 || case["method"] == "HEAD" {
                assert_eq!(text, case["body"].as_str().ok_or("Body missing")?);
            } else {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&text)?,
                    serde_json::from_str::<serde_json::Value>(
                        case["body"].as_str().ok_or("Error body missing")?
                    )?
                );
            }
        }
        let protected = super::super::application::router_with_dashboard(
            root.path().join("native-protected"),
            "fixture-api-token",
            "",
            None,
            "fixture-host",
            super::super::application::SessionApplication {
                installed: Default::default(),
                sessions: std::sync::Arc::new(crate::ownership::registry::Registry::new(
                    root.path().join("native-protected"),
                    "fixture-host".into(),
                    None,
                )?),
            },
            (None, Some(&build)),
        )
        .await?;
        let authenticated = document["authenticated"]
            .as_array()
            .ok_or("Protected dashboard cases missing")?;
        assert_eq!(authenticated.len(), 4);
        for case in authenticated {
            let mut request = axum::http::Request::builder()
                .uri(case["path"].as_str().ok_or("Protected path missing")?);
            if case["authorized"] == true {
                request = request.header("authorization", "Bearer fixture-api-token");
            }
            let response = protected
                .clone()
                .oneshot(request.body(Body::empty())?)
                .await?;
            assert_eq!(
                u16::from(response.status()),
                case["status"].as_u64().ok_or("Protected status missing")? as u16,
                "{}",
                case["path"]
            );
            assert_eq!(
                response.headers()["content-type"].to_str()?,
                case["type"].as_str().ok_or("Protected type missing")?
            );
            let text = String::from_utf8(to_bytes(response.into_body(), 65536).await?.to_vec())?;
            if case["path"] == "/" {
                assert_eq!(
                    text,
                    case["body"].as_str().ok_or("Protected SPA body missing")?
                );
            } else {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&text)?,
                    serde_json::from_str::<serde_json::Value>(
                        case["body"].as_str().ok_or("Protected API body missing")?
                    )?
                );
            }
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;
    assert_eq!(discover(&root.path().join("source")), Some(build));
    root.cleanup()?;
    Ok(())
}
