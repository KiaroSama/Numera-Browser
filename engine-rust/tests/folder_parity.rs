use numera_core::storage::folders::Folders;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn folder_transactions_keep_profiles_and_follow_retired_names()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/folders.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing folder cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("folders-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("dashboard"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("dashboard-config.json"),
                serde_json::to_vec(&case["meta"])?,
            )?;
            let profiles = case["profiles"].as_object().ok_or("Missing profiles")?;
            let mut before = Vec::new();
            for (name, data) in profiles {
                let path = root.join("dashboard").join(name);
                let bytes = serde_json::to_vec(data)?;
                fs::write(&path, &bytes)?;
                before.push((name, path, bytes));
            }
            let folders = Folders::new(root.clone());
            assert_eq!(serde_json::to_value(folders.listing()?)?, case["listed"]);
            let args = case["args"].as_array().ok_or("Missing arguments")?;
            let first = args[0].as_str().ok_or("Missing name")?;
            let changed = match case["operation"].as_str() {
                Some("create") => folders.create(first),
                Some("rename") => folders.rename(first, args[1].as_str().ok_or("Missing target")?),
                Some("delete") => folders.delete(first, args.get(1).and_then(Value::as_str)),
                _ => return Err("Unknown folder operation".into()),
            };
            match changed {
                Ok(names) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(names)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (name, path, bytes) in before {
                let after = fs::read(path)?;
                assert_eq!(
                    after == bytes,
                    case["unchanged"][name]
                        .as_bool()
                        .ok_or("Missing byte verdict")?
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][name]
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(root.join("dashboard-config.json"))?)?,
                case["metadata"]
            );
            for (name, expected) in case["resolved"]
                .as_object()
                .ok_or("Missing resolved names")?
            {
                assert_eq!(
                    folders.resolve_name(name)?,
                    expected.as_str().ok_or("Invalid resolved name")?
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
