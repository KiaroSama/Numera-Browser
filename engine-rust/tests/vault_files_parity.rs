use numera_core::vault::{decrypt, is_envelope, migration::Migration};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn encrypting_secret_sites_preserves_other_fields_and_rolls_back_bad_files()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/migration.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing migration cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!(
                "vault-files-{}-{index}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let files = case["files"].as_object().ok_or("Missing files")?;
            let mut before = Vec::new();
            for (name, data) in files {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = if data == "broken-json" {
                    b"{broken".to_vec()
                } else {
                    serde_json::to_vec(data)?
                };
                fs::write(&path, &bytes)?;
                before.push((name, path, bytes));
            }
            let password = case["password"].as_str().ok_or("Password missing")?;
            let salt = case["salt"].as_str().ok_or("Salt missing")?;
            let mut step = Migration::new(root.clone());
            match step.encrypt_all(password, salt) {
                Ok(count) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Some(count as u64), case["count"].as_u64());
                }
                Err(_) => {
                    assert_eq!(case["error"], "unreadable");
                    assert!(step.rollback().is_empty());
                }
            }
            let mut encrypted = 0;
            for (name, path, bytes) in before {
                let after = fs::read(&path)?;
                assert_eq!(
                    bytes == after,
                    case["unchanged"][name]
                        .as_bool()
                        .ok_or("Missing side effect")?
                );
                let data = serde_json::from_slice::<Value>(&after);
                let mut data = match data {
                    Ok(v) => v,
                    Err(_) => {
                        assert_eq!(case["observed"][name], "broken-json");
                        continue;
                    }
                };
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
                    if let Some(value) = holder.get(field).and_then(Value::as_str)
                        && is_envelope(value)
                        && !value.starts_with("enc:v9:")
                    {
                        encrypted += 1;
                        holder[field] = decrypt(password, salt, value)?.into();
                    }
                }
                assert_eq!(data, case["observed"][name]);
            }
            assert_eq!(
                encrypted,
                case["encrypted"]
                    .as_u64()
                    .ok_or("Missing encrypted count")?
            );
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
