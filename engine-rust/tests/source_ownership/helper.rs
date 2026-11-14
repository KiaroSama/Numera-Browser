//! Compiled native subprocess fixture. It invokes the real public ownership and
//! storage APIs, and uses files for deterministic cross-process rendezvous.
use numera_core::{
    ownership::{session::Session, tree::OwnedTree},
    storage::{
        config::ConfigStore,
        locks::{DataLock, SessionLease},
    },
};
use std::{
    fs,
    io::{self, BufRead, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn adapter(root: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    let mode = fs::read_to_string(root.join("adapter-mode")).unwrap_or_else(|_| "normal".into());
    let mut browser = None;
    for line in io::stdin().lock().lines() {
        let request: serde_json::Value = serde_json::from_str(&line?)?;
        let operation = request["operation"].as_str().ok_or("operation absent")?;
        let result = match operation {
            "hello" => {
                serde_json::json!({"backends":{"playwright":{"installed":true,"version":"native-fixture"},"patchright":{"installed":true,"version":"native-fixture"},"camoufox":{"installed":true,"version":"native-fixture"},"cloak":{"installed":true,"version":"native-fixture"}},"default_addons":[]})
            }
            "open" if mode.trim() == "fail-open" => {
                println!(
                    "{}",
                    serde_json::json!({"protocol":1,"id":request["id"],"ok":false,"error":{"kind":"sdk","type":"FixtureContextFailure"}})
                );
                io::stdout().flush()?;
                continue;
            }
            "open" => {
                let claims = fs::read_dir(root.join("sessions"))?
                    .filter_map(Result::ok)
                    .filter_map(|entry| fs::read(entry.path()).ok())
                    .filter_map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                    .collect::<Vec<_>>();
                fs::write(
                    root.join("claims-before-open.json"),
                    serde_json::to_vec(&claims)?,
                )?;
                let mut child = Command::new(std::env::current_exe()?)
                    .arg(if mode.trim() == "lock-browser" {
                        "lock-browser"
                    } else {
                        "leaf"
                    })
                    .arg(if mode.trim() == "lock-browser" {
                        root.to_owned()
                    } else {
                        root.join("browser-pid")
                    })
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?;
                wait(&root.join("browser-pid"))?;
                fs::write(root.join("page-created"), b"page")?;
                fs::write(
                    root.join("private-env"),
                    serde_json::to_vec(
                        &serde_json::json!({"update_token":std::env::var_os("NUMERA_UPDATE_TOKEN").is_some(),"update_ticket":std::env::var_os("NUMERA_UPDATE_TICKET").is_some(),"update_activation":std::env::var_os("NUMERA_UPDATE_ACTIVATION").is_some()}),
                    )?,
                )?;
                let _ = child.try_wait()?;
                browser = Some(child);
                serde_json::json!({"opened":true,"backend":request["backend"]})
            }
            "navigate" => serde_json::json!({"navigated":true}),
            "route" => serde_json::json!({"routed":true}),
            "mouse" => serde_json::json!({"performed":true}),
            "evaluate" => {
                serde_json::json!({"evaluated":true,"value":fs::read(root.join("evaluation.json")).ok().and_then(|bytes|serde_json::from_slice::<serde_json::Value>(&bytes).ok())})
            }
            "inspect" => {
                serde_json::json!({"url":"about:blank","user_agent":"native-fixture-agent"})
            }
            "cookies-read" => serde_json::json!({"cookies":[]}),
            "cookies-clear" => serde_json::json!({"cleared":true}),
            "cookies-add" => serde_json::json!({"added":true}),
            "close" => {
                if mode.trim() != "fail-close" {
                    if let Some(mut child) = browser.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
                serde_json::json!({"closing":mode.trim()!="fail-close"})
            }
            _ => return Err(io::Error::other("Unknown native fixture operation").into()),
        };
        println!(
            "{}",
            serde_json::json!({"protocol":1,"id":request["id"],"ok":true,"result":result})
        );
        io::stdout().flush()?;
        if operation == "close" {
            return Ok(0);
        }
    }
    if let Some(mut child) = browser {
        let _ = child.kill();
        let _ = child.wait();
    }
    Ok(0)
}
fn descriptors() -> io::Result<usize> {
    #[cfg(unix)]
    {
        fs::read_dir(if cfg!(target_os = "linux") {
            Path::new("/proc/self/fd")
        } else {
            Path::new("/dev/fd")
        })
        .map(|entries| entries.count())
    }
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn GetProcessHandleCount(process: *mut std::ffi::c_void, count: *mut u32) -> i32;
        }
        let mut count = 0u32;
        // SAFETY: borrowed pseudo-handle and a valid initialized scalar output.
        if unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(count as usize)
    }
}
fn cancellation_error(root: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    use numera_core::ownership::{
        test_support::{Backend, with_backend},
        unit_report,
    };
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    struct Refuse {
        request: mpsc::Sender<()>,
        acquired: Mutex<mpsc::Receiver<()>>,
        reached: AtomicBool,
        code: i32,
    }
    impl Backend for Refuse {
        fn before_spawn(&self) -> Option<io::Result<()>> {
            self.reached.store(true, Ordering::SeqCst);
            Some((|| {
                self.request.send(()).map_err(io::Error::other)?;
                self.acquired
                    .lock()
                    .map_err(|_| io::Error::other("Receipt rendezvous poisoned"))?
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(io::Error::other)?;
                Err(io::Error::from_raw_os_error(self.code))
            })())
        }
    }
    let receipt =
        std::env::var_os(unit_report::ENV).ok_or("Cancellation fixture receipt absent")?;
    let lock = DataLock::for_file(Path::new(&receipt))?;
    let (request, requested) = mpsc::channel();
    let (held, acquired) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let holder = thread::spawn(move || -> io::Result<()> {
        requested
            .recv_timeout(Duration::from_secs(3))
            .map_err(io::Error::other)?;
        let _guard = lock.acquire(Duration::from_secs(1))?;
        held.send(()).map_err(io::Error::other)?;
        released
            .recv_timeout(Duration::from_secs(8))
            .map_err(io::Error::other)?;
        Ok(())
    });
    let code = if cfg!(windows) { 5 } else { 13 };
    let original = io::Error::from_raw_os_error(code);
    let hook = Arc::new(Refuse {
        request,
        acquired: Mutex::new(acquired),
        reached: AtomicBool::new(false),
        code,
    });
    let mut session = Session::admit(root, "p", "cancellation-fixture")?;
    let work = fs::read_dir(root.join(".updates/work"))?
        .next()
        .ok_or("Admission receipt absent")??
        .path();
    let before = fs::read(&work)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["emit", "unexpected", "0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let started = Instant::now();
    let outcome = with_backend(hook.clone(), || {
        session
            .spawn(&mut command, "inert fixture")
            .map(|tree| tree.pid())
    });
    let elapsed = started.elapsed();
    release.send(())?;
    holder
        .join()
        .map_err(|_| "Receipt cancellation lock holder panicked")??;
    assert!(hook.reached.load(Ordering::SeqCst));
    let error = outcome.err().ok_or("Pre-birth failure was accepted")?;
    assert!(
        error.kind() == original.kind(),
        "Cancellation failure replaced the original OS error kind"
    );
    assert!(
        error.raw_os_error() == original.raw_os_error(),
        "Cancellation failure replaced the original raw OS error"
    );
    assert!(
        error.to_string() == original.to_string(),
        "Cancellation failure replaced the original error display"
    );
    assert!(
        elapsed < Duration::from_secs(7),
        "Failed receipt cancellation exceeded its existing five-second lock window"
    );
    drop(session);
    assert!(
        fs::read(&work)? == before,
        "Failed cancellation released or changed its admission receipt"
    );
    let report = unit_report::read(Path::new(&receipt))?;
    assert!(
        !report.complete,
        "Failed cancellation invented a completed receipt"
    );
    assert!(
        report.trees.is_empty(),
        "Pre-birth refusal invented a process tree"
    );
    assert!(matches!(
        numera_core::ownership::claims::read(root, "p"),
        numera_core::ownership::claims::Claim::Absent
    ));
    Ok(0)
}
#[cfg(unix)]
fn uncertain_spawn(root: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    use std::{
        ffi::CString,
        os::{
            fd::AsRawFd,
            unix::{ffi::OsStrExt, process::CommandExt},
        },
    };
    let marker = fs::File::create(root.join("forked-child"))?;
    let descriptor = marker.as_raw_fd();
    let mut command = Command::new(std::env::current_exe()?);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: the callback uses only async-signal-safe OS calls and stack data.
    // It deliberately leaves a real descendant, then reports ENOENT before exec.
    unsafe {
        command.pre_exec(move || {
            let descendant = libc::fork();
            if descendant < 0 {
                return Err(io::Error::last_os_error());
            }
            if descendant == 0 {
                libc::close(descriptor);
                loop {
                    libc::pause();
                }
            }
            let row = [libc::getpid(), descendant];
            if libc::write(descriptor, row.as_ptr().cast(), std::mem::size_of_val(&row))
                != std::mem::size_of_val(&row) as isize
            {
                libc::kill(descendant, libc::SIGKILL);
                libc::waitpid(descendant, std::ptr::null_mut(), 0);
                return Err(io::Error::last_os_error());
            }
            Err(io::Error::from_raw_os_error(libc::ENOENT))
        });
    }
    let outcome = OwnedTree::spawn(&mut command);
    let row = fs::read(root.join("forked-child"))?;
    let leader = i32::from_ne_bytes(row.get(..4).ok_or("Fork leader missing")?.try_into()?);
    let descendant = u32::try_from(i32::from_ne_bytes(
        row.get(4..8).ok_or("Fork descendant missing")?.try_into()?,
    ))?;
    let token = numera_core::ownership::identity::started_at(descendant)?
        .ok_or("Real fork descendant vanished")?;
    let alive = numera_core::ownership::identity::alive(descendant, Some(token))?;
    // SAFETY: this controlled paused descendant cannot exit/recycle itself;
    // its exact observed birth still matches immediately before termination.
    if alive && unsafe { libc::kill(i32::try_from(descendant)?, libc::SIGKILL) } != 0 {
        return Err(io::Error::last_os_error().into());
    }
    let tree = serde_json::json!({"tree":{"kind":"pgid","group":leader}});
    let end = Instant::now() + Duration::from_secs(2);
    while !numera_core::ownership::tree::record_empty(&tree)? {
        if Instant::now() >= end {
            return Err("Fork control tree did not settle".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        alive,
        "Post-fork failure control never had its real live descendant"
    );
    let error = outcome.err().ok_or("Post-fork ENOENT was accepted")?;
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(error.raw_os_error(), Some(libc::ENOENT));
    let path = std::env::var_os(numera_core::ownership::unit_report::ENV)
        .ok_or("Negative fixture receipt absent")?;
    let report = numera_core::ownership::unit_report::read(Path::new(&path))?;
    assert!(
        !report.complete,
        "Post-fork ENOENT resolved an uncertain spawn intent"
    );
    assert!(
        report.trees.is_empty(),
        "Failed generic spawn invented a tree identity"
    );

    // A generic factory can create its executable in pre_exec. Builtin worker
    // validation must never become an unconditional Command path preflight.
    let destination = root.join("callback-created-executable");
    let source = CString::new(std::env::current_exe()?.as_os_str().as_bytes())?;
    let target = CString::new(destination.as_os_str().as_bytes())?;
    let mut command = Command::new(&destination);
    command
        .args(["emit", "created", "0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: stable NUL-terminated buffers and one async-signal-safe OS call.
    unsafe {
        command.pre_exec(move || {
            if libc::symlink(source.as_ptr(), target.as_ptr()) == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        });
    }
    let mut owner = OwnedTree::spawn(&mut command)?;
    owner.settle(Duration::from_secs(2))?;
    assert!(destination.is_file());
    assert!(
        !numera_core::ownership::unit_report::read(Path::new(&path))?.complete,
        "A later successful spawn cancelled an unrelated uncertain attempt"
    );
    Ok(0)
}
fn capture_initialization_diagnostic() -> io::Result<serde_json::Value> {
    #[cfg(windows)]
    {
        // Diagnostics deliberately follow the existing baseline. Equality
        // assertions stay unchanged until the owning Windows run classifies
        // every persistent initialization handle; no tolerance is introduced.
        let cold = descriptors()?;
        let mut entropy = [0u8; 16];
        getrandom::fill(&mut entropy).map_err(io::Error::other)?;
        let rng_first = descriptors()?;
        getrandom::fill(&mut entropy).map_err(io::Error::other)?;
        let rng_second = descriptors()?;
        let mut pipe_closed = Vec::new();
        for _ in 0..2 {
            // Independent std primitives, not production capture. Rust1.99.1
            // child_pipe.rs caches exactly one NamedPipe filesystem handle.
            let mut child = Command::new(std::env::current_exe()?)
                .args(["emit", "warm", "0"])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            let status = match child.wait() {
                Ok(status) => status,
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(error);
                }
            };
            drop(child);
            if !status.success() {
                return Err(io::Error::other(
                    "Independent stdio diagnostic fixture failed",
                ));
            }
            pipe_closed.push(descriptors()?);
        }
        Ok(
            serde_json::json!({"cold":cold,"rng_first":rng_first,"rng_second":rng_second,"stdio_closed":pipe_closed}),
        )
    }
    #[cfg(not(windows))]
    {
        Ok(serde_json::Value::Null)
    }
}
fn fd_capture(timeout: bool) -> Result<i32, Box<dyn std::error::Error>> {
    let before = descriptors()?;
    let initialization = capture_initialization_diagnostic()?;
    #[cfg(target_os = "linux")]
    let threads_before = fs::read_dir("/proc/self/task")?.count();
    let mut child = Command::new(std::env::current_exe()?);
    if timeout {
        child.arg("sleep");
    } else {
        child.args(["emit", "done", "0"]);
    }
    let budget = if timeout {
        Duration::from_millis(500)
    } else {
        Duration::from_secs(3)
    };
    let result = numera_core::ownership::capture::run(
        &mut child,
        numera_core::ownership::capture::Limits {
            wall: budget,
            idle: budget,
            bytes: 65536,
            cleanup: Duration::from_secs(2),
        },
    );
    let timed_out = result
        .as_ref()
        .err()
        .is_some_and(|error| error.kind() == io::ErrorKind::TimedOut);
    if !timeout {
        assert_eq!(result?.stdout, b"done");
    }
    let after = descriptors()?;
    #[cfg(target_os = "linux")]
    let threads_unchanged = Some(fs::read_dir("/proc/self/task")?.count() == threads_before);
    #[cfg(not(target_os = "linux"))]
    let threads_unchanged: Option<bool> = None;
    println!(
        "{}",
        serde_json::json!({"before":before,"after":after,"timed_out":timed_out,"threads_unchanged":threads_unchanged,"initialization":initialization})
    );
    Ok(0)
}
fn fd_failure(marker: &Path) -> Result<i32, Box<dyn std::error::Error>> {
    use numera_core::ownership::{
        capture, identity,
        test_support::{Backend, with_backend},
    };
    use std::sync::{Arc, Mutex};
    // Establish the OS handle cost of an owned, uncaptured child separately.
    let mut control = Command::new(std::env::current_exe()?);
    control
        .arg("sleep")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    {
        let mut warm = OwnedTree::spawn(&mut control)?;
        warm.settle(Duration::from_secs(2))?;
    }
    let baseline = descriptors()?;
    let owner_extra = {
        let mut tree = OwnedTree::spawn(&mut control)?;
        let extra = descriptors()?.saturating_sub(baseline);
        tree.settle(Duration::from_secs(2))?;
        extra
    };
    let before = descriptors()?;
    let initialization = capture_initialization_diagnostic()?;
    struct Refused {
        marker: std::path::PathBuf,
        record: Mutex<Option<serde_json::Value>>,
    }
    impl Backend for Refused {
        fn spawned(&self, record: &serde_json::Value) -> Option<io::Result<()>> {
            *self.record.lock().expect("born record") = Some(record.clone());
            let mut record = record.clone();
            record["pidToken"] = record["token"].take();
            Some(numera_core::storage::atomic::write_json(
                &self.marker,
                &record,
            ))
        }
        fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
            Some(Ok(false))
        }
        fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
            Some(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "controlled cleanup refusal",
            )))
        }
    }
    let observer = Arc::new(Refused {
        marker: marker.into(),
        record: Mutex::new(None),
    });
    let mut child = Command::new(std::env::current_exe()?);
    child.arg("sleep");
    let begin = Instant::now();
    let mut failure = with_backend(observer.clone(), || {
        capture::run(
            &mut child,
            capture::Limits {
                wall: Duration::from_millis(150),
                idle: Duration::from_millis(150),
                bytes: 65536,
                cleanup: Duration::from_millis(100),
            },
        )
    })
    .err()
    .ok_or("cleanup refusal was returned as success")?;
    let record = observer
        .record
        .lock()
        .expect("record")
        .clone()
        .ok_or("child was not started")?;
    let pid = record["pid"].as_u64().ok_or("pid absent")? as u32;
    let writer_live = identity::alive(pid, record["token"].as_u64())?;
    let retained = failure.retained_record().is_some();
    let before_cleanup = descriptors()?;
    let elapsed = begin.elapsed();
    failure.settle(Duration::from_secs(2))?;
    drop(failure);
    let after_cleanup = descriptors()?;
    println!(
        "{}",
        serde_json::json!({"before":before,"owner_extra":owner_extra,"before_cleanup":before_cleanup,"after_cleanup":after_cleanup,"writer_live":writer_live,"retained":retained,"elapsed_ms":elapsed.as_millis(),"initialization":initialization})
    );
    Ok(0)
}
fn fd_pipe_failure(stage: &str) -> Result<i32, Box<dyn std::error::Error>> {
    use numera_core::ownership::{
        capture, identity,
        test_support::{Backend, with_backend},
    };
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    struct Fault {
        stage: String,
        born: Mutex<Option<serde_json::Value>>,
        prepared: AtomicUsize,
        reads: AtomicUsize,
    }
    impl Backend for Fault {
        fn spawned(&self, record: &serde_json::Value) -> Option<io::Result<()>> {
            *self.born.lock().expect("born") = Some(record.clone());
            (self.stage == "spawned")
                .then(|| Err(io::Error::other("controlled spawned ownership failure")))
        }
        fn pipe_prepared(&self, index: usize) -> Option<io::Result<()>> {
            self.prepared.fetch_add(1, Ordering::SeqCst);
            (self.stage == "prepare" && index == 1).then(|| {
                Err(io::Error::other(
                    "controlled second pipe preparation failure",
                ))
            })
        }
        fn pipe_read(&self, index: usize) -> Option<io::Result<()>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            (self.stage == "read" && index == 1)
                .then(|| Err(io::Error::other("controlled stderr read failure")))
        }
    }
    let before = descriptors()?;
    let initialization = capture_initialization_diagnostic()?;
    let fault = Arc::new(Fault {
        stage: stage.into(),
        born: Mutex::new(None),
        prepared: AtomicUsize::new(0),
        reads: AtomicUsize::new(0),
    });
    let mut child = Command::new(std::env::current_exe()?);
    child.arg("sleep");
    let begin = Instant::now();
    let mut failure = with_backend(fault.clone(), || {
        capture::run(
            &mut child,
            capture::Limits {
                wall: Duration::from_secs(3),
                idle: Duration::from_secs(3),
                bytes: 65536,
                cleanup: Duration::from_secs(1),
            },
        )
    })
    .err()
    .ok_or("pipe failure was accepted")?;
    let first = failure.to_string();
    let kind = failure.kind();
    for _ in 0..3 {
        failure.settle(Duration::from_millis(20))?;
        assert_eq!(failure.to_string(), first);
        assert_eq!(failure.kind(), kind);
    }
    let record = fault
        .born
        .lock()
        .expect("born")
        .clone()
        .ok_or("no actual child")?;
    let pid = record["pid"].as_u64().ok_or("no PID")? as u32;
    let child_live = identity::alive(pid, record["token"].as_u64())?;
    let after = descriptors()?;
    println!(
        "{}",
        serde_json::json!({"before":before,"after":after,"child_live":child_live,"error":first,"prepared":fault.prepared.load(Ordering::SeqCst),"reads":fault.reads.load(Ordering::SeqCst),"elapsed_ms":begin.elapsed().as_millis(),"initialization":initialization})
    );
    Ok(0)
}
fn receipt_run(root: &Path, damage: &str) -> Result<i32, Box<dyn std::error::Error>> {
    use numera_core::ownership::{
        capture,
        capture_units::Units,
        identity, recorded_tree,
        test_support::{Backend, with_backend},
        unit_report,
    };
    use std::sync::Arc;
    let mut command = Command::new(std::env::current_exe()?);
    if damage == "create" {
        fs::write(root.join("jobs"), b"obstruct receipt directory")?;
        command.arg("sleep").arg(root.join("unexpected-child"));
        let refused = Units::new(root, &mut command).is_err();
        println!(
            "{}",
            serde_json::json!({"refused":refused,"spawned":root.join("unexpected-child").exists()})
        );
        return Ok(0);
    }
    if damage == "spawn" {
        command = Command::new(root.join("unavailable-native-command"));
    } else if damage == "remove" {
        command.args(["emit", "complete", "0"]);
    } else if damage == "leader" {
        command
            .arg("closed-writers")
            .arg(root.join("producer-born"));
    } else {
        command.arg("damage-nested").arg(root).arg(damage);
    }
    let mut units = Units::new(root, &mut command)?;
    let receipt = command
        .get_envs()
        .find_map(|(key, value)| {
            (key == unit_report::ENV)
                .then(|| value.map(std::path::PathBuf::from))
                .flatten()
        })
        .ok_or("receipt env absent")?;
    let before = descriptors()?;
    let begin = Instant::now();
    let limits = || capture::Limits {
        wall: Duration::from_millis(500),
        idle: Duration::from_millis(500),
        bytes: 65536,
        cleanup: Duration::from_millis(400),
    };
    struct Refused {
        root: std::path::PathBuf,
        leader: bool,
    }
    impl Backend for Refused {
        fn group_empty(&self, group: i32) -> Option<io::Result<bool>> {
            if self.leader {
                return Some(Ok(false));
            }
            fs::read(self.root.join("escaped.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|record| {
                    (record["pid"].as_u64() == Some(group as u64)).then_some(Ok(false))
                })
        }
        fn signal(&self, target: i32, _: bool) -> Option<io::Result<()>> {
            if self.leader {
                return Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled producer cleanup refusal",
                )));
            }
            fs::read(self.root.join("escaped.json"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                .and_then(|record| {
                    (record["pid"].as_i64() == Some(-i64::from(target))).then(|| {
                        Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "controlled nested cleanup refusal",
                        ))
                    })
                })
        }
    }
    let answer = if damage == "tree" || damage == "leader" {
        with_backend(
            Arc::new(Refused {
                root: root.into(),
                leader: damage == "leader",
            }),
            || units.run(&mut command, limits(), || false),
        )
    } else if damage == "remove" {
        numera_core::vault::testing::with_observer(
            |event| {
                if event.operation == "before-capture-report-remove" {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "controlled receipt removal refusal",
                    ))
                } else {
                    Ok(numera_core::vault::testing::Action::Continue)
                }
            },
            || units.run(&mut command, limits(), || false),
        )
    } else {
        units.run(&mut command, limits(), || false)
    };
    let elapsed = begin.elapsed();
    let failed = answer.is_err();
    let diagnostic = answer.as_ref().err().map(ToString::to_string);
    let pending = units.pending();
    let after = descriptors()?;
    let retained = answer
        .as_ref()
        .err()
        .and_then(|failure| failure.retained_record())
        .is_some();
    let producer_live = if damage == "leader" {
        let pid: u32 = fs::read_to_string(root.join("producer-born"))?.parse()?;
        identity::alive(pid, identity::started_at(pid)?)?
    } else {
        false
    };
    let receipt_exists = receipt.is_file();
    let before_retry_pending = units.pending();
    // Preserve acceptance observations before this explicit fixture retry.
    let mut answer = answer;
    if let Err(failure) = &mut answer {
        failure.settle(Duration::from_secs(1))?;
    }
    if let Ok(bytes) = fs::read(root.join("escaped.json")) {
        let record: serde_json::Value = serde_json::from_slice(&bytes)?;
        let _ = recorded_tree::settle(&record, Duration::from_secs(1));
    }
    if matches!(damage, "missing" | "malformed" | "utf8") {
        if receipt.exists() {
            fs::remove_file(&receipt)?;
        }
        unit_report::initialize(&receipt)?;
    }
    units.settle(Duration::from_secs(1))?;
    println!(
        "{}",
        serde_json::json!({"failed":failed,"diagnostic":diagnostic,"pending":pending,"retained_tree":retained,"producer_live":producer_live,"receipt_exists":receipt_exists,"before":before,"after":after,"elapsed_ms":elapsed.as_millis(),"before_retry_pending":before_retry_pending,"after_retry_pending":units.pending()})
    );
    Ok(0)
}
fn wait(path: &Path) -> io::Result<()> {
    let end = Instant::now() + Duration::from_secs(30);
    while !path.is_file() {
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Fixture rendezvous expired",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn run() -> Result<i32, Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|argument| argument == "--data-dir")
    {
        if args.len() < 3 {
            return Err(io::Error::other("fixture data-root argument is incomplete").into());
        }
        args.drain(..2);
    }
    if args.is_empty() {
        return adapter(&std::env::current_dir()?);
    }
    match args[0].as_str() {
        "--metadata" => {
            println!(
                "{}",
                serde_json::json!({"backends":{"playwright":{"installed":true},"patchright":{"installed":true},"camoufox":{"installed":true},"cloak":{"installed":true}},"default_addons":[]})
            );
            Ok(0)
        }

        "closed-writers" => {
            fs::write(&args[1], std::process::id().to_string())?;
            #[cfg(unix)]
            {
                unsafe extern "C" {
                    fn close(fd: i32) -> i32;
                }
                // SAFETY: this isolated fixture owns its redirected standard
                // pipe writers and performs no further stream I/O afterwards.
                if unsafe { close(1) } != 0 || unsafe { close(2) } != 0 {
                    return Err(io::Error::last_os_error().into());
                }
            }
            #[cfg(windows)]
            {
                #[link(name = "kernel32")]
                unsafe extern "system" {
                    fn GetStdHandle(kind: u32) -> *mut std::ffi::c_void;
                    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
                }
                // SAFETY: two redirected owned writer handles, closed only in
                // this isolated fixture; no later stream I/O is performed.
                for kind in [(-11i32) as u32, (-12i32) as u32] {
                    let handle = unsafe { GetStdHandle(kind) };
                    if handle.is_null()
                        || handle as isize == -1
                        || unsafe { CloseHandle(handle) } == 0
                    {
                        return Err(io::Error::last_os_error().into());
                    }
                }
            }
            thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        "lock-renderer" => {
            let root = Path::new(&args[1]);
            let lock = DataLock::new(root.join("renderer.lock"))?;
            let _guard = lock.acquire(Duration::from_secs(1))?;
            fs::write(root.join("renderer-pid"), std::process::id().to_string())?;
            thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        "lock-browser" => {
            let root = Path::new(&args[1]);
            let lock = DataLock::new(root.join("browser.lock"))?;
            let _guard = lock.acquire(Duration::from_secs(1))?;
            let mut renderer = Command::new(std::env::current_exe()?)
                .arg("lock-renderer")
                .arg(root)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            wait(&root.join("renderer-pid"))?;
            let _ = renderer.try_wait()?;
            fs::write(root.join("browser-pid"), std::process::id().to_string())?;
            thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        "native-serve" => {
            let root = Path::new(&args[1]);
            let browser = numera_core::browser::runtime::Runtime {
                adapter: std::env::current_exe()?,
                cwd: root.into(),
                home: root.join("home"),
                logs: root.join("logs"),
                country: None,
                relay: None,
                binary_options: Default::default(),
                registry: None,
                deployment: None,
            };
            let environment = numera_core::native_environment::Environment {
                app: root.into(),
                home: root.join("home"),
                cwd: root.into(),
                host: "source-host".into(),
                browser,
            };
            struct Announce {
                path: std::path::PathBuf,
                bytes: Vec<u8>,
            }
            impl Write for Announce {
                fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                    self.bytes.extend_from_slice(bytes);
                    if let Ok(text) = std::str::from_utf8(&self.bytes) {
                        if let Some(address) = text
                            .strip_prefix("Numera listening on http://")
                            .and_then(|text| text.split_once('\n').map(|v| v.0))
                        {
                            fs::write(&self.path, address)?;
                        }
                    }
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> io::Result<()> {
                    Ok(())
                }
            }
            let mut output = numera_core::cli::output::Output::new(
                Announce {
                    path: root.join("server-address"),
                    bytes: Vec::new(),
                },
                io::sink(),
                false,
                false,
            );
            let args =
                serde_json::json!({"command":"serve","host":"127.0.0.1","port":0,"json":false});
            Ok(numera_core::native_entry::dispatch(
                args.as_object().ok_or("arguments")?,
                &[],
                &mut output,
                environment,
                root,
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                &numera_core::runtime_log::RuntimeLog::new(&root.join("logs"), "fixture-entry"),
            )?)
        }
        "partial-writer" => {
            io::stdout().write_all(b"PARTIAL")?;
            io::stdout().flush()?;
            thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        "panic-capture" => {
            use numera_core::ownership::{capture, identity, recorded_tree};
            let before = descriptors()?;
            let mut child = Command::new(std::env::current_exe()?);
            child.arg("sleep");
            let mut born = None;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                capture::run_observed(
                    &mut child,
                    capture::Limits {
                        wall: Duration::from_secs(2),
                        idle: Duration::from_secs(2),
                        bytes: 4096,
                        cleanup: Duration::from_secs(1),
                    },
                    || false,
                    |event, _| {
                        if let capture::Observation::Spawned { record } = event {
                            born = Some(record);
                            panic!("controlled request unwinding");
                        }
                    },
                )
            }));
            let mut record = born.ok_or("panic fixture never spawned")?;
            record["pidToken"] = record["token"].take();
            let gone = !identity::alive(
                record["pid"].as_u64().ok_or("pid")? as u32,
                record["pidToken"].as_u64(),
            )?;
            println!(
                "{}",
                serde_json::json!({"panicked":result.is_err(),"child_gone":gone,"tree_empty":recorded_tree::empty(&record)?,"before":before,"after":descriptors()?})
            );
            Ok(0)
        }
        "receipt-run" => receipt_run(Path::new(&args[1]), &args[2]),
        "damage-nested" => {
            let root = Path::new(&args[1]);
            let mut command = Command::new(std::env::current_exe()?);
            command.arg("leaf").arg(root.join("nested-born"));
            let child = OwnedTree::spawn(&mut command)?;
            let mut record = child.record();
            record["pidToken"] = record["token"].take();
            numera_core::storage::atomic::write_json(&root.join("escaped.json"), &record)?;
            std::mem::forget(child);
            wait(&root.join("nested-born"))?;
            let report = std::env::var_os(numera_core::ownership::unit_report::ENV)
                .ok_or("nested report missing")?;
            match args[2].as_str() {
                "missing" => fs::remove_file(report)?,
                "malformed" => fs::write(report, b"17\n")?,
                "utf8" => fs::write(report, [255])?,
                "tree" => {}
                _ => return Err(io::Error::other("unknown receipt damage").into()),
            };
            io::stdout().write_all(b"TAIL")?;
            io::stdout().flush()?;
            Ok(0)
        }
        "fd-pipe-failure" => fd_pipe_failure(&args[1]),
        "fd-failure" => fd_failure(Path::new(&args[1])),
        "fd-capture" => fd_capture(args.get(1).is_some_and(|value| value == "timeout")),
        "adapter" => adapter(
            args.get(1)
                .map(Path::new)
                .unwrap_or(&std::env::current_dir()?),
        ),
        "ready" => {
            let path = Path::new(&args[1]);
            match args[2].as_str() {
                "ok" => {
                    numera_core::storage::atomic::write_json(path, &serde_json::json!({"ok":true}))?
                }
                "locked" => numera_core::storage::atomic::write_json(
                    path,
                    &serde_json::json!({"ok":false,"kind":"locked","error":"the vault is locked"}),
                )?,
                "silent" => return Ok(3),
                "wait" => thread::sleep(Duration::from_secs(30)),
                _ => return Err(io::Error::other("Unknown readiness mode").into()),
            };
            if let Some(release) = args.get(3) {
                wait(Path::new(release))?;
            }
            Ok(args
                .get(4)
                .and_then(|value| value.parse().ok())
                .unwrap_or(0))
        }
        "watch" => {
            let parent = numera_core::ownership::parent::Monitor::from_environment()?
                .ok_or("Parent monitor not created")?;
            fs::write(&args[1], b"watching")?;
            let end = Instant::now() + Duration::from_secs(20);
            while parent.alive()? && !Path::new(&args[2]).exists() {
                if Instant::now() >= end {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Parent fixture did not finish",
                    )
                    .into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(0)
        }
        "emit" => {
            let payload = match args[1].as_str() {
                "empty" => String::new(),
                "tail" => "tail".into(),
                "unicode" => "سلام\nΩ\r\nZ\rW".into(),
                "large" => "x".repeat(200000),
                value => value.into(),
            };
            io::stdout().write_all(payload.as_bytes())?;
            io::stderr().write_all(payload.as_bytes())?;
            Ok(args[2].parse()?)
        }
        "cancellation-error" => cancellation_error(Path::new(&args[1])),
        #[cfg(unix)]
        "uncertain-spawn" => uncertain_spawn(Path::new(&args[1])),
        "leaf" => {
            fs::write(&args[1], std::process::id().to_string())?;
            thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        "lease-leaf" => {
            let lock = DataLock::new(args[3].clone().into())?;
            let _held = lock.acquire(Duration::from_secs(2))?;
            fs::write(&args[1], std::process::id().to_string())?;
            wait(Path::new(&args[2]))?;
            Ok(0)
        }
        "plain-orphan" => {
            let root = Path::new(&args[1]);
            let mut child = Command::new(std::env::current_exe()?)
                .arg("lease-leaf")
                .arg(root.join("orphan-pid"))
                .arg(root.join("orphan-release"))
                .arg(root.join("orphan.lock"))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            wait(&root.join("orphan-pid"))?;
            let _ = child.try_wait()?;
            Ok(0)
        }
        "membership" => {
            let claim = Path::new(&args[1])
                .join("sessions")
                .join(format!("{}.json", args[2]));
            wait(&claim)?;
            let record: serde_json::Value = serde_json::from_slice(&fs::read(&claim)?)?;
            let same_pid = record["pid"].as_u64() == Some(u64::from(std::process::id()));
            #[cfg(unix)]
            let member = {
                unsafe extern "C" {
                    fn getpgrp() -> i32;
                }
                // SAFETY: read-only scalar kernel identity, no pointers.
                record["tree"]["kind"] == "pgid"
                    && record["tree"]["group"].as_i64() == Some(i64::from(unsafe { getpgrp() }))
            };
            #[cfg(windows)]
            let member = {
                #[link(name = "kernel32")]
                unsafe extern "system" {
                    fn GetCurrentProcess() -> *mut std::ffi::c_void;
                    fn OpenJobObjectW(
                        access: u32,
                        inherit: i32,
                        name: *const u16,
                    ) -> *mut std::ffi::c_void;
                    fn IsProcessInJob(
                        process: *mut std::ffi::c_void,
                        job: *mut std::ffi::c_void,
                        result: *mut i32,
                    ) -> i32;
                    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
                }
                let name = record["tree"]["group"].as_str().ok_or("job name absent")?;
                let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                // SAFETY: trusted claim name is NUL terminated; the returned
                // query-only handle is closed on both success/error paths.
                let job = unsafe { OpenJobObjectW(4, 0, wide.as_ptr()) };
                if job.is_null() {
                    return Err(io::Error::last_os_error().into());
                }
                let mut member = 0;
                let ok = unsafe { IsProcessInJob(GetCurrentProcess(), job, &mut member) };
                let error = (ok == 0).then(io::Error::last_os_error);
                unsafe { CloseHandle(job) };
                if let Some(error) = error {
                    return Err(error.into());
                }
                member != 0
            };
            println!(
                "{}",
                serde_json::json!({"claim_preceded_observation":same_pid,"same_kernel_tree":member})
            );
            Ok(0)
        }
        "tree" => {
            let root = Path::new(&args[1]);
            let marker = root.join("descendant");
            let mut child = Command::new(std::env::current_exe()?)
                .arg("leaf")
                .arg(&marker)
                .spawn()?;
            wait(&marker)?;
            fs::write(root.join("leader"), std::process::id().to_string())?;
            io::stdout().write_all(b"TAIL")?;
            io::stdout().flush()?;
            if args[2] == "hold" {
                thread::sleep(Duration::from_secs(30));
            } else {
                let _ = child.try_wait()?;
            }
            Ok(0)
        }
        "reported-tree" => {
            let root = Path::new(&args[1]);
            let marker = root.join("descendant");
            let mut command = Command::new(std::env::current_exe()?);
            command.arg("leaf").arg(&marker);
            if args[2] == "separate" {
                let child = OwnedTree::spawn(&mut command)?;
                let mut record = child.record();
                record["pidToken"] = record["token"].take();
                numera_core::storage::atomic::write_json(&root.join("descendant.json"), &record)?;
                // The caller must settle this separately owned, durably
                // reported tree. Ending it in fixture teardown would hide a
                // broken capture boundary.
                std::mem::forget(child);
            } else {
                let mut child = command.spawn()?;
                let token = numera_core::ownership::identity::started_at(child.id())?;
                numera_core::storage::atomic::write_json(
                    &root.join("descendant.json"),
                    &serde_json::json!({"pid":child.id(),"pidToken":token}),
                )?;
                let _ = child.try_wait()?;
            }
            wait(&marker)?;
            io::stdout().write_all(b"TAIL")?;
            io::stdout().flush()?;
            if args[3] == "hold" {
                thread::sleep(Duration::from_secs(30));
            }
            Ok(0)
        }
        "sleep" => {
            if let Some(marker) = args.get(1) {
                fs::write(marker, std::process::id().to_string())?;
            }
            thread::sleep(Duration::from_secs(30));
            Ok(0)
        }
        "lock" => {
            let lock = DataLock::new(args[1].clone().into())?;
            match lock.acquire(Duration::from_millis(60)) {
                Ok(_guard) => println!("entered"),
                Err(error) if error.kind() == io::ErrorKind::TimedOut => println!("timeout"),
                Err(error) => return Err(error.into()),
            };
            Ok(0)
        }
        "hold-lock" => {
            let lock = DataLock::new(args[1].clone().into())?;
            let _guard = lock.acquire(Duration::from_secs(3))?;
            fs::write(&args[2], b"held")?;
            wait(Path::new(&args[3]))?;
            Ok(0)
        }
        "hold-lease" => {
            let path = SessionLease::path_for(Path::new(&args[1]), &args[2])?;
            let _lease = SessionLease::acquire(&path)?;
            fs::write(&args[3], b"held")?;
            wait(Path::new(&args[4]))?;
            Ok(0)
        }
        "config" => {
            let root = Path::new(&args[1]);
            wait(Path::new(&args[3]))?;
            let store = ConfigStore::new(root.into());
            for n in 0..8 {
                store.set(&format!("{}-{n}", args[2]), n.into())?;
            }
            Ok(0)
        }
        "cross-write" => {
            let root = Path::new(&args[1]);
            let who: usize = args[2].parse()?;
            fs::write(root.join(format!("writer-ready-{who}")), b"ready")?;
            wait(Path::new(&args[3]))?;
            let repo = numera_core::storage::repo::ProfileRepo::new(root.into(), None);
            match args[4].as_str() {
                "profile" => {
                    repo.edit(
                        &format!("p{who}"),
                        |profile| {
                            profile.notes = format!("w{who}");
                            Ok(None)
                        },
                        1001.0,
                    )?;
                }
                "pool" => {
                    numera_core::storage::pool::ProxyPool::new(root.into(), None).add_many(vec![serde_json::json!({"type":"HTTP","host":format!("10.0.0.{who}"),"port":"8080","user":"source-user","pass":"source-secret"})])?;
                }
                "schedule" => {
                    numera_core::storage::schedules::ScheduleStore::new(root.into()).add(
                        &format!("w{who}"),
                        serde_json::json!({"everyHours":6,"minutes":30}),
                        Some(1001.0),
                    )?;
                }
                "vault" if who == 0 => {
                    let mut repo = repo;
                    let pause_root = root.to_owned();
                    numera_core::vault::testing::with_observer(
                        move |event| {
                            if event.operation == "before-migration" {
                                fs::write(pause_root.join("migration-active"), b"held")?;
                                wait(&pause_root.join("editor-started"))?;
                            }
                            Ok(numera_core::vault::testing::Action::Continue)
                        },
                        || repo.enable_vault("source-master"),
                    )?;
                }
                "vault" => {
                    wait(&root.join("migration-active"))?;
                    fs::write(root.join("editor-started"), b"trying")?;
                    let result = repo.edit(
                        "p3",
                        |profile| {
                            profile.notes = "edited".into();
                            Ok(None)
                        },
                        1001.0,
                    );
                    // A locked writer may refuse; it must never put plaintext
                    // back into the generation committed by the migration.
                    fs::write(
                        root.join("editor-outcome"),
                        if result.is_ok() {
                            b"written".as_slice()
                        } else {
                            b"refused".as_slice()
                        },
                    )?;
                }
                _ => return Err(io::Error::other("Unknown cross-process store fixture").into()),
            }
            Ok(0)
        }
        "session" => {
            let root = Path::new(&args[1]);
            let mut session = Session::admit(root, &args[2], "source-host")?;
            let mut command = Command::new(std::env::current_exe()?);
            command
                .arg("tree")
                .arg(root)
                .arg("hold")
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            session.spawn(&mut command, "source-browser")?;
            wait(&root.join("leader"))?;
            fs::write(root.join("session-ready"), b"ready")?;
            wait(&root.join("release"))?;
            session.finish(Duration::from_secs(2))?;
            Ok(0)
        }
        "owned-exit" => {
            let mut command = Command::new(std::env::current_exe()?);
            command
                .arg("sleep")
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            let mut tree = OwnedTree::spawn(&mut command)?;
            println!("{}", tree.pid());
            tree.settle(Duration::from_secs(2))?;
            Ok(0)
        }
        _ => Err(io::Error::other("Unknown native fixture mode").into()),
    }
}
fn main() {
    if let Some(path) = std::env::var_os("NUMERA_NATIVE_SUPERVISE") {
        match numera_core::job_supervisor::run_internal(Path::new(&path)) {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("Native fixture supervisor failed: {error}");
                std::process::exit(111);
            }
        }
    }
    if let Some(path) = std::env::var_os("NUMERA_NATIVE_JOB_GATE") {
        if let Err(error) = numera_core::job_supervisor::await_gate(Path::new(&path)) {
            eprintln!("Native fixture gate failed: {error}");
            std::process::exit(111);
        }
        // SAFETY: this isolated fixture has not started any worker threads.
        // Descendants must not reuse a command admission gate already consumed.
        unsafe { std::env::remove_var("NUMERA_NATIVE_JOB_GATE") };
    }
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(111)
        }
    }
}
