use numera_core::storage::dashboard::DashboardStore;
use serde_json::{Map, Value};
use std::fs;
mod common;

#[test]
fn dashboard_readers_preserve_strict_text_admission_and_original_fallbacks()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::repo::RepoError;

    let temporary = common::TempDirectory::new("dashboard-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/dashboard_readers_preserve_strict_text_admission_and_original_fallbacks.json"
    ))?;
    assert_eq!(cases.len(), 18, "Dashboard text observations missing");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["error"] == "UnicodeDecodeError")
            .count(),
        9,
        "Original dashboard UTF-8 refusals missing"
    );
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("dashboard"))?;
        let inputs = case["inputs"]
            .as_object()
            .ok_or("Dashboard input bytes missing")?;
        for (name, raw) in inputs {
            if !raw.is_null() {
                let bytes: Vec<u8> = serde_json::from_value(raw.clone())?;
                fs::write(root.join(name), &bytes)?;
                assert_eq!(
                    fs::read(root.join(name))?,
                    bytes,
                    "Dashboard input bytes differ"
                );
            }
        }
        let store = DashboardStore::new(root.clone(), None);
        let result = match case["operation"].as_str() {
            Some("get_meta") => store.get_meta().map(serde_json::to_value),
            Some("list") => store.list().map(serde_json::to_value),
            Some("get") => store.get("b").map(serde_json::to_value),
            _ => return Err("Unknown dashboard text operation".into()),
        };
        let error = match &result {
            Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData => {
                assert!(error.to_string().contains("UTF-8"));
                Some("UnicodeDecodeError")
            }
            Err(RepoError::InvalidRecord(_)) => Some("JSONDecodeError"),
            Ok(_) => None,
            Err(_) => return Err("Unexpected dashboard text refusal".into()),
        };
        assert_eq!(
            error,
            case["error"].as_str(),
            "Dashboard text admission differs: case={index}"
        );
        if error.is_none() {
            assert_eq!(
                result??, case["result"],
                "Dashboard text result differs: case={index}"
            );
        }
        for (name, raw) in inputs {
            if raw.is_null() {
                assert!(
                    !root.join(name).try_exists()?,
                    "Dashboard read created missing input"
                );
            } else {
                assert_eq!(
                    fs::read(root.join(name))?,
                    serde_json::from_value::<Vec<u8>>(raw.clone())?,
                    "Dashboard read changed stored bytes: case={index} file={name}"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn dashboard_save_keeps_richer_fields_revision_and_atomic_engine_twin()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/dashboard-store.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Dashboard cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let temporary = common::TempDirectory::new("dashboard-store")?;
        let root = temporary.path().to_path_buf();
        fs::create_dir(root.join("dashboard"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("dashboard-config.json"),
                serde_json::to_vec(&case["meta"])?,
            )?;
            fs::write(root.join("config.json"), b"{\"future\":7}")?;
            let before = fs::read(root.join("config.json"))?;
            let mut originals = Vec::new();
            for (name, value) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(value)?;
                fs::write(&path, &bytes)?;
                originals.push((path, bytes));
            }
            let store = DashboardStore::new(root.clone(), None);
            match store.save(
                case["input"].clone(),
                serde_json::json!({"default_engine":"patchright"})
                    .as_object()
                    .ok_or("Changes missing")?
                    .clone(),
                case.get("expect_rev").filter(|value| !value.is_null()),
                None,
                1700000000.0,
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(value, case["result"], "{}", case["name"]);
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            let mut written = Map::new();
            for kind in ["profiles", "dashboard"] {
                let directory = root.join(kind);
                if !directory.exists() {
                    continue;
                }
                for entry in fs::read_dir(directory)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|ext| ext == "json") {
                        written.insert(
                            format!(
                                "{kind}/{}",
                                path.file_name()
                                    .ok_or("Filename missing")?
                                    .to_str()
                                    .ok_or("Invalid filename")?
                            ),
                            serde_json::from_slice(&fs::read(path)?)?,
                        );
                    }
                }
            }
            assert_eq!(Value::Object(written), case["written"], "{}", case["name"]);
            let after = fs::read(root.join("config.json"))?;
            assert_eq!(Value::Bool(before == after), case["config_unchanged"]);
            assert_eq!(serde_json::from_slice::<Value>(&after)?, case["config"]);
            let unchanged = originals.iter().try_fold(
                true,
                |same, (path, bytes)| -> std::io::Result<bool> {
                    Ok(same && fs::read(path)? == *bytes)
                },
            )?;
            assert_eq!(Value::Bool(unchanged), case["original_unchanged"]);
            Ok(())
        })();
        temporary.cleanup()?;
        result?;
    }
    Ok(())
}
