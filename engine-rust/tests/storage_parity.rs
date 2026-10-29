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
    use numera_core::ownership::capture;
    use serde_json::Value;
    use std::{path::Path, process::Command};

    let temporary = common::TempDirectory::new("persisted-json")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command
        .args([
            "-I",
            "-B",
            "-c",
            r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.paths import write_json
from run_logging import run_log

with run_log('persisted-json-oracle',root/'logs') as log:
    values=[{'text':'日本語 α فارسی'},
            {'controls':''.join(map(chr,range(32)))+chr(127)},
            {'astral':chr(0x1f600)+chr(0x1d120)},
            {'日本語':{'z':'last','a':['first',{},[]]}},
            {'literal':'quote" backslash\\ slash/ newline\n tab\t'},
            {'nested':[{'enabled':True,'empty':None},False,[],{}]},
            {},[],None,True,'plain ASCII',{'integers':[0,-1,42]}]
    rows=[]
    for index,value in enumerate(values):
        path=root/'cases'/str(index)/'document.json'
        write_json(path,value)
        rows.append({'input':value,'bytes':list(path.read_bytes())})
    log.info('Observed persisted JSON cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path())
        .env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original persisted JSON observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
    assert_eq!(cases.len(), 12, "Persisted JSON observations missing");
    for (index, case) in cases.iter().enumerate() {
        let path = temporary
            .path()
            .join("cases")
            .join(index.to_string())
            .join("document.json");
        let expected: Vec<u8> = serde_json::from_value(case["bytes"].clone())?;
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
