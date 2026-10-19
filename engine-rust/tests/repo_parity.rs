use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
mod common;

#[test]
fn public_divergence_reports_all_ordered_fields_without_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use numera_core::{ownership::capture, storage::repo::RepoError};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("repo-divergence")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import base64,copy,json,os,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.repo import ProfileRepo
from numera.store import ProfileStore
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases']
base=fixture[0]
cases=[]
def add(label,engine=None,dashboard=None,config=None,password=None,raw=None):
    cases.append((label,copy.deepcopy(base['engine'] if engine is None else engine),
                  copy.deepcopy(base['dashboard'] if dashboard is None else dashboard),
                  config or {},password,raw or {}))
add('healthy')
engine=copy.deepcopy(base['engine'])
engine.update(notes='different',name='different',tags=['different'],engine='camoufox')
engine['fingerprint']['locale']='fa-IR'
engine['proxy'].update(password='different',server='http://127.0.0.1:8081')
engine.update({'z':1,'a':1,chr(0xe000):1,chr(0x1f600):1,'absent-null':None})
add('complete-sorted',engine=engine)
engine=copy.deepcopy(base['engine'])
engine.update(id='other',created_at=-1,updated_at=99)
engine['fingerprint'].update(viewport_width=1,viewport_height=2)
add('ignored-identity-and-viewport',engine=engine)
for field,value in [('name','مهاجرت α 😀'),('notes','日本語'),('tags',['β','α']),
                    ('engine','patchright'),('startup_urls',['https://example.test']),
                    ('bookmarks',[{'name':'α','url':'https://example.test','extra':[True,None]}]),
                    ('future',{'b':[1,True],'a':None})]:
    engine=copy.deepcopy(base['engine']); engine[field]=value
    add('field-'+field,engine=engine)
for value in (16.0,True,False,0,-0.0,0.25,9007199254740993,1e100):
    engine=copy.deepcopy(base['engine']); engine['fingerprint']['device_memory']=value
    add('numeric-'+repr(value),engine=engine)
for stored,fresh in [('fixture-secret','different'),(None,'fixture-secret'),
                     ('fixture-secret',None),('',''),('enc:v9:opaque','different')]:
    engine=copy.deepcopy(base['engine']); dashboard=copy.deepcopy(base['dashboard'])
    engine['proxy']['password']=stored; dashboard['proxy']['pass']=fresh
    add('secret-'+str(len(cases)),engine=engine,dashboard=dashboard)
for field,value in [('type','http'),('port',8080),('country','GB')]:
    dashboard=copy.deepcopy(base['dashboard']); dashboard['proxy'][field]=value
    add('proxy-'+field,dashboard=dashboard)
for side in ('profiles','dashboard'):
    for payload in (None,b'{broken',b'null',b'{}',b'false',b'[]',b'\xff',b'{"notes":"\xe2\x82'):
        add('raw-'+side+'-'+str(len(cases)),raw={side:payload})
add('both-absent',raw={'profiles':None,'dashboard':None})
for index in (10,11,12):
    case=fixture[index]
    add('encrypted-'+str(index),engine=case['engine'],dashboard=case['dashboard'],
        config=case['config'],password=case['password'])
# An unconvertible dashboard answers no comparison, not a guessed repair.
dashboard=copy.deepcopy(base['dashboard']); dashboard['proxy']['type']='invalid'
add('unconvertible',dashboard=dashboard)
observed=[]
with run_log('repo-divergence-oracle',parent/'logs') as log:
    for label,engine,dashboard,config,password,raw in cases:
        root=parent/str(len(observed)); store=ProfileStore(root,password=password)
        repo=ProfileRepo(store)
        inputs={'profiles/fixture00001.json':json.dumps(engine).encode('utf-8'),
                'dashboard/fixture00001.json':json.dumps(dashboard).encode('utf-8'),
                'config.json':json.dumps(config).encode('utf-8')}
        for side,payload in raw.items(): inputs[side+'/fixture00001.json']=payload
        for name,payload in inputs.items():
            if payload is not None:
                path=root/name; path.parent.mkdir(parents=True,exist_ok=True); path.write_bytes(payload)
        before={name:(root/name).read_bytes() if (root/name).exists() else None for name in inputs}
        try: result,error=repo.divergence('fixture00001'),None
        except UnicodeDecodeError: result,error=None,'UnicodeDecodeError'
        after={name:(root/name).read_bytes() if (root/name).exists() else None for name in inputs}
        assert after==before,'Original divergence changed profile bytes'
        observed.append({'label':label,'files':{name:base64.b64encode(value).decode('ascii') if value is not None else None
                                              for name,value in before.items()},
                         'password':password,'result':result,'error':error})
    complete=next(case['result'] for case in observed if case['label']=='complete-sorted')
    assert len(complete)>8 and complete.index('proxy.password')<complete.index('proxy')
    log.info('Observed public divergence cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1_048_576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original divergence observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 47, "Divergence observations missing");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["error"] == "UnicodeDecodeError")
            .count(),
        4,
        "Original divergence text refusals missing"
    );
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let files = case["files"]
            .as_object()
            .ok_or("Divergence inputs missing")?;
        for (name, bytes) in files {
            if !bytes.is_null() {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
                let expected = STANDARD.decode(bytes.as_str().ok_or("Input bytes missing")?)?;
                fs::write(&path, &expected)?;
                assert_eq!(fs::read(path)?, expected, "Divergence input bytes differ");
            }
        }
        let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
        let result = repo.divergence("fixture00001", 1700000000.0);
        let invalid = matches!(&result, Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Divergence text admission differs: {}",
            case["label"]
        );
        if !invalid {
            assert_eq!(
                serde_json::to_value(result?)?,
                case["result"],
                "Complete ordered divergence differs: {}",
                case["label"]
            );
        }
        for (name, bytes) in files {
            let path = root.join(name);
            if bytes.is_null() {
                assert!(!path.try_exists()?, "Divergence created an absent record");
            } else {
                assert_eq!(
                    fs::read(path)?,
                    STANDARD.decode(bytes.as_str().ok_or("Input bytes missing")?)?,
                    "Divergence rewrote stored bytes: {}",
                    case["label"]
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn dual_store_derivation_preserves_viewport_credentials_and_unchanged_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing reconciliation cases")?;
    assert!(!cases.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "repo-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(root.join("profiles"))?;
    fs::create_dir_all(root.join("dashboard"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for case in cases {
            let paths = [
                root.join("profiles/fixture00001.json"),
                root.join("dashboard/fixture00001.json"),
            ];
            for (path, key) in paths.iter().zip(["engine", "dashboard"]) {
                if path.exists() {
                    fs::remove_file(path)?;
                }
                if !case[key].is_null() {
                    fs::write(path, serde_json::to_vec(&case[key])?)?;
                }
            }
            let before: Vec<_> = paths.iter().map(fs::read).map(Result::ok).collect();
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
            let out = if case["operation"] == "adopt" {
                repo.adopt("fixture00001", 1700000000.0).map(Value::Bool)
            } else {
                repo.materialize("fixture00001", 1700000000.0).map(|p| {
                    p.map_or(Value::Null, |p| {
                        serde_json::to_value(p).expect("Profile serialization")
                    })
                })
            };
            match out {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(value, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (index, path) in paths.iter().enumerate() {
                let after = fs::read(path).ok();
                assert_eq!(
                    before[index] == after,
                    case["unchanged"][index]
                        .as_bool()
                        .ok_or("Missing byte observation")?
                );
                let disk = after
                    .map(|bytes| serde_json::from_slice::<Value>(&bytes))
                    .transpose()?
                    .unwrap_or(Value::Null);
                assert_eq!(disk, case["written"][index]);
            }
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
