use numera_core::{browser::admission, ownership::claims};
use serde_json::{Value, json};
use std::fs;
mod common;

#[test]
fn native_launch_reads_current_profile_under_lease_and_refuses_unreadable_secret()
-> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-admission.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Admission cases missing")?;
    assert_eq!(cases.len(), 5);
    for case in cases {
        let temporary = common::TempDirectory::new("browser-admission")?;
        let root = temporary.path();
        for (name, value) in case["files"].as_object().ok_or("Files missing")? {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
            fs::write(path, serde_json::to_vec(value)?)?;
        }
        let result = admission::current(
            root,
            "a",
            "stale",
            "fixture-host",
            case["password"].as_str(),
            1000.0,
        );
        match result {
            Ok((session, profile)) => {
                assert!(case["error"].is_null());
                assert_eq!(
                    serde_json::to_value(profile)?,
                    case["expected"],
                    "Current profile {}",
                    case["name"]
                );
                assert!(
                    admission::current(root, "a", "stale", "fixture-host", None, 1000.0).is_err(),
                    "Lease was not retained"
                );
                drop(session);
            }
            Err(error) => {
                assert_eq!(json!(error.kind), case["kind"]);
                assert_eq!(json!(error.message), case["error"]);
            }
        }
        assert!(
            matches!(claims::read(root, "a"), claims::Claim::Absent),
            "Admission minted browser capability"
        );
        assert!(!root.join("user-data/a").exists());
        temporary.cleanup()?;
    }
    Ok(())
}
