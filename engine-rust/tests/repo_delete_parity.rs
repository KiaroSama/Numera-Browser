use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;

#[test]
fn direct_session_paths_and_deletion_match_original_without_touching_profiles()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("direct-session-storage")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/direct_session_paths_and_deletion_match_original_without_touching_profiles.json"
    ))?;
    assert_eq!(rows.len(), 19);
    // The current source checkpoint resolves roots before its containment
    // checks; keep that verified safety upgrade separate from the 13b data.
    let current_paths: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/native-storage-paths/current-root-resolution.json"
    ))?;
    assert_eq!(current_paths.len(), 2);

    for (index, row) in rows.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("user-data"))?;
        let repo = ProfileRepo::new(root.clone(), None);
        if row["op"] == "invalid-delete" {
            fs::write(root.join("sentinel"), b"keep")?;
            match repo.delete_session(
                row["id"].as_str().ok_or("Session ID missing")?,
                "fixture-host",
            ) {
                Ok(()) => {
                    assert!(row["error"].is_null());
                    assert!(row["result"].is_null());
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), row["error"].as_str()),
            }
            assert_eq!(
                serde_json::json!(fs::read(root.join("sentinel"))?),
                row["sentinel"]
            );
            assert!(fs::read_dir(root.join("user-data"))?.next().is_none());
            assert_eq!(row["directories"], serde_json::json!([]));
            continue;
        }
        if row["op"] == "path-root" {
            let row = current_paths
                .iter()
                .find(|expected| expected["mode"] == row["mode"])
                .ok_or("Current root resolution observation missing")?;
            let argument = if row["mode"] == "relative" {
                root.strip_prefix(common::canonical_path(&std::env::current_dir()?)?)?
                    .to_path_buf()
            } else {
                fs::create_dir(root.join("alias"))?;
                root.join("alias/../actual")
            };
            let path =
                ProfileRepo::new(argument.clone(), None).user_data_path("fixture-profile")?;
            assert_eq!(serde_json::json!(path.is_absolute()), row["absolute"]);
            assert_eq!(
                serde_json::json!(path == argument.join("user-data/fixture-profile")),
                row["same"]
            );
            assert_eq!(serde_json::json!(path.is_dir()), row["directory"]);
            continue;
        }
        if row["op"] == "path" {
            match repo.user_data_path(row["id"].as_str().ok_or("Session ID missing")?) {
                Ok(path) => {
                    assert!(
                        row["error"].is_null(),
                        "Direct session path unexpectedly succeeded"
                    );
                    assert_eq!(
                        serde_json::json!(
                            path.strip_prefix(&root)?
                                .to_string_lossy()
                                .replace('\\', "/")
                        ),
                        row["result"]
                    );
                    assert!(path.is_dir());
                    assert_eq!(
                        repo.user_data_path(row["id"].as_str().ok_or("Session ID missing")?)?,
                        path
                    );
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), row["error"].as_str()),
            }
            let mut names = fs::read_dir(root.join("user-data"))?
                .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
                .collect::<Result<Vec<_>, _>>()?;
            names.sort();
            assert_eq!(serde_json::json!(names), row["directories"]);
            continue;
        }
        let mut originals = Vec::new();
        for (name, raw) in row["files"]
            .as_object()
            .ok_or("Session input files missing")?
        {
            let bytes: Vec<u8> = serde_json::from_value(raw.clone())?;
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Session parent missing")?)?;
            fs::write(&path, &bytes)?;
            originals.push((path, bytes));
        }
        if row["mode"] != "absent" {
            let path = repo.user_data_path("fixture-profile")?.join("nested");
            fs::create_dir(&path)?;
            fs::write(path.join("session.bin"), b"\x00\xff\n\r\n")?;
        }
        let result = repo
            .delete_session("fixture-profile", "fixture-host")
            .and_then(|()| repo.delete_session("fixture-profile", "fixture-host"));
        match result {
            Ok(()) => assert!(
                row["error"].is_null(),
                "Direct session deletion unexpectedly succeeded"
            ),
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                row["error"].as_str(),
                "Direct session refusal differs"
            ),
        }
        for (path, bytes) in originals {
            assert_eq!(fs::read(path)?, bytes, "Session deletion changed metadata");
        }
        assert_eq!(row["unchanged"], true);
        let session = root.join("user-data/fixture-profile");
        assert_eq!(serde_json::json!(session.exists()), row["session"]);
        let binary = session.join("nested/session.bin");
        let bytes = if binary.exists() {
            serde_json::json!(fs::read(binary)?)
        } else {
            Value::Null
        };
        assert_eq!(bytes, row["binary"]);
        assert!(
            row["tombs"]
                .as_array()
                .ok_or("Session tombstones missing")?
                .is_empty()
        );
        assert!(!fs::read_dir(root.join("user-data"))?.any(|entry| {
            entry.is_ok_and(|entry| entry.file_name().to_string_lossy().contains(".deleting-"))
        }));
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn deletion_removes_both_copies_and_refuses_unreadable_ownership()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo-delete.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing delete cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("delete-{}-{index}", std::process::id()));
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("user-data/fixture-delete"))?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("profiles/fixture-delete.json"),
                serde_json::to_vec(&case["profile"])?,
            )?;
            fs::write(
                root.join("user-data/fixture-delete/fixture-state.txt"),
                "isolated browser state α",
            )?;
            if case["corrupt_claim"] == true {
                fs::create_dir_all(root.join("sessions"))?;
                fs::write(root.join("sessions/fixture-delete.json"), b"{broken")?;
            }
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.delete(
                case["reference"].as_str().ok_or("Reference missing")?,
                "fixture-host",
                1700000000.0,
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(Value::Bool(value), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (key, path) in [
                ("engine_present", "profiles/fixture-delete.json"),
                ("dashboard_present", "dashboard/fixture-delete.json"),
                ("session_present", "user-data/fixture-delete"),
                ("claim_retained", "sessions/fixture-delete.json"),
            ] {
                assert_eq!(
                    root.join(path).exists(),
                    case[key].as_bool().ok_or("Missing side effect")?
                );
            }
            assert!(
                !fs::read_dir(root.join("user-data"))?.any(|entry| entry
                    .is_ok_and(|entry| entry.file_name().to_string_lossy().contains(".deleting-"))),
                "Unreclaimed synthetic tombstone"
            );
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
