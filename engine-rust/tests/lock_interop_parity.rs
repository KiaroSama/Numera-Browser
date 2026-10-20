use numera_core::storage::locks::{DataLock, SessionLease};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct Holder {
    child: Child,
    reader: Option<thread::JoinHandle<()>>,
}
impl Holder {
    fn start(
        python: &Path,
        fixture: &Path,
        kind: &str,
        path: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut child = Command::new(python)
            .args([
                fixture.as_os_str(),
                kind.as_ref(),
                path.as_os_str(),
                "--hold".as_ref(),
            ])
            .env("PYTHONUTF8", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let output = child.stdout.take().ok_or("Missing lock fixture output")?;
        let (send, recv) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(output).read_line(&mut line).map(|_| line);
            let _ = send.send(result);
        });
        let holder = Self {
            child,
            reader: Some(reader),
        };
        let line = recv.recv_timeout(Duration::from_secs(10))??;
        if line.trim() != "acquired" {
            return Err("Python lock holder did not acquire".into());
        }
        Ok(holder)
    }
    fn release(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.child
            .stdin
            .take()
            .ok_or("Missing release input")?
            .write_all(b"release\n")?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(code) = self.child.try_wait()? {
                if !code.success() {
                    return Err("Python lock holder failed".into());
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("Python lock release timed out".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

mod common;

#[test]
fn configured_lock_timeout_values_match_original_policy() -> Result<(), Box<dyn std::error::Error>>
{
    use numera_core::ownership::capture;
    use serde_json::Value;

    let temporary = common::TempDirectory::new("lock-timeout-policy")?;
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
import json,os,sys,threading
from pathlib import Path
repository,parent=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.data_locks import _lock_timeout
from run_logging import run_log
values=[None,'','0','-0','0.05',' 0.125 ','1_0','١.٥','1e-3','-1','nan','inf','-inf',
        'broken','1__0','1e999','0x10','\x1c1\x1f',str(threading.TIMEOUT_MAX),
        str(threading.TIMEOUT_MAX+1)]
rows=[]
with run_log('lock-timeout-policy-oracle',parent/'logs') as log:
    for value in values:
        if value is None:os.environ.pop('NUMERA_LOCK_TIMEOUT',None)
        else:os.environ['NUMERA_LOCK_TIMEOUT']=value
        rows.append({'input':value,'seconds':_lock_timeout()})
    log.info('Observed configured lock policy cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path());
    let original = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        original.exit, 0,
        "Original configured lock policy observer failed"
    );
    let cases: Vec<Value> = serde_json::from_slice(&original.stdout)?;
    assert_eq!(cases.len(), 20);
    for case in cases {
        let expected = Duration::try_from_secs_f64(
            case["seconds"].as_f64().ok_or("Timeout seconds missing")?,
        )?;
        assert_eq!(
            numera_core::storage::locks::configured_timeout(case["input"].as_str()),
            expected,
            "Configured lock timeout policy differs"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

fn configured_contender(root: &Path, operation: &str) -> Command {
    let mut command =
        Command::new(std::env::current_exe().expect("Current test executable missing"));
    command
        .args([
            "--exact",
            "configured_store_acquisitions_honor_original_lock_owners",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("NUMERA_LOCK_TEST_ROOT", root)
        .env("NUMERA_LOCK_TEST_OPERATION", operation)
        .env("NUMERA_LOCK_TIMEOUT", "0.02");
    command
}

fn configured_limits() -> numera_core::ownership::capture::Limits {
    numera_core::ownership::capture::Limits {
        wall: Duration::from_secs(10),
        idle: Duration::from_secs(5),
        bytes: 65536,
        cleanup: Duration::from_secs(5),
    }
}

#[test]
fn configured_store_acquisitions_honor_original_lock_owners()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        http::tokens::TokenStore,
        ownership::{capture, update_admission::Reservation},
        storage::{config::ConfigStore, folders::Folders, pool::ProxyPool, repo::ProfileRepo},
        vault,
    };
    use serde_json::json;

    if let Some(root) = std::env::var_os("NUMERA_LOCK_TEST_ROOT") {
        let root = PathBuf::from(root);
        let operation = std::env::var("NUMERA_LOCK_TEST_OPERATION")?;
        let result: Result<(), String> = match operation.as_str() {
            "pool" => ProxyPool::new(root, None)
                .list()
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "folder" => Folders::new(root)
                .create("After release")
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "config" => ConfigStore::new(root)
                .set("accepted", json!(true))
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "repo" => ProfileRepo::new(root, None)
                .list(1000.0)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "token" => TokenStore::new(root)
                .revoke("missing", false)
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "vault" => vault::transaction::enable(&root, "synthetic-lock-password")
                .map(|_| ())
                .map_err(|e| e.to_string()),
            "admission" => Reservation::begin(&root, "lock-fixture")
                .and_then(|mut reservation| reservation.release())
                .map_err(|e| e.to_string()),
            _ => return Err("Unknown configured lock fixture operation".into()),
        };
        let outcome = match result {
            Err(error) if error.contains("NUMERA_LOCK_TIMEOUT controls this wait") => "timeout",
            Err(_) => "other-error",
            Ok(()) => "acquired",
        };
        println!("configured-lock-outcome={outcome}");
        return Ok(());
    }
    let temporary = common::TempDirectory::new("configured-lock-contention")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?;
    let fixture = repository.join("scripts/migration/lock_fixture.py");
    for (operation, file) in [
        ("pool", None),
        ("folder", Some("dashboard-config.json")),
        ("config", Some("config.json")),
        ("repo", None),
        ("token", Some("tokens.json")),
        ("vault", None),
        ("admission", None),
    ] {
        let root = temporary.path().join(operation);
        fs::create_dir(&root)?;
        let protected = [
            (
                "dashboard-config.json",
                b"{\"future\":[true,7],\"folders\":[]}".as_slice(),
            ),
            ("config.json", b"{\"future\":[true,7]}".as_slice()),
            ("tokens.json", b"[]".as_slice()),
            ("proxies.json", b"[]".as_slice()),
        ];
        for (name, bytes) in &protected {
            fs::write(root.join(name), bytes)?;
        }
        let resource = file.map_or_else(|| root.clone(), |name| root.join(name));
        let mut holder = Holder::start(
            Path::new(&python),
            &fixture,
            if file.is_some() { "file" } else { "root" },
            &resource,
        )?;
        let mut command = configured_contender(&root, operation);
        let (send, receive) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let _ = send.send(capture::run(&mut command, configured_limits()));
        });
        // A passing refusal must finish while the acknowledged source owner is
        // still holding the lock. Late results cannot turn a deadline into success.
        let early = receive.recv_timeout(Duration::from_secs(3));
        let received_while_owned = early.is_ok();
        let holder_live = holder.child.try_wait();
        let released = holder.release();
        drop(holder);
        let result = match early {
            Ok(result) => Ok(result),
            Err(mpsc::RecvTimeoutError::Timeout) => receive.recv_timeout(Duration::from_secs(5)),
            Err(error) => Err(error),
        };
        worker
            .join()
            .map_err(|_| "Configured lock observer panicked")?;
        assert!(
            holder_live?.is_none(),
            "Original lock holder ended before observation"
        );
        released?;
        let result = result?.map_err(|error| {
            format!("Configured lock operation={operation} phase=held: {error:?}")
        })?;
        assert_eq!(
            result.exit, 0,
            "Configured lock contender failed unexpectedly: operation={operation} phase=held"
        );
        let output = String::from_utf8(result.stdout)?;
        assert_eq!(
            output.matches("configured-lock-outcome=").count(),
            1,
            "Configured lock contender did not report one outcome"
        );
        assert!(
            received_while_owned
                && output
                    .lines()
                    .any(|line| line.ends_with("configured-lock-outcome=timeout")),
            "Configured data-lock acquisition ignored the original timeout"
        );
        for (name, bytes) in &protected {
            assert_eq!(
                fs::read(root.join(name))?,
                *bytes,
                "Configured lock refusal changed stored bytes"
            );
        }
        assert!(!root.join("vault-migration.json").exists());
        assert!(!root.join(".updates/work").exists());
        let result = capture::run(
            &mut configured_contender(&root, operation),
            configured_limits(),
        )
        .map_err(|error| {
            format!("Configured lock operation={operation} phase=released: {error:?}")
        })?;
        assert_eq!(
            result.exit, 0,
            "Released configured operation failed: operation={operation}"
        );
        assert!(
            String::from_utf8(result.stdout)?
                .lines()
                .any(|line| line.ends_with("configured-lock-outcome=acquired")),
            "Released configured operation did not acquire"
        );
        match operation {
            "config" => assert_eq!(ConfigStore::new(root).read()?["accepted"], true),
            "folder" => assert_eq!(Folders::new(root).listing()?, vec!["After release"]),
            "vault" => assert!(ConfigStore::new(root).read()?.contains_key("vault")),
            _ => {
                for (name, bytes) in &protected {
                    assert_eq!(
                        fs::read(root.join(name))?,
                        *bytes,
                        "Released read changed stored bytes"
                    );
                }
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn native_session_lease_excludes_original_until_drop_and_reacquires_after_release()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;

    let python = std::env::var_os("NUMERA_ORACLE_PYTHON")
        .ok_or("Original interpreter required for session lease interoperability")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let fixture = repository.join("scripts/migration/lock_fixture.py");
    let temporary = common::TempDirectory::new("native-session-lease")?;
    let root = temporary.path();
    let path = SessionLease::path_for(root, "fixture-profile")?;
    let refused = || -> Result<(), Box<dyn std::error::Error>> {
        match SessionLease::acquire(&path) {
            Err(error) => assert_eq!(
                error.kind(),
                std::io::ErrorKind::WouldBlock,
                "Native session lease refusal differs"
            ),
            Ok(_) => return Err("Native session lease ignored an existing owner".into()),
        }
        Ok(())
    };
    let mut original = Holder::start(Path::new(&python), &fixture, "session", root)?;
    refused()?;
    original.release()?;
    drop(original);

    let native = SessionLease::acquire(&path)?;
    refused()?;
    let mut command = Command::new(&python);
    command
        .arg(&fixture)
        .arg("session")
        .arg(root)
        .env("PYTHONUTF8", "1")
        .env("PYTHONDONTWRITEBYTECODE", "1");
    let result = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(10),
            idle: Duration::from_secs(10),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        result.exit, 1,
        "Original session lease ignored native ownership"
    );
    assert!(
        String::from_utf8(result.stderr)?.contains("numera.data_locks.SessionBusy:"),
        "Original session contender failed for a reason other than SessionBusy"
    );
    refused()?;
    drop(native);

    let mut original = Holder::start(Path::new(&python), &fixture, "session", root)?;
    refused()?;
    original.release()?;
    drop(original);
    drop(SessionLease::acquire(&path)?);
    assert!(path.is_file(), "Releasing a lease removed its lock domain");
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn standalone_session_deletion_refuses_original_lease_then_preserves_profile_records()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::repo::{ProfileRepo, RepoError};

    let temporary = common::TempDirectory::new("standalone-session-lease")?;
    let root = temporary.path();
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let fixture = repository.join("scripts/migration/lock_fixture.py");
    let repo = ProfileRepo::new(root.to_owned(), None);
    let session = repo.user_data_path("fixture-profile")?;
    let binary = session.join("session.bin");
    let bytes = b"\x00\xff\n\r\n";
    fs::write(&binary, bytes)?;
    let metadata = [
        root.join("profiles/fixture-profile.json"),
        root.join("dashboard/fixture-profile.json"),
    ];
    for path in &metadata {
        fs::create_dir_all(path.parent().ok_or("Profile parent missing")?)?;
        fs::write(path, b"{\"future\":\"untouched\"}")?;
    }
    let mut original = Holder::start(Path::new(&python), &fixture, "session", root)?;
    match repo.delete_session("fixture-profile", "fixture-host") {
        Err(RepoError::SessionBusy(message)) => assert_eq!(
            message,
            "another process has this profile's browser session open. Close it there, or wait for it to finish."
        ),
        Err(error) => {
            return Err(format!("Session deletion failed for a non-lease reason: {error}").into());
        }
        Ok(()) => return Err("Session deletion ignored the original lease".into()),
    }
    assert_eq!(fs::read(&binary)?, bytes);
    original.release()?;
    drop(original);
    repo.delete_session("fixture-profile", "fixture-host")?;
    assert!(!session.exists());
    for path in metadata {
        assert_eq!(fs::read(path)?, b"{\"future\":\"untouched\"}");
    }
    let original = Holder::start(Path::new(&python), &fixture, "session", root)?;
    drop(original);
    repo.delete_session("fixture-profile", "fixture-host")?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn python_and_rust_contend_on_the_same_root_file_and_session()
-> Result<(), Box<dyn std::error::Error>> {
    let python = PathBuf::from(
        std::env::var_os("NUMERA_ORACLE_PYTHON")
            .ok_or("NUMERA_ORACLE_PYTHON is required; interoperability must not be skipped")?,
    );
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .to_owned();
    let fixture = repository.join("scripts/migration/lock_fixture.py");
    let root = repository.join(".ci-work/migration-temp").join(format!(
        "lock interop-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for kind in ["root", "file", "session"] {
            let path = if kind == "file" {
                root.join("config.json")
            } else {
                root.clone()
            };
            let mut holder = Holder::start(&python, &fixture, kind, &path)?;
            let lock_path = match kind {
                "root" => DataLock::transaction(&root)?.path().to_owned(),
                "file" => DataLock::for_file(&path)?.path().to_owned(),
                _ => SessionLease::path_for(&root, "fixture-profile")?,
            };
            let locked = DataLock::new(lock_path.clone())?;
            assert!(
                locked.acquire(Duration::ZERO).is_err(),
                "Rust ignored Python's {kind} lock"
            );
            if kind == "root" {
                let began = Instant::now();
                thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
                    let busy = scope.spawn(|| locked.acquire(Duration::from_millis(300)).is_err());
                    // Other threads must share the deadline, not wait for the
                    // first contender's OS acquisition while a mutex is held.
                    assert!(locked.acquire(Duration::from_millis(50)).is_err());
                    assert!(began.elapsed() < Duration::from_millis(250));
                    assert!(busy.join().map_err(|_| "Lock contender panicked")?);
                    Ok(())
                })?;
            }
            holder.release()?;
            let guard = locked.acquire(Duration::from_secs(1))?;
            // The opposite direction uses the same original source entry point.
            let child = Command::new(&python)
                .args([fixture.as_os_str(), kind.as_ref(), path.as_os_str()])
                .env("PYTHONUTF8", "1")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .stdin(Stdio::null())
                .spawn()?;
            let mut contender = Holder {
                child,
                reader: None,
            };
            let deadline = Instant::now() + Duration::from_secs(5);
            let code = loop {
                if let Some(code) = contender.child.try_wait()? {
                    break code;
                }
                if Instant::now() >= deadline {
                    contender.child.kill()?;
                    contender.child.wait()?;
                    return Err(
                        "Python contender did not refuse within its zero lock timeout".into(),
                    );
                }
                thread::sleep(Duration::from_millis(10));
            };
            assert!(!code.success(), "Python ignored Rust's {kind} lock");
            drop(guard);
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
