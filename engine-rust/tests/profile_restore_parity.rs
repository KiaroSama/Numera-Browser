use base64::{Engine, engine::general_purpose::STANDARD};
use numera_core::storage::repo::ProfileRepo;
use serde_json::{Map, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn session_files(base: &Path, dir: &Path, found: &mut Map<String, Value>) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            session_files(base, &path, found)?;
        } else {
            let name = path
                .strip_prefix(base)
                .map_err(std::io::Error::other)?
                .to_str()
                .ok_or_else(|| std::io::Error::other("Invalid session path"))?
                .replace('\\', "/");
            found.insert(name, Value::String(STANDARD.encode(fs::read(path)?)));
        }
    }
    Ok(())
}

#[test]
fn restore_stages_then_replaces_session_and_commits_both_records()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/profile-restore.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Restore cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("restore-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            for (name, value) in case["initial"]
                .as_object()
                .ok_or("Initial records missing")?
            {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(path, serde_json::to_vec(value)?)?;
            }
            for (name, value) in case["initial_session"]
                .as_object()
                .ok_or("Initial session missing")?
            {
                let path = root.join("user-data").join(name);
                fs::create_dir_all(path.parent().ok_or("Parent missing")?)?;
                fs::write(
                    path,
                    STANDARD.decode(value.as_str().ok_or("File bytes missing")?)?,
                )?;
            }
            if case["damaged_claim"] == true {
                fs::create_dir_all(root.join("sessions"))?;
                fs::write(root.join("sessions/fixture-restore.json"), b"{ broken")?;
            }
            let source = root.join("incoming.zip");
            fs::write(
                &source,
                STANDARD.decode(case["archive"].as_str().ok_or("ZIP bytes missing")?)?,
            )?;
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.restore(&source, false, "fixture-host", 1700000000.0) {
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
                let dir = root.join(kind);
                if !dir.exists() {
                    continue;
                }
                for entry in fs::read_dir(dir)? {
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
            let mut session = Map::new();
            session_files(
                &root.join("user-data"),
                &root.join("user-data"),
                &mut session,
            )?;
            assert_eq!(Value::Object(session), case["session"], "{}", case["name"]);
            assert!(!fs::read_dir(&root)?.any(|entry| entry.is_ok_and(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("numera-restore-")
            })));
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn failed_metadata_commit_restores_old_session_and_new_id_keeps_original()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/profile-restore.json"))?;
    let case = &oracle["cases"][0];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("restore-rollback-{}", std::process::id()));
    fs::create_dir_all(root.join("profiles"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let source = root.join("incoming.zip");
        fs::write(
            &source,
            STANDARD.decode(case["archive"].as_str().ok_or("ZIP bytes missing")?)?,
        )?;
        let repo = ProfileRepo::new(root.clone(), None);
        let original = repo.restore(&source, false, "fixture-host", 1700000000.0)?;
        let session = root.join("user-data").join(&original.id);
        fs::write(session.join("old.txt"), b"previous session must survive")?;
        let original_bytes = fs::read(root.join("profiles").join(format!("{}.json", original.id)))?;
        // A real directory obstructs the second metadata write after the session swap.
        let dashboard = root.join("dashboard").join(format!("{}.json", original.id));
        fs::remove_file(&dashboard)?;
        fs::create_dir(&dashboard)?;
        assert!(
            repo.restore(&source, false, "fixture-host", 1700000001.0)
                .is_err()
        );
        assert_eq!(
            fs::read(session.join("old.txt"))?,
            b"previous session must survive"
        );
        assert_eq!(
            fs::read(root.join("profiles").join(format!("{}.json", original.id)))?,
            original_bytes
        );
        assert!(dashboard.is_dir());
        fs::remove_dir(&dashboard)?;
        let copy = repo.restore(&source, true, "fixture-host", 1700000002.0)?;
        assert_ne!(copy.id, original.id);
        assert_eq!(copy.id.len(), 12);
        assert!(copy.id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(copy.name, "restore α (copy)");
        assert_eq!(
            fs::read(
                root.join("user-data")
                    .join(&copy.id)
                    .join("Default/Cookies")
            )?,
            b"fixture-new-session"
        );
        assert_eq!(
            fs::read(session.join("old.txt"))?,
            b"previous session must survive"
        );
        assert_eq!(
            fs::read(root.join("profiles").join(format!("{}.json", original.id)))?,
            original_bytes
        );
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result?;
    Ok(())
}
