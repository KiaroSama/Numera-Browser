use numera_core::{
    browser::{adapter::Adapter, runtime::Runtime},
    cli::{self, application::Application},
    fingerprint::{GenerateOptions, generate},
    models::Profile,
    ownership::{claims, registry::Registry},
    storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
#[path = "support/attach_output.rs"]
mod attach_output;
mod common;

#[test]
fn foreground_launch_reuses_admission_and_native_backend_lifetime_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("cli-launch")?;
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
from cli_launch_cases import cases, attach_cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{'cases':cases(os.environ['FIXTURE_ROOT']),'attach':attach_cases(os.environ['FIXTURE_ROOT']),'human':cases(os.path.join(os.environ['FIXTURE_ROOT'],'human'),human=True),'humanAttach':attach_cases(os.path.join(os.environ['FIXTURE_ROOT'],'human-attach'),human=True)}}),flush=True)
"#]).env("PYTHONUTF8","1").env("FIXTURE_ROOT",temporary.path().join("source")).env("PYTHONPATH",std::env::join_paths([repository.join("scripts/migration"),repository.join("engine")])?);
    let mut source = Adapter::spawn(
        temporary.path(),
        "oracle",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call(
        "observe-launch-cli",
        Default::default(),
        Duration::from_secs(30),
    )?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
    let root = temporary.path().join("native");
    let repo = ProfileRepo::open(root.clone(), None, None)?;
    repo.save(Profile::from_record(&json!({"id":"fixture","name":"Fixture","fingerprint":generate(GenerateOptions{seed:42,..Default::default()})?,"created_at":1000.0,"updated_at":1000.0}))?,1000.0)?;
    let script = root.join("sdk.py");
    std::fs::write(
        &script,
        r#"
import json,os,pathlib,sys,threading
seen=[]
for line in sys.stdin:
    request=json.loads(line);op=request['operation'];seen.append(op)
    if op=='hello': result={'backends':{'playwright':{'installed':True},'camoufox':{'installed':True}}}
    elif op=='open':
        result={'opened':True,'backend':request['backend']}
        pathlib.Path(__file__).with_suffix('.policy').write_text(json.dumps({'backend':request['backend'],'first':os.environ.get('NUMERA_ADAPTER_FIRST_PAGE')}),encoding='utf-8')
    else: result={'closing':True}
    pathlib.Path(__file__).with_suffix('.seen').write_text(json.dumps(seen),encoding='utf-8')
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close': break
"#,
    )?;
    let sessions = Registry::new(root.clone(), "fixture-host".into(), None)?;
    let browser = Runtime {
        python: python.into(),
        adapter: script,
        cwd: root.clone(),
        home: root.clone(),
        logs: root.join("logs"),
        country: None,
        relay: None,
        binary_options: Default::default(),
    };
    let installed = BTreeMap::new();
    let application = Application {
        root: &root,
        home: &root,
        repo: &repo,
        installed: &installed,
        host: "fixture-host",
        password: None,
        environment_token: false,
        sessions: &sessions,
        browser: Some(&browser),
    };
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in document["cases"]
        .as_array()
        .ok_or("Launch cases missing")?
        .iter()
        .enumerate()
    {
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(
                case["argv"]
                    .as_array()
                    .ok_or("Launch argv missing")?
                    .iter()
                    .map(|v| v.as_str().expect("Source argument text")),
            ),
        )?;
        let values = cli::reflection::values(&registry, &matches);
        let seen = root.join("sdk.seen");
        if seen.exists() {
            std::fs::remove_file(&seen)?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let outcome = application.execute_with_output(
            values.as_object().ok_or("Launch args missing")?,
            1000.0,
            |_| panic!("Launch fixture prompted"),
            || {
                // Cancellation follows observed SDK readiness, never a blind delay.
                seen.exists() || Instant::now() >= deadline
            },
            || panic!("Launch fixture read stdin"),
            &mut output,
        )?;
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            document["human"][index]["stdout"]
                .as_str()
                .ok_or("Launch source bytes missing")?
        );
        assert!(stderr.is_empty());
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Launch source exit missing")? as i32
        );
        assert_eq!(outcome.payload, case["payload"]);
        if outcome.exit == 0 {
            let policy: Value = serde_json::from_slice(&std::fs::read(root.join("sdk.policy"))?)?;
            assert_eq!(policy["backend"], outcome.payload["engine"]);
            assert_eq!(
                policy["first"],
                if outcome.payload["engine"] == "camoufox" {
                    "0"
                } else {
                    "1"
                }
            );
            let calls: Value = serde_json::from_slice(&std::fs::read(&seen)?)?;
            assert_eq!(calls, json!(["hello", "open", "close"]));
        } else {
            assert!(!seen.exists(), "Missing profile activated SDK");
        }
        assert!(matches!(
            claims::read(&root, "fixture"),
            claims::Claim::Absent
        ));
    }
    assert_eq!(
        numera_core::network::devtools::snippets(
            17777,
            "ws://127.0.0.1:17777/devtools/browser/fixture"
        ),
        document["attach"]["snippets"]
            .as_str()
            .ok_or("Source snippets missing")?
    );
    for case in document["attach"]["headings"]
        .as_array()
        .ok_or("Attach heading observations missing")?
    {
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        cli::attach_commands::ready(&mut output, &case["info"], 17777)?;
        output.say("\nClosing.", None)?;
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Attach heading source bytes missing")?
        );
        assert!(stderr.is_empty());
    }
    let attach_script = root.join("attach-sdk.py");
    std::fs::write(
        &attach_script,
        r#"
import json,os,pathlib,socket,sys,threading
server=None;worker=None;stop=threading.Event()
def serve():
    while not stop.is_set():
        try: client,address=server.accept()
        except socket.timeout: continue
        except OSError: return
        with client:
            client.settimeout(1)
            request=client.recv(4096)
            assert b'/json/version' in request
            port=server.getsockname()[1]
            body=json.dumps({'Browser':'Fixture Chromium','webSocketDebuggerUrl':f'ws://127.0.0.1:{port}/devtools/browser/fixture'}).encode('utf-8')
            client.sendall(b'HTTP/1.1 200 OK\r\nContent-Length: '+str(len(body)).encode('ascii')+b'\r\nConnection: close\r\n\r\n'+body)
try:
    for line in sys.stdin:
        request=json.loads(line);op=request['operation']
        if op=='hello': result={'backends':{'playwright':{'installed':True}}}
        elif op=='open':
            args=request['options']['args'];flag=next(arg for arg in args if arg.startswith('--remote-debugging-port='))
            port=int(flag.split('=',1)[1]);server=socket.socket();server.bind(('127.0.0.1',port));server.listen();server.settimeout(.1)
            worker=threading.Thread(target=serve);worker.start()
            pathlib.Path(__file__).with_suffix('.open').write_text(json.dumps(request),encoding='utf-8')
            result={'opened':True,'backend':request['backend']}
        elif op=='close':
            stop.set()
            if server: server.close()
            if worker:
                worker.join(timeout=2)
                if worker.is_alive(): raise RuntimeError('Fixture worker failed to finish')
            result={'closing':True}
        else: raise AssertionError('Unexpected attach operation')
        print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
        if op=='close': break
        if op=='open' and os.environ.get('FIXTURE_NATURAL'):
            release=pathlib.Path(__file__).with_suffix('.release')
            deadline=__import__('time').monotonic()+5
            while not release.exists() and __import__('time').monotonic()<deadline:
                stop.wait(.01)
            if not release.exists(): raise RuntimeError('Natural close signal missing')
            break
finally:
    stop.set()
    if server: server.close()
    if worker: worker.join(timeout=2)
"#,
    )?;
    let browser = Runtime {
        adapter: attach_script.clone(),
        ..browser
    };
    let application = Application {
        browser: Some(&browser),
        ..application
    };
    let attach = document["attach"]["cases"]
        .as_array()
        .ok_or("Attach cases missing")?;
    assert_eq!(attach.len(), 2);
    for case in attach {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let mut argv = case["argv"]
            .as_array()
            .ok_or("Attach argv missing")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or("Attach argument not text")
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>, _>>()?;
        argv[3] = port.to_string();
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera".to_owned()).chain(argv))?;
        let args = cli::reflection::values(&registry, &matches);
        let published = std::cell::RefCell::new(None);
        let outcome = application.execute_streaming(
            args.as_object().ok_or("Attach arguments missing")?,
            1000.0,
            |_| panic!("Attach confirmation"),
            || published.borrow().is_some(),
            (
                || panic!("Attach stdin"),
                |value: &Value| {
                    assert!(
                        claims::state(&claims::read(&root, "fixture"), "fixture-host")
                            != claims::State::Free
                    );
                    assert!(attach_script.with_extension("open").exists());
                    *published.borrow_mut() = Some(value.clone());
                    Ok(())
                },
            ),
        )?;
        let mut expected = case["payload"].clone();
        if expected["ok"] == true {
            expected["port"] = port.into();
            expected["wsUrl"] = format!("ws://127.0.0.1:{port}/devtools/browser/fixture").into();
            assert_eq!(published.into_inner(), Some(expected.clone()));
            let open: Value =
                serde_json::from_slice(&std::fs::read(attach_script.with_extension("open"))?)?;
            assert!(
                open["options"]["args"]
                    .as_array()
                    .ok_or("Attach options missing")?
                    .contains(&json!(format!("--remote-debugging-port={port}")))
            );
            assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
        } else {
            assert!(published.into_inner().is_none());
        }
        assert_eq!(outcome.payload, expected);
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Attach exit missing")? as i32
        );
        assert!(matches!(
            claims::read(&root, "fixture"),
            claims::Claim::Absent
        ));
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let natural_script = root.join("natural-sdk.py");
    std::fs::write(
        &natural_script,
        format!(
            "import os\nos.environ['FIXTURE_NATURAL']='1'\nexec(compile(open({:?},encoding='utf-8').read(),__file__,'exec'))\n",
            attach_script.to_str().ok_or("Attach script path nontext")?
        ),
    )?;
    let browser = Runtime {
        adapter: natural_script.clone(),
        ..browser
    };
    let application = Application {
        browser: Some(&browser),
        ..application
    };
    let matches = registry.clone().try_get_matches_from([
        "numera",
        "attach",
        "Fixture",
        "--headless",
        "--port",
        &port.to_string(),
        "--json",
    ])?;
    let args = cli::reflection::values(&registry, &matches);
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = attach_output::Published {
        bytes: bytes.clone(),
        root: root.clone(),
        release: natural_script.with_extension("release"),
        ready: natural_script.with_extension("open"),
    };
    let mut output = cli::output::Output::new(writer, Vec::new(), true, false);
    let outcome = application.execute_with_output(
        args.as_object().ok_or("Natural attach arguments missing")?,
        1000.0,
        |_| panic!("Natural attach confirmation"),
        || false,
        || panic!("Natural attach stdin"),
        &mut output,
    )?;
    assert!(output.emitted());
    let published: Value = serde_json::from_slice(
        bytes
            .lock()
            .map_err(|_| "Attach output lock poisoned")?
            .as_slice(),
    )?;
    assert_eq!(published, outcome.payload);
    assert!(output.emit(&outcome.payload, outcome.exit).is_err());
    drop(output);
    assert_eq!(
        outcome.exit, 0,
        "Natural attachment close became a false failure"
    );
    assert!(matches!(
        claims::read(&root, "fixture"),
        claims::Claim::Absent
    ));
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    drop(sessions);
    temporary.cleanup()?;
    Ok(())
}
