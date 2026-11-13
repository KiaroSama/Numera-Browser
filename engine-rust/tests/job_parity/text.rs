use super::common;
use numera_core::jobs::JobStore;
use serde_json::{Value, json};
use std::{fs, io};

#[test]
fn job_text_admission_matches_original_without_replacing_identity_or_outcome()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("job-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/job_text_admission_matches_original_without_replacing_identity_or_outcome.json"
    ))?;
    assert_eq!(cases.len(), 54, "Job text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let directory = root.join("jobs");
        fs::create_dir_all(&directory)?;
        let record = directory.join("sample.json");
        let outcome = directory.join("sample.exit");
        for (path, key) in [(&record, "identity"), (&outcome, "outcome")] {
            if !case[key].is_null() {
                fs::write(path, serde_json::from_value::<Vec<u8>>(case[key].clone())?)?;
            }
        }
        let store = JobStore::new(root);
        let result = match case["operation"].as_str() {
            Some("get") => store.get("sample", 1000.0).map(|value| json!(value)),
            Some("listing") => store.listing(1000.0).map(|value| json!(value)),
            Some("record_exit") => store.record_exit("sample", 7, 1000.0).map(|()| Value::Null),
            _ => return Err("Unknown job text operation".into()),
        };
        let actual = match result {
            Ok(value) => json!({"ok":value}),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                json!({"error":"InvalidData"})
            }
            Err(error) => return Err(error.into()),
        };
        assert_eq!(
            actual, case["result"],
            "Job text admission differs: {index}"
        );
        for (path, key) in [(&record, "after_identity"), (&outcome, "after_outcome")] {
            let expected: Option<Vec<u8>> = serde_json::from_value(case[key].clone())?;
            let actual = match fs::read(path) {
                Ok(bytes) => Some(bytes),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            assert_eq!(actual, expected, "Job text bytes differ: {index}/{key}");
        }
    }
    temporary.cleanup()?;
    Ok(())
}
