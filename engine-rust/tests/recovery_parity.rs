use numera_core::vault::{decrypt, recovery::recover_enable};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;

#[test]
fn unreadable_active_config_preserves_original_recovery_causes_and_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("recovery-config-causes")?;
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
from numera.vault_recovery import recover
from run_logging import run_log
fixture=json.loads((repository/'engine-rust/tests/fixtures/recovery.json').read_text(encoding='utf-8'))['cases'][0]
observed=[]
with run_log('recovery-config-causes-oracle',parent/'logs') as log:
    for index,raw in enumerate([b'{',b'\xff',b'{"vault":',b'"\xc3"',None]):
        root=parent/str(index);root.mkdir(parents=True)
        files={'vault-migration.json':json.dumps(fixture['journal']).encode('utf-8'),
               'proxies.json':b'[]'}
        if raw is None:(root/'config.json').mkdir()
        else:files['config.json']=raw
        for name,data in files.items():(root/name).write_bytes(data)
        attempts=[]
        for _ in range(2):
            try:
                result=recover(root,lambda:{},lambda _:None)
                error=None;codes=[]
            except RuntimeError as exc: result,error,codes=None,str(exc),sorted(getattr(exc,'codes',set()))
            assert all((root/name).read_bytes()==data for name,data in files.items())
            if raw is None:assert (root/'config.json').is_dir() and not list((root/'config.json').iterdir())
            attempts.append({'result':result,'error':error,'codes':codes})
        observed.append({'files':{name:list(data) for name,data in files.items()},
                         'config_directory':raw is None,'attempts':attempts})
    log.info('Observed unreadable config cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 32768,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(original.exit, 0, "Original recovery config observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 5);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(&root)?;
        if case["config_directory"] == true {
            fs::create_dir(root.join("config.json"))?;
        }
        let mut before = Vec::new();
        for (name, data) in case["files"].as_object().ok_or("Files missing")? {
            let bytes: Vec<u8> = serde_json::from_value(data.clone())?;
            fs::write(root.join(name), &bytes)?;
            before.push((name, bytes));
        }
        for expected in case["attempts"].as_array().ok_or("Attempts missing")? {
            let actual = match recover_enable(&root, None) {
                Ok(result) => serde_json::json!({"result":result,"error":null,"codes":[]}),
                Err(error) => {
                    serde_json::json!({"result":null,"error":error.to_string(),"codes":error.codes})
                }
            };
            assert_eq!(
                actual, *expected,
                "Active config recovery causes differ for case {index}"
            );
            for (name, bytes) in &before {
                assert_eq!(
                    fs::read(root.join(name))?,
                    *bytes,
                    "Recovery changed unreadable evidence"
                );
            }
            if case["config_directory"] == true {
                let path = root.join("config.json");
                assert!(path.is_dir() && fs::read_dir(path)?.next().is_none());
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn interrupted_enable_is_authenticated_or_refused_without_losing_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/recovery.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing recovery cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!(
                "recovery-{}-{index}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let files = case["files"].as_object().ok_or("Missing files")?;
            for (name, data) in files {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(path, serde_json::to_vec(data)?)?;
            }
            fs::write(root.join("config.json"), b"{}")?;
            fs::write(
                root.join("vault-migration.json"),
                serde_json::to_vec(&case["journal"])?,
            )?;
            let before = case["unchanged"]
                .as_object()
                .ok_or("Missing side effects")?
                .keys()
                .map(|name| Ok((name.clone(), fs::read(root.join(name))?)))
                .collect::<Result<Vec<_>, std::io::Error>>()?;
            match recover_enable(&root, case["password"].as_str()) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value), case["result"]);
                }
                Err(error) => {
                    assert_eq!(Some(error.to_string().as_str()), case["error"].as_str());
                    assert_eq!(serde_json::to_value(error.codes)?, case["codes"]);
                }
            }
            assert_eq!(
                root.join("vault-migration.json").exists(),
                case["journal_retained"]
                    .as_bool()
                    .ok_or("Missing journal observation")?
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(root.join("config.json"))?)?,
                case["config"]
            );
            for (name, bytes) in before {
                assert_eq!(
                    fs::read(root.join(&name)).ok().as_ref() == Some(&bytes),
                    case["unchanged"][&name]
                        .as_bool()
                        .ok_or("Missing byte observation")?
                );
            }
            for name in files.keys() {
                let mut data: Value = serde_json::from_slice(&fs::read(root.join(name))?)?;
                if let Some(rows) = data.as_array_mut() {
                    for row in rows {
                        if let Some(token) = row
                            .get("pass")
                            .and_then(Value::as_str)
                            .filter(|t| t.starts_with("enc:v2:"))
                        {
                            row["pass"] = decrypt(
                                "fixture-recovery-master",
                                case["descriptor"]["salt"].as_str().ok_or("Salt missing")?,
                                token,
                            )?
                            .into();
                        }
                    }
                }
                assert_eq!(data, case["observed"][name]);
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
