use numera_core::vault::{check_password, completion::problems, decrypt, transaction::enable};
use serde_json::Value;
use std::{fs, path::PathBuf};

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
