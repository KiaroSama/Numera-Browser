use numera_core::vault::{check_password, decrypt, transaction::rotate};
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

#[test]
fn public_password_change_updates_only_the_successful_store_instance()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::repo::ProfileRepo;

    let temporary = common::TempDirectory::new("vault-store-change")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/public_password_change_updates_only_the_successful_store_instance.json"
    ))?;
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
