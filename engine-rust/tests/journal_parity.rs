use numera_core::vault::journal::{JournalState, classify};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;

#[test]
fn journal_descriptor_boundaries_match_original_without_repairing_evidence()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::vault::journal::{descriptors, usable_descriptor};

    let temporary = common::TempDirectory::new("journal-descriptor-boundaries")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/journal_descriptor_boundaries_match_original_without_repairing_evidence.json"
    ))?;
    assert_eq!(cases.len(), 145, "Journal descriptor observations missing");
    let root = temporary.path().join("native");
    fs::create_dir_all(&root)?;
    let path = root.join("vault-migration.json");
    for (index, case) in cases.iter().enumerate() {
        let bytes = serde_json::to_vec(&case["input"])?;
        fs::write(&path, &bytes)?;
        let (new, old) = descriptors(&case["input"]);
        let (state, record) = classify(&root);
        let label = match state {
            JournalState::Absent => "absent",
            JournalState::Valid => "valid",
            JournalState::Unreadable => "unreadable",
            JournalState::Unsupported => "unsupported",
        };
        assert_eq!(
            serde_json::json!({"new":new,"old":old,"usable":usable_descriptor(&new),
                       "state":label,"record":record}),
            serde_json::json!({"new":case["new"],"old":case["old"],"usable":case["usable"],
                       "state":case["state"],"record":case["record"]}),
            "Journal descriptor boundary differs for case {index}"
        );
        assert_eq!(
            fs::read(&path)?,
            bytes,
            "Journal classification changed evidence"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn journal_classification_retains_uncertainty_and_original_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/journal.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing journal cases")?;
    assert!(!cases.is_empty(), "Journal oracle must not be empty");
    let parent = std::env::var_os("NUMERA_TEST_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp"));
    let root = parent.join(format!(
        "journal-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = root.join("vault-migration.json");
        for case in cases {
            if path.exists() {
                fs::remove_file(&path)?;
            }
            if case["input"] == "broken-json" {
                fs::write(&path, b"{broken")?;
            } else if !case["input"].is_null() {
                fs::write(&path, serde_json::to_vec(&case["input"])?)?;
            }
            let before = fs::read(&path).ok();
            let (state, record) = classify(&root);
            let label = match state {
                JournalState::Absent => "absent",
                JournalState::Valid => "valid",
                JournalState::Unreadable => "unreadable",
                JournalState::Unsupported => "unsupported",
            };
            assert_eq!(
                Value::String(label.into()),
                case["expected"],
                "Journal case {}",
                case["name"]
            );
            assert_eq!(
                record.unwrap_or(Value::Null),
                case["record"],
                "Journal record {}",
                case["name"]
            );
            assert_eq!(
                fs::read(&path).ok(),
                before,
                "Classification must not mutate recovery evidence"
            );
        }
        Ok(())
    })();
    fs::remove_dir_all(&root)?;
    result
}
