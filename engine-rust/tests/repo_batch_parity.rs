use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};

#[test]
fn batch_names_and_config_commit_with_both_stores_or_not_at_all()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo-batch.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing batch cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("batch-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let before = serde_json::to_vec(&case["config"])?;
            fs::write(root.join("config.json"), &before)?;
            let pending = case["profiles"]
                .as_array()
                .ok_or("Profiles missing")?
                .iter()
                .map(Profile::from_record)
                .collect::<Result<Vec<_>, _>>()?;
            for (name, data) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(path, serde_json::to_vec(data)?)?;
            }
            let repo = ProfileRepo::new(root.clone(), None);
            let changes = case["changes"]
                .as_object()
                .ok_or("Changes missing")?
                .clone();
            let policy = case["policy"].as_str().ok_or("Policy missing")?;
            let saved = if policy == "patch" {
                let mut refs = pending.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
                refs.push("missing".into());
                repo.patch_many(
                    &refs,
                    |profile| {
                        profile.tags.push("applied".into());
                        Ok(true)
                    },
                    changes,
                    "reject",
                    1700000000.0,
                )
            } else {
                repo.save_many(
                    pending,
                    changes,
                    if policy.ends_with("-held") || policy == "legacy-list-name" {
                        "reject"
                    } else {
                        policy
                    },
                    1700000000.0,
                )
            };
            match saved {
                Ok(saved) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(saved)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            let mut written = Map::new();
            for kind in ["profiles", "dashboard"] {
                let directory = root.join(kind);
                if !directory.exists() {
                    continue;
                }
                for entry in fs::read_dir(directory)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|v| v == "json") {
                        let name = format!(
                            "{kind}/{}",
                            path.file_name()
                                .ok_or("Filename missing")?
                                .to_str()
                                .ok_or("Filename not Unicode")?
                        );
                        written.insert(name, serde_json::from_slice(&fs::read(path)?)?);
                    }
                }
            }
            assert_eq!(Value::Object(written), case["written"]);
            let after = fs::read(root.join("config.json"))?;
            assert_eq!(
                before == after,
                case["config_unchanged"]
                    .as_bool()
                    .ok_or("Byte verdict missing")?
            );
            assert_eq!(serde_json::from_slice::<Value>(&after)?, case["persisted"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
