//! Isolated, hash-pinned Rust bootstrap: repository contents are scan data.
mod contract;
use contract::{Result, fail, hash};
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
static PHASE: AtomicU64 = AtomicU64::new(0);

fn read(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit {
        return fail("Scanner input is not a bounded regular file");
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return fail("Scanner input exceeds its bound");
    }
    Ok(bytes)
}
fn emit(level: &str, message: &str) {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    println!("[{time} UTC] [{level}] [security-scan] {message}");
}
struct Private(PathBuf);
impl Private {
    fn new(parent: &Path) -> Result<Self> {
        use std::os::unix::fs::DirBuilderExt;
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..100 {
            let path = parent.join(format!(
                "numera-security-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
        fail("Private scanner namespace unavailable")
    }
    fn finish(self) -> Result<()> {
        fs::remove_dir_all(&self.0)?;
        Ok(())
    }
}
impl Drop for Private {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Captured {
    exit: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}
fn captured(command: &mut Command, seconds: u64) -> Result<Captured> {
    captured_input(command, seconds, None)
}
fn captured_input(command: &mut Command, seconds: u64, input: Option<Vec<u8>>) -> Result<Captured> {
    // GNU timeout owns a fresh command group; the runner enforces an outer job
    // deadline as well. No repository-supplied executable is admitted here.
    let mut timed = Command::new("timeout");
    timed
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_PAGER", "cat")
        .env("PAGER", "cat")
        .args(["--signal=TERM", "--kill-after=10s", &format!("{seconds}s")])
        .arg(command.get_program())
        .args(command.get_args());
    if let Some(dir) = command.get_current_dir() {
        timed.current_dir(dir);
    }
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => {
                timed.env(key, value);
            }
            None => {
                timed.env_remove(key);
            }
        }
    }
    let mut child = timed
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let writer = if let Some(bytes) = input {
        let mut pipe = child.stdin.take().ok_or("Git query input missing")?;
        Some(thread::spawn(move || pipe.write_all(&bytes)))
    } else {
        None
    };
    let stdout = child.stdout.take().ok_or("Scanner stdout missing")?;
    let stderr = child.stderr.take().ok_or("Scanner stderr missing")?;
    fn drain(mut input: impl Read) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        input
            .by_ref()
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut data)?;
        if data.len() > 4 * 1024 * 1024 {
            return Err(io::Error::other("Private diagnostic bound exceeded"));
        }
        Ok(data)
    }
    let a = thread::spawn(move || drain(stdout));
    let b = thread::spawn(move || drain(stderr));
    let status = child.wait();
    if let Some(writer) = writer {
        writer.join().map_err(|_| "Git query writer panicked")??;
    }
    let stdout = a.join().map_err(|_| "Scanner output reader panicked")??;
    let stderr = b
        .join()
        .map_err(|_| "Scanner diagnostic reader panicked")??;
    Ok(Captured {
        exit: status?.code().unwrap_or(128),
        stdout,
        stderr,
    })
}
fn real_git() -> Result<PathBuf> {
    for dir in env::split_paths(&env::var_os("PATH").ok_or("PATH missing")?) {
        let path = dir.join("git");
        if path.is_file() {
            return Ok(path.canonicalize()?);
        }
    }
    fail("Trusted runner Git executable unavailable")
}
fn private_receipt(path: &Path, value: &Value) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    Ok(())
}
fn shim() -> Result<i32> {
    let git = PathBuf::from(env::var_os("NUMERA_SCAN_REAL_GIT").ok_or("Git binding missing")?);
    let expected = env::var("NUMERA_SCAN_GIT_HASH")?;
    if hash(&read(&git, 64 * 1024 * 1024)?) != expected {
        return fail("Trusted Git binary binding differs");
    }
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let history = args.get(2).is_some_and(|s| s == "log");
    let result = Command::new(&git)
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    let exit = result.code().unwrap_or(128);
    if history {
        let path = PathBuf::from(env::var_os("NUMERA_SCAN_RECEIPT").ok_or("Git receipt missing")?);
        let nonce = env::var("NUMERA_SCAN_NONCE")?;
        let own = env::current_exe()?;
        private_receipt(
            &path,
            &json!({"schema":1,"completed":true,"exit":exit,"nonce":nonce,"git_sha256":expected,"shim_sha256":hash(&read(&own,64*1024*1024)?)}),
        )?;
    }
    Ok(exit)
}
fn validate_fixture(root: &Path) -> Result<()> {
    let dir = root.join("tools/xtask/tests/fixtures/windows-signing");
    let mut names = fs::read_dir(&dir)?
        .map(|e| e.map(|e| e.file_name()))
        .collect::<io::Result<Vec<_>>>()?;
    names.sort();
    let mut expected = contract::FIXTURES
        .iter()
        .map(|r| std::ffi::OsString::from(r.0))
        .collect::<Vec<_>>();
    expected.sort();
    if names != expected {
        return fail("Public signing fixture inventory differs");
    }
    for (name, digest, size) in contract::FIXTURES {
        let bytes = read(&dir.join(name), size)?;
        if bytes.len() as u64 != size || hash(&bytes) != digest {
            return fail("Public signing fixture differs from its reviewed bytes");
        }
    }
    Ok(())
}
fn validate_tracked_material(root: &Path, git: &Path) -> Result<()> {
    let output = captured(
        Command::new(git)
            .arg("-C")
            .arg(root)
            .args(["ls-files", "-z"]),
        15,
    )?;
    if output.exit != 0 || !output.stderr.is_empty() {
        return fail("Tracked signing-fixture scope is unavailable");
    }
    let names = std::str::from_utf8(&output.stdout)?;
    if names.is_empty() {
        return fail("Tracked scan scope is empty");
    }
    for name in names.split('\0').filter(|name| !name.is_empty()) {
        let path = Path::new(name);
        if path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return fail("Tracked scan path is unsafe");
        }
        if path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| ["pfx", "p12", "der"].contains(&x.to_ascii_lowercase().as_str()))
            && !contract::FIXTURES[..2].iter().any(|(allowed, _, _)| {
                path == Path::new("tools/xtask/tests/fixtures/windows-signing").join(allowed)
            })
        {
            return fail("Signing material is outside its exact reviewed tracked path");
        }
    }
    validate_fixture(root)
}
fn validate_historical_material(root: &Path, git: &Path) -> Result<()> {
    let head = captured(
        Command::new(git)
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--verify", "HEAD^{commit}"]),
        15,
    )?;
    if head.exit != 0
        || !head.stderr.is_empty()
        || !std::str::from_utf8(&head.stdout)
            .is_ok_and(|value| contract::canonical_sha(value.trim_end_matches('\n')))
    {
        return fail("Reviewed Git history has no canonical HEAD commit");
    }
    let shallow = captured(
        Command::new(git)
            .arg("-C")
            .arg(root)
            .args(["rev-parse", "--is-shallow-repository"]),
        15,
    )?;
    if shallow.exit != 0 || shallow.stdout != b"false\n" || !shallow.stderr.is_empty() {
        return fail("Complete signing-material history is unavailable");
    }
    let output = captured(
        Command::new(git).arg("-C").arg(root).args([
            "log",
            "--all",
            "--raw",
            "-z",
            "--no-abbrev",
            "--format=",
            "--root",
            "--no-relative",
            "--no-notes",
            "--full-history",
            "-m",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=all",
            "--",
            ":(icase,glob)**/*.pfx",
            ":(icase,glob)**/*.p12",
            ":(icase,glob)**/*.der",
            "tools/xtask/tests/fixtures/windows-signing/provenance.json",
        ]),
        30,
    )?;
    if output.exit != 0 || !output.stderr.is_empty() {
        return fail("Signing-material history producer failed");
    }
    let mut queries = std::collections::BTreeSet::new();
    for (path, old, new) in contract::history_records(&output.stdout)? {
        for oid in [old, new] {
            if oid != "0".repeat(40) && queries.insert((path.clone(), oid.clone())) {
                let packet = captured_input(
                    Command::new(git)
                        .arg("-C")
                        .arg(root)
                        .args(["cat-file", "--batch"]),
                    15,
                    Some(format!("{oid}\n").into_bytes()),
                )?;
                if packet.exit != 0 || !packet.stderr.is_empty() {
                    return fail("Historical signing-material blob read failed");
                }
                contract::history_blob(&path, &oid, &packet.stdout)?;
            }
        }
    }
    Ok(())
}
fn scan(workspace: &Path, binary: &Path, official: &Path, reviewed: &Path) -> Result<i32> {
    PHASE.store(1, Ordering::Relaxed);
    if env::var("RUNNER_DEBUG").is_ok_and(|s| s == "1") {
        return fail("Disable step debugging before scanning sensitive Git history");
    }
    let root = workspace.canonicalize()?;
    if !root.join(".git").is_dir() {
        return fail("Complete Git checkout unavailable");
    }
    validate_fixture(&root)?;
    if hash(&read(binary, 64 * 1024 * 1024)?) != contract::BINARY_HASH {
        return fail("Pinned Gitleaks executable checksum differs");
    }
    let version = captured(Command::new(binary).arg("version"), 15)?;
    if version.exit != 0
        || String::from_utf8(version.stdout)?.trim() != contract::VERSION
        || !version.stderr.is_empty()
    {
        return fail("Scanner version cannot be verified");
    }
    let parent =
        PathBuf::from(env::var_os("RUNNER_TEMP").ok_or("Runner private directory missing")?)
            .canonicalize()?;
    let private = Private::new(&parent)?;
    let sanitized = private.0.join("official.toml");
    let config = private.0.join("strict.toml");
    let (a, b) = contract::configuration(
        &read(official, 1024 * 1024)?,
        &read(reviewed, 1024 * 1024)?,
        sanitized.to_str().ok_or("Private path not Unicode")?,
    )?;
    fs::write(&sanitized, a)?;
    fs::write(&config, b)?;
    PHASE.store(3, Ordering::Relaxed);
    let event = env::var("GITHUB_EVENT_NAME")?;
    let mut options =
        "--full-history --diff-filter=uxdb --no-ext-diff --no-textconv --ignore-submodules=all -m"
            .to_owned();
    let git = real_git()?;
    let git_hash = hash(&read(&git, 64 * 1024 * 1024)?);
    let scope = if event == "pull_request" {
        let event_path =
            PathBuf::from(env::var_os("GITHUB_EVENT_PATH").ok_or("Event file unavailable")?);
        let data: Value = serde_json::from_slice(&read(&event_path, 16 * 1024 * 1024)?)?;
        let base = data["pull_request"]["base"]["sha"]
            .as_str()
            .filter(|s| contract::canonical_sha(s))
            .ok_or("Pull-request base invalid")?;
        let head = data["pull_request"]["head"]["sha"]
            .as_str()
            .filter(|s| contract::canonical_sha(s))
            .ok_or("Pull-request head invalid")?;
        for rev in [base, head] {
            if captured(
                Command::new(&git).arg("-C").arg(&root).args([
                    "cat-file",
                    "-e",
                    &format!("{rev}^{{commit}}"),
                ]),
                15,
            )?
            .exit
                != 0
            {
                return fail("Pull-request revision absent from full checkout");
            }
        }
        options.push_str(&format!(" {base}..{head}"));
        "Commits introduced by this pull request, including merge diffs"
    } else if matches!(event.as_str(), "push" | "schedule" | "workflow_dispatch") {
        options.push_str(" --all");
        "All fetched branches and tags, including full history and merge diffs"
    } else {
        return fail("Unsupported scanner event");
    };
    PHASE.store(4, Ordering::Relaxed);
    validate_tracked_material(&root, &git)?;
    validate_historical_material(&root, &git)?;
    let shim = private.0.join("git");
    fs::copy(env::current_exe()?, &shim)?;
    let shim_hash = hash(&read(&shim, 64 * 1024 * 1024)?);
    let receipt = private.0.join("git-history.json");
    let mut random = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
    let nonce = hash(&random);
    let report = private.0.join("findings.json");
    let path = env::join_paths(std::iter::once(private.0.clone()).chain(env::split_paths(
        &env::var_os("PATH").ok_or("PATH missing")?,
    )))?;
    emit(
        "INFO",
        "Starting checksum-pinned Git-history scan; values and source excerpts remain private",
    );
    PHASE.store(5, Ordering::Relaxed);
    let result = captured(
        Command::new(binary)
            .arg("git")
            .arg(root.join(".git"))
            .arg("--config")
            .arg(&config)
            .arg("--gitleaks-ignore-path")
            .arg(private.0.join("no-exclusions"))
            .args([
                "--ignore-gitleaks-allow",
                "--redact=100",
                "--no-banner",
                "--no-color",
                "--log-level=error",
                "--exit-code=2",
                "--report-format=json",
                "--report-path",
            ])
            .arg(&report)
            .arg(format!("--log-opts={options}"))
            .env("PATH", path)
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_PAGER", "cat")
            .env("PAGER", "cat")
            .env("NUMERA_SCAN_REAL_GIT", &git)
            .env("NUMERA_SCAN_GIT_HASH", &git_hash)
            .env("NUMERA_SCAN_RECEIPT", &receipt)
            .env("NUMERA_SCAN_NONCE", &nonce),
        930,
    )?;
    PHASE.store(6, Ordering::Relaxed);
    let checked = (|| -> Result<i32> {
        contract::receipt(
            &serde_json::from_slice(&read(&receipt, 65536)?)?,
            &nonce,
            &git_hash,
            &shim_hash,
        )?;
        let findings = contract::report(
            result.exit,
            &result.stdout,
            &result.stderr,
            &read(&report, 16 * 1024 * 1024)?,
        )?;
        if let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") {
            fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)?
                .write_all(contract::summary(&findings, scope).as_bytes())?;
        }
        if !findings.is_empty() {
            emit(
                "WARNING",
                &format!(
                    "{} potential secrets; inspect redacted locations in the job summary",
                    findings.len()
                ),
            );
            return Ok(1);
        }
        emit(
            "INFO",
            "Git producer completed successfully; no matching credentials found",
        );
        Ok(0)
    })();
    let cleanup = private.finish();
    match (checked, cleanup) {
        (Err(primary), Err(_)) => {
            Err(io::Error::other(format!("{primary}; private scanner cleanup failed")).into())
        }
        (Err(e), _) | (_, Err(e)) => Err(e),
        (Ok(code), Ok(())) => Ok(code),
    }
}
fn main() {
    let is_shim = env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s == "git"))
        .unwrap_or(false);
    let result = if is_shim {
        shim()
    } else {
        let args = env::args_os()
            .skip(1)
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        if args.len() != 4 {
            fail("Usage: security-scan WORKSPACE PINNED-BINARY OFFICIAL-CONFIG REVIEWED-CONFIG")
        } else {
            scan(&args[0], &args[1], &args[2], &args[3])
        }
    };
    match result {
        Ok(code) => std::process::exit(code),
        Err(_) => {
            emit(
                "ERROR",
                &format!(
                    "The verified scan could not complete; phase={}; no clean result was recorded",
                    PHASE.load(Ordering::Relaxed)
                ),
            );
            std::process::exit(1);
        }
    }
}
