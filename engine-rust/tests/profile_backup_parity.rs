use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{fs, io::Read, path::PathBuf};

#[test]
fn backup_carries_complete_profile_and_session_or_refuses_before_output()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/profile-backup.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Backup cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("backup-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            if case["missing"] != true {
                fs::write(
                    root.join("profiles/fixture-backup.json"),
                    serde_json::to_vec(&case["profile"])?,
                )?;
            }
            for (name, value) in case["session"].as_object().ok_or("Session missing")? {
                let path = root.join("user-data/fixture-backup").join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    STANDARD.decode(value.as_str().ok_or("File bytes missing")?)?,
                )?;
            }
            if case["damaged_claim"] == true {
                fs::create_dir_all(root.join("sessions"))?;
                fs::write(root.join("sessions/fixture-backup.json"), b"{ broken")?;
            }
            let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
            let dest = root.join("backup.zip");
            match repo.backup("fixture-backup", &dest, "fixture-host") {
                Ok(value) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(Value::Bool(value.is_some()), case["result"]);
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            assert_eq!(Value::Bool(dest.exists()), case["destination_present"]);
            if dest.exists() {
                let mut zip = zip::ZipArchive::new(fs::File::open(dest)?)?;
                let mut members = Map::new();
                for index in 0..zip.len() {
                    let mut file = zip.by_index(index)?;
                    let name = file.name()?.into_owned();
                    let mut bytes = Vec::new();
                    file.read_to_end(&mut bytes)?;
                    let value = if name == "profile.json" {
                        serde_json::from_slice(&bytes)?
                    } else {
                        Value::String(STANDARD.encode(bytes))
                    };
                    members.insert(name, value);
                }
                assert_eq!(Value::Object(members), case["members"], "{}", case["name"]);
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn backup_refuses_its_own_session_destination_without_changing_existing_output()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/profile-backup.json"))?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("backup-placement-{}", std::process::id()));
    fs::create_dir_all(root.join("profiles"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(
            root.join("profiles/fixture-backup.json"),
            serde_json::to_vec(&oracle["cases"][0]["profile"])?,
        )?;
        let session = root.join("user-data/fixture-backup");
        fs::create_dir_all(&session)?;
        let destination = session.join("backup.zip");
        fs::write(&destination, b"existing archive must survive")?;
        let repo = ProfileRepo::new(root.clone(), None);
        assert!(
            repo.backup("fixture-backup", &destination, "fixture-host")
                .is_err()
        );
        assert_eq!(fs::read(&destination)?, b"existing archive must survive");
        assert_eq!(fs::read_dir(&session)?.count(), 1);
        // Relative paths retain caller-CWD semantics without mutating global CWD.
        let relative_root = root.strip_prefix(std::env::current_dir()?)?;
        let relative = relative_root.join("outside.zip");
        assert_eq!(
            repo.backup("fixture-backup", &relative, "fixture-host")?,
            Some(relative)
        );
        assert!(root.join("outside.zip").is_file());
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
