use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn saving_both_copies_keeps_dashboard_fields_and_explicit_secret_intent()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-write.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing write cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!(
                "repo-write-{}-{index}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
        for kind in ["profiles", "dashboard"] {
            fs::create_dir_all(root.join(kind))?;
        }
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut before = Vec::new();
            for kind in ["profiles", "dashboard"] {
                let path = root.join(kind).join("fixture-write.json");
                let bytes = serde_json::to_vec(&case["initial"][kind])?;
                fs::write(&path, &bytes)?;
                before.push((kind, path, bytes));
            }
            if !case["other"].is_null() {
                fs::write(
                    root.join("profiles/fixture-other.json"),
                    serde_json::to_vec(&case["other"])?,
                )?;
            }
            let mut profile = Profile::from_record(&case["profile"])?;
            if case["intent"] != "preserve"
                && let Some(proxy) = profile.proxy.as_mut()
            {
                proxy.set_password(
                    case["profile"]["proxy"]["password"]
                        .as_str()
                        .map(str::to_owned),
                );
            }
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.save(profile, 1700000000.0) {
                Ok(profile) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(profile)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (kind, path, bytes) in before {
                let after = fs::read(path)?;
                assert_eq!(
                    after == bytes,
                    case["unchanged"][kind]
                        .as_bool()
                        .ok_or("Missing byte observation")?
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][kind]
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn failed_second_store_write_rolls_back_first_store_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-write.json"))?;
    let case = &oracle["cases"][0];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "write-failure-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(root.join("profiles"))?;
    fs::create_dir_all(root.join("dashboard/fixture-write.json"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = root.join("profiles/fixture-write.json");
        let before = serde_json::to_vec(&case["initial"]["profiles"])?;
        fs::write(&path, &before)?;
        let repo = ProfileRepo::new(root.clone(), None);
        assert!(
            repo.save(Profile::from_record(&case["profile"])?, 1700000000.0)
                .is_err()
        );
        assert_eq!(
            fs::read(path)?,
            before,
            "Failed dashboard write lost original engine bytes"
        );
        assert!(
            root.join("dashboard/fixture-write.json").is_dir(),
            "Caller filesystem obstruction changed"
        );
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
