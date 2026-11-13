use super::{ConfigStore, Value, common, fs};
use numera_core::storage::config::ConfigError;
use std::collections::BTreeMap;

#[test]
fn settings_getters_match_original_strict_text_admission_and_values()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("strict-settings-getters")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/settings_getters_match_original_strict_text_admission_and_values.json"
    ))?;
    assert_eq!(cases.len(), 33);
    let installed = BTreeMap::from([("camoufox".into(), true)]);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("config.json");
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
        }
        let store = ConfigStore::new(root);
        let result = match case["operation"].as_str().ok_or("Operation missing")? {
            "read" => store.read().map(Value::Object),
            "engine" => store.default_engine(&installed),
            "headless" => store.headless_launch().map(Value::Bool),
            _ => return Err("Unknown getter operation".into()),
        };
        assert_eq!(
            result.is_err(),
            case["error"] == "UnicodeDecodeError",
            "Strict settings getter admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(value, case["result"], "Getter value differs: case={index}"),
            Err(error) => assert!(matches!(error, ConfigError::Io(ref e)
                if e.kind() == std::io::ErrorKind::InvalidData)),
        }
        assert_eq!(fs::read(path).ok(), raw, "Getter changed settings bytes");
    }
    temporary.cleanup()?;
    Ok(())
}
