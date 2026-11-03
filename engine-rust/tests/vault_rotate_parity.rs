use numera_core::vault::{check_password, decrypt, transaction::rotate};
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

#[test]
fn public_password_change_updates_only_the_successful_store_instance()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::ProfileRepo};
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("vault-store-change")?;
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
from numera import crypto
from numera.store import ProfileStore
from numera.vault import decrypt_for_read
from run_logging import run_log
fixtures=repository/'engine-rust/tests/fixtures'
cases=json.loads((fixtures/'vault-rotate.json').read_text(encoding='utf-8'))['cases']
profile=json.loads((fixtures/'repo.json').read_text(encoding='utf-8'))['cases'][0]['engine']
observed=[]
with run_log('vault-store-change-oracle',parent/'logs') as log:
    for index,case in enumerate(cases):
        root=parent/str(index);prior=None if index==0 else 'fixture-old-master'
        store=ProfileStore(root,password=prior)
        files=copy.deepcopy(case['files']);files['config.json']=case['config']
        if index==0:
            token=files['proxies.json'][0]['pass']
            record=copy.deepcopy(profile);record['id']='a';record['future']=7
            record['proxy']['password']=token
            files['profiles/a.json']=record
            files['dashboard/a.json']={'proxy':{'pass':token},'folder':'kept'}
        for name,value in files.items():
            path=root/name;path.parent.mkdir(parents=True,exist_ok=True)
            path.write_text(json.dumps(value),encoding='utf-8')
        session=root/'user-data'/'kept'/'session.bin';session.parent.mkdir(parents=True)
        session.write_bytes(b'\x00\xff\r\n')
        before={name:(root/name).read_bytes() for name in files}
        try: count,error=store.change_vault_password(case['old_password'],case['password']),None
        except (RuntimeError,ValueError) as exc: count,error=None,str(exc)
        status={'enabled':store.vault_enabled(),'unlocked':store.unlocked()}
        after={name:(root/name).read_bytes() for name in files}
        disk={name:json.loads(raw) for name,raw in after.items()}
        if count is not None:
            descriptor=store.vault()
            assert status=={'enabled':True,'unlocked':True}
            assert crypto.check_password(case['password'],descriptor['salt'],descriptor['verifier'])
            assert not crypto.check_password(case['old_password'],descriptor['salt'],descriptor['verifier'])
            disk['config.json']['vault']={'authenticated':True}
            for name,data in disk.items():
                if name=='config.json':continue
                rows=data if name=='proxies.json' else [data]
                for row in rows:
                    holder=row if name=='proxies.json' else row.get('proxy')
                    key='password' if name.startswith('profiles/') else 'pass'
                    if isinstance(holder,dict) and holder.get(key):
                        assert holder[key].startswith('enc:v2:'),'Original rotation left plaintext'
                        holder[key]=decrypt_for_read(store,holder[key])
        else: assert after==before,'Original refused rotation changed bytes'
        found=store.get('a');read=None if found is None else found.to_dict()
        assert session.read_bytes()==b'\x00\xff\r\n'
        observed.append({'files':files,'prior':prior,'old':case['old_password'],'new':case['password'],
                         'count':count,'error':error,'status':status,'read':read,'disk':disk,
                         'unchanged':{name:raw==after[name] for name,raw in before.items()},
                         'journal':(root/'vault-migration.json').exists()})
    log.info('Observed public password change cases=%s',len(observed))
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
        "Original public password change observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 5);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-change-{index}"));
        let files = case["files"].as_object().ok_or("Rotation inputs missing")?;
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
        let old = case["old"].as_str().ok_or("Old password missing")?;
        let new = case["new"].as_str().ok_or("New password missing")?;
        let (count, error) = match repo.change_vault_password(old, new) {
            Ok(count) => (Some(count), None),
            Err(error) => (None, Some(error.to_string())),
        };
        assert_eq!(
            serde_json::to_value(count)?,
            case["count"],
            "Public rotation count differs"
        );
        assert_eq!(
            serde_json::to_value(error)?,
            case["error"],
            "Public rotation refusal differs"
        );
        assert_eq!(
            serde_json::json!({"enabled":repo.vault_enabled()?,"unlocked":repo.unlocked()?}),
            case["status"],
            "Public rotation instance state differs"
        );
        assert_eq!(
            serde_json::to_value(repo.get("a")?)?,
            case["read"],
            "Same-instance rotated read differs"
        );
        let descriptor = repo.vault()?.unwrap_or(Value::Null);
        let mut disk = serde_json::Map::new();
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
                let salt = descriptor["salt"].as_str().ok_or("Salt missing")?;
                if name == "config.json" {
                    let verifier = descriptor["verifier"].as_str().ok_or("Verifier missing")?;
                    assert!(check_password(new, salt, verifier));
                    assert!(
                        !check_password(old, salt, verifier),
                        "Old key remained accepted"
                    );
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
                                "Public rotation left plaintext"
                            );
                            holder[key] = decrypt(new, salt, secret)?.into();
                        }
                    }
                }
            }
            disk.insert(name, value);
        }
        assert_eq!(
            Value::Object(disk),
            case["disk"],
            "Public rotation stored values differ"
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
fn master_password_rotation_keeps_the_old_generation_on_every_refusal()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/vault-rotate.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing rotation cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("rotate-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            let files = case["files"].as_object().ok_or("Missing files")?;
            for (name, data) in files {
                fs::write(root.join(name), serde_json::to_vec(data)?)?;
            }
            let before = case["unchanged"]
                .as_object()
                .ok_or("Missing bytes")?
                .keys()
                .map(|name| Ok((name.clone(), fs::read(root.join(name))?)))
                .collect::<Result<Vec<_>, std::io::Error>>()?;
            match rotate(
                &root,
                case["old_password"]
                    .as_str()
                    .ok_or("Old password missing")?,
                case["password"].as_str().ok_or("Password missing")?,
            ) {
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
                        .ok_or("Byte verdict missing")?
                );
            }
            assert_eq!(
                root.join("vault-migration.json").exists(),
                case["journal_retained"]
                    .as_bool()
                    .ok_or("Journal verdict missing")?
            );
            let mut config: Value = serde_json::from_slice(&fs::read(root.join("config.json"))?)?;
            let descriptor = config["vault"].clone();
            let password = if case["error"].is_null() {
                "fixture-new-master"
            } else {
                "fixture-old-master"
            };
            if !descriptor.is_null() {
                let salt = descriptor["salt"].as_str().ok_or("Salt missing")?;
                let verifier = descriptor["verifier"].as_str().ok_or("Verifier missing")?;
                assert!(check_password(password, salt, verifier));
                if case["error"].is_null() {
                    assert!(
                        !check_password("fixture-old-master", salt, verifier),
                        "Old key remained accepted"
                    );
                }
                config["vault"] = serde_json::json!({"salt":"authenticated-fixture-salt","verifier":"authenticated-fixture-verifier"});
            }
            assert_eq!(config, case["persisted"]);
            for name in files.keys() {
                let mut data: Value = serde_json::from_slice(&fs::read(root.join(name))?)?;
                for row in data.as_array_mut().ok_or("Pool missing")? {
                    if let Some(token) = row
                        .get("pass")
                        .and_then(Value::as_str)
                        .filter(|v| v.starts_with("enc:v2:"))
                    {
                        row["pass"] = decrypt(
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
