use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{fs, path::PathBuf};
mod common;
#[path = "repo_lookup_parity/read_warnings.rs"]
mod read_warnings;

#[test]
fn direct_name_lookup_matches_original_without_id_precedence()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("direct-name-lookup")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/direct_name_lookup_matches_original_without_id_precedence.json"
    ))?;
    assert_eq!(cases.len(), 14);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let files = case["files"].as_object().ok_or("Lookup files missing")?;
        let mut before = Map::new();
        for (name, value) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Lookup parent missing")?)?;
            let bytes = serde_json::to_vec(value)?;
            fs::write(path, &bytes)?;
            before.insert(name.clone(), serde_json::json!(bytes));
        }
        let repo = ProfileRepo::new(root.clone(), None);
        let actual = repo.find_by_name(
            case["reference"].as_str().ok_or("Lookup name missing")?,
            1700000000.0,
        );
        match actual {
            Ok(found) => {
                assert!(
                    case["error"].is_null(),
                    "Direct name lookup unexpectedly succeeded: {index}"
                );
                assert_eq!(
                    serde_json::to_value(found)?,
                    case["result"],
                    "Direct name lookup differs: {index}"
                );
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "Direct name refusal differs: {index}"
            ),
        }
        let mut disk = Map::new();
        for kind in ["profiles", "dashboard"] {
            let directory = root.join(kind);
            if !directory.exists() {
                continue;
            }
            for entry in fs::read_dir(directory)? {
                let path = entry?.path();
                if path.extension().is_none_or(|value| value != "json") {
                    continue;
                }
                let name = format!(
                    "{kind}/{}",
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .ok_or("Lookup filename missing")?
                );
                disk.insert(name, serde_json::from_slice(&fs::read(path)?)?);
            }
        }
        assert_eq!(
            Value::Object(disk),
            case["disk"],
            "Direct name lookup disk differs: {index}"
        );
        for (name, bytes) in before {
            assert_eq!(
                Value::Bool(serde_json::json!(fs::read(root.join(&name))?) == bytes),
                case["unchanged"][&name],
                "Direct name preservation differs: {index}"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn damaged_profile_text_refuses_before_repair_or_replacement()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        models::Profile,
        storage::{dashboard::DashboardStore, repo::RepoError},
    };

    let temporary = common::TempDirectory::new("profile-text-admission")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let document: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/damaged_profile_text_refuses_before_repair_or_replacement.json"
    ))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Profile text cases missing")?;
    assert_eq!(cases.len(), 52, "Profile text observations missing");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["error"] == "UnicodeDecodeError")
            .count(),
        48,
        "Original damaged profile refusals missing"
    );
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let paths = [
            root.join("profiles/fixture00001.json"),
            root.join("dashboard/fixture00001.json"),
        ];
        for (path, bytes) in paths
            .iter()
            .zip(case["files"].as_array().ok_or("Input bytes missing")?)
        {
            fs::create_dir_all(path.parent().ok_or("Profile parent missing")?)?;
            let expected = serde_json::from_value::<Vec<u8>>(bytes.clone())?;
            fs::write(path, &expected)?;
            assert_eq!(fs::read(path)?, expected, "Profile text input bytes differ");
        }
        let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
        let repo = ProfileRepo::new(root.clone(), None);
        let dashboard = DashboardStore::new(root, None);
        let time = 1700000000.0;
        let result = match case["operation"].as_str() {
            Some("materialize") => repo
                .materialize("fixture00001", time)
                .map(serde_json::to_value),
            Some("adopt") => repo.adopt("fixture00001", time).map(serde_json::to_value),
            Some("coherent") => repo
                .coherent("fixture00001", time)
                .map(serde_json::to_value),
            Some("resolve") => repo.resolve("fixture00001", time).map(serde_json::to_value),
            Some("list") => repo.list(time).map(serde_json::to_value),
            Some("save") => repo
                .save(Profile::from_record(&document["profile"])?, time)
                .map(serde_json::to_value),
            Some("dash-save") => dashboard
                .save(document["dashboard"].clone(), Map::new(), None, None, time)
                .map(serde_json::to_value),
            Some("dash-patch") => dashboard
                .patch(
                    "fixture00001",
                    |mut record| {
                        record["notes"] = "changed".into();
                        Ok((record, Map::new()))
                    },
                    None,
                    None,
                    time,
                )
                .map(serde_json::to_value),
            _ => return Err("Unknown profile text operation".into()),
        };
        let invalid = matches!(&result, Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Profile text admission differs: case={index} operation={}",
            case["operation"]
        );
        if !invalid {
            assert_eq!(
                result??, case["result"],
                "Profile text result differs: case={index}"
            );
        }
        for (position, (path, before)) in paths.iter().zip(before).enumerate() {
            let after = fs::read(path)?;
            assert_eq!(
                Value::Bool(before == after),
                case["unchanged"][position],
                "Profile text byte preservation differs: case={index}"
            );
            if invalid {
                assert_eq!(
                    after, before,
                    "Damaged profile bytes replaced: case={index}"
                );
            } else if case["unchanged"][position] == false {
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][position],
                    "Valid UTF8 fallback state differs: case={index}"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

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

#[test]
fn ordinary_encrypted_reads_preserve_original_bytes_and_repair_only_real_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("encrypted read parity")?;
    // Immutable original observation; native operations and full assertions remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/native-storage-characterization/ordinary_encrypted_reads_preserve_original_bytes_and_repair_only_real_drift.json"
    ))?;
    assert_eq!(cases.len(), 26, "Encrypted read observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("dashboard"))?;
        fs::write(
            root.join("config.json"),
            serde_json::to_vec(&case["config"])?,
        )?;
        let paths = [
            root.join("profiles/fixture00001.json"),
            root.join("dashboard/fixture00001.json"),
        ];
        for (path, key) in paths.iter().zip(["engine", "dashboard"]) {
            fs::write(path, serde_json::to_vec(&case[key])?)?;
        }
        let before = paths.iter().map(fs::read).collect::<Result<Vec<_>, _>>()?;
        let repo = ProfileRepo::new(root, case["password"].as_str().map(str::to_owned));
        let actual = if case["coherent"] == true {
            repo.coherent("fixture00001", 1700000000.0)
                .map(serde_json::to_value)
        } else if case["operation"] == "resolve" {
            repo.resolve("fixture00001", 1700000000.0)
                .map(serde_json::to_value)
        } else {
            repo.list(1700000000.0).map(serde_json::to_value)
        };
        for (position, path) in paths.iter().enumerate() {
            let after = fs::read(path)?;
            assert_eq!(
                Value::Bool(before[position] == after),
                case["unchanged"][position],
                "Ordinary encrypted read rewrote protected bytes: {}",
                case["name"]
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&after)?,
                case["written"][position],
                "{}",
                case["name"]
            );
        }
        match actual {
            Ok(value) => {
                assert!(case["error"].is_null(), "{}", case["name"]);
                assert_eq!(value?, case["result"], "{}", case["name"]);
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "{}",
                case["name"]
            ),
        }
    }
    temporary.cleanup()?;
    Ok(())
}
