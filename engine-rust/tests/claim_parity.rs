use numera_core::ownership::claims::{self, Claim, State};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn unreadable_and_unverifiable_claims_never_admit_a_second_session()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/claims.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing claim cases")?;
    assert!(!cases.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("claims-{}", std::process::id()));
    fs::create_dir_all(root.join("sessions"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for case in cases {
            let claim = if let Some(raw) = case.get("raw") {
                let path = root.join("sessions/fixture-profile.json");
                if let Some(text) = raw.as_str() {
                    fs::write(&path, text)?;
                } else if path.exists() {
                    fs::remove_file(&path)?;
                }
                claims::read(&root, "fixture-profile")
            } else if case["input"].is_null() {
                Claim::Absent
            } else {
                Claim::Record(case["input"].clone())
            };
            let expected = match case["state"].as_str() {
                Some("free") => State::Free,
                Some("busy") => State::Busy,
                Some("unknown") => State::Unknown,
                _ => return Err("Invalid source claim verdict".into()),
            };
            assert_eq!(
                claims::state(&claim, "fixture-local-host"),
                expected,
                "{case}"
            );
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
