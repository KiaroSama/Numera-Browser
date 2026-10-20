use super::{ProfileRepo, Value, common, fs};
use numera_core::{
    models::Profile,
    ownership::capture,
    storage::{dashboard::DashboardStore, pool::ProxyPool, repo::RepoError},
};
use serde_json::{Map, json};
use std::{path::Path, process::Command, time::Duration};

#[test]
fn storage_settings_errors_match_original_lazy_reads_and_preserve_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("storage-settings-errors")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys,time
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.store import ProfileStore
from numera.repo import ProfileRepo
from numera.models import Profile
from numera.server.stores import DashboardStore,ProxyPool
from run_logging import run_log
base=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases'][0]
time.time=lambda:1700000000.0
operations=['vault','profile','profile-empty','dashboard-plain','dashboard-envelope',
            'pool-plain','pool-envelope','pool-withheld','profile-withheld','divergence',
            'refuse','refuse-missing','materialize','materialize-empty','materialize-null',
            'materialize-no-proxy','materialize-kept','materialize-kept-envelope',
            'repo-save','repo-config','dashboard-save','dashboard-config',
            'pool-update','pool-update-empty','pool-delete','pool-delete-envelope','pool-add-empty']
# Deliberately invalid UTF-8 tests decoding refusal, not JSON fallback.
settings=[bytes([255]),bytes([0xc3,0x28]),bytes([0xe2,0x82]),b'{}',b'{broken',b'[]',None]
observed=[]
with run_log('storage-settings-errors-oracle',parent/'logs') as log:
    for operation in operations:
        for raw in settings:
            root=parent/str(len(observed)); store=ProfileStore(root,password=None)
            repo=ProfileRepo(store); dashboard=DashboardStore(root,engine_store=store)
            pool=ProxyPool(root,engine_store=store)
            engine=copy.deepcopy(base['engine']); dash=copy.deepcopy(base['dashboard'])
            dash['folder']=''
            secret='enc:v9:opaque' if operation in ['dashboard-envelope','pool-envelope',
                'pool-withheld','profile-withheld','divergence','refuse','pool-delete-envelope'] else 'fixture-secret'
            dash['proxy']['pass']=secret
            if operation=='profile-withheld':engine['proxy']['password']=secret
            if operation in ['profile-empty','materialize-no-proxy']:
                engine['proxy']=None; dash['proxy']=None
            if operation in ['materialize-empty','materialize-null','repo-save']:
                engine['proxy']['password']=None
                dash['proxy']['pass']='' if operation=='materialize-empty' else None
            if operation in ['materialize-kept','materialize-kept-envelope']:
                dash['proxy']['pass']=''
                if operation=='materialize-kept-envelope':engine['proxy']['password']='enc:v9:retained'
            row={'id':'proxy00001','host':'127.0.0.1','port':'8080','type':'http','pass':secret}
            if operation=='pool-update-empty':row['pass']=None
            inputs={'profiles/fixture00001.json':json.dumps(engine).encode('utf-8'),
                    'dashboard/fixture00001.json':json.dumps(dash).encode('utf-8'),
                    'proxies.json':json.dumps([row]).encode('utf-8'),'config.json':raw}
            for name,payload in inputs.items():
                if payload is not None:
                    path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(payload)
            try:
                if operation=='vault':result=store.vault()
                elif operation in ['profile','profile-empty']:result=store.get('fixture00001').to_dict()
                elif operation in ['dashboard-plain','dashboard-envelope']:result=dashboard.get('fixture00001')
                elif operation in ['pool-plain','pool-envelope']:result=pool.list()
                elif operation=='pool-withheld':result=pool.secret_withheld('proxy00001')
                elif operation=='profile-withheld':result=store.secret_withheld('fixture00001')
                elif operation=='divergence':result=repo.divergence('fixture00001')
                elif operation in ['refuse','refuse-missing']:
                    result=repo.refuse_unresolved_secret('fixture00001','fixture-secret',
                        None if operation=='refuse-missing' else secret)
                elif operation.startswith('materialize'):
                    result=repo.materialize_current('fixture00001').to_dict()
                elif operation=='repo-save':result=store.save(Profile.from_dict(engine)).to_dict()
                elif operation=='repo-config':result=repo.save_many([],config_change={'future':7})
                elif operation.startswith('dashboard-'):
                    result=dashboard.save(dash,secret_intent='preserve' if operation=='dashboard-config' else 'replace',
                        config_change={'future':7} if operation=='dashboard-config' else None)
                elif operation.startswith('pool-update'):result=pool.update('proxy00001',{'status':'checked'})
                elif operation.startswith('pool-delete'):result=pool.delete('proxy00001')
                else:result=pool.add_many([])
                error=None
            except UnicodeDecodeError:result,error=None,'UnicodeDecodeError'
            except Exception as exc:result,error=None,str(exc)
            after={name:(root/name).read_bytes() if (root/name).exists() else None for name in inputs}
            unchanged=[after[name]==payload for name,payload in inputs.items()]
            if error is not None:assert all(unchanged),'Original refusal changed input bytes'
            assert after['config.json']==raw or operation in ['repo-config','dashboard-config']
            observed.append({'operation':operation,'raw':list(raw) if raw is not None else None,
                'engine':json.loads(inputs['profiles/fixture00001.json']),
                'dashboard':json.loads(inputs['dashboard/fixture00001.json']),
                'secret':secret,'result':result,'error':error,'unchanged':unchanged,
                'written':{name:json.loads(payload) if payload is not None else None
                           for name,payload in after.items() if name!='config.json'},
                'config':json.loads(after['config.json']) if operation in ['repo-config','dashboard-config'] and error is None else None})
    log.info('Observed storage settings cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 4_194_304,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original storage settings observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 189);
    for (index, case) in cases.iter().enumerate() {
        let operation = case["operation"].as_str().ok_or("Operation missing")?;
        let root = temporary.path().join(format!("native-{index}"));
        let engine = &case["engine"];
        let dash = &case["dashboard"];
        let row = json!({"id":"proxy00001","host":"127.0.0.1","port":"8080","type":"http",
            "pass":if operation == "pool-update-empty" { Value::Null } else { case["secret"].clone() }});
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        let inputs = [
            (
                "profiles/fixture00001.json",
                Some(serde_json::to_vec(engine)?),
            ),
            (
                "dashboard/fixture00001.json",
                Some(serde_json::to_vec(dash)?),
            ),
            ("proxies.json", Some(serde_json::to_vec(&json!([row]))?)),
            ("config.json", raw.clone()),
        ];
        for (name, bytes) in &inputs {
            if let Some(bytes) = bytes {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
                fs::write(path, bytes)?;
            }
        }
        let repo = ProfileRepo::new(root.clone(), None);
        let dashboard = DashboardStore::new(root.clone(), None);
        let pool = ProxyPool::new(root.clone(), None);
        let changes = json!({"future":7})
            .as_object()
            .ok_or("Changes missing")?
            .clone();
        let result = (|| -> Result<Value, RepoError> {
            Ok(match operation {
                "vault" => repo.vault()?.unwrap_or(Value::Null),
                "profile" | "profile-empty" => json!(repo.get("fixture00001")?),
                "dashboard-plain" | "dashboard-envelope" => {
                    dashboard.get("fixture00001")?.unwrap_or(Value::Null)
                }
                "pool-plain" | "pool-envelope" => Value::Array(pool.list()?),
                "pool-withheld" => pool.secret_withheld("proxy00001")?.into(),
                "profile-withheld" => repo.secret_withheld("fixture00001")?.into(),
                "divergence" => json!(repo.divergence("fixture00001", 1700000000.0)?),
                "refuse" | "refuse-missing" => {
                    repo.refuse_unresolved_secret(
                        "fixture00001",
                        Some("fixture-secret"),
                        if operation == "refuse-missing" {
                            None
                        } else {
                            case["secret"].as_str()
                        },
                    )?;
                    Value::Null
                }
                "materialize"
                | "materialize-empty"
                | "materialize-null"
                | "materialize-no-proxy"
                | "materialize-kept"
                | "materialize-kept-envelope" => {
                    json!(repo.materialize("fixture00001", 1700000000.0)?)
                }
                "repo-save" => json!(repo.save(
                    Profile::from_record(engine).map_err(|e| RepoError::Message(e.to_string()))?,
                    1700000000.0
                )?),
                "repo-config" => {
                    json!(repo.save_many(Vec::new(), changes.clone(), "reject", 1700000000.0)?)
                }
                "dashboard-save" | "dashboard-config" => dashboard.save(
                    dash.clone(),
                    if operation == "dashboard-config" {
                        changes.clone()
                    } else {
                        Map::new()
                    },
                    None,
                    Some(if operation == "dashboard-config" {
                        "preserve"
                    } else {
                        "replace"
                    }),
                    1700000000.0,
                )?,
                "pool-update" | "pool-update-empty" => pool
                    .update(
                        "proxy00001",
                        json!({"status":"checked"})
                            .as_object()
                            .ok_or_else(|| RepoError::Message("Patch missing".into()))?
                            .clone(),
                    )?
                    .unwrap_or(Value::Null),
                "pool-delete" | "pool-delete-envelope" => pool.delete("proxy00001")?.into(),
                "pool-add-empty" => Value::Array(pool.add_many(Vec::new())?),
                _ => return Err(RepoError::Message("Unknown test operation".into())),
            })
        })();
        let invalid = matches!(&result, Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Storage settings admission differs: operation={operation} case={index}"
        );
        match result {
            Ok(value) => {
                assert!(case["error"].is_null());
                assert_eq!(value, case["result"], "Storage result differs: {operation}");
            }
            Err(error) if !invalid => {
                assert_eq!(Some(error.to_string().as_str()), case["error"].as_str())
            }
            Err(_) => {}
        }
        for (slot, (name, before)) in inputs.iter().enumerate() {
            let after = fs::read(root.join(name)).ok();
            assert_eq!(
                json!(after == *before),
                case["unchanged"][slot],
                "Storage byte preservation differs: {operation} {name}"
            );
            if *name == "config.json" {
                if case["config"].is_null() {
                    assert_eq!(after, raw, "Settings bytes changed");
                } else {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&after.ok_or("Config missing")?)?,
                        case["config"]
                    );
                }
            } else {
                assert_eq!(
                    after
                        .map(|bytes| serde_json::from_slice::<Value>(&bytes))
                        .transpose()?
                        .unwrap_or(Value::Null),
                    case["written"][*name]
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}
