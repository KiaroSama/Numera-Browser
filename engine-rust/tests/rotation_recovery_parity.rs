use numera_core::vault::{decrypt, recovery::recover};
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

#[test]
fn public_resume_returns_the_authenticated_store_or_preserves_recovery_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::repo::ProfileRepo;

    let temporary = common::TempDirectory::new("vault-store-resume")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/public_resume_returns_the_authenticated_store_or_preserves_recovery_evidence.json"
    ))?;
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
    let temporary = common::TempDirectory::new("rotation-metadata")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/committed_rotation_metadata_matches_original_without_rewriting_generation.json"
    ))?;
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
