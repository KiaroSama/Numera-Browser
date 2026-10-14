use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use numera_core::{
    browser::{adapter::Adapter, runtime::Runtime},
    http::application::{SessionApplication, router_with_browser},
    ownership::{claims, registry::Registry},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::Path, process::Command, sync::Arc, time::Duration};
use tower::ServiceExt;
mod common;

#[test]
fn launch_optional_body_and_refusals_match_original_before_native_activation()
-> Result<(), Box<dyn std::error::Error>> {
    let source_root = common::TempDirectory::new("launch-http-source")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let mut command = Command::new(&python);
    command.args(["-u","-c",r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from launch_http_cases import cases, attach_cases, attach_success, warmup_cases, scheduler_cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT']),'attach':attach_cases(os.environ['FIXTURE_ROOT']),'attached':attach_success(os.environ['FIXTURE_ROOT']),'warmup':warmup_cases(os.environ['FIXTURE_ROOT']),'scheduler':scheduler_cases(os.environ['FIXTURE_ROOT'])}}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",source_root.path().join("source")).env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        source_root.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-launch-http",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    source_root.cleanup()?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Launch HTTP cases missing")?;
    assert_eq!(cases.len(), 10);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    for case in cases {
        let temporary = common::TempDirectory::new("launch-http-native")?;
        let root = temporary.path();
        std::fs::write(
            root.join("config.json"),
            serde_json::to_vec(&json!({"headless_launch":case["default"]}))?,
        )?;
        if case["exists"] == true {
            std::fs::create_dir_all(root.join("dashboard"))?;
            std::fs::write(
                root.join("dashboard/a.json"),
                serde_json::to_vec(&json!({"id":"a","name":"Fixture","seed":42}))?,
            )?;
        }
        let adapter = root.join("sdk.py");
        std::fs::write(
            &adapter,
            r#"
import json,pathlib,sys
for line in sys.stdin:
    request=json.loads(line);op=request['operation']
    if op=='hello': result={'backends':{'playwright':{'installed':True}}}
    elif op=='open':
        result={'opened':True,'backend':request['backend']}
        pathlib.Path(__file__).with_suffix('.headless').write_text(json.dumps(request['options']['headless']),encoding='utf-8')
    else: result={'closing':True}
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
        let result = runtime.block_on(async {
            let app = router_with_browser(
                root.into(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                SessionApplication {
                    installed: BTreeMap::from([("playwright".into(), true)]),
                    sessions: sessions.clone(),
                },
                Some(browser),
            )
            .await?;
            let mut builder = Request::builder()
                .method("POST")
                .uri("/api/profiles/a/launch")
                .header("content-type", "application/json");
            if case["authorized"] == true {
                builder = builder.header("authorization", "Bearer fixture-api");
            }
            let response = app
                .oneshot(builder.body(Body::from(case["text"].as_str().unwrap_or("").to_owned()))?)
                .await?;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"]
                    .as_u64()
                    .ok_or("Source HTTP status missing")?,
                "{}",
                case["name"]
            );
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
            if case["status"] == 200 {
                // The source stub observes launch options only; native retains a
                // real protocol owner, so its running state must be checked separately.
                assert_eq!(body["id"], "a");
                assert_eq!(body["status"], "running");
                assert!(sessions.owns("a")?);
                let headless: Value =
                    serde_json::from_slice(&std::fs::read(root.join("sdk.headless"))?)?;
                assert_eq!(headless, case["calls"][0]["headless"]);
            } else {
                assert_eq!(body, case["body"], "{}", case["name"]);
                assert!(
                    !root.join("sdk.headless").exists(),
                    "Refused HTTP request activated SDK"
                );
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        });
        let failed = sessions.shutdown(Duration::from_secs(5));
        assert!(failed.is_empty(), "HTTP launch cleanup failed: {failed:?}");
        drop(sessions);
        result?;
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        temporary.cleanup()?;
    }
    let refusals = document["attach"]
        .as_array()
        .ok_or("Attach refusals missing")?;
    assert_eq!(refusals.len(), 4);
    for case in refusals {
        let temporary = common::TempDirectory::new("attach-http-refusal")?;
        let root = temporary.path();
        if case["exists"] == true {
            std::fs::create_dir_all(root.join("dashboard"))?;
            std::fs::write(
                root.join("dashboard/a.json"),
                serde_json::to_vec(&json!({
                    "id":"a","name":"Fixture","seed":42,"engine":case["engine"]
                }))?,
            )?;
        }
        if let Some(claim) = case["claim"].as_str() {
            std::fs::create_dir_all(root.join("sessions"))?;
            std::fs::write(root.join("sessions/a.json"), claim)?;
        }
        let sessions = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
        let result = runtime.block_on(async {
            let app = router_with_browser(
                root.into(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                SessionApplication {
                    installed: Default::default(),
                    sessions: sessions.clone(),
                },
                Some(Runtime {
                    python: python.clone().into(),
                    adapter: root.join("must-not-start.py"),
                    cwd: root.into(),
                    home: root.into(),
                    logs: root.join("logs"),
                    country: None,
                    relay: None,
                    binary_options: Default::default(),
                    deployment: None,
                }),
            )
            .await?;
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/profiles/a/attach");
            if case["authorized"] == true {
                request = request.header("authorization", "Bearer fixture-api");
            }
            let response = app.oneshot(request.body(Body::empty())?).await?;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"].as_u64().ok_or("Attach status missing")?,
                "{}",
                case["name"]
            );
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
            assert_eq!(body, case["body"], "{}", case["name"]);
            assert!(!sessions.owns("a")?);
            assert!(
                !root.join("profiles/a.json").exists(),
                "Refusal materialized profile"
            );
            assert!(!root.join("logs").exists(), "Refusal activated SDK");
            if let Some(claim) = case["claim"].as_str() {
                assert_eq!(
                    std::fs::read_to_string(root.join("sessions/a.json"))?,
                    claim
                );
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        });
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        drop(sessions);
        result?;
        temporary.cleanup()?;
    }
    warmup_http(
        &runtime,
        &python,
        &document["warmup"],
        &document["scheduler"],
    )?;
    attached_lifetime(&runtime, &python, &document["attached"])?;
    Ok(())
}

fn warmup_http(
    runtime: &tokio::runtime::Runtime,
    python: &std::ffi::OsStr,
    cases: &Value,
    scheduled: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let cases = cases.as_array().ok_or("Warmup HTTP source cases missing")?;
    assert_eq!(cases.len(), 7);
    for case in cases {
        let temporary = common::TempDirectory::new("warmup-http")?;
        let root = temporary.path();
        if case["exists"] == true {
            std::fs::create_dir_all(root.join("dashboard"))?;
            std::fs::write(
                root.join("dashboard/a.json"),
                serde_json::to_vec(&json!({"id":"a","name":"Fixture","seed":42}))?,
            )?;
        }
        if let Some(claim) = case["claim"].as_str() {
            std::fs::create_dir_all(root.join("sessions"))?;
            std::fs::write(root.join("sessions/a.json"), claim)?;
        }
        let script = root.join("sdk.py");
        std::fs::write(
            &script,
            r#"
import json,pathlib,sys
for line in sys.stdin:
    request=json.loads(line);op=request['operation']
    if op=='hello': result={'backends':{'playwright':{'installed':True}}}
    elif op=='open':
        pathlib.Path(__file__).with_suffix('.open').write_text(json.dumps(request),encoding='utf-8')
        result={'opened':True,'backend':request['backend']}
    elif op=='navigate': result={'navigated':True}
    elif op=='mouse': result={'performed':True}
    elif op=='evaluate': result={'evaluated':True,'value':[]}
    elif op=='close': result={'closing':True}
    else: raise AssertionError('Unexpected warmup operation')
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close': break
"#,
        )?;
        let sessions = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
        let result = runtime.block_on(async {
            let app = router_with_browser(
                root.into(),
                "fixture-api",
                "",
                None,
                "fixture-host",
                SessionApplication {
                    installed: BTreeMap::from([("playwright".into(), true)]),
                    sessions: sessions.clone(),
                },
                Some(Runtime {
                    python: python.into(),
                    adapter: script.clone(),
                    cwd: root.into(),
                    home: root.into(),
                    logs: root.join("logs"),
                    country: None,
                    relay: None,
                    binary_options: Default::default(),
                    deployment: None,
                }),
            )
            .await?;
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/api/profiles/a/warmup")
                        .header("authorization", "Bearer fixture-api")
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_vec(&case["request"])?))?,
                )
                .await?;
            assert_eq!(
                u64::from(response.status().as_u16()),
                case["status"]
                    .as_u64()
                    .ok_or("Warmup source status missing")?,
                "{}",
                case["name"]
            );
            let body: Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
            if case["status"] == 200 {
                assert_eq!(body["id"], "a");
                assert_eq!(body["status"], "running");
                assert!(sessions.owns("a")?);
                assert_eq!(case["calls"][0]["kind"], "manual_warmup");
                assert_eq!(case["calls"][0]["headless"], true);
                let open: Value =
                    serde_json::from_slice(&std::fs::read(script.with_extension("open"))?)?;
                assert_eq!(open["options"]["headless"], true);
                let stopped = app
                    .oneshot(
                        Request::builder()
                            .method("POST")
                            .uri("/api/profiles/a/stop")
                            .header("authorization", "Bearer fixture-api")
                            .body(Body::empty())?,
                    )
                    .await?;
                assert_eq!(stopped.status(), 200);
                assert!(!sessions.owns("a")?);
                assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
                if case["name"] == "news" {
                    for (pid, extra) in [
                        (
                            "bad",
                            json!({"proxy":{"type":"INVALID","host":"fixture.test","port":"80"}}),
                        ),
                        ("good", json!({})),
                    ] {
                        let mut row = json!({"id":pid,"name":pid,"seed":42});
                        row.as_object_mut()
                            .ok_or("Scheduler record missing")?
                            .extend(
                                extra
                                    .as_object()
                                    .cloned()
                                    .ok_or("Scheduler fields missing")?,
                            );
                        std::fs::write(
                            root.join(format!("dashboard/{pid}.json")),
                            serde_json::to_vec(&row)?,
                        )?;
                    }
                    std::fs::write(
                        root.join("schedules.json"),
                        serde_json::to_vec(&scheduled["rows"])?,
                    )?;
                    let scheduler = numera_core::http::scheduler::Scheduler::new(
                        root.into(),
                        None,
                        "fixture-host".into(),
                        sessions.clone(),
                        Runtime {
                            python: python.into(),
                            adapter: script.clone(),
                            cwd: root.into(),
                            home: root.into(),
                            logs: root.join("logs"),
                            country: None,
                            relay: None,
                            binary_options: Default::default(),
                            deployment: None,
                        },
                    )?;
                    tokio::task::spawn_blocking(move || scheduler.tick()).await??;
                    assert!(sessions.owns("good")?);
                    assert!(!sessions.owns("bad")?);
                    assert_eq!(
                        scheduled["calls"],
                        json!([{"id":"good","kind":"scheduled_warmup","schedule":"healthy"}])
                    );
                    let store = numera_core::storage::schedules::ScheduleStore::new(root.into());
                    let rows = store.list()?;
                    let good = rows
                        .iter()
                        .find(|row| row["id"] == "healthy")
                        .ok_or("Healthy schedule missing")?;
                    assert_eq!(good["lastOutcome"], "started");
                    assert!(
                        good["lastRun"]
                            .as_f64()
                            .is_some_and(|time| time > 1700000000.0)
                    );
                    assert!(good["claimedAt"].is_null());
                    let bad = rows
                        .iter()
                        .find(|row| row["id"] == "broken")
                        .ok_or("Broken schedule missing")?;
                    assert_eq!(bad["lastOutcome"], scheduled["stored"][0]["lastOutcome"]);
                    assert!(bad["lastRun"].is_null());
                    assert!(bad["claimedAt"].is_null());
                    assert_eq!(rows[2], scheduled["stored"][2]);
                    assert!(sessions.stop("good", Duration::from_secs(5))?);
                    assert!(matches!(claims::read(root, "good"), claims::Claim::Absent));
                }
            } else {
                assert_eq!(body, case["body"]);
                assert!(!script.with_extension("open").exists());
                assert!(!sessions.owns("a")?);
            }
            Ok::<_, Box<dyn std::error::Error>>(())
        });
        let failed = sessions.shutdown(Duration::from_secs(5));
        assert!(failed.is_empty(), "Warmup HTTP cleanup failed: {failed:?}");
        drop(sessions);
        result?;
        temporary.cleanup()?;
    }
    Ok(())
}

fn attached_lifetime(
    runtime: &tokio::runtime::Runtime,
    python: &std::ffi::OsStr,
    expected: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("attach-http-lifetime")?;
    let root = temporary.path();
    std::fs::create_dir_all(root.join("dashboard"))?;
    std::fs::write(
        root.join("dashboard/a.json"),
        serde_json::to_vec(&json!({
            "id":"a","name":"Fixture","seed":42,"engine":"playwright"
        }))?,
    )?;
    let script = root.join("sdk.py");
    std::fs::write(
        &script,
        r#"
import json,os,pathlib,socket,sys,threading
server=None;worker=None;stop=threading.Event();seen=[];queries=[]
def serve():
    while not stop.is_set():
        try: client,address=server.accept()
        except socket.timeout: continue
        except OSError: return
        with client:
            client.settimeout(1)
            request=client.recv(4096)
            if b'/json/version' not in request: raise AssertionError('Wrong CDP request')
            queries.append(True)
            pathlib.Path(__file__).with_suffix('.queries').write_text(json.dumps(len(queries)),encoding='utf-8')
            port=server.getsockname()[1]
            body=json.dumps({'Browser':'Fixture Chromium','webSocketDebuggerUrl':f'ws://127.0.0.1:{port}/devtools/browser/fixture'}).encode('utf-8')
            client.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: '+str(len(body)).encode('ascii')+b'\r\nConnection: close\r\n\r\n'+body)
try:
    for line in sys.stdin:
        request=json.loads(line);op=request['operation'];seen.append(op)
        if op=='hello': result={'backends':{'playwright':{'installed':True}}}
        elif op=='open':
            flag=next(arg for arg in request['options']['args'] if arg.startswith('--remote-debugging-port='))
            port=int(flag.split('=',1)[1]);server=socket.socket();server.bind(('127.0.0.1',port));server.listen();server.settimeout(.1)
            worker=threading.Thread(target=serve);worker.start()
            pathlib.Path(__file__).with_suffix('.open').write_text(json.dumps({'request':request,'first':os.environ['NUMERA_ADAPTER_FIRST_PAGE']}),encoding='utf-8')
            result={'opened':True,'backend':request['backend']}
        elif op=='close':
            stop.set();server.close();worker.join(timeout=2)
            if worker.is_alive(): raise RuntimeError('CDP worker failed to finish')
            result={'closing':True}
        else: raise AssertionError('Unexpected operation')
        pathlib.Path(__file__).with_suffix('.seen').write_text(json.dumps(seen),encoding='utf-8')
        print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
        if op=='close': break
finally:
    stop.set()
    if server: server.close()
    if worker: worker.join(timeout=2)
"#,
    )?;
    let sessions = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
    let result = runtime.block_on(async {
        let app = router_with_browser(
            root.into(),
            "fixture-api",
            "",
            None,
            "fixture-host",
            SessionApplication {
                installed: BTreeMap::from([("playwright".into(), true)]),
                sessions: sessions.clone(),
            },
            Some(Runtime {
                python: python.into(),
                adapter: script.clone(),
                cwd: root.into(),
                home: root.into(),
                logs: root.join("logs"),
                country: None,
                relay: None,
                binary_options: Default::default(),
                deployment: None,
            }),
        )
        .await?;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/profiles/a/attach")
                    .header("authorization", "Bearer fixture-api")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"headless":true}"#))?,
            )
            .await?;
        assert_eq!(
            u64::from(response.status().as_u16()),
            expected["status"]
                .as_u64()
                .ok_or("Source attach status missing")?
        );
        let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65536).await?)?;
        let port = u16::try_from(body["port"].as_u64().ok_or("Native port missing")?)?;
        let source_port = expected["body"]["port"]
            .as_u64()
            .ok_or("Source port missing")?;
        let mut normalized = body.clone();
        normalized["port"] = source_port.into();
        for field in ["wsUrl", "snippets"] {
            normalized[field] = body[field]
                .as_str()
                .ok_or("Endpoint text missing")?
                .replace(&port.to_string(), &source_port.to_string())
                .into();
        }
        assert_eq!(normalized, expected["body"]);
        assert_eq!(expected["repeated"], expected["body"]);
        assert_eq!(expected["queries"], 1);
        assert_eq!(
            expected["calls"],
            json!([{"kind":"attach","headless":true}])
        );
        assert!(sessions.owns("a")?);
        assert_eq!(sessions.attached("a")?, Some(body.clone()));
        assert!(claims::state(&claims::read(root, "a"), "fixture-host") != claims::State::Free);
        let open: Value = serde_json::from_slice(&std::fs::read(script.with_extension("open"))?)?;
        assert_eq!(open["first"], "1");
        assert_eq!(open["request"]["options"]["headless"], true);
        let repeated = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/profiles/a/attach")
                    .header("authorization", "Bearer fixture-api")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"headless":false}"#))?,
            )
            .await?;
        assert_eq!(
            u64::from(repeated.status().as_u16()),
            expected["repeatedStatus"]
                .as_u64()
                .ok_or("Source repeat status missing")?
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&to_bytes(repeated.into_body(), 65536).await?)?,
            body
        );
        assert_eq!(
            std::fs::read_to_string(script.with_extension("queries"))?,
            "1"
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(script.with_extension("seen"))?)?,
            json!(["hello", "open"])
        );
        let stopped = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/profiles/a/stop")
                    .header("authorization", "Bearer fixture-api")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(
            u64::from(stopped.status().as_u16()),
            expected["stopStatus"]
                .as_u64()
                .ok_or("Source stop status missing")?
        );
        let stopped: Value = serde_json::from_slice(&to_bytes(stopped.into_body(), 65536).await?)?;
        assert_eq!(stopped["status"], "stopped");
        assert!(!sessions.owns("a")?);
        assert!(sessions.attached("a")?.is_none());
        assert!(!sessions.running("a")?);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        assert!(
            std::net::TcpStream::connect_timeout(
                &([127, 0, 0, 1], port).into(),
                Duration::from_millis(200)
            )
            .is_err()
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&std::fs::read(script.with_extension("seen"))?)?,
            json!(["hello", "open", "close"])
        );
        Ok::<_, Box<dyn std::error::Error>>(())
    });
    let failed = sessions.shutdown(Duration::from_secs(5));
    assert!(
        failed.is_empty(),
        "HTTP attachment cleanup failed: {failed:?}"
    );
    drop(sessions);
    result?;
    temporary.cleanup()?;
    Ok(())
}
