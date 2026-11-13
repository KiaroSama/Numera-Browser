#![cfg(target_os = "linux")]

use numera_core::storage::first_run::import_from;
use serde_json::{Map, Value, json};
use std::{
    fs, io,
    path::Path,
    time::{Duration, UNIX_EPOCH},
};
mod common;
const NAMES: [&str; 5] = [
    "tokens.json",
    "profiles/existing/state.bin",
    "profiles/new/state.bin",
    "profiles/new/nested/deep.bin",
    "config.json",
];
fn summary(source: &Path, destination: &Path, copied: &[&str], skipped: &[&str]) -> Value {
    json!({"source":source,"destination":destination,"copied":copied,"skipped":skipped,"failed":[],"profiles":0})
}
fn access_snapshot(source: &Path, destination: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let accessed = 1_600_000_000_000_000_000u128;
    let mut rows = Map::new();
    // Stat every file before reading bytes, as in the retained source observer.
    for name in NAMES {
        let src = fs::metadata(source.join(name))?;
        let dst = fs::metadata(destination.join(name))?;
        let source_access = src.accessed()?.duration_since(UNIX_EPOCH)?.as_nanos();
        let destination_access = dst.accessed()?.duration_since(UNIX_EPOCH)?.as_nanos();
        rows.insert(name.into(),json!({"source_access_changed":source_access!=accessed,"copied_initial_access":destination_access==accessed,"copied_post_read_access":destination_access==source_access,"modified":dst.modified()?.duration_since(UNIX_EPOCH)?.as_nanos(),"mode":dst.permissions().mode()&0o7777}));
    }
    for name in NAMES {
        rows[name]["bytes"] = json!(fs::read(destination.join(name))?);
        rows[name]["source_bytes"] = json!(fs::read(source.join(name))?);
    }
    Ok(Value::Object(rows))
}
#[test]
fn direct_copy_access_times_match_original_without_changing_copytree_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let temporary = common::TempDirectory::new("first-run-access-times")?;
    let source = temporary.path().join("source");
    let destination = temporary.path().join("destination");
    fs::create_dir_all(source.join("profiles/existing"))?;
    fs::create_dir_all(source.join("profiles/new/nested"))?;
    fs::create_dir_all(destination.join("profiles/existing"))?;
    for (index, name) in NAMES.iter().enumerate() {
        let path = source.join(name);
        fs::write(&path, [0, 255, 10, index as u8])?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640))?;
        fs::set_times(
            &path,
            fs::FileTimes::new()
                .set_accessed(UNIX_EPOCH + Duration::from_secs(1_600_000_000))
                .set_modified(
                    UNIX_EPOCH
                        + Duration::from_secs(1_500_000_000)
                        + Duration::from_nanos(index as u64),
                ),
        )?;
    }
    let kept = destination.join("config.json");
    fs::write(&kept, b"destination wins")?;
    fs::set_permissions(&kept, fs::Permissions::from_mode(0o600))?;
    fs::set_times(
        &kept,
        fs::FileTimes::new()
            .set_accessed(UNIX_EPOCH + Duration::from_secs(1_599_999_999))
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_499_999_999)),
    )?;
    let expected_first = summary(
        &source,
        &destination,
        &["profiles (2)", "tokens.json"],
        &["config.json"],
    );
    let expected_again = summary(
        &source,
        &destination,
        &[],
        &["profiles (3 already here)", "config.json", "tokens.json"],
    );
    assert_eq!(
        serde_json::to_value(import_from(&source, &destination)?)?,
        expected_first,
        "Access-time import summary differs"
    );
    assert_eq!(
        serde_json::to_value(import_from(&source, &destination)?)?,
        expected_again,
        "Access-time repeat import summary differs"
    );
    let actual = access_snapshot(&source, &destination)?;
    assert_eq!(actual.as_object().map(|rows| rows.len()), Some(5));
    let mut expected = Map::new();
    for (index, name) in NAMES.iter().enumerate() {
        let retained = *name == "config.json";
        let copied_post = index < 2;
        let copied_initial = (2..4).contains(&index);
        expected.insert((*name).into(),json!({"source_access_changed":!retained,"copied_initial_access":copied_initial,"copied_post_read_access":copied_post,"modified":if retained{1_499_999_999_000_000_000u64}else{1_500_000_000_000_000_000u64+index as u64},"mode":if retained{0o600}else{0o640},"bytes":if retained{b"destination wins".to_vec()}else{vec![0,255,10,index as u8]},"source_bytes":[0,255,10,index as u8]}));
    }
    assert!(
        NAMES[..4]
            .iter()
            .all(|name| actual[name]["source_access_changed"] == true),
        "Hosted filesystem did not exercise source access-time updates"
    );
    assert_eq!(
        actual,
        Value::Object(expected),
        "First-run access-time copy policy differs"
    );
    temporary.cleanup()?;
    Ok(())
}
#[allow(unsafe_code)]
fn attributes(path: &Path, label: &[u8]) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let path = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    for (name, value) in [
        (c"user.numera.fixture", label),
        (c"user.numera.empty", &[][..]),
    ] {
        // SAFETY: valid owned C paths/names and a borrowed initialized byte slice.
        if unsafe {
            libc::setxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
#[allow(unsafe_code)]
fn read_attributes(path: &Path) -> io::Result<Map<String, Value>> {
    use std::{
        ffi::{CStr, CString},
        os::unix::ffi::OsStrExt,
    };
    let path = CString::new(path.as_os_str().as_bytes()).map_err(io::Error::other)?;
    // SAFETY: a zero length query borrows only the valid C path.
    let size = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0) };
    if size < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut names = vec![0u8; size as usize];
    // SAFETY: writable initialized buffer is exactly the supplied capacity.
    let count = unsafe { libc::listxattr(path.as_ptr(), names.as_mut_ptr().cast(), names.len()) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    names.truncate(count as usize);
    let mut result = Map::new();
    for name in names.split_inclusive(|byte| *byte == 0) {
        let name = CStr::from_bytes_with_nul(name).map_err(io::Error::other)?;
        // SAFETY: valid C path/name and a zero length size query.
        let size = unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0) };
        if size < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut bytes = vec![0u8; size as usize];
        // SAFETY: the initialized output buffer is valid for the supplied size.
        let count = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error());
        }
        bytes.truncate(count as usize);
        result.insert(
            name.to_str().map_err(io::Error::other)?.into(),
            json!(bytes),
        );
    }
    Ok(result)
}
fn attribute_snapshot(base: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    fn walk(
        base: &Path,
        at: &Path,
        rows: &mut Map<String, Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for entry in fs::read_dir(at)? {
            let path = entry?.path();
            let relative = path.strip_prefix(base)?;
            if relative.components().any(|c| c.as_os_str() == ".locks") {
                continue;
            }
            let mut row = json!({"attributes":read_attributes(&path)?});
            if path.is_file() {
                row["bytes"] = json!(fs::read(&path)?);
            }
            rows.insert(
                relative
                    .to_str()
                    .ok_or("NonUnicode fixture path")?
                    .replace('\\', "/"),
                row,
            );
            if path.is_dir() {
                walk(base, &path, rows)?;
            }
        }
        Ok(())
    }
    let mut rows = Map::new();
    walk(base, base, &mut rows)?;
    Ok(Value::Object(rows))
}
#[test]
fn imported_attributes_match_original_without_replacing_destination_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let temporary = common::TempDirectory::new("first-run-attributes")?;
    let mut expected_cases = Vec::new();
    for variant in ["new", "existing", "mixed"] {
        let case = temporary.path().join(variant);
        let source = case.join("source 日本語");
        let destination = case.join("destination with spaces");
        fs::create_dir_all(source.join("profiles/new/nested"))?;
        fs::create_dir_all(&destination)?;
        for (name, raw) in [
            ("profiles/new/state.bin", &[0, 255, 10, 13][..]),
            ("profiles/new/nested/deep.bin", &b"deep"[..]),
            ("tokens.json", &b"fixture"[..]),
        ] {
            let path = source.join(name);
            fs::write(&path, raw)?;
            attributes(&path, b"source\x00\xff")?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o440))?;
        }
        for name in ["profiles", "profiles/new", "profiles/new/nested"] {
            attributes(&source.join(name), b"source directory")?;
        }
        if variant != "new" {
            fs::create_dir_all(destination.join("profiles/new"))?;
            attributes(&destination.join("profiles"), b"destination root")?;
            attributes(&destination.join("profiles/new"), b"destination directory")?;
            fs::write(destination.join("tokens.json"), b"destination file")?;
            attributes(&destination.join("tokens.json"), b"destination\x00\xff")?;
        }
        if variant == "existing" {
            fs::create_dir(destination.join("profiles/new/nested"))?;
            attributes(
                &destination.join("profiles/new/nested"),
                b"destination nested",
            )?;
            fs::write(
                destination.join("profiles/new/state.bin"),
                b"destination state",
            )?;
            attributes(
                &destination.join("profiles/new/state.bin"),
                b"destination state attributes",
            )?;
        }
        let source_state = attribute_snapshot(&source)?;
        let mut written = attribute_snapshot(&destination)?
            .as_object()
            .ok_or("Initial destination state missing")?
            .clone();
        for (name, value) in source_state.as_object().ok_or("Source state missing")? {
            written.entry(name.clone()).or_insert_with(|| value.clone());
        }
        if variant == "new" {
            // The immutable source _merge_dir creates its top-level container
            // with mkdir; only absent child subtrees use copytree/copystat.
            // Observe default directory attributes from an independent owned
            // input, never from the importer output under assertion.
            let control = case.join("default-merge-directory");
            fs::create_dir(&control)?;
            let defaults = read_attributes(&control)?;
            assert!(!defaults.contains_key("user.numera.fixture"));
            written.insert("profiles".into(), json!({"attributes": defaults}));
        }
        let (copied, skipped) = match variant {
            "new" => (vec!["profiles (1)", "tokens.json"], vec![]),
            "existing" => (
                vec!["profiles (1)"],
                vec!["profiles (1 already here)", "tokens.json"],
            ),
            _ => (vec!["profiles (2)"], vec!["tokens.json"]),
        };
        let expected = json!({"source":source,"destination":destination,"first":summary(&source,&destination,&copied,&skipped),"again":summary(&source,&destination,&[],&["profiles (2 already here)","tokens.json"]),"source_state":source_state,"written":written});
        expected_cases.push(expected);
    }
    assert_eq!(expected_cases.len(), 3, "Attribute observations missing");
    for case in &expected_cases {
        let source = Path::new(case["source"].as_str().ok_or("Source missing")?);
        let destination = Path::new(case["destination"].as_str().ok_or("Destination missing")?);
        assert_eq!(
            serde_json::to_value(import_from(source, destination)?)?,
            case["first"]
        );
        assert_eq!(
            serde_json::to_value(import_from(source, destination)?)?,
            case["again"]
        );
    }
    let actual=expected_cases.iter().map(|case|{let source=Path::new(case["source"].as_str().ok_or("Source missing")?);let destination=Path::new(case["destination"].as_str().ok_or("Destination missing")?);Ok(json!({"source_state":attribute_snapshot(source)?,"written":attribute_snapshot(destination)?}))}).collect::<Result<Vec<Value>,Box<dyn std::error::Error>>>()?;
    assert_eq!(actual.len(), expected_cases.len());
    for (index, (actual, expected)) in actual.iter().zip(&expected_cases).enumerate() {
        assert_eq!(
            actual["source_state"], expected["source_state"],
            "Source attributes changed"
        );
        assert_eq!(
            actual["written"], expected["written"],
            "First-run extended attributes differ: case={index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
