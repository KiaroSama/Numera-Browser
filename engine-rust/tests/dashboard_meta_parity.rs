use numera_core::storage::dashboard::DashboardStore;
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn dashboard_metadata_preserves_reads_and_delete_delegates_owned_transaction()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/dashboard-meta.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Metadata cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let temporary = common::TempDirectory::new("dashboard-meta")?;
        let root = temporary.path().to_path_buf();
        fs::create_dir(root.join("dashboard"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let store = DashboardStore::new(root.clone(), None);
            if case.get("raw").is_some() {
                let path = root.join("dashboard-config.json");
                if let Some(raw) = case["raw"].as_str() {
                    fs::write(&path, raw.as_bytes())?;
                }
                let before = fs::read(&path).ok();
                assert_eq!(store.get_meta()?, case["read"], "{}", case["name"]);
                assert_eq!(
                    Value::Bool(before == fs::read(&path).ok()),
                    case["read_unchanged"]
                );
                assert_eq!(store.set_meta(case["input"].clone())?, case["result"]);
                assert_eq!(store.get_meta()?, case["written"]);
                assert_eq!(
                    serde_json::to_value(store.list()?)?,
                    case["profile_listing"]
                );
            } else {
                if case["present"] == true {
                    for kind in ["profiles", "dashboard"] {
                        fs::create_dir_all(root.join(kind))?;
                        fs::write(
                            root.join(kind).join("fixture-delete.json"),
                            serde_json::to_vec(
                                &serde_json::json!({"id":"fixture-delete","name":"Delete α"}),
                            )?,
                        )?;
                    }
                    fs::create_dir_all(root.join("user-data/fixture-delete"))?;
                    fs::write(
                        root.join("user-data/fixture-delete/Cookies"),
                        b"fixture-cookies",
                    )?;
                }
                if case["damaged_claim"] == true {
                    fs::create_dir_all(root.join("sessions"))?;
                    fs::write(root.join("sessions/fixture-delete.json"), b"{")?;
                }
                match store.delete("fixture-delete", "fixture-host", 1700000000.0) {
                    Ok(deleted) => {
                        assert!(case["error"].is_null(), "{}", case["name"]);
                        assert_eq!(Value::Bool(deleted), case["result"]);
                    }
                    Err(error) => {
                        assert_eq!(Some(error.to_string().as_str()), case["error"].as_str())
                    }
                }
                assert_eq!(
                    Value::Bool(root.join("profiles/fixture-delete.json").exists()),
                    case["engine_exists"]
                );
                assert_eq!(
                    Value::Bool(root.join("dashboard/fixture-delete.json").exists()),
                    case["dashboard_exists"]
                );
                assert_eq!(
                    Value::Bool(root.join("user-data/fixture-delete").exists()),
                    case["session_exists"]
                );
            }
            Ok(())
        })();
        temporary.cleanup()?;
        result?;
    }
    Ok(())
}
