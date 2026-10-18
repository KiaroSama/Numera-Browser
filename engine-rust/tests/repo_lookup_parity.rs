use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};
mod common;

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
