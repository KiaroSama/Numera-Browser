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
