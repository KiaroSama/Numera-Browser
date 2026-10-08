use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};

#[test]
fn a_later_batch_write_failure_restores_all_prior_store_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo-batch.json"))?;
    let case = &fixture["cases"][0];
    let profiles = case["profiles"].as_array().ok_or("Profiles missing")?;
    assert_eq!(profiles.len(), 2);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("batch-rollback-{}", std::process::id()));
    fs::create_dir_all(root.join("profiles"))?;
    fs::create_dir_all(root.join("dashboard/fixture-1.json"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let first = root.join("profiles/fixture-0.json");
        let before = serde_json::to_vec(&profiles[0])?;
        fs::write(&first, &before)?;
        let config = b"{\"future\":7}";
        fs::write(root.join("config.json"), config)?;
        let pending = profiles
            .iter()
            .map(Profile::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        assert!(
            ProfileRepo::new(root.clone(), None)
                .save_many(pending, Map::new(), "reject", 1700000000.0)
                .is_err()
        );
        assert_eq!(
            fs::read(first)?,
            before,
            "First record was not restored byte-for-byte"
        );
        assert!(
            !root.join("dashboard/fixture-0.json").exists(),
            "Failed batch left a newly created twin"
        );
        assert!(
            !root.join("profiles/fixture-1.json").exists(),
            "Failed batch left its second engine copy"
        );
        assert!(
            root.join("dashboard/fixture-1.json").is_dir(),
            "Caller obstruction was removed"
        );
        assert_eq!(fs::read(root.join("config.json"))?, config);
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
