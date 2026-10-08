use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn deletion_removes_both_copies_and_refuses_unreadable_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo-delete.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing delete cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("delete-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("user-data/fixture-delete"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("profiles/fixture-delete.json"),
                serde_json::to_vec(&case["profile"])?,
            )?;
            fs::write(
                root.join("user-data/fixture-delete/fixture-state.txt"),
                "isolated browser state α",
            )?;
            if case["corrupt_claim"] == true {
                fs::create_dir_all(root.join("sessions"))?;
                fs::write(root.join("sessions/fixture-delete.json"), b"{broken")?;
            }
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.delete(
                case["reference"].as_str().ok_or("Reference missing")?,
                "fixture-host",
                1700000000.0,
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (key, path) in [
                ("engine_present", "profiles/fixture-delete.json"),
                ("dashboard_present", "dashboard/fixture-delete.json"),
                ("session_present", "user-data/fixture-delete"),
                ("claim_retained", "sessions/fixture-delete.json"),
            ] {
                assert_eq!(
                    root.join(path).exists(),
                    case[key].as_bool().ok_or("Missing side effect")?
                );
            }
            assert!(
                !fs::read_dir(root.join("user-data"))?.any(|entry| entry
                    .is_ok_and(|entry| entry.file_name().to_string_lossy().contains(".deleting-"))),
                "Unreclaimed synthetic tombstone"
            );
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
