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
