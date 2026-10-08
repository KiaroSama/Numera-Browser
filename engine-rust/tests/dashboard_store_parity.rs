use numera_core::storage::dashboard::DashboardStore;
use serde_json::{Map, Value};
use std::fs;
mod common;

#[test]
fn dashboard_save_keeps_richer_fields_revision_and_atomic_engine_twin()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/dashboard-store.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Dashboard cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let temporary = common::TempDirectory::new("dashboard-store")?;
        let root = temporary.path().to_path_buf();
        fs::create_dir(root.join("dashboard"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("dashboard-config.json"),
                serde_json::to_vec(&case["meta"])?,
            )?;
            fs::write(root.join("config.json"), b"{\"future\":7}")?;
            let before = fs::read(root.join("config.json"))?;
            let mut originals = Vec::new();
            for (name, value) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(value)?;
                fs::write(&path, &bytes)?;
                originals.push((path, bytes));
            }
            let store = DashboardStore::new(root.clone(), None);
            match store.save(
                case["input"].clone(),
                serde_json::json!({"default_engine":"patchright"})
                    .as_object()
                    .ok_or("Changes missing")?
                    .clone(),
                case.get("expect_rev").filter(|value| !value.is_null()),
                None,
                1700000000.0,
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(value, case["result"], "{}", case["name"]);
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            let mut written = Map::new();
            for kind in ["profiles", "dashboard"] {
                let directory = root.join(kind);
                if !directory.exists() {
                    continue;
                }
                for entry in fs::read_dir(directory)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|ext| ext == "json") {
                        written.insert(
                            format!(
                                "{kind}/{}",
                                path.file_name()
                                    .ok_or("Filename missing")?
                                    .to_str()
                                    .ok_or("Invalid filename")?
                            ),
                            serde_json::from_slice(&fs::read(path)?)?,
                        );
                    }
                }
            }
            assert_eq!(Value::Object(written), case["written"], "{}", case["name"]);
            let after = fs::read(root.join("config.json"))?;
            assert_eq!(Value::Bool(before == after), case["config_unchanged"]);
            assert_eq!(serde_json::from_slice::<Value>(&after)?, case["config"]);
            let unchanged = originals.iter().try_fold(
                true,
                |same, (path, bytes)| -> std::io::Result<bool> {
                    Ok(same && fs::read(path)? == *bytes)
                },
            )?;
            assert_eq!(Value::Bool(unchanged), case["original_unchanged"]);
            Ok(())
        })();
        temporary.cleanup()?;
        result?;
    }
    Ok(())
}
