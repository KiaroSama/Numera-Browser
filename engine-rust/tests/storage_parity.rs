use numera_core::storage::{
    atomic::{Batch, write_json, write_stream},
    locks::{DataLock, SessionLease},
};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

mod common;

#[test]
fn persisted_json_text_matches_original_escaping_and_platform_newlines()
-> Result<(), Box<dyn std::error::Error>> {
    use serde_json::Value;

    let temporary = common::TempDirectory::new("persisted-json")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/persisted_json_text_matches_original_escaping_and_platform_newlines.json"
    ))?;
    assert_eq!(cases.len(), 12, "Persisted JSON observations missing");
    for (index, case) in cases.iter().enumerate() {
        let path = temporary
            .path()
            .join("cases")
            .join(index.to_string())
            .join("document.json");
        let expected: Vec<u8> = serde_json::from_value(case["bytes"].clone())?;
        #[cfg(windows)]
        let expected = String::from_utf8(expected)?
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .into_bytes();
        // The retired observer prepared this file before native publication.
        // Recreate its independently frozen bytes in an owned directory.
        fs::create_dir_all(path.parent().ok_or("Original JSON parent missing")?)?;
        fs::write(&path, &expected)?;
        assert_eq!(fs::read(&path)?, expected, "Original JSON bytes changed");
        fs::write(&path, b"previous document must be replaced completely")?;
        write_json(&path, &case["input"])?;
        assert_eq!(
            fs::read(&path)?,
            expected,
            "Persisted JSON bytes differ: case={index}"
        );
        assert_eq!(serde_json::from_slice::<Value>(&expected)?, case["input"]);
        write_json(&path, &case["input"])?;
        assert_eq!(fs::read(&path)?, expected, "Repeated JSON write differs");
        assert_eq!(
            fs::read_dir(path.parent().ok_or("Parent missing")?)?.count(),
            1,
            "JSON publication left temporary files"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn locked_atomic_write_and_rollback_preserve_original_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let parent = std::env::var_os("NUMERA_TEST_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp"));
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = parent.join(format!("storage-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = root.join("profile.json");
        let lock = DataLock::new(root.join("transaction.lock"))?;
        let guard = lock.acquire(Duration::from_millis(100))?;
        let inner = lock.acquire(Duration::ZERO)?;
        write_json(&path, &json!({"name":"Unicode α فارسی","seed":42}))?;
        let original = fs::read(&path)?;
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&original)?,
            json!({"name":"Unicode α فارسی","seed":42})
        );
        let failed = write_stream(&path, |file| {
            use std::io::Write;
            file.write_all(b"partial archive")?;
            Err(std::io::Error::other("controlled stream failure"))
        });
        assert!(failed.is_err());
        assert_eq!(
            fs::read(&path)?,
            original,
            "Failed stream must never publish partial bytes"
        );
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            write_stream(&path, |file| -> std::io::Result<()> {
                use std::io::Write;
                file.write_all(b"partial secret before a writer panic")?;
                std::panic::panic_any((17_u32, "controlled writer panic"));
            })
        }))
        .expect_err("The writer's original panic must keep unwinding");
        assert_eq!(
            panic.downcast_ref::<(u32, &'static str)>(),
            Some(&(17, "controlled writer panic")),
            "Atomic publication must preserve the exact panic payload"
        );
        assert_eq!(fs::read(&path)?, original);
        assert!(
            !fs::read_dir(&root)?.any(|entry| entry
                .is_ok_and(|entry| { entry.file_name().to_string_lossy().contains(".tmp-") })),
            "An unwinding writer must remove its owned partial temporary file"
        );
        let mut batch = Batch::default();
        batch.write(&path, &json!({"name":"changed"}))?;
        let created = root.join("new.json");
        batch.write(&created, &json!({"name":"new"}))?;
        assert!(
            batch.rollback().is_empty(),
            "Rollback must report any unrestored path"
        );
        assert_eq!(
            fs::read(&path)?,
            original,
            "Rollback must restore exact bytes"
        );
        assert!(
            !created.exists(),
            "A newly created file must be removed by rollback"
        );
        drop(inner);
        drop(guard);
        let lease_path = root.join("session.lock");
        let lease = SessionLease::acquire(&lease_path)?;
        assert!(
            SessionLease::acquire(&lease_path).is_err(),
            "Second session in same process must be refused"
        );
        drop(lease);
        drop(SessionLease::acquire(&lease_path)?);
        assert!(!fs::read_dir(&root)?.any(|entry| entry.is_ok_and(|e|e.file_name().to_string_lossy().contains(".tmp-"))),"Atomic write must not leave temporary files");
        Ok(())
    })();
    fs::remove_dir_all(&root)?;
    result
}

#[test]
fn atomic_output_accepts_a_bare_filename_without_changing_global_cwd()
-> Result<(), Box<dyn std::error::Error>> {
    let name = format!(".numera-atomic-test-{}.json", std::process::id());
    let path = PathBuf::from(&name);
    assert!(
        !path.exists(),
        "Test must not overwrite a pre-existing file"
    );
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        write_json(&path, &json!({"text":"Unicode α"}))?;
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&path)?)?,
            json!({"text":"Unicode α"})
        );
        Ok(())
    })();
    if path.exists() {
        fs::remove_file(path)?;
    }
    result
}
