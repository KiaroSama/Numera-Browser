use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;
#[path = "support/platform_text.rs"]
mod platform_text;

#[test]
fn withheld_text_admission_matches_original_without_replacing_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::repo::RepoError;

    let temporary = common::TempDirectory::new("withheld-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/withheld_text_admission_matches_original_without_replacing_bytes.json"
    ))?;
    assert_eq!(rows.len(), 14);
    for (index, row) in rows.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let config: Vec<u8> = serde_json::from_value(row["config"].clone())?;
        fs::write(root.join("config.json"), &config)?;
        let path = root.join("profiles/fixture-export.json");
        let raw: Option<Vec<u8>> = serde_json::from_value(row["raw"].clone())?;
        if let Some(bytes) = &raw {
            fs::write(&path, bytes)?;
        }
        let repo = ProfileRepo::new(root.clone(), row["password"].as_str().map(str::to_owned));
        let result = repo.secret_withheld("fixture-export");
        if row["error"] == "UnicodeDecodeError" {
            assert!(
                matches!(result, Err(RepoError::Io(ref error))
                if error.kind() == std::io::ErrorKind::InvalidData),
                "Withheld text admission differs: {index}"
            );
        } else {
            assert_eq!(
                serde_json::json!(result?),
                row["result"],
                "Withheld credential result differs: {index}"
            );
        }
        assert_eq!(
            fs::read(&path).ok(),
            raw,
            "Withheld check replaced stored bytes"
        );
        assert_eq!(fs::read(root.join("config.json"))?, config);
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn exported_profile_bytes_match_original_text_format_and_preserve_refusals()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("profile-export-bytes")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/exported_profile_bytes_match_original_text_format_and_preserve_refusals.json"
    ))?;
    assert_eq!(rows.len(), 13);
    for (index, row) in rows.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("profiles"))?;
        let mut before = Vec::new();
        for (name, raw) in row["files"].as_object().ok_or("Export inputs missing")? {
            let path = root.join(name);
            let bytes: Vec<u8> = serde_json::from_value(raw.clone())?;
            fs::write(&path, &bytes)?;
            before.push((path, bytes));
        }
        let dest = root.join("export.json");
        fs::write(&dest, b"preserve destination\x00\xff")?;
        let repo = ProfileRepo::new(root, row["password"].as_str().map(str::to_owned));
        match repo.export("fixture-export", &dest) {
            Ok(result) => {
                assert!(row["error"].is_null(), "Export refusal missing: {index}");
                assert_eq!(serde_json::json!(result.is_some()), row["result"]);
                assert_eq!(result, (row["result"] == true).then(|| dest.clone()));
                assert_eq!(repo.export("fixture-export", &dest)?, result);
            }
            Err(error) => assert_eq!(Some(error.to_string().as_str()), row["error"].as_str()),
        }
        let expected: Vec<u8> = serde_json::from_value(row["bytes"].clone())?;
        let expected = if row["result"] == true {
            platform_text::written_bytes(expected)
        } else {
            expected
        };
        assert_eq!(
            fs::read(dest)?,
            expected,
            "Profile export bytes differ: {index}"
        );
        for (path, bytes) in before {
            assert_eq!(fs::read(path)?, bytes);
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn profile_export_refuses_to_drop_withheld_credentials() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/profile-export.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("export-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            if case["missing"] != true {
                fs::write(
                    root.join("profiles/fixture-export.json"),
                    serde_json::to_vec(&case["profile"])?,
                )?;
            }
            let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
            let dest = root.join("export.json");
            match repo.export("fixture-export", &dest) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value.is_some()), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            assert_eq!(
                dest.exists(),
                case["destination_present"]
                    .as_bool()
                    .ok_or("Destination verdict missing")?
            );
            if dest.exists() {
                assert_eq!(
                    serde_json::from_slice::<Value>(&fs::read(dest)?)?,
                    case["written"]
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
