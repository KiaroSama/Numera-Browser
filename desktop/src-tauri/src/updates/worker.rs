use super::{
    files, manifest,
    transaction::{self, Journal},
};
use std::{
    fs, io,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn committed(journal: &Journal) -> io::Result<Option<(String, u64)>> {
    let bytes = match files::read(&journal.directory.join("commit.json"), 4096) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| manifest::error("Update admission commit is malformed"))?;
    let sha = value["sourceSha"]
        .as_str()
        .ok_or_else(|| manifest::error("Committed source identity missing"))?;
    manifest::decode_hex::<20>(sha)?;
    let generation = value["generation"]
        .as_u64()
        .filter(|number| *number > 0)
        .ok_or_else(|| manifest::error("Committed generation missing"))?;
    if value["nonce"] != journal.nonce {
        return Err(manifest::error(
            "Update admission commit belongs to another transaction",
        ));
    }
    Ok(Some((sha.into(), generation)))
}

pub fn spawn(journal: &Journal) -> io::Result<Child> {
    let executable = std::env::current_exe()?;
    let worker = journal.directory.join(if cfg!(windows) {
        "update-worker.exe"
    } else {
        "update-worker"
    });
    fs::copy(&executable, &worker)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&worker, fs::Permissions::from_mode(0o700))?;
    }
    if files::hash(&worker)? != files::hash(&executable)? {
        return Err(manifest::error("Update worker copy failed readback"));
    }
    let mut command = Command::new(worker);
    command
        .arg("--numera-update-worker")
        .arg(journal.directory.join("journal.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_remove("NUMERA_UPDATE_TOKEN")
        .env_remove("NUMERA_UPDATE_TICKET")
        .env_remove("NUMERA_PASSWORD");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.spawn()
}
fn parent_exited() -> io::Result<()> {
    // The sole reader is joined; an EOF is the shell's actual released pipe,
    // not a recycled PID. A finite watchdog closes the wait on a stalled shell.
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let input = io::stdin();
        let fd = input.as_raw_fd();
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let mut item = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let result = unsafe { libc::poll(&mut item, 1, 100) };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            if result > 0 {
                let mut byte = [0];
                if unsafe { libc::read(fd, byte.as_mut_ptr().cast(), 1) } == 0 {
                    return Ok(());
                }
            }
            if Instant::now() >= end {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Desktop did not release its update handoff",
                ));
            }
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};
        let input = io::stdin();
        let handle = input.as_raw_handle();
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            let mut available = 0;
            let ok = unsafe {
                PeekNamedPipe(
                    handle,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                    return Ok(());
                }
                return Err(error);
            }
            if Instant::now() >= end {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Desktop did not release its update handoff",
                ));
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}
pub fn run(path: &Path) -> io::Result<()> {
    let mut journal = Journal::load(path)?;
    let log = super::log::Log::new(&journal.root, "update-worker");
    log.event("INFO", "Activation worker started");
    if journal.phase != "ready" || journal.ticket.is_empty() {
        return Err(manifest::error(
            "Update worker has no approved ready transaction",
        ));
    }
    parent_exited()?;
    let _lock = files::lock(&journal.root)?;
    let key = manifest::decode_hex::<32>(manifest::PUBLIC_KEY)?;
    let platform = manifest::platform()?;
    let old = transaction::installed(&journal.root, &key, platform)?;
    let next = transaction::installed(&journal.staged(), &key, platform)?;
    transaction::validate_files(&journal.root, &old)?;
    transaction::validate_files(&journal.staged(), &next)?;
    let bytes = files::read(
        &journal.directory.join("manifest.json"),
        manifest::METADATA_LIMIT,
    )?;
    let signature = files::read(&journal.directory.join("manifest.sig"), 64)?;
    let manifest = manifest::signed_manifest(
        &bytes,
        &signature,
        &key,
        platform,
        transaction::generation_floor(&journal.root, old.generation)?,
        transaction::now()?,
    )?;
    if manifest::digest(&bytes) != journal.manifest_digest
        || manifest.generation != next.generation
        || manifest.source_sha != next.source_sha
    {
        return Err(manifest::error(
            "Ready update no longer matches its approved manifest",
        ));
    }
    let exclusive = match super::install_lease::exclusive(&journal.root, Duration::from_secs(10)) {
        Ok(lock) => lock,
        Err(error) => {
            // Nothing was replaced. Reopen the old verified generation only;
            // its private resume releases the current root admission ticket.
            start_and_wait(&journal, &old.source_sha)?;
            journal.phase = "rolled-back".into();
            journal.save()?;
            return Err(error);
        }
    };
    super::install_lease::mark(&journal.root, &journal.nonce)?;
    let activation = transaction::activate(&mut journal, &old, &next);
    drop(exclusive);
    let outcome = activation.and_then(|_| start_and_wait(&journal, &next.source_sha));
    if let Err(error) = outcome {
        if journal.directory.join("commit.json").exists() {
            journal.phase = "committed-needs-resume".into();
            journal.save()?;
            return Err(error);
        }
        let exclusive = super::install_lease::exclusive(&journal.root, Duration::from_secs(10))?;
        if journal.directory.join("old-BUILD.json").is_file() {
            transaction::rollback(&mut journal, &old, &next)?;
        }
        drop(exclusive);
        start_and_wait(&journal, &old.source_sha)?;
        super::install_lease::clear(&journal.root, &journal.nonce)?;
        journal.phase = "rolled-back".into();
        journal.save()?;
        return Err(error);
    }
    super::install_lease::clear(&journal.root, &journal.nonce)?;
    transaction::commit_floor(&journal.root, next.generation)?;
    journal.phase = "complete".into();
    journal.save()?;
    log.event("INFO", "Generation activated and readiness verified");
    Ok(())
}
fn start_and_wait(journal: &Journal, sha: &str) -> io::Result<()> {
    let receipt = journal.directory.join("ready.json");
    for name in ["ready.json", "healthy.json"] {
        let path = journal.directory.join(name);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    let key = manifest::decode_hex::<32>(manifest::PUBLIC_KEY)?;
    let build = transaction::installed(&journal.root, &key, manifest::platform()?)?;
    if build.source_sha != sha
        || committed(journal)?
            .is_some_and(|identity| identity != (sha.to_owned(), build.generation))
    {
        return Err(manifest::error(
            "Recovery cannot replace an already committed generation",
        ));
    }
    let executable = journal.root.join(if cfg!(windows) {
        "numera.exe"
    } else {
        "numera"
    });
    let mut command = Command::new(executable);
    command
        .current_dir(&journal.root)
        .env(
            "NUMERA_UPDATE_RECOVERY",
            journal.directory.join("journal.json"),
        )
        .env("NUMERA_UPDATE_EXPECTED_SHA", sha)
        .env("NUMERA_DATA_DIR", &journal.data_root)
        .env("NUMERA_UPDATE_ACTIVATION", &journal.nonce)
        .env("NUMERA_UPDATE_TICKET", &journal.ticket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env_remove("NUMERA_UPDATE_TOKEN");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn()?;
    let end = Instant::now() + Duration::from_secs(60);
    let outcome = (|| -> io::Result<()> {
        loop {
            let healthy = journal.directory.join("healthy.json");
            if healthy.exists() && !journal.directory.join("commit.json").exists() {
                let value: serde_json::Value =
                    serde_json::from_slice(&files::read(&healthy, 4096)?)
                        .map_err(|_| manifest::error("Update health receipt is malformed"))?;
                if value["nonce"] != journal.nonce
                    || value["sourceSha"] != sha
                    || value["pid"].as_u64() != Some(child.id() as u64)
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(manifest::error("Updated health identity changed"));
                }
                let key = manifest::decode_hex::<32>(manifest::PUBLIC_KEY)?;
                let build = transaction::installed(&journal.root, &key, manifest::platform()?)?;
                transaction::validate_files(&journal.root, &build)?;
                if build.source_sha != sha {
                    return Err(manifest::error(
                        "Activated generation changed before admission commit",
                    ));
                }
                transaction::commit_floor(&journal.root, build.generation)?;
                files::json(
                    &journal.directory.join("commit.json"),
                    &serde_json::json!({"nonce":journal.nonce,"sourceSha":sha,"generation":build.generation}),
                )?;
            }
            if receipt.exists() {
                let value: serde_json::Value =
                    serde_json::from_slice(&files::read(&receipt, 4096)?)
                        .map_err(|_| manifest::error("Update readiness is malformed"))?;
                if value["nonce"] == journal.nonce
                    && value["sourceSha"] == sha
                    && value["pid"].as_u64() == Some(child.id() as u64)
                    && value["ready"] == true
                {
                    return Ok(());
                }
                return Err(manifest::error(
                    "Update readiness belongs to another generation",
                ));
            }
            if child.try_wait()?.is_some() {
                return Err(io::Error::other("Updated desktop exited before readiness"));
            }
            if Instant::now() >= end {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Updated desktop did not publish readiness",
                ));
            }
            thread::sleep(Duration::from_millis(25));
        }
    })();
    if outcome.is_err() && !journal.directory.join("commit.json").exists() {
        let _ = child.kill();
        child.wait()?;
    }
    if outcome.is_err() && journal.directory.join("commit.json").exists() {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "Healthy generation commit is retained; automatic rollback refused",
        ));
    }
    // Successful handoff deliberately leaves the verified desktop running;
    // failures never release its actual process handle before waiting.
    outcome
}
pub fn restart_recovered(journal: &Journal, sha: &str) -> io::Result<()> {
    start_and_wait(journal, sha)
}
pub fn recovery_ready(root: &Path, port: u16, token: &str) -> io::Result<()> {
    let Some(path) = std::env::var_os("NUMERA_UPDATE_RECOVERY") else {
        return Ok(());
    };
    let journal = Journal::load(Path::new(&path))?;
    if journal.root != root.canonicalize()? {
        return Err(manifest::error("Update recovery installation root changed"));
    }
    let key = manifest::decode_hex::<32>(manifest::PUBLIC_KEY)?;
    let build = transaction::installed(root, &key, manifest::platform()?)?;
    let expected = std::env::var("NUMERA_UPDATE_EXPECTED_SHA")
        .map_err(|_| manifest::error("Expected recovery generation missing"))?;
    if build.source_sha != expected
        || env!("NUMERA_BUILD_SOURCE_SHA") != expected
        || env!("NUMERA_BUILD_GENERATION").parse::<u64>().ok() != Some(build.generation)
    {
        return Err(manifest::error("Updated generation identity changed"));
    }
    files::json(
        &journal.directory.join("healthy.json"),
        &serde_json::json!({"nonce":journal.nonce,"sourceSha":expected,"pid":std::process::id()}),
    )?;
    let end = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(identity) = committed(&journal)? {
            if identity != (expected.clone(), build.generation) {
                return Err(manifest::error(
                    "Update admission commit belongs to another generation",
                ));
            }
            break;
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Update worker did not commit the healthy generation",
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(io::Error::other)?;
    // The durable commit receipt forbids rollback once admission may reopen.
    // Readiness itself is published only after the actual private resume succeeds.
    let response = client
        .post(format!("http://127.0.0.1:{port}/api/updates/resume"))
        .bearer_auth(token)
        .json(&serde_json::json!({"ticket":journal.ticket}))
        .send()
        .map_err(io::Error::other)?;
    if !response.status().is_success() {
        return Err(io::Error::other(
            "Updated generation could not release its admission gate",
        ));
    }
    files::json(
        &journal.directory.join("ready.json"),
        &serde_json::json!({"nonce":journal.nonce,"sourceSha":expected,"pid":std::process::id(),"ready":true}),
    )
}
