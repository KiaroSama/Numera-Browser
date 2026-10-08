use numera_core::{storage::dashboard::DashboardStore, vault};
use serde_json::Value;
use std::fs;
mod common;

fn opened(
    mut value: Value,
    password: &str,
    salt: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    if value["proxy"].is_object() {
        for field in ["password", "pass"] {
            if let Some(secret) = value["proxy"][field]
                .as_str()
                .filter(|secret| vault::is_envelope(secret))
                .map(str::to_owned)
            {
                value["proxy"][field] = vault::decrypt(password, salt, &secret)?.into();
            }
        }
    }
    Ok(value)
}
#[test]
fn dashboard_password_intent_keeps_each_withheld_copy_and_refuses_unmeant_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/dashboard-vault.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Vault cases missing")?;
    assert!(!cases.is_empty());
    let password = "fixture-dashboard-master";
    let salt = "AAECAwQFBgcICQoLDA0ODw==";
    for case in cases {
        let temporary = common::TempDirectory::new("dashboard-vault")?;
        let root = temporary.path().to_path_buf();
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut before = Vec::new();
            for (name, value) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(value)?;
                fs::write(&path, &bytes)?;
                before.push((name.clone(), bytes));
            }
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            let store =
                DashboardStore::new(root.clone(), case["password"].as_str().map(str::to_owned));
            match store.save(
                case["input"].clone(),
                serde_json::Map::new(),
                None,
                case["intent"].as_str(),
                1700000000.0,
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    let expected = opened(case["result"].clone(), password, salt)?;
                    assert_eq!(opened(value, password, salt)?, expected, "{}", case["name"]);
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            let mut observed = serde_json::Map::new();
            let mut unchanged = true;
            for (name, bytes) in &before {
                let after = fs::read(root.join(name))?;
                unchanged &= after == *bytes;
                observed.insert(
                    name.clone(),
                    opened(serde_json::from_slice(&after)?, password, salt)?,
                );
            }
            assert_eq!(
                Value::Object(observed),
                case["observed"],
                "{}",
                case["name"]
            );
            assert_eq!(
                Value::Bool(unchanged),
                case["unchanged"],
                "{}",
                case["name"]
            );
            let engine: Value =
                serde_json::from_slice(&fs::read(root.join("profiles/fixture-vault.json"))?)?;
            let dashboard: Value =
                serde_json::from_slice(&fs::read(root.join("dashboard/fixture-vault.json"))?)?;
            assert_eq!(
                Value::Bool(
                    engine["proxy"]["password"]
                        == case["initial"]["profiles/fixture-vault.json"]["proxy"]["password"]
                ),
                case["engine_secret_unchanged"]
            );
            assert_eq!(
                Value::Bool(
                    dashboard["proxy"]["pass"]
                        == case["initial"]["dashboard/fixture-vault.json"]["proxy"]["pass"]
                ),
                case["dashboard_secret_unchanged"]
            );
            assert_eq!(
                store.get("fixture-vault")?.ok_or("Profile missing")?,
                case["listed"],
                "{}",
                case["name"]
            );
            Ok(())
        })();
        temporary.cleanup()?;
        result?;
    }
    Ok(())
}
