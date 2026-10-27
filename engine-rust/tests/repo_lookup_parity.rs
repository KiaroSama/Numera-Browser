use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};
mod common;
#[path = "repo_lookup_parity/read_warnings.rs"]
mod read_warnings;

#[test]
fn direct_name_lookup_matches_original_without_id_precedence()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("direct-name-lookup")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys,warnings
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import convert
from numera.store import ProfileStore
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/repo-lookup.json').read_text(encoding='utf-8'))['cases']
original=convert.Profile
def clocked(*args,**kwargs):
    kwargs.setdefault('created_at',1700000000.0)
    kwargs.setdefault('updated_at',1700000000.0)
    return original(*args,**kwargs)
convert.Profile=clocked
variants=[(copy.deepcopy(fixture[i]['files']),reference) for i,reference in
          [(0,'alpha'),(0,'fixture-a'),(1,'alpha'),(2,'missing'),(3,'alpha'),(4,'alpha'),(7,'alpha')]]
files=copy.deepcopy(fixture[0]['files'])
other=copy.deepcopy(files['profiles/fixture-a.json'])
other.update(id='fixture-b',name='fixture-a')
files['profiles/fixture-b.json']=other
variants.extend((copy.deepcopy(files),reference) for reference in ['fixture-a','fixture-b','alpha',''])
for stored_id,name,reference in [('fixture-a','',''),('missing-id','needle','needle'),('', 'alpha','alpha')]:
    files=copy.deepcopy(fixture[0]['files'])
    files['profiles/fixture-a.json'].update(id=stored_id,name=name)
    variants.append((files,reference))
observed=[]
with run_log('direct-name-oracle',parent/'logs') as log,warnings.catch_warnings():
    warnings.simplefilter('ignore',RuntimeWarning)
    for index,(files,reference) in enumerate(variants):
        root=parent/str(index)
        store=ProfileStore(root)
        for name,value in files.items():
            path=root/name
            path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text(json.dumps(value),encoding='utf-8')
        before={name:(root/name).read_bytes() for name in files}
        try:
            found=store.find_by_name(reference)
            result=None if found is None else found.to_dict()
            error=None
        except RuntimeError as exc:
            result,error=None,str(exc)
        disk={str(p.relative_to(root)).replace('\\','/'):json.loads(p.read_text(encoding='utf-8'))
              for kind in ['profiles','dashboard'] for p in (root/kind).glob('*.json')}
        observed.append({'files':files,'reference':reference,'result':result,'error':error,'disk':disk,
                         'unchanged':{name:(root/name).read_bytes()==raw for name,raw in before.items()}})
    log.info('Observed direct name lookup cases=%s',len(observed))
    print(json.dumps(observed))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1_048_576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original direct-name observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 14);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let files = case["files"].as_object().ok_or("Lookup files missing")?;
        let mut before = Map::new();
        for (name, value) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Lookup parent missing")?)?;
            let bytes = serde_json::to_vec(value)?;
            fs::write(path, &bytes)?;
            before.insert(name.clone(), serde_json::json!(bytes));
        }
        let repo = ProfileRepo::new(root.clone(), None);
        let actual = repo.find_by_name(
            case["reference"].as_str().ok_or("Lookup name missing")?,
            1700000000.0,
        );
        match actual {
            Ok(found) => {
                assert!(
                    case["error"].is_null(),
                    "Direct name lookup unexpectedly succeeded: {index}"
                );
                assert_eq!(
                    serde_json::to_value(found)?,
                    case["result"],
                    "Direct name lookup differs: {index}"
                );
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "Direct name refusal differs: {index}"
            ),
        }
        let mut disk = Map::new();
        for kind in ["profiles", "dashboard"] {
            let directory = root.join(kind);
            if !directory.exists() {
                continue;
            }
            for entry in fs::read_dir(directory)? {
                let path = entry?.path();
                if path.extension().is_none_or(|value| value != "json") {
                    continue;
                }
                let name = format!(
                    "{kind}/{}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .ok_or("Lookup filename missing")?
                );
                disk.insert(name, serde_json::from_slice(&fs::read(path)?)?);
            }
        }
        assert_eq!(
            Value::Object(disk),
            case["disk"],
            "Direct name lookup disk differs: {index}"
        );
        for (name, bytes) in before {
            assert_eq!(
                Value::Bool(serde_json::json!(fs::read(root.join(&name))?) == bytes),
                case["unchanged"][&name],
                "Direct name preservation differs: {index}"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn damaged_profile_text_refuses_before_repair_or_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        models::Profile,
        ownership::capture,
        storage::{dashboard::DashboardStore, repo::RepoError},
    };
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("profile-text-admission")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import convert
from numera.models import Profile
from numera.repo import ProfileRepo
from numera.store import ProfileStore
from numera.server.stores import DashboardStore
from run_logging import run_log
base=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases'][0]
original=convert.Profile
def clocked(*args,**kwargs):
    kwargs.setdefault('created_at',1700000000.0)
    kwargs.setdefault('updated_at',1700000000.0)
    return original(*args,**kwargs)
convert.Profile=clocked
payloads=[b'\xff',b'{"notes":"\xff"}',b'{"notes":"\xe2\x82',b'{broken']
operations=('materialize','adopt','coherent','resolve','list','save','dash-save','dash-patch')
observed=[]
with run_log('profile-text-oracle',parent/'logs') as log:
    for damaged in ('profiles','dashboard'):
        for payload in payloads:
            for operation in operations:
                if payload==b'{broken' and operation not in ('materialize','adopt'):
                    continue
                root=parent/str(len(observed))
                store=ProfileStore(root)
                repo=ProfileRepo(store)
                dashboard=DashboardStore(root,store)
                paths=[root/'profiles/fixture00001.json',root/'dashboard/fixture00001.json']
                for path,value in zip(paths,(base['engine'],base['dashboard'])):
                    path.write_text(json.dumps(value),encoding='utf-8')
                (root/damaged/'fixture00001.json').write_bytes(payload)
                before=[path.read_bytes() for path in paths]
                try:
                    if operation=='materialize': result=repo.materialize_current('fixture00001')
                    elif operation=='adopt': result=repo.adopt_engine_record('fixture00001')
                    elif operation=='coherent': result=repo.coherent('fixture00001')
                    elif operation=='resolve': result=store.resolve('fixture00001')
                    elif operation=='list': result=store.list()
                    elif operation=='save': result=repo.create(Profile.from_dict(copy.deepcopy(base['engine'])),unique_name=False)
                    elif operation=='dash-save': result=dashboard.save(copy.deepcopy(base['dashboard']))
                    else: result=dashboard.patch('fixture00001',lambda value:(dict(value,notes='changed'),{}))
                    result=([value.to_dict() for value in result] if isinstance(result,list)
                            else result.to_dict() if isinstance(result,Profile) else result)
                    error=None
                except UnicodeDecodeError:
                    result,error=None,'UnicodeDecodeError'
                after=[path.read_bytes() for path in paths]
                unchanged=[a==b for a,b in zip(before,after)]
                if error=='UnicodeDecodeError':
                    assert all(unchanged),'Original refusal replaced damaged profile bytes'
                observed.append({'operation':operation,'files':[list(raw) for raw in before],
                                 'result':result,'error':error,'unchanged':unchanged,
                                 'written':[json.loads(raw) if changed else None
                                            for raw,changed in zip(after,(not value for value in unchanged))]})
    log.info('Observed damaged profile text cases=%s',len(observed))
    print(json.dumps({'cases':observed,'profile':base['engine'],'dashboard':base['dashboard']}))
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
    assert_eq!(observed.exit, 0, "Original profile text observer failed");
    let document: Value = serde_json::from_slice(&observed.stdout)?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Profile text cases missing")?;
    assert_eq!(cases.len(), 52, "Profile text observations missing");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["error"] == "UnicodeDecodeError")
            .count(),
        48,
        "Original damaged profile refusals missing"
    );
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let paths = [
            root.join("profiles/fixture00001.json"),
            root.join("dashboard/fixture00001.json"),
        ];
        for (path, bytes) in paths
            .iter()
            .zip(case["files"].as_array().ok_or("Input bytes missing")?)
        {
            fs::create_dir_all(path.parent().ok_or("Profile parent missing")?)?;
            let expected = serde_json::from_value::<Vec<u8>>(bytes.clone())?;
            fs::write(path, &expected)?;
            assert_eq!(fs::read(path)?, expected, "Profile text input bytes differ");
        }
        let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
        let repo = ProfileRepo::new(root.clone(), None);
        let dashboard = DashboardStore::new(root, None);
        let time = 1700000000.0;
        let result = match case["operation"].as_str() {
            Some("materialize") => repo
                .materialize("fixture00001", time)
                .map(serde_json::to_value),
            Some("adopt") => repo.adopt("fixture00001", time).map(serde_json::to_value),
            Some("coherent") => repo
                .coherent("fixture00001", time)
                .map(serde_json::to_value),
            Some("resolve") => repo.resolve("fixture00001", time).map(serde_json::to_value),
            Some("list") => repo.list(time).map(serde_json::to_value),
            Some("save") => repo
                .save(Profile::from_record(&document["profile"])?, time)
                .map(serde_json::to_value),
            Some("dash-save") => dashboard
                .save(document["dashboard"].clone(), Map::new(), None, None, time)
                .map(serde_json::to_value),
            Some("dash-patch") => dashboard
                .patch(
                    "fixture00001",
                    |mut record| {
                        record["notes"] = "changed".into();
                        Ok((record, Map::new()))
                    },
                    None,
                    None,
                    time,
                )
                .map(serde_json::to_value),
            _ => return Err("Unknown profile text operation".into()),
        };
        let invalid = matches!(&result, Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Profile text admission differs: case={index} operation={}",
            case["operation"]
        );
        if !invalid {
            assert_eq!(
                result??, case["result"],
                "Profile text result differs: case={index}"
            );
        }
        for (position, (path, before)) in paths.iter().zip(before).enumerate() {
            let after = fs::read(path)?;
            assert_eq!(
                Value::Bool(before == after),
                case["unchanged"][position],
                "Profile text byte preservation differs: case={index}"
            );
            if invalid {
                assert_eq!(
                    after, before,
                    "Damaged profile bytes replaced: case={index}"
                );
            } else if case["unchanged"][position] == false {
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][position],
                    "Valid UTF8 fallback state differs: case={index}"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn lookup_reconciles_both_copies_and_refuses_ambiguous_names()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-lookup.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing lookup cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("lookup-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (name, data) in case["files"].as_object().ok_or("Files missing")? {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    if data == "broken-json" {
                        b"{broken".to_vec()
                    } else {
                        serde_json::to_vec(data)?
                    },
                )?;
            }
            let repo = ProfileRepo::new(root.clone(), None);
            let reference = case["reference"].as_str().unwrap_or("");
            let actual = match case["operation"].as_str() {
                Some("resolve") => repo.resolve(reference, 1700000000.0).and_then(|v| {
                    serde_json::to_value(v).map_err(|e| {
                        numera_core::storage::repo::RepoError::from(std::io::Error::other(e))
                    })
                }),
                Some("coherent") => repo.coherent(reference, 1700000000.0).and_then(|v| {
                    serde_json::to_value(v).map_err(|e| {
                        numera_core::storage::repo::RepoError::from(std::io::Error::other(e))
                    })
                }),
                Some("list") => repo.list(1700000000.0).and_then(|v| {
                    serde_json::to_value(v).map_err(|e| {
                        numera_core::storage::repo::RepoError::from(std::io::Error::other(e))
                    })
                }),
                _ => return Err("Unknown lookup operation".into()),
            };
            match actual {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(value, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            let mut written = Map::new();
            for kind in ["profiles", "dashboard"] {
                let directory = root.join(kind);
                if !directory.exists() {
                    continue;
                }
                for entry in fs::read_dir(directory)? {
                    let path = entry?.path();
                    if path.extension().is_none_or(|v| v != "json") {
                        continue;
                    }
                    let name = format!(
                        "{kind}/{}",
                        path.file_name()
                            .ok_or("Filename missing")?
                            .to_str()
                            .ok_or("Filename not Unicode")?
                    );
                    let value = serde_json::from_slice::<Value>(&fs::read(path)?)
                        .unwrap_or(Value::String("broken-json".into()));
                    written.insert(name, value);
                }
            }
            assert_eq!(Value::Object(written), case["written"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn ordinary_encrypted_reads_preserve_original_bytes_and_repair_only_real_drift()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("encrypted read parity")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Source interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,os,sys,warnings
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
os.environ.pop('NUMERA_PASSWORD',None)
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import convert
from numera.models import Profile
from numera.repo import ProfileRepo
from numera.store import ProfileStore
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases']
base=fixture[11]
variants=[]
for operation in ('resolve','list'):
    for mode in ('unlocked','locked','missing-secret','added-secret','conflict','notes',
                 'unknown-null','unknown-value','viewport','numeric','large-integer',
                 'invalid-dashboard','coherent-conflict'):
        engine,dashboard=copy.deepcopy(base['engine']),copy.deepcopy(base['dashboard'])
        password=base['password'] if mode!='locked' else None
        if mode=='missing-secret': dashboard['proxy']['pass']=''
        if mode=='added-secret': engine['proxy']['password']=None
        if mode=='conflict' or mode=='coherent-conflict': dashboard=copy.deepcopy(fixture[12]['dashboard'])
        if mode=='notes': dashboard['notes']='dashboard edit'
        if mode=='unknown-null': engine['future']=None
        if mode=='unknown-value': engine['future']={'keep':True}
        if mode=='viewport': engine['fingerprint']['viewport_width']=777
        if mode=='numeric': engine['fingerprint']['device_memory']=float(engine['fingerprint']['device_memory'])
        if mode=='large-integer': engine['fingerprint']['device_memory']=9007199254740993; dashboard['memory']=9007199254740992.0
        if mode=='invalid-dashboard': dashboard['proxy']['type']='unsupported'
        variants.append((operation,mode,engine,dashboard,password))
parent.mkdir(parents=True)
original=convert.Profile
def clocked(*args,**kwargs):
    kwargs.setdefault('created_at',1700000000.0)
    kwargs.setdefault('updated_at',1700000000.0)
    return original(*args,**kwargs)
convert.Profile=clocked
observed=[]
with run_log('encrypted-read-oracle',parent/'logs') as log, warnings.catch_warnings():
    warnings.simplefilter('ignore',RuntimeWarning)
    for index,(operation,mode,engine,dashboard,password) in enumerate(variants):
        root=parent/str(index)
        store=ProfileStore(root,password=password)
        (root/'dashboard').mkdir()
        (root/'config.json').write_text(json.dumps(base['config']),encoding='utf-8')
        paths=[root/'profiles/fixture00001.json',root/'dashboard/fixture00001.json']
        for path,value in zip(paths,(engine,dashboard)):
            path.write_text(json.dumps(value),encoding='utf-8')
        before=[path.read_bytes() for path in paths]
        try:
            value=(ProfileRepo(store).coherent('fixture00001') if mode=='coherent-conflict'
                   else store.resolve('fixture00001') if operation=='resolve' else store.list())
            result=[p.to_dict() for p in value] if isinstance(value,list) else value.to_dict()
            error=None
        except RuntimeError as exc:
            result,error=None,str(exc)
        after=[path.read_bytes() for path in paths]
        observed.append({'name':operation+'-'+mode,'operation':operation,'coherent':mode=='coherent-conflict',
                         'engine':engine,'dashboard':dashboard,'config':base['config'],'password':password,
                         'result':result,'error':error,'unchanged':[a==b for a,b in zip(before,after)],
                         'written':[json.loads(raw) for raw in after]})
    log.info('Observed encrypted read cases=%s',len(observed))
    print(json.dumps(observed))
"#]).arg(repository).arg(temporary.path().join("source"));
    let source = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1_048_576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(source.exit, 0, "Original encrypted read observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&source.stdout)?;
    assert_eq!(cases.len(), 26, "Encrypted read observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("dashboard"))?;
        fs::write(
            root.join("config.json"),
            serde_json::to_vec(&case["config"])?,
        )?;
        let paths = [
            root.join("profiles/fixture00001.json"),
            root.join("dashboard/fixture00001.json"),
        ];
        for (path, key) in paths.iter().zip(["engine", "dashboard"]) {
            fs::write(path, serde_json::to_vec(&case[key])?)?;
        }
        let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
        let repo = ProfileRepo::new(root, case["password"].as_str().map(str::to_owned));
        let actual = if case["coherent"] == true {
            repo.coherent("fixture00001", 1700000000.0)
                .map(serde_json::to_value)
        } else if case["operation"] == "resolve" {
            repo.resolve("fixture00001", 1700000000.0)
                .map(serde_json::to_value)
        } else {
            repo.list(1700000000.0).map(serde_json::to_value)
        };
        for (position, path) in paths.iter().enumerate() {
            let after = fs::read(path)?;
            assert_eq!(
                Value::Bool(before[position] == after),
                case["unchanged"][position],
                "Ordinary encrypted read rewrote protected bytes: {}",
                case["name"]
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&after)?,
                case["written"][position],
                "{}",
                case["name"]
            );
        }
        match actual {
            Ok(value) => {
                assert!(case["error"].is_null(), "{}", case["name"]);
                assert_eq!(value?, case["result"], "{}", case["name"]);
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "{}",
                case["name"]
            ),
        }
    }
    temporary.cleanup()?;
    Ok(())
}
