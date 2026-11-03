use numera_core::vault::{decrypt, recovery::recover};
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

#[test]
fn public_resume_returns_the_authenticated_store_or_preserves_recovery_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::ProfileRepo};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("vault-store-resume")?;
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
from numera.vault import decrypt_for_read
from run_logging import run_log
cases=json.loads((repository/'engine-rust/tests/fixtures/rotation-recovery.json').read_text(encoding='utf-8'))['cases']
observed=[]
with run_log('vault-store-resume-oracle',parent/'logs') as log:
    for index,case in enumerate(cases):
        root=parent/str(index);root.mkdir(parents=True)
        files={'proxies.json':copy.deepcopy(case['pool']),'config.json':case['config'],
               'vault-migration.json':case['journal']}
        for name,value in files.items():(root/name).write_text(json.dumps(value),encoding='utf-8')
        session=root/'user-data'/'kept'/'session.bin';session.parent.mkdir(parents=True)
        session.write_bytes(b'\x00\xff\r\n')
        before={name:(root/name).read_bytes() for name in files}
        old=case['old_password'] or '';new=case['password']
        try:
            store=ProfileStore.resume_vault(root,old,new)
            result={'enabled':store.vault_enabled(),'unlocked':store.unlocked(),
                    'values':[decrypt_for_read(store,row.get('pass')) for row in
                              json.loads((root/'proxies.json').read_text(encoding='utf-8'))]}
            error=None
        except (RuntimeError,ValueError) as exc: result,error=None,str(exc)
        after={name:(root/name).read_bytes() if (root/name).exists() else None for name in files}
        if error is not None:assert after==before,'Original refused resume changed bytes'
        assert session.read_bytes()==b'\x00\xff\r\n'
        observed.append({'files':files,'old':old,'new':new,'result':result,'error':error,
                         'config':json.loads(after['config.json']),
                         'journal':after['vault-migration.json'] is not None,
                         'unchanged':{name:raw==after[name] for name,raw in before.items()}})
    log.info('Observed public vault resume cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
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
        "Original public vault resume observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 10);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-resume-{index}"));
        fs::create_dir_all(&root)?;
        let files = case["files"].as_object().ok_or("Recovery inputs missing")?;
        for (name, value) in files {
            fs::write(root.join(name), serde_json::to_vec(value)?)?;
        }
        let before = files
            .keys()
            .map(|name| Ok((name.clone(), fs::read(root.join(name))?)))
            .collect::<Result<Vec<_>, std::io::Error>>()?;
        let session = root.join("user-data/kept/session.bin");
        fs::create_dir_all(session.parent().ok_or("Session parent missing")?)?;
        fs::write(&session, b"\0\xff\r\n")?;
        let (result, error) = match ProfileRepo::resume_vault(
            root.clone(),
            case["old"].as_str().ok_or("Old password missing")?,
            case["new"].as_str().ok_or("New password missing")?,
        ) {
            Ok(repo) => {
                let descriptor = repo.vault()?.ok_or("Resumed descriptor missing")?;
                let password = case["new"].as_str().ok_or("New password missing")?;
                let salt = descriptor["salt"].as_str().ok_or("Salt missing")?;
                let pool: Value = serde_json::from_slice(&fs::read(root.join("proxies.json"))?)?;
                let values = pool
                    .as_array()
                    .ok_or("Resumed pool missing")?
                    .iter()
                    .map(|row| {
                        let value = &row["pass"];
                        value
                            .as_str()
                            .filter(|secret| numera_core::vault::is_envelope(secret))
                            .map_or_else(
                                || Ok(value.clone()),
                                |secret| {
                                    decrypt(password, salt, secret)
                                        .map(Value::String)
                                        .map_err(|error| error.to_string())
                                },
                            )
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                (
                    serde_json::json!({"enabled":repo.vault_enabled()?,"unlocked":repo.unlocked()?,
                                   "values":values}),
                    None,
                )
            }
            Err(error) => (Value::Null, Some(error.to_string())),
        };
        assert_eq!(
            result, case["result"],
            "Resumed public store differs for case {index}"
        );
        assert_eq!(
            serde_json::to_value(error)?,
            case["error"],
            "Public resume refusal differs"
        );
        for (name, bytes) in before {
            assert_eq!(
                fs::read(root.join(&name)).ok().as_ref() == Some(&bytes),
                case["unchanged"][&name]
                    .as_bool()
                    .ok_or("Byte verdict missing")?
            );
        }
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(root.join("config.json"))?)?,
            case["config"]
        );
        assert_eq!(
            root.join("vault-migration.json").exists(),
            case["journal"].as_bool().ok_or("Journal verdict missing")?
        );
        assert_eq!(fs::read(session)?, b"\0\xff\r\n");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn committed_rotation_metadata_matches_original_without_rewriting_generation()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("rotation-metadata")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import copy,json,sys
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.vault_recovery import recover
from numera.paths import write_json
from run_logging import run_log
cases=json.loads((repository/'engine-rust/tests/fixtures/rotation-recovery.json').read_text(encoding='utf-8'))['cases']
base=cases[2];inputs=[]
for metadata in [{'future':{'format':3}}, {}, {'extra':None}, {'label':'α فارسی'}]:
    case=copy.deepcopy(base);case['config']['vault'].update(metadata);inputs.append(case)
for change in ['verifier','missing-verifier','salt','stale','no-key','wrong-key']:
    case=copy.deepcopy(base)
    if change=='verifier':case['config']['vault']['verifier']='enc:v9:fixture'
    elif change=='missing-verifier':case['config']['vault'].pop('verifier')
    elif change=='salt':case['config']['vault']['salt']=cases[0]['journal']['old']['salt']
    elif change=='stale':case['pool']=cases[1]['pool']
    elif change=='no-key':case['password']=None
    elif change=='wrong-key':case['password']='fixture-wrong-master'
    inputs.append(case)
observed=[]
with run_log('rotation-metadata-oracle',parent/'logs') as log:
    for index,case in enumerate(inputs):
        root=parent/str(index);root.mkdir(parents=True)
        token=case['pool'][0]['pass']
        files={'config.json':case['config'],'vault-migration.json':case['journal'],
               'proxies.json':case['pool'],'profiles/a.json':{'proxy':{'password':token}},
               'dashboard/a.json':{'proxy':{'pass':token}}}
        before={}
        for name,value in files.items():
            path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
            raw=json.dumps(value).encode('utf-8');path.write_bytes(raw);before[name]=raw
        marker=root/'user-data/kept/session.bin';marker.parent.mkdir(parents=True)
        marker.write_bytes(b'\x00\xff\r\n')
        attempts=[]
        for _ in range(2):
            try:
                result=recover(root,lambda:json.loads((root/'config.json').read_text(encoding='utf-8')),
                               lambda value:write_json(root/'config.json',value),password=case['password'])
                error=None;codes=[];role=None
            except RuntimeError as exc:
                result,error,codes,role=None,str(exc),sorted(getattr(exc,'codes',set())),getattr(exc,'role',None)
            unchanged={name:(root/name).exists() and (root/name).read_bytes()==raw for name,raw in before.items()}
            assert all(kept for name,kept in unchanged.items() if name!='vault-migration.json')
            assert marker.read_bytes()==b'\x00\xff\r\n'
            attempts.append({'result':result,'error':error,'codes':codes,'role':role,
                             'unchanged':unchanged,'journal':(root/'vault-migration.json').exists()})
        observed.append({'files':files,'password':case['password'],'attempts':attempts})
    log.info('Observed committed rotation metadata cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
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
        "Original rotation metadata observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 10);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(&root)?;
        let mut before = Vec::new();
        for (name, value) in case["files"].as_object().ok_or("Files missing")? {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
            let bytes = serde_json::to_vec(value)?;
            fs::write(path, &bytes)?;
            before.push((name, bytes));
        }
        let marker = root.join("user-data/kept/session.bin");
        fs::create_dir_all(marker.parent().ok_or("Marker parent missing")?)?;
        fs::write(&marker, b"\0\xff\r\n")?;
        for expected in case["attempts"].as_array().ok_or("Attempts missing")? {
            let mut actual = match recover(&root, case["password"].as_str(), None) {
                Ok(result) => {
                    serde_json::json!({"result":result,"error":null,"codes":[],"role":null})
                }
                Err(error) => serde_json::json!({"result":null,"error":error.to_string(),
                    "codes":error.codes,"role":error.role}),
            };
            let unchanged: serde_json::Map<String, Value> = before
                .iter()
                .map(|(name, bytes)| {
                    (
                        (*name).clone(),
                        Value::Bool(fs::read(root.join(name)).ok().as_ref() == Some(bytes)),
                    )
                })
                .collect();
            actual["unchanged"] = unchanged.into();
            actual["journal"] = root.join("vault-migration.json").exists().into();
            assert_eq!(
                actual, *expected,
                "Committed rotation metadata differs for case {index}"
            );
            assert_eq!(fs::read(&marker)?, b"\0\xff\r\n");
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn interrupted_rotation_authenticates_both_keys_or_retains_its_journal()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/rotation-recovery.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing rotation recovery cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("rotation-recovery-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (name, key) in [
                ("proxies.json", "pool"),
                ("config.json", "config"),
                ("vault-migration.json", "journal"),
            ] {
                fs::write(root.join(name), serde_json::to_vec(&case[key])?)?;
            }
            let before = case["unchanged"]
                .as_object()
                .ok_or("Missing bytes")?
                .keys()
                .map(|name| Ok((name.clone(), fs::read(root.join(name))?)))
                .collect::<Result<Vec<_>, std::io::Error>>()?;
            match recover(
                &root,
                case["password"].as_str(),
                case["old_password"].as_str(),
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value), case["result"]);
                }
                Err(error) => {
                    assert_eq!(Some(error.to_string().as_str()), case["error"].as_str());
                    assert_eq!(serde_json::to_value(error.codes)?, case["codes"]);
                    assert_eq!(serde_json::to_value(error.role)?, case["role"]);
                }
            }
            assert_eq!(
                root.join("vault-migration.json").exists(),
                case["journal_retained"]
                    .as_bool()
                    .ok_or("Journal verdict missing")?
            );
            for (name, bytes) in before {
                assert_eq!(
                    fs::read(root.join(&name)).ok().as_ref() == Some(&bytes),
                    case["unchanged"][name]
                        .as_bool()
                        .ok_or("Byte verdict missing")?
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(root.join("config.json"))?)?,
                case["persisted"]
            );
            let mut pool: Value = serde_json::from_slice(&fs::read(root.join("proxies.json"))?)?;
            for row in pool.as_array_mut().ok_or("Pool missing")? {
                if let Some(token) = row
                    .get("pass")
                    .and_then(Value::as_str)
                    .filter(|s| s.starts_with("enc:v2:"))
                {
                    let value = decrypt("fixture-new-master", "EBESExQVFhcYGRobHB0eHw==", token)
                        .or_else(|_| {
                            decrypt("fixture-old-master", "AAECAwQFBgcICQoLDA0ODw==", token)
                        })?;
                    row["pass"] = value.into();
                }
            }
            assert_eq!(pool, case["observed"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
