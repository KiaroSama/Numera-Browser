use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
mod common;

#[test]
fn repository_settings_cache_matches_original_across_secret_reads()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        ownership::capture,
        storage::{dashboard::DashboardStore, pool::ProxyPool},
    };
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("repository-settings-cache")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,os,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.store import ProfileStore
from numera.server.stores import DashboardStore,ProxyPool
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases'][11]
a=json.dumps(fixture['config'],separators=(',',':')).encode('utf-8')
salt=fixture['config']['vault']['salt'].encode('ascii')
changed=(b'A' if salt[:1]!=b'A' else b'B')+salt[1:]
b=a.replace(salt,changed)
assert a!=b and len(a)==len(b)
steps=[(a,0),(b,0),(b,1),(a,1),(a+b' ',1),(None,1),(a,1),
       (b'{broken',2),(b,1),(b'[]',3),(b,1),(a,4)]
observed=[]
with run_log('repository-settings-cache-oracle',parent/'logs') as log:
    for kind in ['status','profile','dashboard','pool']:
        root=parent/kind
        store=ProfileStore(root,password=fixture['password'])
        dashboard=DashboardStore(root,engine_store=store)
        pool=ProxyPool(root,engine_store=store)
        files={'profiles/fixture00001.json':json.dumps(fixture['engine']).encode('utf-8'),
               'dashboard/fixture00001.json':json.dumps(fixture['dashboard']).encode('utf-8'),
               'proxies.json':json.dumps([{'id':'proxy00001','pass':fixture['dashboard']['proxy']['pass']}]).encode('utf-8')}
        for name,raw in files.items():
            path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
        def read(engine,dash,proxies):
            if kind=='status':return engine.vault()
            if kind=='profile':return engine.get('fixture00001').proxy.password
            if kind=='dashboard':return dash.get('fixture00001')['proxy']['pass']
            return proxies.list()[0]['pass']
        for raw,tick in steps:
            path=root/'config.json'
            if raw is None:path.unlink()
            else:
                path.write_bytes(raw);stamp=1800000000*1000000000+tick*100
                os.utime(path,ns=(stamp,stamp))
                assert path.stat().st_mtime_ns==stamp,'Fixture timestamp was not retained'
            result=read(store,dashboard,pool)
            fresh=ProfileStore(root,password=fixture['password'])
            independent=read(fresh,DashboardStore(root,engine_store=fresh),ProxyPool(root,engine_store=fresh))
            assert (path.read_bytes() if path.exists() else None)==raw
            assert all((root/name).read_bytes()==value for name,value in files.items())
            observed.append({'kind':kind,'raw':list(raw) if raw is not None else None,
                             'tick':tick,'result':result,'independent':independent})
        # A cache hit cannot bypass a journal introduced after this instance opened.
        journal=root/'vault-migration.json';journal.write_bytes(b'{')
        try:read(store,dashboard,pool)
        except RuntimeError:pass
        else:raise AssertionError('Original warmed cache bypassed unresolved journal')
        assert journal.read_bytes()==b'{' and path.read_bytes()==a
        journal.unlink()
    log.info('Observed repository cache cases=%s',len(observed))
    print(json.dumps({'cases':observed,'fixture':fixture},ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original repository cache observer failed"
    );
    let observed: Value = serde_json::from_slice(&original.stdout)?;
    let fixture = &observed["fixture"];
    let cases = observed["cases"].as_array().ok_or("Cache cases missing")?;
    assert_eq!(cases.len(), 48);
    for kind in ["status", "profile", "dashboard", "pool"] {
        let root = temporary.path().join("native").join(kind);
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("dashboard"))?;
        let inputs = [
            (
                root.join("profiles").join("fixture00001.json"),
                serde_json::to_vec(&fixture["engine"])?,
            ),
            (
                root.join("dashboard").join("fixture00001.json"),
                serde_json::to_vec(&fixture["dashboard"])?,
            ),
            (
                root.join("proxies.json"),
                serde_json::to_vec(&serde_json::json!([
                {"id":"proxy00001","pass":fixture["dashboard"]["proxy"]["pass"]}]))?,
            ),
        ];
        for (path, bytes) in &inputs {
            fs::write(path, bytes)?;
        }
        let password = fixture["password"]
            .as_str()
            .ok_or("Fixture password missing")?;
        let repo = ProfileRepo::new(root.clone(), Some(password.into()));
        let dashboard = DashboardStore::new(root.clone(), Some(password.into()));
        let pool = ProxyPool::new(root.clone(), Some(password.into()));
        let read = |repo: &ProfileRepo,
                    dashboard: &DashboardStore,
                    pool: &ProxyPool|
         -> Result<Value, Box<dyn std::error::Error>> {
            Ok(match kind {
                "status" => repo.vault()?.unwrap_or(Value::Null),
                "profile" => repo
                    .get("fixture00001")?
                    .ok_or("Profile missing")?
                    .proxy
                    .ok_or("Proxy missing")?
                    .to_record()["password"]
                    .clone(),
                "dashboard" => dashboard.get("fixture00001")?.ok_or("Dashboard missing")?["proxy"]
                    ["pass"]
                    .clone(),
                _ => pool.list()?.first().ok_or("Pool entry missing")?["pass"].clone(),
            })
        };
        let path = root.join("config.json");
        let mut retained_safety_cases = 0;
        for (index, case) in cases.iter().filter(|case| case["kind"] == kind).enumerate() {
            let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
            if let Some(bytes) = &raw {
                fs::write(&path, bytes)?;
                let stamp = UNIX_EPOCH
                    + Duration::from_secs(1_800_000_000)
                    + Duration::from_nanos(case["tick"].as_u64().ok_or("Tick missing")? * 100);
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)?
                    .set_modified(stamp)?;
                assert_eq!(fs::metadata(&path)?.modified()?, stamp);
            } else {
                fs::remove_file(&path)?;
            }
            let result = read(&repo, &dashboard, &pool)?;
            let independent = read(
                &ProfileRepo::new(root.clone(), Some(password.into())),
                &DashboardStore::new(root.clone(), Some(password.into())),
                &ProxyPool::new(root.clone(), Some(password.into())),
            )?;
            // Retain the native never-ciphertext safeguard where the original
            // returns stored ciphertext because the vault descriptor is absent.
            let compare = |actual: Value, expected: &Value| {
                if kind == "profile" && expected == &fixture["engine"]["proxy"]["password"] {
                    assert!(actual.is_null(), "Missing vault exposed stored ciphertext");
                    true
                } else {
                    assert_eq!(
                        actual, *expected,
                        "Repository settings cache differs: kind={kind} step={index}"
                    );
                    false
                }
            };
            if compare(result, &case["result"]) {
                retained_safety_cases += 1;
            }
            compare(independent, &case["independent"]);
            assert_eq!(fs::read(&path).ok(), raw, "Cache read changed settings");
            for (path, bytes) in &inputs {
                assert_eq!(fs::read(path)?, *bytes);
            }
        }
        assert_eq!(
            retained_safety_cases,
            if kind == "profile" { 3 } else { 0 },
            "Original/native absent-vault safety differences changed"
        );
        let before = fs::read(&path)?;
        let journal = root.join("vault-migration.json");
        fs::write(&journal, b"{")?;
        assert!(
            read(&repo, &dashboard, &pool).is_err(),
            "Warmed cache bypassed unresolved journal"
        );
        assert_eq!(fs::read(&journal)?, b"{");
        assert_eq!(fs::read(&path)?, before);
        fs::remove_file(journal)?;
    }
    temporary.cleanup()?;
    Ok(())
}

#[path = "repo_parity/settings_errors.rs"]
mod settings_errors;

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
