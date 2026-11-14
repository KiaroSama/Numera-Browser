use numera_core::{ownership::capture, storage::first_run::run_if_needed};
use serde_json::{Map, Value};
use std::{fs, path::Path, process::Command, time::Duration};

// The immutable observation contains this particular readdir order. Replay
// only the order; the entries, FIFOs, permissions and copy operations are real.
fn frozen_order(_: &Path, entries: &mut [fs::DirEntry]) {
    entries.sort_by_key(|entry| {
        use std::os::unix::ffi::OsStrExt;
        let name = entry.file_name();
        let bytes = name.as_bytes();
        match bytes {
            b"z-good.bin" => 0,
            name if name.starts_with(b"raw-") => 1,
            b"c-pipe" => 2,
            b"a-pipe" => 3,
            b"b-nested" => 4,
            _ => 5,
        }
    });
}

fn reverse_order(path: &Path, entries: &mut [fs::DirEntry]) {
    frozen_order(path, entries);
    entries.reverse();
}

fn failure_tuples(summary: &Value) -> Vec<String> {
    let message = summary["failed"][0].as_str().expect("one aggregate");
    let tuples = message
        .strip_prefix("profiles: [(")
        .and_then(|text| text.strip_suffix(")]"))
        .expect("complete failure tuples");
    let mut result = tuples.split("), (").map(str::to_owned).collect::<Vec<_>>();
    result.sort();
    result
}

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
                let result = numera_core::storage::first_run_test_support::with_directory_order(
                    frozen_order,
                    || run_if_needed(destination, &[source.to_owned()]),
                );
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
            if row["mode"] == "new" {
                // Counterexample: reverse the same real entries into a fresh
                // destination. All three original FIFO errors must survive,
                // together with both healthy-file copies and retryability.
                let alternate = destination.with_file_name("alternate-order");
                let result = numera_core::storage::first_run_test_support::with_directory_order(
                    reverse_order,
                    || run_if_needed(&alternate, &[source.to_owned()]),
                )?
                .ok_or("Alternate copy not attempted")?;
                let mut observed = serde_json::to_value(result)?;
                fn rebase(value: &mut Value, old: &str, new: &str) {
                    match value {
                        Value::String(text) => *text = text.replace(old, new),
                        Value::Array(rows) => rows.iter_mut().for_each(|row| rebase(row, old, new)),
                        Value::Object(rows) => {
                            rows.values_mut().for_each(|row| rebase(row, old, new))
                        }
                        _ => {}
                    }
                }
                rebase(
                    &mut observed,
                    alternate.to_str().ok_or("Alternate path")?,
                    destination.to_str().ok_or("Destination path")?,
                );
                assert_eq!(failure_tuples(&observed), failure_tuples(&row["first"]));
                assert_ne!(
                    observed["failed"], row["first"]["failed"],
                    "the alternate order must actually differ"
                );
                observed["failed"] = row["first"]["failed"].clone();
                assert_eq!(observed, row["first"], "alternate summary fields");
                assert_eq!(snapshot(&alternate, &alternate)?, row["written"]);
                assert!(!alternate.join(".imported").exists());
            }
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
    let mut rows: Vec<Value> = super::support::load(
        include_str!(
            "../fixtures/characterization/new_subtree_errors_match_original_continuation_summaries_and_retry.json"
        ),
        &root,
    )?;
    assert_eq!(rows.len(), 6, "Copytree failure observations missing");
    let document = root.join("observations.json");
    for row in &mut rows {
        use std::os::unix::ffi::OsStrExt;
        let source = std::path::PathBuf::from(row["source"].as_str().ok_or("Source missing")?);
        let destination =
            std::path::PathBuf::from(row["destination"].as_str().ok_or("Destination missing")?);
        let mode = row["mode"].as_str().ok_or("Mode missing")?.to_owned();
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
            let original = source.join(std::ffi::OsStr::from_bytes(&bytes));
            match super::fifo(&original) {
                Ok(()) => {}
                #[cfg(target_os = "macos")]
                Err(error) if error.raw_os_error() == Some(libc::EILSEQ) => {
                    assert_eq!(
                        original.strip_prefix(&source)?.as_os_str().as_bytes(),
                        bytes,
                        "the rejected OS path retained every original filename byte"
                    );
                    assert!(matches!(
                        fs::symlink_metadata(&original)
                            .expect_err("APFS refused filename")
                            .raw_os_error(),
                        Some(libc::EILSEQ) | Some(libc::ENOENT)
                    ));
                    let target = destination.join(std::ffi::OsStr::from_bytes(&bytes));
                    let diagnostic = numera_core::storage::first_run_test_support::pipe_failure(
                        &original, &target,
                    );
                    assert!(
                        row["first"]["failed"][0]
                            .as_str()
                            .ok_or("Frozen failure")?
                            .contains(&diagnostic[1..diagnostic.len() - 1]),
                        "raw OS-byte diagnostic changed"
                    );
                    // APFS cannot own the Linux byte filename. Retain the
                    // original pure diagnostic above, and exercise its FIFO
                    // copy/retry effects using a clearly identified alias.
                    let alias = b"profiles/branch/raw-platform-unrepresentable-pipe";
                    super::fifo(&source.join(std::ffi::OsStr::from_bytes(alias)))?;
                    for phase in ["first", "again"] {
                        if let Some(message) = row[phase]["failed"][0].as_str() {
                            row[phase]["failed"][0] = message
                                .replace(
                                    "raw-\\udcff-\\udce2\\udc82-pipe",
                                    "raw-platform-unrepresentable-pipe",
                                )
                                .into();
                        }
                    }
                    row["nonunicode"] = serde_json::to_value(alias.as_slice())?;
                }
                Err(error) => return Err(error.into()),
            }
        }
        fs::write(source.join("schedules.json"), b"independent")?;
        fs::create_dir_all(&destination)?;
        if ["existing", "mixed"].contains(&mode.as_str()) {
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
