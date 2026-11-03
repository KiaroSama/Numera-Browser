use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;

#[test]
fn withheld_text_admission_matches_original_without_replacing_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::RepoError};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("withheld-text")?;
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
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/profile-export.json').read_text(encoding='utf-8'))['cases']
inputs=[(raw,{},None) for raw in (None,b'\xff',b'{"proxy":{"password":"\xff"}}',
        b'{"proxy":{"password":"\xe2\x82',b'{broken',b'{}',b'{"proxy":null}',
        b'{"proxy":{"password":null}}',b'{"proxy":{"password":""}}',
        b'{"proxy":{"password":"plain"}}',b'{"proxy":{"password":"enc:v9:opaque"}}')]
inputs.extend((json.dumps(fixture[2]['profile']).encode('utf-8'),fixture[2]['config'],password)
              for password in (None,'fixture-export-master','wrong-fixture-password'))
rows=[]
with run_log('withheld-text-oracle',parent/'logs') as log:
    for raw,config,password in inputs:
        root=parent/str(len(rows)); store=ProfileStore(root,password=password)
        config_raw=json.dumps(config).encode('utf-8'); (root/'config.json').write_bytes(config_raw)
        path=root/'profiles/fixture-export.json'
        if raw is not None:path.write_bytes(raw)
        try:result,error=store.secret_withheld('fixture-export'),None
        except UnicodeDecodeError:result,error=None,'UnicodeDecodeError'
        assert (path.read_bytes() if path.exists() else None)==raw
        assert (root/'config.json').read_bytes()==config_raw
        rows.append({'raw':None if raw is None else list(raw),'config':list(config_raw),
                     'password':password,'result':result,'error':error})
    log.info('Observed withheld text cases=%s',len(rows))
    print(json.dumps(rows))
"#]).arg(repository).arg(temporary.path().join("source"));
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original withheld text observer failed");
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(rows.len(), 14);
    for (index, row) in rows.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let config: Vec<u8> = serde_json::from_value(row["config"].clone())?;
        fs::write(root.join("config.json"), &config)?;
        let path = root.join("profiles/fixture-export.json");
        let raw: Option<Vec<u8>> = serde_json::from_value(row["raw"].clone())?;
        if let Some(bytes) = &raw {
            fs::write(&path, bytes)?;
        }
        let repo = ProfileRepo::new(root.clone(), row["password"].as_str().map(str::to_owned));
        let result = repo.secret_withheld("fixture-export");
        if row["error"] == "UnicodeDecodeError" {
            assert!(
                matches!(result, Err(RepoError::Io(ref error))
                if error.kind() == std::io::ErrorKind::InvalidData),
                "Withheld text admission differs: {index}"
            );
        } else {
            assert_eq!(
                serde_json::json!(result?),
                row["result"],
                "Withheld credential result differs: {index}"
            );
        }
        assert_eq!(
            fs::read(&path).ok(),
            raw,
            "Withheld check replaced stored bytes"
        );
        assert_eq!(fs::read(root.join("config.json"))?, config);
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn exported_profile_bytes_match_original_text_format_and_preserve_refusals()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("profile-export-bytes")?;
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
from numera.store import ProfileStore
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/profile-export.json').read_text(encoding='utf-8'))['cases']
inputs=copy.deepcopy(fixture)
for field,value in [('name','مهاجرت α 😀'),('notes','line\r\nnext\t\x00\x7f'),
                    ('tags',['日本語','α']),('proxy',None),
                    ('bookmarks',[{'name':'😀','url':'https://example.test/α','future':[True,None]}])]:
    case=copy.deepcopy(fixture[0]); case['profile'][field]=value; inputs.append(case)
case=copy.deepcopy(fixture[2]); case['password']='wrong-fixture-password'; inputs.append(case)
for created,updated in [(1700000000,1700000001),(0,-1),(9007199254740993,9007199254740995)]:
    case=copy.deepcopy(fixture[0]); case['profile'].update(created_at=created,updated_at=updated); inputs.append(case)
rows=[]
with run_log('profile-export-bytes-oracle',parent/'logs') as log:
    for case in inputs:
        root=parent/str(len(rows)); store=ProfileStore(root,password=case['password'])
        files={'config.json':json.dumps(case['config']).encode('utf-8')}
        if not case['missing']:files['profiles/fixture-export.json']=json.dumps(case['profile']).encode('utf-8')
        for name,raw in files.items():(root/name).write_bytes(raw)
        dest=root/'export.json'; dest.write_bytes(b'preserve destination\x00\xff')
        try:
            result=store.export('fixture-export',dest)
            assert result is None or result==dest
            assert store.export('fixture-export',dest)==result
            result,error=result is not None,None
        except RuntimeError as exc:result,error=None,str(exc)
        assert all((root/name).read_bytes()==raw for name,raw in files.items())
        rows.append({'files':{name:list(raw) for name,raw in files.items()},'password':case['password'],
                     'result':result,'error':error,'bytes':list(dest.read_bytes())})
    log.info('Observed exact profile export cases=%s',len(rows))
    print(json.dumps(rows))
"#]).arg(repository).arg(temporary.path().join("source"));
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 524288,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        observed.exit, 0,
        "Original profile export byte observer failed"
    );
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(rows.len(), 13);
    for (index, row) in rows.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let mut before = Vec::new();
        for (name, raw) in row["files"].as_object().ok_or("Export inputs missing")? {
            let path = root.join(name);
            let bytes: Vec<u8> = serde_json::from_value(raw.clone())?;
            fs::write(&path, &bytes)?;
            before.push((path, bytes));
        }
        let dest = root.join("export.json");
        fs::write(&dest, b"preserve destination\x00\xff")?;
        let repo = ProfileRepo::new(root, row["password"].as_str().map(str::to_owned));
        match repo.export("fixture-export", &dest) {
            Ok(result) => {
                assert!(row["error"].is_null(), "Export refusal missing: {index}");
                assert_eq!(serde_json::json!(result.is_some()), row["result"]);
                assert_eq!(result, (row["result"] == true).then(|| dest.clone()));
                assert_eq!(repo.export("fixture-export", &dest)?, result);
            }
            Err(error) => assert_eq!(Some(error.to_string().as_str()), row["error"].as_str()),
        }
        let expected: Vec<u8> = serde_json::from_value(row["bytes"].clone())?;
        assert_eq!(
            fs::read(dest)?,
            expected,
            "Profile export bytes differ: {index}"
        );
        for (path, bytes) in before {
            assert_eq!(fs::read(path)?, bytes);
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn profile_export_refuses_to_drop_withheld_credentials() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/profile-export.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("export-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            if case["missing"] != true {
                fs::write(
                    root.join("profiles/fixture-export.json"),
                    serde_json::to_vec(&case["profile"])?,
                )?;
            }
            let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
            let dest = root.join("export.json");
            match repo.export("fixture-export", &dest) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value.is_some()), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            assert_eq!(
                dest.exists(),
                case["destination_present"]
                    .as_bool()
                    .ok_or("Destination verdict missing")?
            );
            if dest.exists() {
                assert_eq!(
                    serde_json::from_slice::<Value>(&fs::read(dest)?)?,
                    case["written"]
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
