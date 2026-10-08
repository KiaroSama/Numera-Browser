use numera_core::vault::{check_password, decrypt, transaction::rotate};
use serde_json::Value;
use std::{fs, path::PathBuf};

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
