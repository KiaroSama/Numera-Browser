use numera_core::{ownership::capture, storage::first_run::run_if_needed};
use serde_json::{Map, Value};
use std::{fs, path::Path, process::Command, time::Duration};

fn snapshot(base: &Path, directory: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let mut result = Map::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .strip_prefix(base)?
            .to_str()
            .ok_or("NonUnicode fixture path")?
            .replace('\\', "/");
        if name == ".imported" || name.split('/').any(|part| part == ".locks") {
            continue;
        }
        if path.is_dir() {
            result.insert(name, Value::Null);
            result.extend(
                snapshot(base, &path)?
                    .as_object()
                    .ok_or("Snapshot missing")?
                    .clone(),
            );
        } else {
            result.insert(name, serde_json::to_value(fs::read(path)?)?);
        }
    }
    Ok(Value::Object(result))
}

#[test]
fn new_subtree_errors_match_original_continuation_summaries_and_retry()
-> Result<(), Box<dyn std::error::Error>> {
    const TEST: &str =
        "copytree_errors::new_subtree_errors_match_original_continuation_summaries_and_retry";
    if let Some(document) = std::env::var_os("NUMERA_COPYTREE_ERRORS_FIXTURE") {
        let rows: Vec<Value> = serde_json::from_slice(&fs::read(document)?)?;
        for row in rows {
            let source = Path::new(row["source"].as_str().ok_or("Source missing")?);
            let destination = Path::new(row["destination"].as_str().ok_or("Destination missing")?);
            let denied: Vec<String> = serde_json::from_value(row["denied"].clone())?;
            let attempt = || -> Result<_, Box<dyn std::error::Error>> {
                use std::os::unix::fs::PermissionsExt;
                for name in &denied {
                    fs::set_permissions(source.join(name), fs::Permissions::from_mode(0o0))?;
                }
                let result = run_if_needed(destination, &[source.to_owned()]);
                for name in &denied {
                    fs::set_permissions(source.join(name), fs::Permissions::from_mode(0o700))?;
                }
                Ok(result?)
            };
            let first = attempt()?;
            if row["mode"] == "unreadable-directory" {
                assert!(!destination.join("profiles/branch/b-nested").exists());
            }
            assert_eq!(
                serde_json::to_value(first)?,
                row["first"],
                "First-run copytree failure aggregation differs mode={}",
                row["mode"]
            );
            assert_eq!(snapshot(destination, destination)?, row["written"]);
            assert!(!destination.join(".imported").exists());
            assert_eq!(fs::read(source.join("schedules.json"))?, b"independent");
            if let Some(bytes) = row["nonunicode"].as_array() {
                use std::os::unix::{ffi::OsStrExt, fs::FileTypeExt};
                let name: Vec<u8> = bytes
                    .iter()
                    .map(|byte| {
                        byte.as_u64()
                            .and_then(|value| u8::try_from(value).ok())
                            .ok_or("Invalid fixture byte")
                    })
                    .collect::<Result<_, _>>()?;
                let path = source.join(std::ffi::OsStr::from_bytes(&name));
                assert!(fs::symlink_metadata(&path)?.file_type().is_fifo());
                fs::remove_file(path)?;
            }
            let again = attempt()?;
            assert_eq!(serde_json::to_value(again)?, row["again"]);
            assert_eq!(snapshot(destination, destination)?, row["repeated"]);
            for name in row["pipes"].as_array().ok_or("Pipes missing")? {
                let path = source.join(name.as_str().ok_or("Pipe name missing")?);
                use std::os::unix::fs::FileTypeExt;
                assert!(fs::symlink_metadata(&path)?.file_type().is_fifo());
                fs::remove_file(&path)?;
                fs::write(path, b"retried")?;
            }
            let retried = run_if_needed(destination, &[source.to_owned()])?;
            assert_eq!(serde_json::to_value(retried)?, row["retried"]);
            assert_eq!(snapshot(destination, destination)?, row["complete"]);
            assert!(destination.join(".imported").is_file());
            assert!(run_if_needed(destination, &[source.to_owned()])?.is_none());
        }
        return Ok(());
    }
    let temporary = super::common::TempDirectory::new("copytree-errors")?;
    let root: std::path::PathBuf = temporary.path().components().collect();
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = super::support::load(
        include_str!(
            "../fixtures/characterization/new_subtree_errors_match_original_continuation_summaries_and_retry.json"
        ),
        &root,
    )?;
    assert_eq!(rows.len(), 6, "Copytree failure observations missing");
    let document = root.join("observations.json");
    for row in &rows {
        use std::os::unix::ffi::OsStrExt;
        let source = Path::new(row["source"].as_str().ok_or("Source missing")?);
        let destination = Path::new(row["destination"].as_str().ok_or("Destination missing")?);
        let mode = row["mode"].as_str().ok_or("Mode missing")?;
        fs::create_dir_all(source.join("profiles/branch"))?;
        super::fifo(&source.join("profiles/branch/c-pipe"))?;
        fs::create_dir(source.join("profiles/branch/b-nested"))?;
        super::fifo(&source.join("profiles/branch/b-nested/a-pipe"))?;
        fs::write(
            source.join("profiles/branch/b-nested/z-good.bin"),
            b"nested\x00\xff",
        )?;
        super::fifo(&source.join("profiles/branch/a-pipe"))?;
        fs::write(
            source.join("profiles/branch/z-good.bin"),
            b"healthy\r\n\x00",
        )?;
        if let Some(bytes) = row["nonunicode"].as_array() {
            let bytes = bytes
                .iter()
                .map(|b| {
                    b.as_u64()
                        .and_then(|b| u8::try_from(b).ok())
                        .ok_or("Invalid filename byte")
                })
                .collect::<Result<Vec<_>, _>>()?;
            super::fifo(&source.join(std::ffi::OsStr::from_bytes(&bytes)))?;
        }
        fs::write(source.join("schedules.json"), b"independent")?;
        fs::create_dir_all(destination)?;
        if ["existing", "mixed"].contains(&mode) {
            fs::create_dir_all(destination.join("profiles/branch"))?;
        }
        if mode == "mixed" {
            fs::write(
                destination.join("profiles/branch/a-pipe"),
                b"destination wins",
            )?;
        }
    }
    fs::write(&document, serde_json::to_vec(&rows)?)?;
    let limits = || capture::Limits {
        wall: Duration::from_secs(15),
        idle: Duration::from_secs(10),
        bytes: 131072,
        cleanup: Duration::from_secs(5),
    };
    let mut native = Command::new(std::env::current_exe()?);
    native
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env("NUMERA_COPYTREE_ERRORS_FIXTURE", &document);
    let observed = capture::run(&mut native, limits())?;
    if observed.exit != 0 {
        let output = String::from_utf8(observed.stdout)?;
        let evidence = String::from_utf8(observed.stderr)?;
        if observed.exit == 101
            && output.contains("running 1 test")
            && output.contains(&format!("test {TEST} ... FAILED"))
            && output.contains("test result: FAILED. 0 passed; 1 failed; 0 ignored")
        {
            assert!(
                !evidence.contains("First-run copytree failure aggregation differs"),
                "First-run copytree failure aggregation differs: {evidence}"
            );
        }
        return Err(format!("Native copytree failure observer failed: {}", evidence).into());
    }
    let output = String::from_utf8(observed.stdout)?;
    assert!(
        output.contains("running 1 test") && output.contains("test result: ok. 1 passed; 0 failed")
    );
    temporary.cleanup()?;
    Ok(())
}
