use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn profile_export_refuses_to_drop_withheld_credentials() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/profile-export.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("export-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            if case["missing"] != true {
                fs::write(
                    root.join("profiles/fixture-export.json"),
                    serde_json::to_vec(&case["profile"])?,
                )?;
            }
            let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
            let dest = root.join("export.json");
            match repo.export("fixture-export", &dest) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value.is_some()), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            assert_eq!(
                dest.exists(),
                case["destination_present"]
                    .as_bool()
                    .ok_or("Destination verdict missing")?
            );
            if dest.exists() {
                assert_eq!(
                    serde_json::from_slice::<Value>(&fs::read(dest)?)?,
                    case["written"]
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
