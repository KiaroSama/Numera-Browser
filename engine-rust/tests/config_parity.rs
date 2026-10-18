use numera_core::storage::config::ConfigStore;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;

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
