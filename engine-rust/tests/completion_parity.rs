use numera_core::vault::completion::problems;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;

#[test]
fn completion_diagnostics_match_original_distinct_paths_and_readable_holders()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("completion-diagnostics")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/completion_diagnostics_match_original_distinct_paths_and_readable_holders.json"
    ))?;
    assert_eq!(cases.len(), 36);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(&root)?;
        let mut before = Vec::new();
        for (name, data) in case["files"].as_object().ok_or("Files missing")? {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
            let raw = serde_json::to_vec(data)?;
            fs::write(&path, &raw)?;
            before.push((path, raw));
        }
        assert_eq!(
            serde_json::to_value(problems(
                &root,
                &case["descriptor"],
                case["password"].as_str()
            )?)?,
            case["expected"],
            "Completion diagnostic differs for case {index}"
        );
        for (path, raw) in before {
            assert_eq!(fs::read(path)?, raw, "Completion changed evidence");
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn only_authenticated_readable_generation_can_complete_vault()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/completion.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing completion cases")?;
    assert!(!cases.is_empty());
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp");
    for (index, case) in cases.iter().enumerate() {
        let root = parent.join(format!(
            "completion-{}-{index}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let files = case["files"].as_object().ok_or("Missing files")?;
            let mut before = Vec::new();
            for (name, value) in files {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(value)?;
                fs::write(&path, &bytes)?;
                before.push((path, bytes));
            }
            let actual = problems(&root, &case["descriptor"], case["password"].as_str())?;
            assert_eq!(serde_json::to_value(actual)?, case["expected"]);
            for (path, bytes) in before {
                assert_eq!(fs::read(path)?, bytes, "Completion changed evidence");
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
