use numera_core::vault::{decrypt, recovery::recover_enable};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

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
