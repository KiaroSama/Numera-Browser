use numera_core::storage::dashboard::DashboardStore;
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn dashboard_batch_io_failure_restores_prior_twins_and_config()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/dashboard-batch.json"))?;
    let inputs = fixture["cases"][0]["input"]
        .as_array()
        .ok_or("Inputs missing")?
        .clone();
    let temporary = common::TempDirectory::new("dashboard-rollback")?;
    let root = temporary.path().to_path_buf();
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let store = DashboardStore::new(root.clone(), None);
        let first = store.save(
            inputs[0].clone(),
            serde_json::Map::new(),
            None,
            None,
            1700000000.0,
        )?;
        fs::write(root.join("config.json"), b"{\"future\":7}")?;
        let engine = root.join("profiles/fixture-0.json");
        let dash = root.join("dashboard/fixture-0.json");
        let before_engine = fs::read(&engine)?;
        let before_dash = fs::read(&dash)?;
        let before_config = fs::read(root.join("config.json"))?;
        fs::create_dir(root.join("profiles/fixture-1.json"))?;
        let mut pending = inputs;
        pending[0]["notes"] = "would change".into();
        assert!(
            store
                .save_many(
                    pending,
                    serde_json::json!({"default_engine":"patchright"})
                        .as_object()
                        .ok_or("Changes missing")?
                        .clone(),
                    None,
                    1700000001.0
                )
                .is_err()
        );
        assert_eq!(fs::read(&engine)?, before_engine);
        assert_eq!(fs::read(&dash)?, before_dash);
        assert_eq!(fs::read(root.join("config.json"))?, before_config);
        assert!(!root.join("dashboard/fixture-1.json").exists());
        assert!(root.join("profiles/fixture-1.json").is_dir());
        let changed = store
            .patch(
                "fixture-0",
                |mut stored| {
                    assert_eq!(stored, first);
                    stored["tags"] = serde_json::json!(["patched"]);
                    Ok((stored, serde_json::Map::new()))
                },
                Some(&first["_rev"]),
                Some("preserve"),
                1700000002.0,
            )?
            .ok_or("Profile missing")?;
        assert_eq!(changed["_rev"], 2);
        assert_eq!(changed["tags"], serde_json::json!(["patched"]));
        assert!(
            store
                .patch(
                    "missing",
                    |_| panic!("Missing profile must not invoke mutation"),
                    None,
                    None,
                    1700000000.0
                )?
                .is_none()
        );
        Ok(())
    })();
    temporary.cleanup()?;
    result
}
