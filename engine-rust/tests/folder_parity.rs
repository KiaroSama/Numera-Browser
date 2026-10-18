use numera_core::storage::folders::Folders;
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

#[test]
fn malformed_folder_text_refuses_without_replacing_source_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("folder-text")?;
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
from numera import folders
from run_logging import run_log

def parsed(raw):
    try:
        return json.loads(raw.decode('utf-8'))
    except (UnicodeDecodeError,json.JSONDecodeError):
        return None

with run_log('folder-text-oracle',root/'logs') as log:
    initial={'dashboard-config.json':{'folders':['source','alpha'],'future':'α'},
             'dashboard/a.json':{'id':'a','folder':'source','future':[7]},
             'dashboard/b.json':{'id':'b','folder':'alpha'}}
    rows=[]
    for location in ['dashboard-config.json','dashboard/b.json']:
        valid=dict(initial[location],future='日本語')
        for payload in [b'\xff',b'{"future":"\xc3("}',b'{"future":"\xe2\x82',
                        b'{broken',json.dumps(valid,ensure_ascii=False).encode('utf-8')]:
            for operation,args in [('listing',[]),('create',['new']),
                                   ('rename',['source','target']),('delete',['source'])]:
                case=root/'source'/str(len(rows))
                (case/'dashboard').mkdir(parents=True)
                inputs={name:json.dumps(value).encode('utf-8') for name,value in initial.items()}
                inputs[location]=payload
                for name,raw in inputs.items():
                    (case/name).write_bytes(raw)
                try:
                    result,error=getattr(folders,operation)(case,*args),None
                except (UnicodeDecodeError,ValueError) as exc:
                    result,error=None,type(exc).__name__
                after={name:(case/name).read_bytes() for name in inputs}
                rows.append({'operation':operation,'args':args,'location':location,
                             'inputs':{name:raw.hex() for name,raw in inputs.items()},
                             'result':result,'error':error,
                             'written':{name:parsed(raw) for name,raw in after.items()},
                             'unchanged':{name:raw==after[name] for name,raw in inputs.items()}})
    log.info('Observed folder text cases=%s',len(rows))
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
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original folder text observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 40, "Folder text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(root.join("dashboard"))?;
        let mut before = Vec::new();
        for (name, raw) in case["inputs"].as_object().ok_or("Inputs missing")? {
            let raw = raw.as_str().ok_or("Input bytes missing")?;
            let bytes = (0..raw.len())
                .step_by(2)
                .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                .collect::<Result<Vec<_>, _>>()?;
            fs::write(root.join(name), &bytes)?;
            before.push((name, bytes));
        }
        let folders = Folders::new(root.clone());
        let outcome = match case["operation"].as_str() {
            Some("listing") => folders.listing(),
            Some("create") => folders.create("new"),
            Some("rename") => folders.rename("source", "target"),
            Some("delete") => folders.delete("source", None),
            _ => return Err("Unknown folder operation".into()),
        };
        assert_eq!(
            outcome.is_ok(),
            case["error"].is_null(),
            "Folder text admission differs: case={index} operation={} location={}",
            case["operation"],
            case["location"]
        );
        match outcome {
            Ok(names) => assert_eq!(serde_json::to_value(names)?, case["result"]),
            Err(error) => {
                assert_eq!(case["error"], "UnicodeDecodeError");
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("UTF-8"));
            }
        }
        for (name, original) in before {
            let after = fs::read(root.join(name))?;
            assert_eq!(
                Value::Bool(after == original),
                case["unchanged"][name],
                "Folder text byte preservation differs: case={index} file={name}"
            );
            if !case["error"].is_null() {
                assert_eq!(after, original, "Refused folder text changed bytes");
            }
            let parsed = serde_json::from_slice::<Value>(&after).unwrap_or(Value::Null);
            assert_eq!(
                parsed, case["written"][name],
                "Folder text state differs: case={index} file={name}"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn folder_name_whitespace_matches_original_operations() -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("folder-whitespace")?;
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
from numera import folders
from run_logging import run_log
with run_log('folder-whitespace-oracle',root/'logs') as log:
    meta={'folders':['source','alpha'],'renamed':{'before':'source'},'future':{'kept':'α'}}
    profiles={'a.json':{'id':'a','folder':'source','future':[7]},
              'b.json':{'id':'b','folder':'alpha'}}
    rows=[]
    for marker in map(chr,[0x1c,0x1d,0x1e,0x1f,0x85,0xa0,
                           0x2003,0x202f,0x3000,0x1b,0x200b,0xfeff]):
        variants=[('create',[marker+'new'+marker]),('create',[marker]),
                  ('create',['inner'+marker+'name']),
                  ('rename',[marker+'source'+marker,'target']),
                  ('rename',['source',marker+'target'+marker]),
                  ('delete',[marker+'source'+marker]),
                  ('delete',['source',marker+'target'+marker]),
                  ('delete',['source',marker])]
        for operation,args in variants:
            case=root/'source'/str(len(rows))
            (case/'dashboard').mkdir(parents=True)
            (case/'dashboard-config.json').write_text(json.dumps(meta),encoding='utf-8')
            for name,data in profiles.items():
                (case/'dashboard'/name).write_text(json.dumps(data),encoding='utf-8')
            paths={'dashboard-config.json':case/'dashboard-config.json',
                   **{name:case/'dashboard'/name for name in profiles}}
            before={name:path.read_bytes() for name,path in paths.items()}
            try:
                result,error=getattr(folders,operation)(case,*args),None
            except ValueError as exc:
                result,error=None,str(exc)
            rows.append({'operation':operation,'args':args,'meta':meta,'profiles':profiles,
                         'result':result,'error':error,
                         'written':{name:json.loads(path.read_text(encoding='utf-8'))
                                    for name,path in paths.items()},
                         'unchanged':{name:before[name]==path.read_bytes()
                                      for name,path in paths.items()}})
    log.info('Observed folder whitespace cases=%s',len(rows))
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
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original folder whitespace observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 96, "Folder whitespace observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(root.join("dashboard"))?;
        let mut paths = vec![("dashboard-config.json", root.join("dashboard-config.json"))];
        fs::write(&paths[0].1, serde_json::to_vec(&case["meta"])?)?;
        for (name, value) in case["profiles"].as_object().ok_or("Profiles missing")? {
            let path = root.join("dashboard").join(name);
            fs::write(&path, serde_json::to_vec(value)?)?;
            paths.push((name.as_str(), path));
        }
        let before = paths
            .iter()
            .map(|(_, path)| fs::read(path))
            .collect::<Result<Vec<_>, _>>()?;
        let folders = Folders::new(root);
        let args = case["args"].as_array().ok_or("Folder arguments missing")?;
        let first = args[0].as_str().ok_or("Folder name missing")?;
        let outcome = match case["operation"].as_str() {
            Some("create") => folders.create(first),
            Some("rename") => folders.rename(first, args[1].as_str().ok_or("Target missing")?),
            Some("delete") => folders.delete(first, args.get(1).and_then(Value::as_str)),
            _ => return Err("Unknown folder operation".into()),
        };
        let (result, error) = match outcome {
            Ok(names) => (serde_json::to_value(names)?, Value::Null),
            Err(error) => (Value::Null, Value::String(error.to_string())),
        };
        assert_eq!(
            (result, error),
            (case["result"].clone(), case["error"].clone()),
            "Folder name normalization differs: {} {}",
            case["operation"],
            case["args"]
        );
        for ((name, path), before) in paths.iter().zip(before) {
            let after = fs::read(path)?;
            assert_eq!(
                serde_json::from_slice::<Value>(&after)?,
                case["written"][*name],
                "Folder state differs: case={index} file={name}"
            );
            assert_eq!(
                Value::Bool(before == after),
                case["unchanged"][*name],
                "Folder byte-preservation verdict differs: case={index} file={name}"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn folder_transactions_keep_profiles_and_follow_retired_names()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/folders.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing folder cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("folders-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("dashboard"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("dashboard-config.json"),
                serde_json::to_vec(&case["meta"])?,
            )?;
            let profiles = case["profiles"].as_object().ok_or("Missing profiles")?;
            let mut before = Vec::new();
            for (name, data) in profiles {
                let path = root.join("dashboard").join(name);
                let bytes = serde_json::to_vec(data)?;
                fs::write(&path, &bytes)?;
                before.push((name, path, bytes));
            }
            let folders = Folders::new(root.clone());
            assert_eq!(serde_json::to_value(folders.listing()?)?, case["listed"]);
            let args = case["args"].as_array().ok_or("Missing arguments")?;
            let first = args[0].as_str().ok_or("Missing name")?;
            let changed = match case["operation"].as_str() {
                Some("create") => folders.create(first),
                Some("rename") => folders.rename(first, args[1].as_str().ok_or("Missing target")?),
                Some("delete") => folders.delete(first, args.get(1).and_then(Value::as_str)),
                _ => return Err("Unknown folder operation".into()),
            };
            match changed {
                Ok(names) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(names)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (name, path, bytes) in before {
                let after = fs::read(path)?;
                assert_eq!(
                    after == bytes,
                    case["unchanged"][name]
                        .as_bool()
                        .ok_or("Missing byte verdict")?
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][name]
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(root.join("dashboard-config.json"))?)?,
                case["metadata"]
            );
            for (name, expected) in case["resolved"]
                .as_object()
                .ok_or("Missing resolved names")?
            {
                assert_eq!(
                    folders.resolve_name(name)?,
                    expected.as_str().ok_or("Invalid resolved name")?
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
