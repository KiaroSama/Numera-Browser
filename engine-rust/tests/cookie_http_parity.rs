use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{
    browser::{adapter::Adapter, runtime::Runtime},
    http::application::{SessionApplication, router_with_browser},
    ownership::registry::Registry,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command, sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn cookie_http_uses_native_format_policy_and_owned_sdk() -> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let source_root = common::TempDirectory::new("cookie-http-source")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let script = r#"
import json, os, sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from cookie_http_cases import cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT'])}}),flush=True)
"#;
    let mut command = Command::new(&python);
    command
        .args(["-u", "-c", script])
        .env(
            "PYTHONPATH",
            format!(
                "{}{}{}",
                repository.join("scripts/migration").display(),
                if cfg!(windows) { ";" } else { ":" },
                repository.join("engine").display()
            ),
        )
        .env("FIXTURE_ROOT", source_root.path().join("cases"))
        .env("PYTHONUTF8", "1");
    let mut oracle = Adapter::spawn(
        source_root.path(),
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = oracle.call(
        "observe-cookies",
        Default::default(),
        Duration::from_secs(30),
    )?;
    oracle.finish(Duration::from_secs(5))?;
    drop(oracle);
    source_root.cleanup()?;
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
        std::fs::write(
            root.join("probe.json"),
            serde_json::to_vec(
                &json!({"probe":case["probe"],"network":case["network"],"alignment":case["alignment"]}),
            )?,
        )?;
        let adapter = root.join("sdk.py");
        std::fs::write(
            &adapter,
            r#"
import json, pathlib, sys
seen=[]
for line in sys.stdin:
    request=json.loads(line)
    op=request['operation']
    seen.append(op)
    result={}
    if op=='hello': result={'backends':{'playwright':{'installed':True},'camoufox':{'installed':True}}}
    elif op=='open': result={'opened':True,'backend':request['backend']}
    elif op=='cookies-add': result={'added':True}
    elif op=='cookies-clear': result={'cleared':True}
    elif op=='cookies-read': result={'cookies':[{'name':'fixture','value':'synthetic','domain':'.fixture.test'}]}
    elif op=='route': result={'routed':True}
    elif op=='navigate': result={'navigated':True}
    elif op=='evaluate':
        case=json.loads(pathlib.Path(__file__).with_name('probe.json').read_text(encoding='utf-8'))
        if case['alignment'] is True and request['expression'].startswith('(fonts)'):
            for directory in ['profiles','dashboard']:
                path=pathlib.Path(__file__).with_name(directory).joinpath('a.json')
                current=json.loads(path.read_text(encoding='utf-8'));current['notes']='edited during probe'
                path.write_text(json.dumps(current),encoding='utf-8')
            value=case['probe']['fontsRendered']
        elif case['network'] is not None:
            expression=request['expression']
            kind='info' if 'async (urls)' in expression else 'ice' if 'RTCPeerConnection' in expression else 'dns' if 'idUrl' in expression else 'tls'
            value=case['network'][kind]
        else: value=case['probe']
        result={'evaluated':True,'value':value}
    elif op=='close': result={'closing':True}
    else: raise AssertionError('Unexpected browser operation')
    pathlib.Path(__file__).with_suffix('.seen').write_text(json.dumps(seen),encoding='utf-8')
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close': break
"#,
        )?;
        let sessions = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
        let browser = Runtime {
            python: python.clone().into(),
            adapter,
            cwd: root.into(),
            home: root.into(),
            logs: root.join("logs"),
            country: None,
            relay: None,
            binary_options: Default::default(),
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
