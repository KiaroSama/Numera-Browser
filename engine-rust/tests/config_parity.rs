use numera_core::storage::config::ConfigStore;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;

#[test]
fn direct_settings_and_current_thread_replacement_match_original()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::locks::DataLock};
    use serde_json::json;
    use std::{path::Path, process::Command, sync::mpsc, thread, time::Duration};

    let temporary = common::TempDirectory::new("direct-settings")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys,threading
from contextlib import ExitStack
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.config_store import ConfigStore
from numera.paths import transaction_lock,holding_transaction
from run_logging import run_log
valid=parent/'extension';valid.mkdir()
(valid/'manifest.json').write_text('{}',encoding='utf-8')
missing=str(parent/'missing-extension')
rows=[]
with run_log('direct-settings-oracle',parent/'logs') as log:
    inputs=[None,b'{}',b'{broken',b'[]',b'null',b'false',b'"text"',b'1',b'\xff',
            json.dumps({'future':{'kept':['日本語',True]},'headless_launch':False}).encode('utf-8')]
    jobs=[]
    for raw in inputs:
        jobs.extend((raw,op,value,'none') for op,value in (
            ('set',{'nested':['α',None]}),('engine','unknown-but-stored'),
            ('headless',True),('headless-read',None)) if raw!=b'\xff' or op!='headless-read')
    for value in (None,False,0,0.0,'',[],{},True,1,-1,'false',[0],{'on':False}):
        jobs.append((b'{"future":7}','headless',value,'none'))
    for mode in ('none','owned','other-root','other-thread','reentrant','released'):
        jobs.append((b'{"removed":1,"future":2}','replace',{'replacement':['α',True]},mode))
    jobs.extend((raw,'extensions',value,'none') for raw,value in (
        (b'{}',[str(valid)]),(b'{}',[str(valid),str(valid)]),
        (b'{}',[missing]),(json.dumps({'default_extensions':[missing],'future':7}).encode(),[missing]),
        (json.dumps({'default_extensions':[missing],'future':7}).encode(),[]),
        (b'{}',[]),(b'{}',['']), (b'{}',['https://fixture.invalid/extension']),
        (b'{"default_extensions":17}',[]),(b'[]',[]),(b'\xff',[]),
        (b'\xff',[missing]),(b'\xff',['https://fixture.invalid/extension'])))
    for raw,op,value,mode in jobs:
        root=parent/'source'/str(len(rows));root.mkdir(parents=True)
        store=ConfigStore();store.root=root;store.config_path=root/'config.json'
        if raw is not None:store.config_path.write_bytes(raw)
        def invoke():
            if op=='set':return store._set_config('selected',value)
            if op=='engine':return store.set_default_engine(value)
            if op=='headless':return store.set_headless_launch(value)
            if op=='headless-read':return store.get_headless_launch()
            if op=='extensions':return store.set_default_extensions(value)
            return store._save_config_locked(value)
        thread=None;release=threading.Event();ready=threading.Event();worker_errors=[]
        def hold():
            try:
                with transaction_lock(root):
                    ready.set()
                    if not release.wait(5):raise TimeoutError('Source owner release timed out')
            except BaseException as exc:worker_errors.append(type(exc).__name__);ready.set()
        with ExitStack() as stack:
            if mode in ('owned','reentrant'):
                stack.enter_context(transaction_lock(root))
                if mode=='reentrant':
                    with transaction_lock(root):pass
            elif mode=='other-root':stack.enter_context(transaction_lock(root/'other'))
            elif mode=='released':
                with transaction_lock(root):pass
            elif mode=='other-thread':
                thread=threading.Thread(target=hold);thread.start()
                if not ready.wait(5):raise TimeoutError('Source owner readiness timed out')
            try:
                held=holding_transaction(root)
                try:result,error=invoke(),None
                except (ValueError,TypeError,AttributeError,RuntimeError) as exc:
                    result,error=None,type(exc).__name__
            finally:
                release.set()
                if thread is not None:
                    thread.join(5)
                    assert not thread.is_alive(),'Source owner did not settle'
        assert not worker_errors,'Source owner failed'
        after=store.config_path.read_bytes() if store.config_path.exists() else None
        try:disk=json.loads(after.decode('utf-8')) if after is not None else None
        except (ValueError,UnicodeError):disk=None
        rows.append({'input':raw.hex() if raw is not None else None,'op':op,'value':value,
                     'mode':mode,'held':held,'released':holding_transaction(root),
                     'result':result,'error':error,'disk':disk,'unchanged':after==raw})
    log.info('Observed direct settings cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path());
    let output = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(output.exit, 0, "Original direct settings observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(cases.len(), 71, "Direct settings observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(&root)?;
        let path = root.join("config.json");
        let before = case["input"]
            .as_str()
            .map(|raw| {
                (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        if let Some(bytes) = &before {
            fs::write(&path, bytes)?;
        }
        let store = ConfigStore::new(root.clone());
        let lock = DataLock::transaction(&root)?;
        let other = DataLock::transaction(&root.join("other"))?;
        let mode = case["mode"].as_str().ok_or("Settings lock mode missing")?;
        let guard = match mode {
            "owned" | "reentrant" => Some(lock.acquire(Duration::from_secs(5))?),
            "other-root" => Some(other.acquire(Duration::from_secs(5))?),
            "released" => {
                drop(lock.acquire(Duration::from_secs(5))?);
                None
            }
            _ => None,
        };
        if mode == "reentrant" {
            drop(lock.acquire(Duration::from_secs(5))?);
        }
        let invoke = || -> Result<Value, numera_core::storage::config::ConfigError> {
            match case["op"].as_str().unwrap_or("") {
                "set" => store.set("selected", case["value"].clone()),
                "engine" => store.set_default_engine(case["value"].clone()),
                "headless" => store.set_headless_launch(&case["value"]).map(Value::Bool),
                "headless-read" => Ok(json!(store.headless_launch())),
                "extensions" => store
                    .set_default_extensions(
                        case["value"]
                            .as_array()
                            .expect("String extension list")
                            .iter()
                            .map(|v| v.as_str().expect("String extension entry").to_owned())
                            .collect(),
                    )
                    .map(|v| json!(v)),
                _ => store
                    .replace_locked(
                        case["value"]
                            .as_object()
                            .expect("Settings replacement object")
                            .clone(),
                    )
                    .map(Value::Object),
            }
        };
        let result = thread::scope(|scope| -> Result<_, Box<dyn std::error::Error>> {
            let (ready_send, ready) = mpsc::sync_channel(1);
            let (release, release_recv) = mpsc::sync_channel(1);
            let worker = if mode == "other-thread" {
                let root = &root;
                Some(scope.spawn(move || -> Result<(), String> {
                    let lock = DataLock::transaction(root).map_err(|e| e.to_string())?;
                    let _guard = lock
                        .acquire(Duration::from_secs(5))
                        .map_err(|e| e.to_string())?;
                    ready_send.send(()).map_err(|e| e.to_string())?;
                    release_recv
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(|e| e.to_string())?;
                    Ok(())
                }))
            } else {
                None
            };
            if worker.is_some() {
                ready.recv_timeout(Duration::from_secs(5))?;
            }
            let held = lock.owned_by_current_thread()?;
            let result = invoke();
            if let Some(worker) = worker {
                release.send(())?;
                worker
                    .join()
                    .map_err(|_| "Native settings owner panicked")?
                    .map_err(std::io::Error::other)?;
            }
            assert_eq!(
                json!(held),
                case["held"],
                "Settings ownership differs: {index}"
            );
            Ok(result)
        })?;
        drop(guard);
        assert_eq!(json!(lock.owned_by_current_thread()?), case["released"]);
        match result {
            Ok(value) => {
                assert!(case["error"].is_null(), "Settings refusal differs: {index}");
                assert_eq!(
                    value, case["result"],
                    "Direct settings result differs: {index}"
                );
            }
            Err(numera_core::storage::config::ConfigError::Io(error))
                if error.kind() == std::io::ErrorKind::InvalidData =>
            {
                assert_eq!(case["error"], "UnicodeDecodeError");
            }
            Err(error) => assert_eq!(
                Some(error.source_kind()),
                case["error"].as_str(),
                "Direct settings error differs: {index}"
            ),
        }
        let after = fs::read(&path).ok();
        assert_eq!(
            json!(after == before),
            case["unchanged"],
            "Settings bytes differ: {index}"
        );
        if !case["error"].is_null() {
            assert_eq!(after, before);
        }
        let disk = after
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        assert_eq!(
            disk, case["disk"],
            "Direct settings disk state differs: {index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn malformed_settings_text_refuses_without_replacing_source_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::config::ConfigError};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("config-text")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.config_store import ConfigStore
from run_logging import run_log

with run_log('config-text-oracle',root/'logs') as log:
    rows=[]
    payloads=[b'\xff',b'{"future":"\xc3("}',b'{"future":"\xe2\x82',
              b'{broken',None,b'{}',b'[]',b'null',
              json.dumps({'future':{'kept':['日本語',True]},'headless_launch':False},
                         ensure_ascii=False).encode('utf-8')]
    for payload in payloads:
        for changes in [{'headless_launch':True},{'default_engine':'patchright','future':None}]:
            case=root/'source'/str(len(rows))
            case.mkdir(parents=True)
            store=ConfigStore()
            store.root,store.config_path=case,case/'config.json'
            if payload is not None:
                store.config_path.write_bytes(payload)
            try:
                result,error=store.set_config_many(changes),None
            except (UnicodeDecodeError,AttributeError) as exc:
                result,error=None,type(exc).__name__
            after=store.config_path.read_bytes() if store.config_path.exists() else None
            try:
                written=json.loads(after.decode('utf-8')) if after is not None else None
            except (UnicodeDecodeError,json.JSONDecodeError):
                written=None
            rows.append({'input':payload.hex() if payload is not None else None,
                         'changes':changes,'result':result,'error':error,
                         'written':written,'unchanged':payload==after})
    log.info('Observed settings text cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path())
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original settings text observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 18, "Settings text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(&root)?;
        let path = root.join("config.json");
        let before = case["input"]
            .as_str()
            .map(|raw| {
                (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        if let Some(bytes) = &before {
            fs::write(&path, bytes)?;
        }
        let store = ConfigStore::new(root);
        let result = store.set_many(
            case["changes"]
                .as_object()
                .ok_or("Changes missing")?
                .clone(),
        );
        assert_eq!(
            result.is_ok(),
            case["error"].is_null(),
            "Settings text mutation admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(Value::Object(value), case["result"]),
            Err(ConfigError::Io(error)) => {
                assert_eq!(case["error"], "UnicodeDecodeError");
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("UTF-8"));
            }
            Err(error) => assert_eq!(Some(error.source_kind()), case["error"].as_str()),
        }
        let after = fs::read(&path).ok();
        assert_eq!(
            Value::Bool(before == after),
            case["unchanged"],
            "Settings text byte preservation differs: case={index}"
        );
        if !case["error"].is_null() {
            assert_eq!(after, before, "Refused settings mutation changed bytes");
        }
        let written = after
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        assert_eq!(
            written, case["written"],
            "Settings text state differs: case={index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn settings_read_merge_and_refusal_match_source_without_losing_unknown_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/config.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing config cases")?;
    assert!(!cases.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "config-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for case in cases {
            let path = root.join("config.json");
            if path.exists() {
                fs::remove_file(&path)?;
            }
            if let Some(raw) = case["input"].as_str() {
                fs::write(&path, raw)?;
            }
            let before = fs::read(&path).ok();
            let store = ConfigStore::new(root.clone());
            assert_eq!(Value::Object(store.read()), case["read"]);
            let changes = case["changes"].as_object().ok_or("Missing changes")?;
            match store.set_many(changes.clone()) {
                Ok(value) => {
                    assert!(case["error_type"].is_null());
                    assert_eq!(Value::Object(value), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.source_kind()), case["error_type"].as_str()),
            }
            let after = fs::read(&path).ok();
            assert_eq!(
                before == after,
                case["unchanged"]
                    .as_bool()
                    .ok_or("Missing side effect assertion")?
            );
            if case["error_type"].is_null() {
                let disk = after
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .unwrap_or(Value::Null);
                assert_eq!(disk, case["disk"]);
            }
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
