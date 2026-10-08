use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn dual_store_derivation_preserves_viewport_credentials_and_unchanged_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing reconciliation cases")?;
    assert!(!cases.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "repo-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(root.join("profiles"))?;
    fs::create_dir_all(root.join("dashboard"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for case in cases {
            let paths = [
                root.join("profiles/fixture00001.json"),
                root.join("dashboard/fixture00001.json"),
            ];
            for (path, key) in paths.iter().zip(["engine", "dashboard"]) {
                if path.exists() {
                    fs::remove_file(path)?;
                }
                if !case[key].is_null() {
                    fs::write(path, serde_json::to_vec(&case[key])?)?;
                }
            }
            let before: Vec<_> = paths.iter().map(fs::read).map(Result::ok).collect();
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
            let out = if case["operation"] == "adopt" {
                repo.adopt("fixture00001", 1700000000.0).map(Value::Bool)
            } else {
                repo.materialize("fixture00001", 1700000000.0).map(|p| {
                    p.map_or(Value::Null, |p| {
                        serde_json::to_value(p).expect("Profile serialization")
                    })
                })
            };
            match out {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(value, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (index, path) in paths.iter().enumerate() {
                let after = fs::read(path).ok();
                assert_eq!(
                    before[index] == after,
                    case["unchanged"][index]
                        .as_bool()
                        .ok_or("Missing byte observation")?
                );
                let disk = after
                    .map(|bytes| serde_json::from_slice::<Value>(&bytes))
                    .transpose()?
                    .unwrap_or(Value::Null);
                assert_eq!(disk, case["written"][index]);
            }
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
