use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn opening_a_store_recovers_before_exposing_credentials_and_refuses_late_damage()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/recovery.json"))?;
    let case = &fixture["cases"][4];
    assert_eq!(case["result"], true);
    let profile_fixture: Value = serde_json::from_str(include_str!("fixtures/models.json"))?;
    let record = profile_fixture["cases"]
        .as_array()
        .ok_or("Model cases missing")?
        .iter()
        .find(|case| case["operation"] == "profile")
        .ok_or("Profile case missing")?["expected"]
        .clone();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("store-open-{}", std::process::id()));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(
            root.join("vault-migration.json"),
            serde_json::to_vec(&case["journal"])?,
        )?;
        fs::write(root.join("config.json"), b"{}")?;
        fs::write(
            root.join("proxies.json"),
            serde_json::to_vec(&case["files"]["proxies.json"])?,
        )?;
        assert!(
            ProfileRepo::open(root.clone(), None, None).is_err(),
            "Half-migrated root opened without its key"
        );
        assert!(
            root.join("vault-migration.json").exists(),
            "Failed open destroyed recovery evidence"
        );
        let repo = ProfileRepo::open(root.clone(), Some("fixture-recovery-master".into()), None)?;
        assert!(!root.join("vault-migration.json").exists());
        let mut fresh = Profile::from_record(&record)?;
        let secret = record["proxy"]["password"]
            .as_str()
            .ok_or("Fixture secret missing")?;
        fresh
            .proxy
            .as_mut()
            .ok_or("Fixture proxy missing")?
            .set_password(Some(secret.into()));
        let saved = repo.save(fresh, 1700000000.0)?;
        let read = repo.get(&saved.id)?.ok_or("Saved record missing")?;
        assert_eq!(
            serde_json::to_value(read)?,
            serde_json::to_value(saved)?,
            "Opened vault changed the credential-aware view"
        );
        let before = fs::read(root.join("profiles").join(format!(
            "{}.json",
            record["id"].as_str().ok_or("ID missing")?
        )))?;
        fs::write(root.join("vault-migration.json"), b"{broken")?;
        assert!(
            repo.get(record["id"].as_str().ok_or("ID missing")?)
                .is_err(),
            "Already-open store read through unresolved migration"
        );
        assert_eq!(
            before,
            fs::read(root.join("profiles").join(format!(
                "{}.json",
                record["id"].as_str().ok_or("ID missing")?
            )))?
        );
        assert_eq!(fs::read(root.join("vault-migration.json"))?, b"{broken");
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
