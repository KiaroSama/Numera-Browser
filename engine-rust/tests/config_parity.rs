use numera_core::storage::config::ConfigStore;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn settings_read_merge_and_refusal_match_source_without_losing_unknown_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/config.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing config cases")?;
    assert!(!cases.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "config-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for case in cases {
            let path = root.join("config.json");
            if path.exists() {
                fs::remove_file(&path)?;
            }
            if let Some(raw) = case["input"].as_str() {
                fs::write(&path, raw)?;
            }
            let before = fs::read(&path).ok();
            let store = ConfigStore::new(root.clone());
            assert_eq!(Value::Object(store.read()), case["read"]);
            let changes = case["changes"].as_object().ok_or("Missing changes")?;
            match store.set_many(changes.clone()) {
                Ok(value) => {
                    assert!(case["error_type"].is_null());
                    assert_eq!(Value::Object(value), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.source_kind()), case["error_type"].as_str()),
            }
            let after = fs::read(&path).ok();
            assert_eq!(
                before == after,
                case["unchanged"]
                    .as_bool()
                    .ok_or("Missing side effect assertion")?
            );
            if case["error_type"].is_null() {
                let disk = after
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .unwrap_or(Value::Null);
                assert_eq!(disk, case["disk"]);
            }
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
