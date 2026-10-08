use numera_core::storage::dashboard::DashboardStore;
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn dashboard_revision_increments_source_legacy_values_without_reset_or_partial_write()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/dashboard-revision.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Revision cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let temporary = common::TempDirectory::new("dashboard-revision")?;
        let root = temporary.path().to_path_buf();
        fs::create_dir(root.join("dashboard"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join("dashboard/fixture-revision.json");
            let before = serde_json::to_vec(&case["initial"])?;
            fs::write(&path, &before)?;
            let store = DashboardStore::new(root.clone(), None);
            match store.save(
                case["input"].clone(),
                serde_json::Map::new(),
                None,
                None,
                1700000000.0,
            ) {
                Ok(record) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(record, case["result"], "{}", case["name"]);
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            let bytes = fs::read(&path)?;
            assert_eq!(serde_json::from_slice::<Value>(&bytes)?, case["written"]);
            assert_eq!(Value::Bool(bytes == before), case["unchanged"]);
            assert_eq!(
                Value::Bool(root.join("profiles/fixture-revision.json").exists()),
                case["engine_exists"]
            );
            Ok(())
        })();
        temporary.cleanup()?;
        result?;
    }
    Ok(())
}
