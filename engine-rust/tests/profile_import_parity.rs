use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};

#[test]
fn imported_profile_uses_shared_dual_store_validation_and_names()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-import.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Import cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("import-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut before = Vec::new();
            for (name, value) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                let bytes = serde_json::to_vec(value)?;
                fs::write(&path, &bytes)?;
                before.push((path, bytes));
            }
            let source = root.join("incoming.json");
            fs::write(&source, serde_json::to_vec(&case["input"])?)?;
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.import_file(&source, 1700000000.0) {
                Ok(profile) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(
                        serde_json::to_value(profile)?,
                        case["result"],
                        "{}",
                        case["name"]
                    );
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
                        let name = format!(
                            "{kind}/{}",
                            path.file_name()
                                .ok_or("Filename missing")?
                                .to_str()
                                .ok_or("Invalid filename")?
                        );
                        written.insert(name, serde_json::from_slice(&fs::read(path)?)?);
                    }
                }
            }
            assert_eq!(Value::Object(written), case["written"], "{}", case["name"]);
            let unchanged =
                before
                    .iter()
                    .try_fold(true, |same, (path, bytes)| -> std::io::Result<bool> {
                        Ok(same && fs::read(path)? == *bytes)
                    })?;
            assert_eq!(Value::Bool(unchanged), case["original_unchanged"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
