use numera_core::storage::pool::ProxyPool;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;

#[test]
fn malformed_pool_text_refuses_without_replacing_original_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::RepoError};
    use serde_json::{Map, json};
    use std::{io, path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("pool-text-storage")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.store import ProfileStore
from numera.server.stores import ProxyPool
from run_logging import run_log
rows=[]
inputs=[b'\xff',b'["\xc3("]',b'["\xe2\x82',b'{broken',b'{}',b'[]',b'null',None,
        b'[{"id":"fixture","host":"host.test","port":"80","pass":null,"future":[true,7]}]',b'[{}]']
operations=['list','get','withheld','update','empty-update','update-many','empty-many','delete','missing-delete','add-empty']
with run_log('pool-storage-text-oracle',parent/'logs') as log:
    for raw in inputs:
        for operation in operations:
            root=parent/('source-'+str(len(rows)))
            store=ProfileStore(root);pool=ProxyPool(root,store);path=root/'proxies.json'
            if raw is not None:path.write_bytes(raw)
            try:
                if operation=='list':value=pool.list()
                elif operation=='get':value=pool.get('fixture')
                elif operation=='withheld':value=pool.secret_withheld('fixture')
                elif operation=='update':value=pool.update('fixture',{'label':'α'})
                elif operation=='empty-update':value=pool.update('fixture',{})
                elif operation=='update-many':value=pool.update_many({'fixture':{'label':'α'}})
                elif operation=='empty-many':value=pool.update_many({})
                elif operation=='delete':value=pool.delete('fixture')
                elif operation=='missing-delete':value=pool.delete('missing')
                else:value=pool.add_many([])
                outcome={'result':value,'error':None}
            except UnicodeDecodeError:
                outcome={'result':None,'error':'UnicodeDecodeError'}
            after=path.read_bytes() if path.exists() else None
            disk=None
            if after is not None:
                try:disk=json.loads(after.decode('utf-8'))
                except (UnicodeDecodeError,json.JSONDecodeError):pass
            rows.append({'raw':None if raw is None else raw.hex(),'operation':operation,
                         'outcome':outcome,'disk':disk,'unchanged':raw==after})
    log.info('Observed pooled proxy text storage cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path());
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original pool storage text observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 100, "Pool storage text cases missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("proxies.json");
        let before = case["raw"]
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
        let pool = ProxyPool::new(root, None);
        let operation = case["operation"].as_str().ok_or("Pool operation missing")?;
        let result: Result<Value, RepoError> = match operation {
            "list" => pool.list().map(Value::Array),
            "get" => pool
                .get("fixture")
                .map(|value| value.unwrap_or(Value::Null)),
            "withheld" => pool.secret_withheld("fixture").map(Value::Bool),
            "update" | "empty-update" => pool
                .update(
                    "fixture",
                    if operation == "update" {
                        Map::from_iter([("label".into(), "α".into())])
                    } else {
                        Map::new()
                    },
                )
                .map(|value| value.unwrap_or(Value::Null)),
            "update-many" => pool
                .update_many(Map::from_iter([("fixture".into(), json!({"label":"α"}))]))
                .map(Value::Array),
            "empty-many" => pool.update_many(Map::new()).map(Value::Array),
            "delete" => pool.delete("fixture").map(Value::Bool),
            "missing-delete" => pool.delete("missing").map(Value::Bool),
            "add-empty" => pool.add_many(Vec::new()).map(Value::Array),
            _ => return Err("Unknown pool text operation".into()),
        };
        let outcome = match result {
            Ok(value) => json!({"result":value,"error":null}),
            Err(RepoError::Io(error)) if error.kind() == io::ErrorKind::InvalidData => {
                json!({"result":null,"error":"UnicodeDecodeError"})
            }
            Err(error) => return Err(error.into()),
        };
        assert_eq!(
            outcome["error"], case["outcome"]["error"],
            "Pool storage text admission differs"
        );
        assert_eq!(
            outcome, case["outcome"],
            "Pool storage operation result differs"
        );
        let after = fs::read(&path).ok();
        let disk = after
            .as_ref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        assert_eq!(disk, case["disk"], "Pool storage disk differs");
        assert_eq!(
            before == after,
            case["unchanged"]
                .as_bool()
                .ok_or("Pool byte verdict missing")?,
            "Pool storage preserved-byte verdict differs"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn pool_edits_reread_and_keep_unrelated_fields_and_secret_intent()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/pool-store.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Pool cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("pool-store-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join("proxies.json");
            let before = serde_json::to_vec(&case["initial"])?;
            fs::write(&path, &before)?;
            let pool = ProxyPool::new(root.clone(), None);
            let actual = match case["operation"].as_str() {
                Some("update") => serde_json::to_value(
                    pool.update(
                        case["id"].as_str().ok_or("ID missing")?,
                        case["changes"]
                            .as_object()
                            .ok_or("Changes missing")?
                            .clone(),
                    )?,
                )?,
                Some("update-many") => serde_json::to_value(
                    pool.update_many(
                        case["changes"]
                            .as_object()
                            .ok_or("Changes missing")?
                            .clone(),
                    )?,
                )?,
                Some("delete") => {
                    Value::Bool(pool.delete(case["id"].as_str().ok_or("ID missing")?)?)
                }
                _ => return Err("Unknown pool operation".into()),
            };
            assert_eq!(actual, case["result"]);
            assert_eq!(serde_json::to_value(pool.list()?)?, case["listed"]);
            let after = fs::read(path)?;
            assert_eq!(
                before == after,
                case["unchanged"].as_bool().ok_or("Byte verdict missing")?
            );
            assert_eq!(serde_json::from_slice::<Value>(&after)?, case["written"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
