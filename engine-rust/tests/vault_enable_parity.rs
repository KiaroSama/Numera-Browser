use numera_core::vault::{check_password, completion::problems, decrypt, transaction::enable};
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

#[test]
fn public_vault_status_matches_original_without_mutating_settings()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::ProfileRepo};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("vault-store-status")?;
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
fixture=json.loads((repository/'engine-rust/tests/fixtures/repo.json').read_text(encoding='utf-8'))['cases'][11]
configs=[{},*({'vault':value,'future':7} for value in
    [None,{},False,0,'',[],fixture['config']['vault'],{'salt':'invalid','verifier':'enc:v2:broken'}])]
observed=[]
with run_log('vault-store-status-oracle',parent/'logs') as log:
    for config in configs:
        for password in [None,'fixture-master','wrong-fixture-password']:
            root=parent/str(len(observed));store=ProfileStore(root,password=password)
            path=root/'config.json';raw=json.dumps(config).encode('utf-8');path.write_bytes(raw)
            result={'descriptor':store.vault(),'enabled':store.vault_enabled(),'unlocked':store.unlocked()}
            assert path.read_bytes()==raw,'Original status changed settings'
            observed.append({'config':config,'password':password,'result':result})
    recovery=json.loads((repository/'engine-rust/tests/fixtures/rotation-recovery.json').read_text(encoding='utf-8'))['cases'][0]
    guards=[]
    for index,journal in enumerate(['{',json.dumps({'version':99}),json.dumps(recovery['journal'])]):
        root=parent/('guard-'+str(index));store=ProfileStore(root,password='fixture-master')
        config=json.dumps(fixture['config']).encode('utf-8')
        (root/'config.json').write_bytes(config)
        raw=journal.encode('utf-8');(root/'vault-migration.json').write_bytes(raw)
        errors=[]
        for operation in [store.vault,store.vault_enabled,store.unlocked,
                          lambda:store.enable_vault('replacement'),
                          lambda:store.change_vault_password('fixture-master','replacement')]:
            try:operation()
            except RuntimeError as exc:errors.append(str(exc))
            else:raise AssertionError('Original admitted unresolved migration')
        assert (root/'config.json').read_bytes()==config
        assert (root/'vault-migration.json').read_bytes()==raw
        guards.append({'journal':journal,'config':fixture['config'],'errors':errors})
    log.info('Observed public vault status cases=%s guard_cases=%s',len(observed),len(guards)*5)
    print(json.dumps({'cases':observed,'guards':guards},ensure_ascii=True))
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
        "Original public vault status observer failed"
    );
    let observed: Value = serde_json::from_slice(&original.stdout)?;
    let cases = observed["cases"].as_array().ok_or("Status cases missing")?;
    assert_eq!(cases.len(), 27);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(&root)?;
        let path = root.join("config.json");
        let raw = serde_json::to_vec(&case["config"])?;
        fs::write(&path, &raw)?;
        let repo = ProfileRepo::new(root, case["password"].as_str().map(str::to_owned));
        assert_eq!(
            serde_json::json!({"descriptor":repo.vault()?,
            "enabled":repo.vault_enabled()?,"unlocked":repo.unlocked()?}),
            case["result"],
            "Public vault status differs for case {index}"
        );
        assert_eq!(fs::read(path)?, raw, "Public vault status changed settings");
    }
    let guards = observed["guards"].as_array().ok_or("Guard cases missing")?;
    assert_eq!(guards.len(), 3);
    for (index, case) in guards.iter().enumerate() {
        let root = temporary.path().join(format!("native-guard-{index}"));
        fs::create_dir_all(&root)?;
        let config = serde_json::to_vec(&case["config"])?;
        let journal = case["journal"].as_str().ok_or("Journal text missing")?;
        fs::write(root.join("config.json"), &config)?;
        fs::write(root.join("vault-migration.json"), journal)?;
        let mut repo = ProfileRepo::new(root.clone(), Some("fixture-master".into()));
        let errors = [
            repo.vault().err(),
            repo.vault_enabled().err(),
            repo.unlocked().err(),
            repo.enable_vault("replacement").err(),
            repo.change_vault_password("fixture-master", "replacement")
                .err(),
        ];
        let errors = errors
            .into_iter()
            .map(|error| {
                error
                    .ok_or("Unresolved migration was admitted")
                    .map(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(serde_json::to_value(errors)?, case["errors"]);
        assert_eq!(fs::read(root.join("config.json"))?, config);
        assert_eq!(
            fs::read(root.join("vault-migration.json"))?,
            journal.as_bytes()
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn public_enable_preserves_instance_state_and_every_secret_site()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::ProfileRepo};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("vault-store-enable")?;
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
fixtures=repository/'engine-rust/tests/fixtures'
cases=json.loads((fixtures/'vault-enable.json').read_text(encoding='utf-8'))['cases']
profile=json.loads((fixtures/'repo.json').read_text(encoding='utf-8'))['cases'][0]['engine']
observed=[]
with run_log('vault-store-enable-oracle',parent/'logs') as log:
    for index,case in enumerate(cases):
        root=parent/str(index);prior='fixture-enable-master' if index==4 else None
        store=ProfileStore(root,password=prior)
        files=copy.deepcopy(case['files'])
        if 'profiles/a.json' in files:
            record=copy.deepcopy(profile);record['id']='a';record['future']=7
            record['proxy']['password']=files['profiles/a.json']['proxy']['password']
            files['profiles/a.json']=record
        files['config.json']=case['config']
        for name,value in files.items():
            path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text(json.dumps(value),encoding='utf-8')
        session=root/'user-data'/'kept'/'session.bin';session.parent.mkdir(parents=True)
        session.write_bytes(b'\x00\xff\r\n')
        before={name:(root/name).read_bytes() for name in files}
        password='refused-replacement' if index==4 else case['password']
        try: count,error=store.enable_vault(password),None
        except (RuntimeError,ValueError) as exc: count,error=None,str(exc)
        status={'enabled':store.vault_enabled(),'unlocked':store.unlocked()}
        after={name:(root/name).read_bytes() for name in files}
        disk={name:json.loads(raw) for name,raw in after.items()}
        if count is not None:
            assert status=={'enabled':True,'unlocked':True}
            disk['config.json']['vault']={'authenticated':True}
            for name,data in disk.items():
                if name=='config.json':continue
                rows=data if name=='proxies.json' else [data]
                for row in rows:
                    holder=row if name=='proxies.json' else row.get('proxy')
                    key='password' if name.startswith('profiles/') else 'pass'
                    if isinstance(holder,dict) and holder.get(key):
                        assert holder[key].startswith('enc:v2:'),'Original enable left plaintext'
                        holder[key]=decrypt_for_read(store,holder[key])
        else: assert after==before,'Original refused enable changed bytes'
        found=store.get('a')
        read=None if found is None else found.to_dict()
        assert session.read_bytes()==b'\x00\xff\r\n'
        observed.append({'files':files,'prior':prior,'password':password,'count':count,'error':error,
                         'status':status,'read':read,'disk':disk,
                         'unchanged':{name:raw==after[name] for name,raw in before.items()},
                         'journal':(root/'vault-migration.json').exists()})
    log.info('Observed public vault enable cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path().join("source"));
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
        "Original public vault enable observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 5);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-enable-{index}"));
        let files = case["files"].as_object().ok_or("Enable inputs missing")?;
        for (name, value) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
            fs::write(path, serde_json::to_vec(value)?)?;
        }
        let session = root.join("user-data/kept/session.bin");
        fs::create_dir_all(session.parent().ok_or("Session parent missing")?)?;
        fs::write(&session, b"\0\xff\r\n")?;
        let before = files
            .keys()
            .map(|name| Ok((name.clone(), fs::read(root.join(name))?)))
            .collect::<Result<Vec<_>, std::io::Error>>()?;
        let mut repo = ProfileRepo::new(root.clone(), case["prior"].as_str().map(str::to_owned));
        let result = repo.enable_vault(case["password"].as_str().ok_or("Password missing")?);
        let (count, error) = match result {
            Ok(count) => (Some(count), None),
            Err(error) => (None, Some(error.to_string())),
        };
        assert_eq!(
            serde_json::to_value(count)?,
            case["count"],
            "Public enable count differs"
        );
        assert_eq!(
            serde_json::to_value(error)?,
            case["error"],
            "Public enable refusal differs"
        );
        assert_eq!(
            serde_json::json!({"enabled":repo.vault_enabled()?,"unlocked":repo.unlocked()?}),
            case["status"],
            "Public enable instance state differs"
        );
        assert_eq!(
            serde_json::to_value(repo.get("a")?)?,
            case["read"],
            "Same-instance read differs"
        );
        let mut disk = serde_json::Map::new();
        let descriptor = repo.vault()?.unwrap_or(Value::Null);
        for (name, bytes) in before {
            let after = fs::read(root.join(&name))?;
            assert_eq!(
                after == bytes,
                case["unchanged"][&name]
                    .as_bool()
                    .ok_or("Byte verdict missing")?
            );
            let mut value: Value = serde_json::from_slice(&after)?;
            if count.is_some() {
                if name == "config.json" {
                    assert!(check_password(
                        case["password"].as_str().ok_or("Password missing")?,
                        descriptor["salt"].as_str().ok_or("Salt missing")?,
                        descriptor["verifier"].as_str().ok_or("Verifier missing")?
                    ));
                    value["vault"] = serde_json::json!({"authenticated":true});
                } else {
                    let many = name == "proxies.json";
                    let rows: Vec<&mut Value> = if many {
                        value
                            .as_array_mut()
                            .ok_or("Pool missing")?
                            .iter_mut()
                            .collect()
                    } else {
                        vec![&mut value]
                    };
                    for row in rows {
                        let holder = if many { row } else { &mut row["proxy"] };
                        let key = if name.starts_with("profiles/") {
                            "password"
                        } else {
                            "pass"
                        };
                        if let Some(secret) =
                            holder[key].as_str().filter(|secret| !secret.is_empty())
                        {
                            assert!(
                                secret.starts_with("enc:v2:"),
                                "Public enable left plaintext"
                            );
                            holder[key] = decrypt(
                                case["password"].as_str().ok_or("Password missing")?,
                                descriptor["salt"].as_str().ok_or("Salt missing")?,
                                secret,
                            )?
                            .into();
                        }
                    }
                }
            }
            disk.insert(name, value);
        }
        assert_eq!(
            Value::Object(disk),
            case["disk"],
            "Public enable stored values differ"
        );
        assert_eq!(fs::read(session)?, b"\0\xff\r\n");
        assert_eq!(
            root.join("vault-migration.json").exists(),
            case["journal"].as_bool().ok_or("Journal verdict missing")?
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn ordinary_enable_is_all_or_nothing_and_authenticates_every_secret_site()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/vault-enable.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing vault enable cases")?;
    assert!(!cases.is_empty());
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp");
    for (index, case) in cases.iter().enumerate() {
        let root = parent.join(format!("enable-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            let files = case["files"].as_object().ok_or("Missing files")?;
            for (name, data) in files {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
                fs::write(path, serde_json::to_vec(data)?)?;
            }
            let before = case["unchanged"]
                .as_object()
                .ok_or("Missing byte assertions")?
                .keys()
                .map(|name| Ok((name.clone(), fs::read(root.join(name))?)))
                .collect::<Result<Vec<_>, std::io::Error>>()?;
            let password = case["password"].as_str().ok_or("Password missing")?;
            match enable(&root, password) {
                Ok(count) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Some(count as u64), case["count"].as_u64());
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (name, bytes) in before {
                assert_eq!(
                    fs::read(root.join(&name))? == bytes,
                    case["unchanged"][name]
                        .as_bool()
                        .ok_or("Missing byte verdict")?
                );
            }
            assert_eq!(
                root.join("vault-migration.json").exists(),
                case["journal_retained"]
                    .as_bool()
                    .ok_or("Missing journal verdict")?
            );
            let mut config: Value = serde_json::from_slice(&fs::read(root.join("config.json"))?)?;
            let descriptor = config["vault"].clone();
            if !descriptor.is_null() {
                assert!(check_password(
                    password,
                    descriptor["salt"].as_str().ok_or("Salt missing")?,
                    descriptor["verifier"].as_str().ok_or("Verifier missing")?
                ));
                if case["error"].is_null() {
                    assert!(problems(&root, &descriptor, Some(password))?.is_empty());
                }
                config["vault"] = serde_json::json!({"salt":"authenticated-fixture-salt","verifier":"authenticated-fixture-verifier"});
            }
            assert_eq!(config, case["persisted"]);
            for name in files.keys() {
                let mut data: Value = serde_json::from_slice(&fs::read(root.join(name))?)?;
                let many = name == "proxies.json";
                let rows: Vec<&mut Value> = if many {
                    data.as_array_mut()
                        .ok_or("Pool missing")?
                        .iter_mut()
                        .collect()
                } else {
                    vec![&mut data]
                };
                for row in rows {
                    let field = if name.starts_with("profiles/") {
                        "password"
                    } else {
                        "pass"
                    };
                    let holder = if many { row } else { &mut row["proxy"] };
                    if let Some(token) = holder
                        .get(field)
                        .and_then(Value::as_str)
                        .filter(|t| t.starts_with("enc:v2:"))
                    {
                        holder[field] = decrypt(
                            password,
                            descriptor["salt"].as_str().ok_or("Salt missing")?,
                            token,
                        )?
                        .into();
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
