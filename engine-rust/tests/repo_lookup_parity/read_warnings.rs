use numera_core::{ownership::capture, storage::repo::ProfileRepo};
use serde_json::Value;
use std::{fs, process::Command, time::Duration};

const TEST: &str =
    "read_warnings::read_repair_warnings_match_original_without_changing_read_results";

#[test]
fn read_repair_warnings_match_original_without_changing_read_results()
-> Result<(), Box<dyn std::error::Error>> {
    if let Some(document) = std::env::var_os("NUMERA_READ_WARNING_FIXTURE") {
        let document = std::path::PathBuf::from(document);
        let parent = document.parent().ok_or("Warning fixture parent missing")?;
        let cases: Vec<Value> = serde_json::from_slice(&fs::read(&document)?)?;
        let mut rows = Vec::new();
        for (index, case) in cases.iter().enumerate() {
            let root = parent.join(format!("native-{index}"));
            for (name, value) in case["files"].as_object().ok_or("Warning inputs missing")? {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
                fs::write(path, serde_json::to_vec(value)?)?;
            }
            let paths = [
                root.join("profiles/fixture00001.json"),
                root.join("dashboard/fixture00001.json"),
                root.join("config.json"),
            ];
            let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
            let repo = ProfileRepo::new(root, None);
            eprintln!("READ_REPAIR_BEGIN {index}");
            let result = match case["operation"].as_str() {
                Some("resolve") => repo
                    .resolve("fixture00001", 1700000000.0)
                    .map(serde_json::to_value),
                Some("find") => repo
                    .find_by_name("fixture", 1700000000.0)
                    .map(serde_json::to_value),
                Some("list") => repo.list(1700000000.0).map(serde_json::to_value),
                Some("coherent") => repo
                    .coherent("fixture00001", 1700000000.0)
                    .map(serde_json::to_value),
                Some("get") => repo.get("fixture00001").map(serde_json::to_value),
                _ => return Err("Unknown warning operation".into()),
            };
            eprintln!("READ_REPAIR_END {index}");
            let (result, error) = match result {
                Ok(value) => (value?, Value::Null),
                Err(error) => (Value::Null, Value::String(error.to_string())),
            };
            for (path, bytes) in paths.iter().zip(before) {
                assert_eq!(
                    fs::read(path)?,
                    bytes,
                    "Read warning changed stored bytes: {index}"
                );
            }
            rows.push(serde_json::json!({"result": result, "error": error}));
        }
        println!("\nREAD_REPAIR_ROWS={}", serde_json::to_string(&rows)?);
        return Ok(());
    }

    let temporary = crate::common::TempDirectory::new("read-repair-warnings")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/read_repair_warnings_match_original_without_changing_read_results.json"
    ))?;
    assert_eq!(cases.len(), 20, "Read-warning observations missing");
    let document = temporary.path().join("observations.json");
    fs::write(&document, serde_json::to_vec(&cases)?)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env("NUMERA_READ_WARNING_FIXTURE", &document);
    let native = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 1_048_576,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(native.exit, 0, "Native read-warning observer failed");
    let stdout = String::from_utf8(native.stdout)?;
    assert!(
        stdout.contains("running 1 test") && stdout.contains("test result: ok. 1 passed; 0 failed"),
        "Native warning test did not run"
    );
    let documents = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("READ_REPAIR_ROWS="))
        .collect::<Vec<_>>();
    assert_eq!(documents.len(), 1, "Native warning observations missing");
    let actual: Vec<Value> = serde_json::from_str(documents[0])?;
    assert_eq!(actual.len(), cases.len());
    let stderr = String::from_utf8(native.stderr)?;
    let mut lines = stderr.lines();
    for (index, (case, actual)) in cases.iter().zip(actual).enumerate() {
        assert_eq!(
            lines.next(),
            Some(format!("READ_REPAIR_BEGIN {index}").as_str())
        );
        let end = format!("READ_REPAIR_END {index}");
        let mut warnings = Vec::new();
        loop {
            let line = lines.next().ok_or("Native warning boundary missing")?;
            if line == end {
                break;
            }
            let (category, message) = line.split_once(": ").ok_or("Unexpected read diagnostic")?;
            warnings.push(serde_json::json!({"category":category,"message":message}));
        }
        assert_eq!(
            Value::Array(warnings),
            case["warnings"],
            "Read repair warning differs: case={index}"
        );
        assert_eq!(
            actual["error"], case["error"],
            "Read repair refusal differs: case={index}"
        );
        assert_eq!(
            actual["result"], case["result"],
            "Read repair result differs: case={index}"
        );
    }
    assert!(lines.next().is_none(), "Unattributed native warning output");
    temporary.cleanup()?;
    Ok(())
}
