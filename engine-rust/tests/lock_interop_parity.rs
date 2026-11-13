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
    fn start(kind: &str, path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let mut child = independent_contender(kind, path, true)?
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let output = child.stdout.take().ok_or("Missing lock fixture output")?;
        let (send, recv) = mpsc::channel();
        let reader = thread::spawn(move || {
            let result = (|| -> std::io::Result<String> {
                let mut reader = BufReader::new(output);
                let mut bytes = 0;
                loop {
                    let mut line = String::new();
                    let count = reader.read_line(&mut line)?;
                    bytes += count;
                    if count == 0 || bytes > 65536 {
                        return Err(std::io::Error::other(
                            "Independent holder did not acknowledge ownership",
                        ));
                    }
                    if line.trim() == "acquired" {
                        return Ok(line);
                    }
                }
            })();
            let _ = send.send(result);
        });
        let holder = Self {
            child,
            reader: Some(reader),
        };
        let line = recv.recv_timeout(Duration::from_secs(10))??;
        if line.trim() != "acquired" {
            return Err("Independent OS lock holder did not acquire".into());
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
                    return Err("Independent OS lock holder failed".into());
                }
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("Independent OS lock release timed out".into());
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

// Independent compatibility fixture: derives the historical pathname domain
// without importing the production locks and uses the original OS primitives.
fn independent_path(kind: &str, path: &Path) -> std::io::Result<PathBuf> {
    use sha2::{Digest, Sha256};
    let resolved = match path.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path
            .parent()
            .ok_or_else(|| std::io::Error::other("Lock parent missing"))?
            .canonicalize()?
            .join(
                path.file_name()
                    .ok_or_else(|| std::io::Error::other("Lock name missing"))?,
            ),
        Err(error) => return Err(error),
    };
    let spelling = resolved
        .to_str()
        .ok_or_else(|| std::io::Error::other("Lock path is not Unicode"))?;
    #[cfg(windows)]
    let spelling = if let Some(tail) = spelling.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{tail}")
    } else {
        spelling
            .strip_prefix(r"\\?\")
            .unwrap_or(spelling)
            .to_owned()
    };
    let (key, name, directory) = match kind {
        "root" => (
            spelling.to_string(),
            "transaction".to_owned(),
            resolved.join(".locks"),
        ),
        "file" => (
            spelling.to_string(),
            resolved
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(|| std::io::Error::other("Lock name missing"))?
                .to_owned(),
            resolved
                .parent()
                .ok_or_else(|| std::io::Error::other("Lock parent missing"))?
                .join(".locks"),
        ),
        "session" => (
            format!("{spelling}\0fixture-profile"),
            "session-fixture-profile".to_owned(),
            resolved.join(".locks"),
        ),
        _ => return Err(std::io::Error::other("Unknown independent lock kind")),
    };
    let digest = Sha256::digest(key.as_bytes());
    let digest = digest[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let name = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect::<String>();
    Ok(directory.join(format!("{name}.{digest}.lock")))
}

fn independent_contender(kind: &str, path: &Path, hold: bool) -> std::io::Result<Command> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "configured_store_acquisitions_honor_original_lock_owners",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("NUMERA_INDEPENDENT_LOCK_PATH", path)
        .env("NUMERA_INDEPENDENT_LOCK_KIND", kind)
        .env("NUMERA_INDEPENDENT_LOCK_HOLD", if hold { "1" } else { "0" });
    Ok(command)
}

fn independent_fixture(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let kind = std::env::var("NUMERA_INDEPENDENT_LOCK_KIND")?;
    let path = independent_path(&kind, path)?;
    fs::create_dir_all(path.parent().ok_or("Independent lock parent missing")?)?;
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    #[cfg(unix)]
    let acquired = {
        use std::os::fd::AsRawFd;
        // SAFETY: the fixture exclusively owns this live file descriptor.
        #[allow(unsafe_code)]
        unsafe {
            libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0
        }
    };
    #[cfg(windows)]
    let acquired = {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: one byte at offset zero is the historical msvcrt lock domain.
        #[allow(unsafe_code)]
        unsafe {
            windows_sys::Win32::Storage::FileSystem::LockFile(file.as_raw_handle(), 0, 0, 1, 0) != 0
        }
    };
    if !acquired {
        let cause = std::io::Error::last_os_error();
        if !matches!(
            cause.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::PermissionDenied
        ) && !matches!(cause.raw_os_error(), Some(11 | 13 | 33))
        {
            return Err(cause.into());
        }
        // Retain the exact historical error discriminator asserted by this case.
        eprintln!("numera.data_locks.SessionBusy: independent OS lock is held");
        std::process::exit(1);
    }
    // libtest may have written its case prefix without a newline.
    println!("\nacquired");
    std::io::stdout().flush()?;
    if std::env::var("NUMERA_INDEPENDENT_LOCK_HOLD")?.as_str() == "1" {
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        if line.trim() != "release" {
            return Err("Independent holder release missing".into());
        }
    }
    // Closing the raw file releases the kernel lock; the lock file stays intact.
    drop(file);
    Ok(())
}

#[test]
fn configured_lock_timeout_values_match_original_policy() -> Result<(), Box<dyn std::error::Error>>
{
    use serde_json::Value;

    let temporary = common::TempDirectory::new("lock-timeout-policy")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/configured_lock_timeout_values_match_original_policy.json"
    ))?;
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

const ENABLE_PHASES: &[&str] = &[
    "Enable lock requested",
    "Enable lock acquired",
    "Enable config admitted",
    "Enable descriptor derivation started",
    "Enable salt randomness requested",
    "Enable salt randomness acquired",
    "Enable verifier encryption started",
    "Enable descriptor derivation finished",
    "Enable journal published",
    "Enable commit verified",
    "Enable completed",
];

fn enable_log_paths(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(root.join("logs")) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    entries
        .filter_map(|entry| match entry {
            Ok(entry)
                if entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("native-vault-enable_") =>
            {
                Some(Ok(entry.path()))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect()
}

fn enable_phases(path: &Path) -> std::io::Result<Vec<&'static str>> {
    use std::io::Read;
    let mut text = String::new();
    fs::File::open(path)?.take(8193).read_to_string(&mut text)?;
    if text.len() > 8192 {
        return Err(std::io::Error::other("Enable phase log exceeds its bound"));
    }
    Ok(text
        .lines()
        .filter_map(|line| {
            let phase = line.split_once("] [INFO] [vault] ")?.1;
            ENABLE_PHASES
                .iter()
                .copied()
                .find(|expected| *expected == phase)
        })
        .collect())
}

#[test]
fn enable_phase_projection_accepts_only_fixed_labels_and_bounded_utf8()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("enable-phase-projection")?;
    let path = temporary.path().join("phase.log");
    fs::write(
        &path,
        concat!(
            "[2026-10-21 00:00:00 UTC] [INFO] [vault] Enable lock requested\n",
            "[2026-10-21 00:00:00 UTC] [INFO] [vault] synthetic-private-value\n",
            "[2026-10-21 00:00:00 UTC] [ERROR] [vault] Enable completed\n",
            "[2026-10-21 00:00:00 UTC] [INFO] [vault] Enable descriptor derivation started\n"
        ),
    )?;
    assert_eq!(
        enable_phases(&path)?,
        vec![ENABLE_PHASES[0], ENABLE_PHASES[3]]
    );
    fs::write(&path, [0xff])?;
    assert!(enable_phases(&path).is_err());
    fs::write(&path, vec![b'x'; 8193])?;
    assert!(enable_phases(&path).is_err());
    temporary.cleanup()?;
    Ok(())
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

    if let Some(path) = std::env::var_os("NUMERA_INDEPENDENT_LOCK_PATH") {
        independent_fixture(&PathBuf::from(path))?;
        // This branch is a standalone contender, like the retired source peer.
        // All file guards were dropped above; avoid a libtest trailer on its
        // single readiness pipe after the parent has consumed that handshake.
        std::process::exit(0);
    }
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
        let mut holder = Holder::start(if file.is_some() { "file" } else { "root" }, &resource)?;
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
        let prior_logs = enable_log_paths(&root)?;
        let released_log = || -> std::io::Result<PathBuf> {
            let new = enable_log_paths(&root)?
                .into_iter()
                .filter(|path| !prior_logs.contains(path))
                .collect::<Vec<_>>();
            if new.len() != 1 {
                return Err(std::io::Error::other("Released enable log is not unique"));
            }
            Ok(new[0].clone())
        };
        let result = capture::run(
            &mut configured_contender(&root, operation),
            configured_limits(),
        )
        .map_err(|error| {
            let phase = released_log()
                .and_then(|path| enable_phases(&path))
                .ok()
                .and_then(|phases| phases.last().copied());
            let journal_present = root.join("vault-migration.json").try_exists().ok();
            let config_vault_present = fs::read(root.join("config.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|value| value.as_object().map(|object| object.contains_key("vault")));
            format!(
                "Configured lock operation={operation} phase=released enable_phase={phase:?} journal_present={journal_present:?} config_vault_present={config_vault_present:?}: {error:?}"
            )
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
            "vault" => {
                assert!(ConfigStore::new(root.clone()).read()?.contains_key("vault"));
                assert_eq!(enable_phases(&released_log()?)?, ENABLE_PHASES);
            }
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
    let mut original = Holder::start("session", root)?;
    refused()?;
    original.release()?;
    drop(original);

    let native = SessionLease::acquire(&path)?;
    refused()?;
    let mut command = independent_contender("session", root, false)?;
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

    let mut original = Holder::start("session", root)?;
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
    let mut original = Holder::start("session", root)?;
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
    let original = Holder::start("session", root)?;
    drop(original);
    repo.delete_session("fixture-profile", "fixture-host")?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn python_and_rust_contend_on_the_same_root_file_and_session()
-> Result<(), Box<dyn std::error::Error>> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .to_owned();
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
            let mut holder = Holder::start(kind, &path)?;
            let lock_path = match kind {
                "root" => DataLock::transaction(&root)?.path().to_owned(),
                "file" => DataLock::for_file(&path)?.path().to_owned(),
                _ => SessionLease::path_for(&root, "fixture-profile")?,
            };
            let locked = DataLock::new(lock_path.clone())?;
            assert!(
                locked.acquire(Duration::ZERO).is_err(),
                "Rust ignored the independent OS owner's {kind} lock"
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
            // An independent raw OS contender also refuses the native lock in the opposite direction.
            let child = independent_contender(kind, &path, false)?
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
                        "Independent OS contender did not refuse within its zero lock timeout"
                            .into(),
                    );
                }
                thread::sleep(Duration::from_millis(10));
            };
            assert!(
                !code.success(),
                "Independent OS contender ignored Rust's {kind} lock"
            );
            drop(guard);
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
