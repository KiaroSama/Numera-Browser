use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::Value;
use std::{fs, path::PathBuf};
mod common;

fn directory_access(
    path: &std::path::Path,
    denied: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if denied { 0o500 } else { 0o700 }),
        )?;
    }
    #[cfg(windows)]
    {
        use numera_core::ownership::capture;
        use std::{process::Command, time::Duration};
        let mut command = Command::new("icacls");
        command.arg(path);
        if denied {
            command.args(["/deny", "*S-1-1-0:(AD)"]);
        } else {
            command.args(["/remove:d", "*S-1-1-0"]);
        }
        let output = capture::run(
            &mut command,
            capture::Limits {
                wall: Duration::from_secs(10),
                idle: Duration::from_secs(5),
                bytes: 65536,
                cleanup: Duration::from_secs(3),
            },
        )?;
        if output.exit != 0 {
            return Err("Owned fixture directory permission change failed".into());
        }
    }
    Ok(())
}

fn expected_directory_message(
    case: &Value,
    root: &std::path::Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let old = case["root"].as_str().ok_or("Original root missing")?;
    let directory = if case["existing"] == true {
        "user-data"
    } else {
        "profiles"
    };
    let mut expected = case["message"]
        .as_str()
        .ok_or("Original diagnostic missing")?
        .replace(
            &format!("{old}/{directory}"),
            &root.join(directory).to_string_lossy(),
        )
        .replace(old, &root.to_string_lossy());
    let begin = expected
        .find(" (it is owned by uid ")
        .ok_or("Original owner diagnostic missing")?;
    let end = begin
        + expected[begin..]
            .find("). Point ")
            .ok_or("Original owner diagnostic ending missing")?
        + 1;
    #[cfg(unix)]
    let owner = {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(root)?;
        // SAFETY: these functions only observe the calling process identities.
        #[allow(unsafe_code)]
        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
        format!(
            " (it is owned by uid {}:{}, this process runs as uid {uid}:{gid})",
            metadata.uid(),
            metadata.gid()
        )
    };
    #[cfg(not(unix))]
    let owner = String::new();
    expected.replace_range(begin..end, &owner);
    Ok(expected)
}

#[test]
fn denied_store_directories_match_original_actionable_errors_and_preserve_state()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::repo::RepoError;
    use std::{error::Error, io};

    let temporary = common::TempDirectory::new("store-directory-errors")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/denied_store_directories_match_original_actionable_errors_and_preserve_state.json"
    ))?;
    assert_eq!(cases.len(), 2);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native path {index}"));
        fs::create_dir(&root)?;
        let existing = case["existing"]
            .as_bool()
            .ok_or("Directory fixture missing")?;
        if existing {
            fs::create_dir(root.join("profiles"))?;
        }
        let protected = root.join("config.json");
        fs::write(&protected, b"{\"untouched\":true}")?;
        let expected = expected_directory_message(case, &root)?;
        let denied = (|| -> Result<_, Box<dyn Error>> {
            directory_access(&root, true)?;
            let directory = if existing { "user-data" } else { "profiles" };
            let kernel = fs::create_dir_all(root.join(directory))
                .err()
                .ok_or("Fixture did not deny independent directory creation")?;
            Ok((ProfileRepo::open(root.clone(), None, None), kernel))
        })();
        // Restore before any assertion can unwind through fixture cleanup.
        directory_access(&root, false)?;
        let (denied, kernel) = denied?;
        let error = denied
            .err()
            .ok_or("Fixture did not cause native permission denial")?;
        assert_eq!(kernel.kind(), io::ErrorKind::PermissionDenied);
        #[cfg(unix)]
        assert_eq!(
            kernel.raw_os_error().map(i64::from),
            case["code"].as_i64(),
            "Frozen POSIX denial code differs"
        );
        assert_eq!(
            error.to_string(),
            expected,
            "Store directory permission diagnostic differs"
        );
        let cause = error
            .source()
            .and_then(|cause| cause.downcast_ref::<io::Error>())
            .ok_or("Original directory I/O cause was lost")?;
        assert_eq!(cause.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            cause.raw_os_error().map(i64::from),
            kernel.raw_os_error().map(i64::from),
            "Native OS denial cause differs from independent kernel observation"
        );
        assert_eq!(fs::read(&protected)?, b"{\"untouched\":true}");
        assert_eq!(root.join("profiles").exists(), existing);
        assert!(!root.join("user-data").exists());
        ProfileRepo::open(root.clone(), None, None)?;
        assert!(root.join("profiles").is_dir() && root.join("user-data").is_dir());
        assert_eq!(fs::read(&protected)?, b"{\"untouched\":true}");
    }
    let obstructed = temporary.path().join("not a directory");
    fs::write(&obstructed, b"preserved obstruction")?;
    let expected = fs::create_dir_all(obstructed.join("profiles"))
        .err()
        .ok_or("Obstruction accepted")?;
    let error = ProfileRepo::open(obstructed.clone(), None, None)
        .err()
        .ok_or("Store obstruction accepted")?;
    assert!(
        matches!(error, RepoError::Io(ref cause) if cause.kind() == expected.kind()
                    && cause.raw_os_error() == expected.raw_os_error())
    );
    assert_eq!(fs::read(obstructed)?, b"preserved obstruction");
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn opening_a_store_recovers_before_exposing_credentials_and_refuses_late_damage()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/recovery.json"))?;
    let case = &fixture["cases"][4];
    assert_eq!(case["result"], true);
    let profile_fixture: Value = serde_json::from_str(include_str!("fixtures/models.json"))?;
    let record = profile_fixture["cases"]
        .as_array()
        .ok_or("Model cases missing")?
        .iter()
        .find(|case| case["operation"] == "profile")
        .ok_or("Profile case missing")?["expected"]
        .clone();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("store-open-{}", std::process::id()));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(
            root.join("vault-migration.json"),
            serde_json::to_vec(&case["journal"])?,
        )?;
        fs::write(root.join("config.json"), b"{}")?;
        fs::write(
            root.join("proxies.json"),
            serde_json::to_vec(&case["files"]["proxies.json"])?,
        )?;
        assert!(
            ProfileRepo::open(root.clone(), None, None).is_err(),
            "Half-migrated root opened without its key"
        );
        assert!(
            root.join("vault-migration.json").exists(),
            "Failed open destroyed recovery evidence"
        );
        let repo = ProfileRepo::open(root.clone(), Some("fixture-recovery-master".into()), None)?;
        assert!(!root.join("vault-migration.json").exists());
        let mut fresh = Profile::from_record(&record)?;
        let secret = record["proxy"]["password"]
            .as_str()
            .ok_or("Fixture secret missing")?;
        fresh
            .proxy
            .as_mut()
            .ok_or("Fixture proxy missing")?
            .set_password(Some(secret.into()));
        let saved = repo.save(fresh, 1700000000.0)?;
        let read = repo.get(&saved.id)?.ok_or("Saved record missing")?;
        assert_eq!(
            serde_json::to_value(read)?,
            serde_json::to_value(saved)?,
            "Opened vault changed the credential-aware view"
        );
        let before = fs::read(root.join("profiles").join(format!(
            "{}.json",
            record["id"].as_str().ok_or("ID missing")?
        )))?;
        fs::write(root.join("vault-migration.json"), b"{broken")?;
        assert!(
            repo.get(record["id"].as_str().ok_or("ID missing")?)
                .is_err(),
            "Already-open store read through unresolved migration"
        );
        assert_eq!(
            before,
            fs::read(root.join("profiles").join(format!(
                "{}.json",
                record["id"].as_str().ok_or("ID missing")?
            )))?
        );
        assert_eq!(fs::read(root.join("vault-migration.json"))?, b"{broken");
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
