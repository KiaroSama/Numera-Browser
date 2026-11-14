use numera_core::storage::folders::Folders;
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

#[test]
fn json_filename_selection_matches_original_storage_consumers()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::{dashboard::DashboardStore, first_run};

    let temporary = common::TempDirectory::new("json-filename-selection")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let observed: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/json_filename_selection_matches_original_storage_consumers.json"
    ))?;
    #[cfg(windows)]
    let observed = {
        // Path.glob uses the Windows case-insensitive Unicode matcher.
        // Replay the same authenticated included-file observations for its
        // three additional suffix spellings; immutable Linux data stays put.
        let mut native = observed.clone();
        for case in native["cases"]
            .as_array_mut()
            .ok_or("Filename cases missing")?
        {
            let filename = case["filename"].as_str().ok_or("Filename missing")?;
            if ["b.JSON", "c.JsOn", "long.jſon"].contains(&filename) {
                let reference = observed["cases"]
                    .as_array()
                    .ok_or("Filename cases missing")?
                    .iter()
                    .find(|reference| {
                        reference["operation"] == case["operation"]
                            && reference["filename"] == "a.json"
                    })
                    .ok_or("Included-file observation missing")?;
                *case = serde_json::from_str(
                    &serde_json::to_string(reference)?.replace("a.json", filename),
                )?;
            }
        }
        native
    };
    let cases = observed["cases"]
        .as_array()
        .ok_or("Filename cases missing")?;
    assert_eq!(cases.len(), 50);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let inputs = case["inputs"].as_object().ok_or("Input files missing")?;
        for (relative, raw) in inputs {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
            fs::write(path, serde_json::from_value::<Vec<u8>>(raw.clone())?)?;
        }
        let folders = Folders::new(root.clone());
        let result =
            match case["operation"].as_str() {
                Some("folders") => serde_json::to_value(folders.listing()?)?,
                Some("rename") => serde_json::to_value(folders.rename("source", "target")?)?,
                Some("dashboard") => {
                    serde_json::to_value(DashboardStore::new(root.clone(), None).list()?)?
                }
                Some("first-run") => serde_json::to_value(
                    first_run::import_from(&root, &root.join("destination"))?.profiles,
                )?,
                Some("completion") => serde_json::to_value(
                    numera_core::vault::completion::problems(&root, &observed["descriptor"], None)?,
                )?,
                _ => return Err("Unknown filename operation".into()),
            };
        assert_eq!(
            result, case["result"],
            "JSON filename selection differs: operation={} filename={}",
            case["operation"], case["filename"]
        );
        for (relative, raw) in inputs {
            let after = fs::read(root.join(relative))?;
            assert_eq!(
                serde_json::from_slice::<Value>(&after)?,
                case["after"][relative],
                "Filename operation changed unexpected data"
            );
            assert_eq!(
                after == serde_json::from_value::<Vec<u8>>(raw.clone())?,
                case["unchanged"][relative],
                "Filename operation changed protected bytes"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn malformed_folder_text_refuses_without_replacing_source_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("folder-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/malformed_folder_text_refuses_without_replacing_source_bytes.json"
    ))?;
    assert_eq!(cases.len(), 40, "Folder text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(root.join("dashboard"))?;
        let mut before = Vec::new();
        for (name, raw) in case["inputs"].as_object().ok_or("Inputs missing")? {
            let raw = raw.as_str().ok_or("Input bytes missing")?;
            let bytes = (0..raw.len())
                .step_by(2)
                .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                .collect::<Result<Vec<_>, _>>()?;
            fs::write(root.join(name), &bytes)?;
            before.push((name, bytes));
        }
        let folders = Folders::new(root.clone());
        let outcome = match case["operation"].as_str() {
            Some("listing") => folders.listing(),
            Some("create") => folders.create("new"),
            Some("rename") => folders.rename("source", "target"),
            Some("delete") => folders.delete("source", None),
            _ => return Err("Unknown folder operation".into()),
        };
        assert_eq!(
            outcome.is_ok(),
            case["error"].is_null(),
            "Folder text admission differs: case={index} operation={} location={}",
            case["operation"],
            case["location"]
        );
        match outcome {
            Ok(names) => assert_eq!(serde_json::to_value(names)?, case["result"]),
            Err(error) => {
                assert_eq!(case["error"], "UnicodeDecodeError");
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("UTF-8"));
            }
        }
        for (name, original) in before {
            let after = fs::read(root.join(name))?;
            assert_eq!(
                Value::Bool(after == original),
                case["unchanged"][name],
                "Folder text byte preservation differs: case={index} file={name}"
            );
            if !case["error"].is_null() {
                assert_eq!(after, original, "Refused folder text changed bytes");
            }
            let parsed = serde_json::from_slice::<Value>(&after).unwrap_or(Value::Null);
            assert_eq!(
                parsed, case["written"][name],
                "Folder text state differs: case={index} file={name}"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn folder_name_whitespace_matches_original_operations() -> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("folder-whitespace")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/folder_name_whitespace_matches_original_operations.json"
    ))?;
    assert_eq!(cases.len(), 96, "Folder whitespace observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(root.join("dashboard"))?;
        let mut paths = vec![("dashboard-config.json", root.join("dashboard-config.json"))];
        fs::write(&paths[0].1, serde_json::to_vec(&case["meta"])?)?;
        for (name, value) in case["profiles"].as_object().ok_or("Profiles missing")? {
            let path = root.join("dashboard").join(name);
            fs::write(&path, serde_json::to_vec(value)?)?;
            paths.push((name.as_str(), path));
        }
        let before = paths
            .iter()
            .map(|(_, path)| fs::read(path))
            .collect::<Result<Vec<_>, _>>()?;
        let folders = Folders::new(root);
        let args = case["args"].as_array().ok_or("Folder arguments missing")?;
        let first = args[0].as_str().ok_or("Folder name missing")?;
        let outcome = match case["operation"].as_str() {
            Some("create") => folders.create(first),
            Some("rename") => folders.rename(first, args[1].as_str().ok_or("Target missing")?),
            Some("delete") => folders.delete(first, args.get(1).and_then(Value::as_str)),
            _ => return Err("Unknown folder operation".into()),
        };
        let (result, error) = match outcome {
            Ok(names) => (serde_json::to_value(names)?, Value::Null),
            Err(error) => (Value::Null, Value::String(error.to_string())),
        };
        assert_eq!(
            (result, error),
            (case["result"].clone(), case["error"].clone()),
            "Folder name normalization differs: {} {}",
            case["operation"],
            case["args"]
        );
        for ((name, path), before) in paths.iter().zip(before) {
            let after = fs::read(path)?;
            assert_eq!(
                serde_json::from_slice::<Value>(&after)?,
                case["written"][*name],
                "Folder state differs: case={index} file={name}"
            );
            assert_eq!(
                Value::Bool(before == after),
                case["unchanged"][*name],
                "Folder byte-preservation verdict differs: case={index} file={name}"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn folder_transactions_keep_profiles_and_follow_retired_names()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/folders.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing folder cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("folders-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("dashboard"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("dashboard-config.json"),
                serde_json::to_vec(&case["meta"])?,
            )?;
            let profiles = case["profiles"].as_object().ok_or("Missing profiles")?;
            let mut before = Vec::new();
            for (name, data) in profiles {
                let path = root.join("dashboard").join(name);
                let bytes = serde_json::to_vec(data)?;
                fs::write(&path, &bytes)?;
                before.push((name, path, bytes));
            }
            let folders = Folders::new(root.clone());
            assert_eq!(serde_json::to_value(folders.listing()?)?, case["listed"]);
            let args = case["args"].as_array().ok_or("Missing arguments")?;
            let first = args[0].as_str().ok_or("Missing name")?;
            let changed = match case["operation"].as_str() {
                Some("create") => folders.create(first),
                Some("rename") => folders.rename(first, args[1].as_str().ok_or("Missing target")?),
                Some("delete") => folders.delete(first, args.get(1).and_then(Value::as_str)),
                _ => return Err("Unknown folder operation".into()),
            };
            match changed {
                Ok(names) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(names)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (name, path, bytes) in before {
                let after = fs::read(path)?;
                assert_eq!(
                    after == bytes,
                    case["unchanged"][name]
                        .as_bool()
                        .ok_or("Missing byte verdict")?
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][name]
                );
            }
            assert_eq!(
                serde_json::from_slice::<Value>(&fs::read(root.join("dashboard-config.json"))?)?,
                case["metadata"]
            );
            for (name, expected) in case["resolved"]
                .as_object()
                .ok_or("Missing resolved names")?
            {
                assert_eq!(
                    folders.resolve_name(name)?,
                    expected.as_str().ok_or("Invalid resolved name")?
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
