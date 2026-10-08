use numera_core::vault::{decrypt, recovery::recover};
use serde_json::Value;
use std::{fs, path::PathBuf};

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
