use numera_core::storage::data_root::resolve;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn explicit_environment_portable_and_legacy_roots_follow_source_precedence()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/data-root.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing root cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("data-root-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("app"))?;
        fs::create_dir_all(root.join("home"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            if case["blocked"] == true {
                fs::write(root.join("app/data"), "fixture obstruction")?;
            }
            if case["current"] == true {
                fs::create_dir(root.join("home/.numera"))?;
            }
            if case["legacy"] == true {
                fs::create_dir(root.join("home/.persona"))?;
            }
            let explicit = case["explicit"].as_str().map(|s| root.join(s));
            let environment = case["environment"].as_str().map(|s| root.join(s));
            let path = resolve(
                explicit.as_deref(),
                environment.as_deref(),
                &root.join("app"),
                &root.join("home"),
            )?;
            assert_eq!(
                path,
                root.join(case["result"].as_str().ok_or("Root outcome missing")?)
            );
            assert_eq!(
                root.join("app/data").is_dir(),
                case["portable_created"]
                    .as_bool()
                    .ok_or("Probe side effect missing")?
            );
            assert!(
                !fs::read_dir(root.join("app"))?.any(|entry| entry
                    .is_ok_and(|entry| entry.file_name().to_string_lossy().starts_with(".wtest"))),
                "Write probe left a file"
            );
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
