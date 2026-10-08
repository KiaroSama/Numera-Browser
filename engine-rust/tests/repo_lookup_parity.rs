use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};

#[test]
fn lookup_reconciles_both_copies_and_refuses_ambiguous_names()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-lookup.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing lookup cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("lookup-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (name, data) in case["files"].as_object().ok_or("Files missing")? {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    if data == "broken-json" {
                        b"{broken".to_vec()
                    } else {
                        serde_json::to_vec(data)?
                    },
                )?;
            }
            let repo = ProfileRepo::new(root.clone(), None);
            let reference = case["reference"].as_str().unwrap_or("");
            let actual = match case["operation"].as_str() {
                Some("resolve") => repo.resolve(reference, 1700000000.0).and_then(|v| {
                    serde_json::to_value(v).map_err(|e| {
                        numera_core::storage::repo::RepoError::from(std::io::Error::other(e))
                    })
                }),
                Some("coherent") => repo.coherent(reference, 1700000000.0).and_then(|v| {
                    serde_json::to_value(v).map_err(|e| {
                        numera_core::storage::repo::RepoError::from(std::io::Error::other(e))
                    })
                }),
                Some("list") => repo.list(1700000000.0).and_then(|v| {
                    serde_json::to_value(v).map_err(|e| {
                        numera_core::storage::repo::RepoError::from(std::io::Error::other(e))
                    })
                }),
                _ => return Err("Unknown lookup operation".into()),
            };
            match actual {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(value, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            let mut written = Map::new();
            for kind in ["profiles", "dashboard"] {
                let directory = root.join(kind);
                if !directory.exists() {
                    continue;
                }
                for entry in fs::read_dir(directory)? {
                    let path = entry?.path();
                    if path.extension().is_none_or(|v| v != "json") {
                        continue;
                    }
                    let name = format!(
                        "{kind}/{}",
                        path.file_name()
                            .ok_or("Filename missing")?
                            .to_str()
                            .ok_or("Filename not Unicode")?
                    );
                    let value = serde_json::from_slice::<Value>(&fs::read(path)?)
                        .unwrap_or(Value::String("broken-json".into()));
                    written.insert(name, value);
                }
            }
            assert_eq!(Value::Object(written), case["written"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
