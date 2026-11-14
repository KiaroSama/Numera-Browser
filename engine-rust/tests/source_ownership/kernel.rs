use super::*;

fn inherited_transfer(case: &str, scenario: impl FnOnce() -> TestResult) -> TestResult {
    const CHILD: &str = "NUMERA_OWNERSHIP_TRANSFER_CASE";
    if std::env::var(CHILD).ok().as_deref() == Some(case) {
        return scenario();
    }
    let root = temp("inherited-spawn-transfer")?;
    let mut child = Command::new(std::env::current_exe()?);
    child
        .args(["--exact", case, "--test-threads=1"])
        .env(CHILD, case);
    let mut units = numera_core::ownership::capture_units::Units::new(root.path(), &mut child)?;
    let output = units
        .run(&mut child, limit(Duration::from_secs(45)), || false)
        .map_err(|error| {
            io::Error::other(format!(
                "Original transfer case left its inherited receipt incomplete: {error}"
            ))
        })?;
    let transcript = String::from_utf8_lossy(&output.stdout);
    let snapshot_unavailable = transcript.contains("Receipt counter snapshot unavailable");
    let assertion_line = transcript
        .split("panicked at tests/source_ownership/kernel.rs:")
        .nth(1)
        .and_then(|tail| tail.split(':').next())
        .and_then(|line| line.parse::<u32>().ok());
    assert_eq!(
        output.exit, 0,
        "Original nested transfer case failed (snapshot_unavailable={snapshot_unavailable}, assertion_line={assertion_line:?})"
    );
    assert!(
        !units.pending(),
        "Successful child retained its transfer receipt"
    );
    Ok(())
}
fn selected_pending() -> Result<u64, Box<dyn std::error::Error>> {
    let path = std::env::var_os(unit_report::ENV).ok_or("Inherited receipt absent")?;
    let scope = std::env::var(unit_report::SCOPE_ENV)?;
    // Other producers legitimately append to this shared receipt. Counter
    // assertions need a consistent bounded read, rather than a try-lock poll.
    unit_report::diagnostic(Path::new(&path), Duration::from_secs(1), Some(&scope))
        .map_err(|error| io::Error::new(error.kind(), "Receipt counter snapshot unavailable"))?["pending_selected"]
        .as_u64()
        .ok_or_else(|| "Pending transfer count absent".into())
}
fn contended_snapshot() -> Result<u64, Box<dyn std::error::Error>> {
    let path = std::env::var_os(unit_report::ENV).ok_or("Inherited receipt absent")?;
    let lock = DataLock::for_file(Path::new(&path))?;
    let (held, acquired) = mpsc::channel();
    let holder = thread::spawn(move || -> io::Result<()> {
        let _guard = lock.acquire(Duration::from_secs(1))?;
        held.send(()).map_err(io::Error::other)?;
        thread::sleep(Duration::from_millis(50));
        Ok(())
    });
    acquired.recv_timeout(Duration::from_secs(2))?;
    let snapshot = selected_pending();
    holder
        .join()
        .map_err(|_| "Receipt snapshot lock holder panicked")??;
    snapshot
}
fn cancellation_error_preserves_os_cause() -> TestResult {
    let root = temp("failed-cancellation-os-cause")?;
    let receipt = receipt(root.path())?;
    let mut child = command("cancellation-error")?;
    child
        .arg(root.path())
        .env(unit_report::ENV, &receipt)
        .env(unit_report::SCOPE_ENV, "failed-cancellation-fixture");
    let output = capture::run(&mut child, limit(Duration::from_secs(10)))
        .map_err(|error| io::Error::other(error.to_string()))?;
    assert_eq!(
        output.exit, 0,
        "Held-lock cancellation replaced the original OS cause or released admission"
    );
    assert!(!unit_report::read(&receipt)?.complete);
    assert_eq!(
        unit_report::diagnostic(&receipt, Duration::from_secs(1), None)?["pending_total"],
        1
    );
    Ok(())
}
fn unpublished_query_failure() -> TestResult {
    use numera_core::ownership::test_support::{Backend, with_backend};
    struct Missing {
        pid: std::sync::atomic::AtomicU32,
        queries: std::sync::atomic::AtomicUsize,
    }
    impl Backend for Missing {
        fn birth(&self, pid: u32) -> Option<io::Result<Option<u64>>> {
            self.pid.store(pid, std::sync::atomic::Ordering::SeqCst);
            Some(Ok(None))
        }
        fn group_empty(&self, group: i32) -> Option<io::Result<bool>> {
            (group as u32 == self.pid.load(std::sync::atomic::Ordering::SeqCst)).then(|| {
                self.queries
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled unpublished tree observation refusal",
                ))
            })
        }
        #[cfg(windows)]
        fn job_query(&self) -> Option<io::Result<()>> {
            self.queries
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "controlled unpublished Job observation refusal",
            )))
        }
    }
    let before = selected_pending()?;
    let mut child = command("sleep")?;
    child.stdout(Stdio::null()).stderr(Stdio::null());
    let hook = Arc::new(Missing {
        pid: std::sync::atomic::AtomicU32::new(0),
        queries: std::sync::atomic::AtomicUsize::new(0),
    });
    assert!(with_backend(hook.clone(), || OwnedTree::spawn(&mut child)).is_err());
    let pid = hook.pid.load(std::sync::atomic::Ordering::SeqCst);
    assert!(pid > 0);
    assert!(
        !identity::alive(pid, None)?,
        "Failed birth observation retained a live direct child"
    );
    assert_eq!(
        selected_pending()?,
        before + 1,
        "Unknown kernel result resolved an intent"
    );
    let queries = hook.queries.load(std::sync::atomic::Ordering::SeqCst);
    assert!(!with_backend(hook.clone(), || capture::retry_retained(Duration::ZERO)).is_empty());
    assert_eq!(
        hook.queries.load(std::sync::atomic::Ordering::SeqCst),
        queries,
        "Expired cleanup performed another kernel observation"
    );
    assert!(!with_backend(hook, || capture::retry_retained(Duration::from_secs(1))).is_empty());
    assert_eq!(
        selected_pending()?,
        before + 1,
        "Repeated query failure resolved an intent"
    );
    let receipt = PathBuf::from(std::env::var_os(unit_report::ENV).ok_or("Receipt absent")?);
    let lock = DataLock::for_file(&receipt)?;
    let (held, acquired) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let holder = thread::spawn(move || -> io::Result<()> {
        let _guard = lock.acquire(Duration::from_secs(1))?;
        held.send(()).map_err(io::Error::other)?;
        released.recv().map_err(io::Error::other)?;
        Ok(())
    });
    acquired.recv_timeout(Duration::from_secs(2))?;
    let started = Instant::now();
    let blocked = capture::retry_retained(Duration::from_millis(30));
    let elapsed = started.elapsed();
    release.send(())?;
    holder
        .join()
        .map_err(|_| "Receipt lock holder panicked")??;
    assert!(
        !blocked.is_empty(),
        "Contended cancellation receipt was treated as durable"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "Receipt cancellation extended the original cleanup deadline"
    );
    assert_eq!(selected_pending()?, before + 1);
    assert!(capture::retry_retained(Duration::from_secs(1)).is_empty());
    assert_eq!(
        selected_pending()?,
        before,
        "Proved-empty retry failed to resolve its original receipt"
    );
    Ok(())
}
#[cfg(unix)]
fn unpublished_live_descendant() -> TestResult {
    use numera_core::ownership::test_support::{Backend, with_backend};
    struct Native;
    impl Backend for Native {}
    struct Missing {
        root: PathBuf,
        record: std::sync::Mutex<Option<Value>>,
    }
    impl Backend for Missing {
        fn birth(&self, pid: u32) -> Option<io::Result<Option<u64>>> {
            Some((|| {
                wait_file(&self.root.join("descendant"))?;
                let birth = with_backend(Arc::new(Native), || identity::started_at(pid))?
                    .ok_or_else(|| io::Error::other("Live fixture birth absent"))?;
                *self.record.lock().expect("fixture record") =
                    Some(json!({"pid":pid,"pidToken":birth,"tree":{"kind":"pgid","group":pid}}));
                Ok(None)
            })())
        }
    }
    let root = temp("unpublished-live-descendant")?;
    let before = selected_pending()?;
    let mut child = command("tree")?;
    child
        .arg(root.path())
        .arg("hold")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let hook = Arc::new(Missing {
        root: root.path().into(),
        record: std::sync::Mutex::new(None),
    });
    assert!(with_backend(hook.clone(), || OwnedTree::spawn(&mut child)).is_err());
    let record = hook
        .record
        .lock()
        .expect("fixture record")
        .clone()
        .ok_or("Live tree record absent")?;
    let leaf: u32 = fs::read_to_string(root.path().join("descendant"))?.parse()?;
    assert!(
        process_running(leaf, None)?,
        "Refusal fixture lost its actual live descendant"
    );
    assert!(!recorded_tree::empty(&record)?);
    assert_eq!(
        selected_pending()?,
        before + 1,
        "Live descendant resolved a spawn intent"
    );
    assert!(!capture::retry_retained(Duration::from_secs(1)).is_empty());
    assert_eq!(selected_pending()?, before + 1);
    assert!(
        process_running(leaf, None)?,
        "Unidentified PGID was used as signal authority"
    );
    assert!(recorded_tree::settle(&record, Duration::from_secs(2))?);
    assert!(recorded_tree::empty(&record)?);
    assert!(capture::retry_retained(Duration::from_secs(1)).is_empty());
    assert_eq!(selected_pending()?, before);
    Ok(())
}
#[cfg(unix)]
fn uncertain_post_fork() -> TestResult {
    // Expected-uncertain protocol fixtures use their own receipt. It remains
    // physically pending after the real descendant has been independently
    // settled; no production cancellation or outer-scope suppression occurs.
    let root = temp("uncertain-post-fork")?;
    let receipt = receipt(root.path())?;
    let mut child = command("uncertain-spawn")?;
    child
        .arg(root.path())
        .env(unit_report::ENV, &receipt)
        .env(unit_report::SCOPE_ENV, "uncertain-fixture");
    let output = capture::run(&mut child, limit(Duration::from_secs(5)))
        .map_err(|error| io::Error::other(error.to_string()))?;
    assert_eq!(output.exit, 0, "Generic post-fork fixture failed");
    let report = unit_report::read(&receipt)?;
    assert!(
        !report.complete,
        "Generic post-fork failure was relabelled no-child"
    );
    assert_eq!(
        unit_report::diagnostic(&receipt, Duration::ZERO, None)?["pending_total"],
        1
    );
    for tree in &report.trees {
        assert!(recorded_tree::empty(tree)?);
    }
    Ok(())
}
fn normalized(mut record: Value) -> Value {
    record["pidToken"] = record["token"].take();
    record
}
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs_f64()
}
struct Detached {
    root: PathBuf,
    id: String,
}
impl Drop for Detached {
    fn drop(&mut self) {
        let _ = numera_core::jobs::JobStore::new(self.root.clone()).stop(
            &self.id,
            "source-host",
            now(),
        );
    }
}
fn legacy_refusal() -> TestResult {
    use numera_core::ownership::test_support::{Backend, with_backend};
    struct Signals(std::sync::atomic::AtomicUsize);
    impl Backend for Signals {
        fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Some(Err(io::Error::other("unexpected legacy signal")))
        }
    }
    let t = temp("legacy-no-identity")?;
    let path = t.path().join("jobs/old.json");
    write_json(
        &path,
        &json!({"id":"old","command":"launch","pid":std::process::id(),"pidToken":null,"started":0,"log":""}),
    )?;
    let before = fs::read(&path)?;
    let store = numera_core::jobs::JobStore::new(t.path().into());
    assert_eq!(
        store.get("old", now())?.ok_or("record missing")?["running"],
        true
    );
    let hook = Arc::new(Signals(std::sync::atomic::AtomicUsize::new(0)));
    let error = with_backend(hook.clone(), || store.stop("old", "source-host", now()))
        .expect_err("unverified legacy PID was stopped");
    assert!(error.to_string().contains("verified identity"));
    assert_eq!(hook.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(fs::read(path)?, before);
    assert!(!t.path().join("jobs/old.exit").exists());
    Ok(())
}
struct AfterDescendant {
    root: PathBuf,
    record: Arc<std::sync::Mutex<Option<Value>>>,
}
impl numera_core::ownership::test_support::Backend for AfterDescendant {
    fn spawned(&self, record: &Value) -> Option<io::Result<()>> {
        *self.record.lock().expect("record") = Some(record.clone());
        Some((|| {
            wait_file(&self.root.join("descendant"))?;
            Err(io::Error::other("controlled durable record failure"))
        })())
    }
}
fn claim_failure(descendant: bool) -> TestResult {
    use numera_core::vault::testing::{Action, with_observer};
    let t = temp("real-claim-publication-failure")?;
    let mut session = Session::admit(t.path(), "p", "source-host")?;
    let target = t.path().join("sessions/p.json");
    let root = t.path().to_owned();
    let seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = seen.clone();
    let mut child = command(if descendant { "tree" } else { "sleep" })?;
    if descendant {
        child.arg(t.path()).arg("hold");
    } else {
        child.arg(t.path().join("born"));
    }
    child.stdout(Stdio::null()).stderr(Stdio::null());
    let error = with_observer(
        move |event| {
            if event.operation == "before-atomic-write" && event.path == target {
                wait_file(&root.join(if descendant { "descendant" } else { "born" }))?;
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                return Err(io::Error::other("controlled claim publication refusal"));
            }
            Ok(Action::Continue)
        },
        || session.spawn(&mut child, "browser").map(|tree| tree.pid()),
    )
    .expect_err("failed durable claim was accepted");
    assert!(
        error
            .to_string()
            .contains("controlled claim publication refusal")
    );
    assert!(
        seen.load(std::sync::atomic::Ordering::SeqCst),
        "publication hook was never reached"
    );
    let pid: u32 =
        fs::read_to_string(
            t.path()
                .join(if descendant { "descendant" } else { "born" }),
        )?
        .parse()?;
    assert!(!process_running(pid, None)?);
    assert!(session.tree_record().is_none());
    assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
    Ok(())
}
#[cfg(windows)]
fn windows_refusal(assign: bool) -> TestResult {
    use numera_core::ownership::test_support::{Backend, with_backend};
    struct Refuse {
        assign: bool,
        pid: std::sync::Mutex<Option<u32>>,
        creates: std::sync::atomic::AtomicUsize,
    }
    impl Backend for Refuse {
        fn job_create(&self) -> Option<io::Result<()>> {
            self.creates
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            (!self.assign).then(|| {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled Job creation refusal",
                ))
            })
        }
        fn job_assign(&self, pid: u32) -> Option<io::Result<()>> {
            *self.pid.lock().expect("pid") = Some(pid);
            Some(Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "controlled Job assignment refusal",
            )))
        }
    }
    let t = temp("windows-tree-refusal")?;
    let hook = Arc::new(Refuse {
        assign,
        pid: std::sync::Mutex::new(None),
        creates: std::sync::atomic::AtomicUsize::new(0),
    });
    let mut child = command("sleep")?;
    child
        .arg(t.path().join("unsafe-executed"))
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    assert!(with_backend(hook.clone(), || OwnedTree::spawn(&mut child)).is_err());
    assert_eq!(hook.creates.load(std::sync::atomic::Ordering::SeqCst), 1);
    let pid = *hook.pid.lock().expect("pid");
    if assign {
        assert!(!identity::alive(
            pid.ok_or("suspended child was not created")?,
            None
        )?);
    } else {
        assert!(pid.is_none());
    }
    assert!(
        !t.path().join("unsafe-executed").exists(),
        "unowned suspended child ran user code"
    );
    Ok(())
}

source_test!(
    helper_lifecycle_a_timed_out_helper_is_reaped,
    "tests/test_helper_lifecycle.py::test_a_timed_out_helper_is_reaped",
    {
        let mut child = command("sleep")?;
        let mut born = None;
        let failure = capture::run_observed(
            &mut child,
            limit(Duration::from_millis(90)),
            || false,
            |event, _| {
                if let capture::Observation::Spawned { record } = event {
                    born = Some(record);
                }
            },
        )
        .err()
        .expect("timeout returned output");
        assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
        let born = born.ok_or("child never started")?;
        let pid = born["pid"].as_u64().ok_or("pid absent")? as u32;
        assert!(
            !identity::alive(pid, born["token"].as_u64())?,
            "killed helper was not reaped"
        );
        assert!(recorded_tree::empty(&normalized(born))?);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_an_unexpected_exception_still_ends_the_helper,
    "tests/test_helper_lifecycle.py::test_an_unexpected_exception_still_ends_the_helper",
    {
        let t = temp("panic-capture-worker")?;
        let mut child = command("panic-capture")?;
        child.arg(t.path());
        let output = capture::run(&mut child, limit(Duration::from_secs(4)))?;
        assert_eq!(
            output.exit,
            0,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(value["panicked"], true);
        assert_eq!(value["child_gone"], true);
        assert_eq!(value["tree_empty"], true);
        assert_eq!(
            value["before"], value["after"],
            "unwinding leaked local pipe handles"
        );
        Ok(())
    }
);
source_test!(
    helper_ownership_a_reader_that_cannot_reach_the_end_says_so,
    "tests/test_helper_ownership.py::test_a_reader_that_cannot_reach_the_end_says_so",
    {
        let mut child = command("partial-writer")?;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let failure = capture::run_observed_streamed(
            &mut child,
            limit(Duration::from_millis(140)),
            || false,
            |_, _| {},
            |index, chunk| {
                if index == 0 {
                    stdout.extend_from_slice(chunk)
                } else {
                    stderr.extend_from_slice(chunk)
                }
            },
        )
        .err()
        .expect("a live open writer was reported complete");
        assert_eq!(stdout, b"PARTIAL");
        assert!(stderr.is_empty());
        assert_eq!(failure.kind(), io::ErrorKind::TimedOut);
        assert!(failure.to_string().contains("stdout_eof=false"));
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_clean_exit_with_failed_cleanup_is_not_a_result,
    "tests/test_helper_terminal_state.py::test_a_clean_exit_with_failed_cleanup_is_not_a_result",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Unknown;
        impl Backend for Unknown {
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
        let mut child = command("emit")?;
        child.args(["zero", "0"]);
        let mut failure = with_backend(Arc::new(Unknown), || {
            capture::run(&mut child, limit(Duration::from_secs(2)))
        })
        .err()
        .expect("clean leader exit escaped unproved tree cleanup");
        assert!(
            failure.retained_record().is_some(),
            "unproved cleanup discarded its owner"
        );
        let cause = failure.to_string();
        assert!(cause.contains("cleanup"));
        failure.settle(Duration::from_secs(1))?;
        assert!(failure.retained_record().is_none());
        assert_eq!(failure.to_string(), cause);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_failed_report_after_a_descendant_exists_ends_the_whole_tree,
    "tests/test_helper_lifecycle_matrix.py::test_failed_report_after_a_descendant_exists_ends_the_whole_tree",
    {
        let t = temp("post-descendant-report-failure")?;
        let mut child = command("tree")?;
        child
            .arg(t.path())
            .arg("hold")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let seen = Arc::new(std::sync::Mutex::new(None));
        let hook = AfterDescendant {
            root: t.path().to_owned(),
            record: seen.clone(),
        };
        let error = numera_core::ownership::test_support::with_backend(Arc::new(hook), || {
            OwnedTree::spawn(&mut child)
        })
        .err()
        .ok_or("record failure was accepted")?;
        assert!(
            error
                .to_string()
                .contains("controlled durable record failure")
        );
        let record = seen
            .lock()
            .expect("record")
            .clone()
            .ok_or("capture callback was not reached")?;
        assert!(recorded_tree::empty(&normalized(record.clone()))?);
        assert!(!identity::alive(
            record["pid"].as_u64().ok_or("pid")? as u32,
            record["token"].as_u64()
        )?);
        let descendant: u32 = fs::read_to_string(t.path().join("descendant"))?.parse()?;
        assert!(!process_running(descendant, None)?);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_failed_claim_callback_ends_the_captured_tree,
    "tests/test_helper_lifecycle_matrix.py::test_failed_claim_callback_ends_the_captured_tree",
    { claim_failure(true) }
);
source_test!(
    session_tree_mandatory_a_claim_that_cannot_be_narrowed_stops_the_driver,
    "tests/test_session_tree_mandatory.py::test_a_claim_that_cannot_be_narrowed_stops_the_driver",
    { claim_failure(false) }
);
source_test!(
    session_tree_mandatory_a_session_may_not_open_a_browser_it_cannot_own,
    "tests/test_session_tree_mandatory.py::test_a_session_may_not_open_a_browser_it_cannot_own",
    {
        inherited_transfer(
            "kernel::session_tree_mandatory_a_session_may_not_open_a_browser_it_cannot_own",
            || {
                let t = temp("refused-unnamed-session")?;
                let profile = profile(t.path(), "p")?;
                let session = Session::admit(t.path(), "p", "source-host")?;
                let adapter = t.path().join(if cfg!(windows) {
                    "missing-sdk-driver.exe"
                } else {
                    "missing-sdk-driver"
                });
                let expected = fs::metadata(&adapter)
                    .err()
                    .ok_or("Missing SDK fixture exists")?;
                assert_eq!(expected.kind(), io::ErrorKind::NotFound);
                let runtime = numera_core::browser::runtime::Runtime {
                    adapter,
                    cwd: t.path().into(),
                    home: t.path().into(),
                    logs: t.path().join("logs"),
                    country: None,
                    relay: None,
                    binary_options: Default::default(),
                    deployment: None,
                    registry: None,
                };
                let error = runtime
                    .open(session, profile, true, 1000.0)
                    .err()
                    .ok_or("unnamed driver accepted")?;
                assert_eq!(error.kind, "failed");
                assert_eq!(
                    error.message,
                    expected.to_string(),
                    "Builtin preparation lost the actual NotFound cause"
                );
                assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
                let _lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
                assert_eq!(
                    contended_snapshot()?,
                    0,
                    "No-child builtin refusal created a spawn intent"
                );
                Ok(())
            },
        )
    }
);
source_test!(
    session_tree_mandatory_a_driver_that_started_in_no_tree_is_stopped,
    "tests/test_session_tree_mandatory.py::test_a_driver_that_started_in_no_tree_is_stopped",
    {
        inherited_transfer(
            "kernel::session_tree_mandatory_a_driver_that_started_in_no_tree_is_stopped",
            || {
                use numera_core::ownership::test_support::{Backend, with_backend};
                struct Missing(std::sync::Mutex<Option<u32>>);
                impl Backend for Missing {
                    fn birth(&self, pid: u32) -> Option<io::Result<Option<u64>>> {
                        *self.0.lock().expect("pid") = Some(pid);
                        Some(Ok(None))
                    }
                }
                let t = temp("unnamed-born-driver")?;
                let mut session = Session::admit(t.path(), "p", "source-host")?;
                let missing = Arc::new(Missing(std::sync::Mutex::new(None)));
                let mut child = command("sleep")?;
                child.stdout(Stdio::null()).stderr(Stdio::null());
                assert!(
                    with_backend(missing.clone(), || session.spawn(&mut child, "browser")).is_err()
                );
                let pid = missing
                    .0
                    .lock()
                    .expect("pid")
                    .ok_or("real child was never identified")?;
                assert!(!identity::alive(pid, None)?);
                assert!(session.tree_record().is_none());
                assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
                assert_eq!(
                    selected_pending()?,
                    0,
                    "Proved-empty child left a pending transfer"
                );
                drop(session);
                assert!(
                    fs::read_dir(t.path().join(".updates/work"))?
                        .next()
                        .is_none(),
                    "Proved-empty failed spawn retained its update admission receipt"
                );
                unpublished_query_failure()?;
                cancellation_error_preserves_os_cause()
            },
        )
    }
);
source_test!(
    session_ownership_a_job_that_cannot_be_queried_is_not_empty,
    "tests/test_session_ownership.py::test_a_job_that_cannot_be_queried_is_not_empty",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Unknown(std::sync::atomic::AtomicUsize);
        impl Backend for Unknown {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled kernel query refusal",
                )))
            }
            fn job_query(&self) -> Option<io::Result<()>> {
                self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled job query refusal",
                )))
            }
        }
        let mut child = command("sleep")?;
        child.stdout(Stdio::null()).stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        let hook = Arc::new(Unknown(std::sync::atomic::AtomicUsize::new(0)));
        assert!(
            with_backend(hook.clone(), || tree.empty()).is_err(),
            "query failure was read as empty"
        );
        assert_eq!(hook.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(identity::alive(
            tree.pid(),
            tree.record()["token"].as_u64()
        )?);
        assert!(!tree.empty()?);
        tree.settle(Duration::from_secs(1))?;
        assert!(tree.empty()?);
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    owned_tree_a_group_of_corpses_is_not_a_running_browser,
    "tests/test_owned_tree.py::test_a_group_of_corpses_is_not_a_running_browser",
    {
        let mut child = command("emit")?;
        child
            .args(["dead", "0"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        tree.child_mut().wait()?;
        assert!(
            tree.empty()?,
            "an exited and reaped group was reported live"
        );
        tree.settle(Duration::from_secs(1))?;
        assert!(recorded_tree::empty(&as_record(&tree))?);
        Ok(())
    }
);
source_test!(
    owned_tree_a_job_with_no_verified_identity_refuses_to_stop,
    "tests/test_owned_tree.py::test_a_job_with_no_verified_identity_refuses_to_stop",
    { legacy_refusal() }
);
source_test!(
    owned_tree_a_job_records_the_tree_beside_the_pid,
    "tests/test_owned_tree.py::test_a_job_records_the_tree_beside_the_pid",
    {
        let t = temp("real-detached-job-tree")?;
        let store = numera_core::jobs::JobStore::new(t.path().into());
        let executable = fixture()?;
        let argv = vec![
            "sleep".into(),
            t.path().join("command-born").to_string_lossy().into_owned(),
        ];
        let record = store.spawn_detached(numera_core::jobs::SpawnOptions {
            executable: &executable,
            argv: &argv,
            command: "launch",
            profile: None,
            password: None,
        })?;
        let id = record["id"].as_str().ok_or("job id missing")?;
        let _owner = Detached {
            root: t.path().into(),
            id: id.into(),
        };
        wait_file(&t.path().join("command-born"))?;
        assert!(recorded_tree::verified(&record));
        assert!(matches!(
            record["tree"]["kind"].as_str(),
            Some("pgid" | "job")
        ));
        assert!(record["pidToken"].as_u64().is_some());
        assert_eq!(record["running"], true);
        let stopped = store
            .stop(id, "source-host", now())?
            .ok_or("stopped job absent")?;
        assert_eq!(stopped["running"], false);
        assert!(recorded_tree::empty(&record)?);
        Ok(())
    }
);
source_test!(
    owned_tree_a_child_that_exits_before_it_can_be_named_records_its_outcome,
    "tests/test_owned_tree.py::test_a_child_that_exits_before_it_can_be_named_records_its_outcome",
    {
        let t = temp("real-detached-fast-failure")?;
        let store = numera_core::jobs::JobStore::new(t.path().into());
        let executable = fixture()?;
        let argv = vec!["emit".into(), "failed".into(), "3".into()];
        let record = store.spawn_detached(numera_core::jobs::SpawnOptions {
            executable: &executable,
            argv: &argv,
            command: "warmup",
            profile: None,
            password: None,
        })?;
        let id = record["id"].as_str().ok_or("job id absent")?;
        let _owner = Detached {
            root: t.path().into(),
            id: id.into(),
        };
        let end = Instant::now() + Duration::from_secs(5);
        let after = loop {
            let after = store.get(id, now())?.ok_or("job disappeared")?;
            if after["running"] == false {
                break after;
            }
            assert!(
                Instant::now() < end,
                "fast child outcome never became durable"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(after["state"], "failed");
        assert_eq!(after["exit"], 3);
        assert!(t.path().join("jobs").join(format!("{id}.exit")).is_file());
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    owned_tree_a_job_that_could_not_be_identified_refuses_to_stop,
    "tests/test_owned_tree.py::test_a_job_that_could_not_be_identified_refuses_to_stop",
    {
        inherited_transfer(
            "kernel::owned_tree_a_job_that_could_not_be_identified_refuses_to_stop",
            || {
                use numera_core::ownership::test_support::{Backend, with_backend};
                struct Missing(std::sync::Mutex<Option<u32>>);
                impl Backend for Missing {
                    fn birth(&self, pid: u32) -> Option<io::Result<Option<u64>>> {
                        *self.0.lock().expect("pid") = Some(pid);
                        Some(Ok(None))
                    }
                }
                let mut child = command("sleep")?;
                child.stdout(Stdio::null()).stderr(Stdio::null());
                let hook = Arc::new(Missing(std::sync::Mutex::new(None)));
                assert!(with_backend(hook.clone(), || OwnedTree::spawn(&mut child)).is_err());
                let pid = hook.0.lock().expect("pid").ok_or("child was never born")?;
                assert!(
                    !identity::alive(pid, None)?,
                    "native refusal left an unnamed child alive"
                );
                legacy_refusal()?;
                unpublished_live_descendant()?;
                uncertain_post_fork()?;
                super::signal_errors::controls()
            },
        )
    }
);
source_test!(
    owned_tree_a_finished_job_that_left_something_running_is_not_called_finished,
    "tests/test_owned_tree.py::test_a_finished_job_that_left_something_running_is_not_called_finished",
    {
        let t = temp("real-job-surviving-browser")?;
        let mut tree = spawn_tree(t.path(), false)?;
        tree.child_mut().wait()?;
        let mut record = as_record(&tree);
        record["id"] = "j1".into();
        record["command"] = "launch".into();
        record["started"] = 0.into();
        write_json(&t.path().join("jobs/j1.json"), &record)?;
        write_json(
            &t.path().join("jobs/j1.exit"),
            &json!({"state":"done","exit":0}),
        )?;
        let store = numera_core::jobs::JobStore::new(t.path().into());
        let after = store.get("j1", now())?.ok_or("job missing")?;
        assert_eq!(after["orphaned"], true);
        assert_eq!(after["running"], true);
        assert_eq!(
            store.prune(now())?,
            0,
            "a real surviving browser was pruned"
        );
        tree.settle(Duration::from_secs(1))?;
        assert_eq!(
            store.get("j1", now())?.ok_or("job missing")?["running"],
            false
        );
        Ok(())
    }
);
#[cfg(windows)]
source_test!(
    owned_tree_every_kernel32_call_is_bound_and_the_library_is_one_object,
    "tests/test_owned_tree.py::test_every_kernel32_call_is_bound_and_the_library_is_one_object",
    {
        use numera_core::ownership::test_support;
        let mut child = command("sleep")?;
        child.stdout(Stdio::null()).stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        let record = as_record(&tree);
        let token = record["pidToken"]
            .as_u64()
            .ok_or("Windows birth time absent")?;
        assert_eq!(identity::started_at(tree.pid())?, Some(token));
        assert!(identity::alive(tree.pid(), Some(token))?);
        assert!(!tree.empty()?);
        assert!(
            !tree.all_job_members_exited()?,
            "A live member must forbid accounting settlement"
        );
        let diagnostics = tree.job_member_diagnostics()?;
        assert_eq!(diagnostics["complete"], true);
        assert!(
            diagnostics["accounting"]["active_processes"]
                .as_u64()
                .is_some_and(|n| n > 0)
        );
        assert!(
            diagnostics["accounting"]["total_processes"]
                .as_u64()
                .is_some()
        );
        assert!(
            diagnostics["accounting"]["total_terminated_processes"]
                .as_u64()
                .is_some()
        );
        let rows = diagnostics["members"]
            .as_array()
            .ok_or("retained Job diagnostic rows absent")?;
        assert!(rows.len() <= 512);
        assert!(rows.iter().any(|row| row["pid"] == tree.pid()
            && row["leader"] == true
            && row["membership"] == "retained_leader"
            && row["wait"] == "live"));
        for row in rows {
            assert!(matches!(
                row["image_class"].as_str(),
                Some(
                    "cargo"
                        | "cargo-fmt"
                        | "rustc"
                        | "rustfmt"
                        | "lld-link"
                        | "link"
                        | "mspdbsrv"
                        | "vctip"
                        | "conhost"
                        | "cmd"
                        | "powershell"
                        | "rustup"
                        | "node"
                        | "numera-engine"
                        | "numera-browser-runtime"
                        | "numera-xtask"
                        | "numera-native-candidate"
                        | "numera-http-observer"
                        | "unclassified"
                )
            ));
            if let Some(source) = row.get("image_source") {
                assert!(matches!(
                    source.as_str(),
                    Some(
                        "retained-process-handle"
                            | "process-snapshot"
                            | "process-snapshot-no-match"
                            | "unavailable"
                    )
                ));
            }
            for key in ["image_error_code", "image_snapshot_error_code"] {
                if let Some(code) = row.get(key) {
                    assert!(
                        code.is_null()
                            || code
                                .as_i64()
                                .is_some_and(|value| i32::try_from(value).is_ok()),
                        "Diagnostic image error code exceeded its numeric bound"
                    );
                }
            }
            assert!(row.get("argv").is_none());
            assert!(row.get("path").is_none());
        }
        struct UncertainJob;
        impl test_support::Backend for UncertainJob {
            fn job_query(&self) -> Option<io::Result<()>> {
                Some(Err(io::Error::other("controlled Job query uncertainty")))
            }
        }
        assert!(
            test_support::with_backend(Arc::new(UncertainJob), || tree.all_job_members_exited())
                .is_err()
        );
        assert!(!recorded_tree::empty(&record)?);
        let _peak = tree.peak_job_memory()?;
        tree.settle(Duration::from_secs(2))?;
        assert!(tree.empty()?);
        assert!(tree.all_job_members_exited()?);
        let diagnostics = tree.job_member_diagnostics()?;
        assert_eq!(diagnostics["member_count"], 0);
        assert_eq!(diagnostics["accounting"]["active_processes"], 0);
        assert!(!identity::alive(tree.pid(), Some(token))?);
        assert!(recorded_tree::empty(&record)?);
        Ok(())
    }
);
#[cfg(windows)]
source_test!(
    session_ownership_a_session_that_cannot_be_owned_is_not_spawned,
    "tests/test_session_ownership.py::test_a_session_that_cannot_be_owned_is_not_spawned",
    { windows_refusal(false) }
);
#[cfg(windows)]
source_test!(
    session_ownership_a_child_that_cannot_be_put_in_its_job_is_stopped,
    "tests/test_session_ownership.py::test_a_child_that_cannot_be_put_in_its_job_is_stopped",
    { windows_refusal(true) }
);
#[cfg(windows)]
source_test!(
    session_tree_mandatory_a_driver_that_cannot_be_put_in_a_job_is_stopped,
    "tests/test_session_tree_mandatory.py::test_a_driver_that_cannot_be_put_in_a_job_is_stopped",
    { windows_refusal(true) }
);
#[cfg(windows)]
source_test!(
    session_tree_mandatory_a_job_that_cannot_be_created_refuses_before_anything_starts,
    "tests/test_session_tree_mandatory.py::test_a_job_that_cannot_be_created_refuses_before_anything_starts",
    { windows_refusal(false) }
);
source_test!(
    proved_dead_a_child_that_dies_between_the_poll_and_the_signal_is_fine,
    "tests/test_proved_dead.py::test_a_child_that_dies_between_the_poll_and_the_signal_is_fine",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Raced {
            release: PathBuf,
            record: Value,
            called: std::sync::atomic::AtomicBool,
        }
        impl Backend for Raced {
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                self.called.store(true, std::sync::atomic::Ordering::SeqCst);
                fs::write(&self.release, b"go").expect("release");
                let end = Instant::now() + Duration::from_secs(2);
                while !recorded_tree::empty(&self.record).unwrap_or(false) {
                    assert!(Instant::now() < end, "cooperative child never exited");
                    thread::sleep(Duration::from_millis(5));
                }
                Some(Err(io::Error::from(io::ErrorKind::NotFound)))
            }
        }
        let t = temp("poll-signal-death-race")?;
        let ready = t.path().join("ready");
        let release = t.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let hook = Arc::new(Raced {
            release,
            record: as_record(&tree),
            called: std::sync::atomic::AtomicBool::new(false),
        });
        let result = with_backend(hook.clone(), || tree.settle(Duration::from_secs(2)));
        assert!(hook.called.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            result.is_ok(),
            "a proved-dead signal race was mistaken for refusal: {result:?}"
        );
        assert!(tree.child_mut().try_wait()?.is_some());
        Ok(())
    }
);
