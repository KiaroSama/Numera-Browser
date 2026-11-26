use super::*;

fn adapter_pid(root: &Path, id: &str) -> io::Result<u64> {
    match claims::read(root, id) {
        claims::Claim::Record(record) => record["pid"]
            .as_u64()
            .ok_or_else(|| io::Error::other("claim PID missing")),
        _ => Err(io::Error::other("owned adapter claim missing")),
    }
}
pub(super) fn activate_adapter(
    session: Session,
    root: &Path,
    backend: &str,
) -> Result<numera_core::browser::adapter::Adapter, ready::StartupFailed> {
    let failure = |error: io::Error| ready::StartupFailed {
        kind: "failed".into(),
        message: error.to_string(),
    };
    let mut child = command("adapter").map_err(failure)?;
    child.arg(root);
    let mut adapter = numera_core::browser::adapter::Adapter::spawn_admitted(
        session,
        &mut child,
        Duration::from_secs(2),
    )
    .map_err(failure)?;
    adapter
        .open(
            backend,
            Default::default(),
            Vec::new(),
            Duration::from_secs(2),
        )
        .map_err(failure)?;
    Ok(adapter)
}
pub(super) fn open_adapter(
    root: &Path,
    id: &str,
    backend: &str,
) -> Result<numera_core::browser::adapter::Adapter, Box<dyn std::error::Error>> {
    let session = Session::admit(root, id, "source-host")?;
    let mut child = command("adapter")?;
    child.arg(root);
    let mut adapter = numera_core::browser::adapter::Adapter::spawn_admitted(
        session,
        &mut child,
        Duration::from_secs(2),
    )?;
    assert!(
        !root.join("page-created").exists(),
        "SDK hello created a browser before activation"
    );
    assert_eq!(
        claims::state(&claims::read(root, id), "source-host"),
        claims::State::Busy
    );
    adapter.open(
        backend,
        Default::default(),
        Vec::new(),
        Duration::from_secs(2),
    )?;
    Ok(adapter)
}

source_test!(
    browser_readiness_a_backend_announces_the_browser_it_opened,
    "tests/test_browser_readiness.py::test_a_backend_announces_the_browser_it_opened",
    {
        let t = temp("actual-open-announcement")?;
        profile(t.path(), "p")?;
        let mut adapter = open_adapter(t.path(), "p", "playwright")?;
        assert!(t.path().join("page-created").is_file());
        let claims: Vec<Value> =
            serde_json::from_slice(&fs::read(t.path().join("claims-before-open.json"))?)?;
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0]["pid"], adapter_pid(t.path(), "p")?);
        adapter.finish(Duration::from_secs(2))?;
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    browser_readiness_a_failure_after_the_lease_is_reported_as_a_failure,
    "tests/test_browser_readiness.py::test_a_failure_after_the_lease_is_reported_as_a_failure",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("failed-context-startup")?;
        profile(t.path(), "p")?;
        fs::write(t.path().join("adapter-mode"), b"fail-open")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let answer = registry.open_browser("p", "p", |session, profile| {
            activate_adapter(session, t.path(), &profile.engine)
        });
        let failure = answer.expect_err("failed context reported running");
        assert_eq!(
            failure.message,
            "Browser SDK operation failed; inspect the sanitized adapter log"
        );
        assert!(!failure.message.contains("FixtureContextFailure"));
        assert!(!registry.running("p")?);
        assert!(!registry.owns("p")?);
        assert!(!t.path().join("page-created").exists());
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        let _lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        Ok(())
    }
);
source_test!(
    browser_readiness_a_launch_whose_browser_never_opens_is_not_a_success,
    "tests/test_browser_readiness.py::test_a_launch_whose_browser_never_opens_is_not_a_success",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("failed-context-startup")?;
        profile(t.path(), "p")?;
        fs::write(t.path().join("adapter-mode"), b"fail-open")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let answer = registry.open_browser("p", "p", |session, profile| {
            activate_adapter(session, t.path(), &profile.engine)
        });
        let failure = answer.expect_err("failed context reported running");
        assert_eq!(
            failure.message,
            "Browser SDK operation failed; inspect the sanitized adapter log"
        );
        assert!(!failure.message.contains("FixtureContextFailure"));
        assert!(!registry.running("p")?);
        assert!(!registry.owns("p")?);
        assert!(!t.path().join("page-created").exists());
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        let _lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        Ok(())
    }
);
source_test!(
    browser_readiness_a_scheduled_warm_up_that_never_opens_does_not_advance_the_occurrence,
    "tests/test_browser_readiness.py::test_a_scheduled_warm_up_that_never_opens_does_not_advance_the_occurrence",
    {
        use numera_core::ownership::registry::{BackgroundOptions, Kind, Registry};
        let t = temp("failed-background-activation")?;
        profile(t.path(), "p")?;
        write_json(
            &t.path().join("schedules.json"),
            &json!([{"id":"s","profileId":"p","lastRun":null,"lastOutcome":null}]),
        )?;
        let before = fs::read(t.path().join("schedules.json"))?;
        fs::write(t.path().join("adapter-mode"), b"fail-open")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let committed = std::cell::Cell::new(false);
        let operation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = operation.clone();
        let answer = registry.start_background_checked(
            BackgroundOptions {
                id: "p",
                requested_name: "p",
                kind: Kind::ScheduledWarmup,
                schedule: Some("s".into()),
            },
            |session, profile| activate_adapter(session, t.path(), &profile.engine),
            move |_, _| {
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            },
            || {
                committed.set(true);
                Ok(())
            },
        );
        assert!(answer.is_err());
        assert!(
            !committed.get(),
            "failed activation committed the schedule occurrence"
        );
        assert!(!operation.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(fs::read(t.path().join("schedules.json"))?, before);
        assert!(!registry.running("p")?);
        Ok(())
    }
);
source_test!(
    session_lease_opening_a_session_is_refused_when_the_credential_cannot_be_read,
    "tests/test_session_lease.py::test_opening_a_session_is_refused_when_the_credential_cannot_be_read",
    {
        let t = temp("locked-session-credential")?;
        let record = json!({"id":"p","name":"p","proxy":json!({"server":"http://127.0.0.1:9","username":"source-user","password":"source-secret"})});
        let mut value =
            numera_core::conversion::to_engine(&record, 1000.0).map_err(io::Error::other)?;
        value.proxy = if record["proxy"].is_null() {
            None
        } else {
            Some(numera_core::proxy::Proxy::from_record(
                &record["proxy"],
                false,
            )?)
        };
        let mut repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        repo.save(value, 1000.0)?;
        repo.enable_vault("source-master")?;
        let locked = numera_core::browser::admission::current(
            t.path(),
            "p",
            "p",
            "source-host",
            None,
            1001.0,
        );
        let error = locked.err().ok_or("unreadable credential admitted")?;
        assert_eq!(error.kind, "locked");
        assert!(error.message.contains("proxy password"));
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        let (mut session, current) = numera_core::browser::admission::current(
            t.path(),
            "p",
            "p",
            "source-host",
            Some("source-master"),
            1001.0,
        )?;
        assert_eq!(
            current
                .proxy
                .as_ref()
                .map(|proxy| proxy.to_record()["password"].clone()),
            Some(json!("source-secret"))
        );
        session.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    session_lease_a_passwordless_proxy_still_opens_while_locked,
    "tests/test_session_lease.py::test_a_passwordless_proxy_still_opens_while_locked",
    {
        let t = temp("locked-session-credential")?;
        let record = json!({"id":"p","name":"p","proxy":json!({"server":"http://127.0.0.1:9","username":"source-user","password":null})});
        let mut value =
            numera_core::conversion::to_engine(&record, 1000.0).map_err(io::Error::other)?;
        value.proxy = if record["proxy"].is_null() {
            None
        } else {
            Some(numera_core::proxy::Proxy::from_record(
                &record["proxy"],
                false,
            )?)
        };
        let mut repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        repo.save(value, 1000.0)?;
        repo.enable_vault("source-master")?;
        let locked = numera_core::browser::admission::current(
            t.path(),
            "p",
            "p",
            "source-host",
            None,
            1001.0,
        );
        let (mut session, _) = locked?;
        assert!(SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?).is_err());
        session.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    session_lease_a_profile_with_no_proxy_is_never_blocked,
    "tests/test_session_lease.py::test_a_profile_with_no_proxy_is_never_blocked",
    {
        let t = temp("locked-session-credential")?;
        let record = json!({"id":"p","name":"p","proxy":json!(null)});
        let mut value =
            numera_core::conversion::to_engine(&record, 1000.0).map_err(io::Error::other)?;
        value.proxy = if record["proxy"].is_null() {
            None
        } else {
            Some(numera_core::proxy::Proxy::from_record(
                &record["proxy"],
                false,
            )?)
        };
        let mut repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        repo.save(value, 1000.0)?;
        repo.enable_vault("source-master")?;
        let locked = numera_core::browser::admission::current(
            t.path(),
            "p",
            "p",
            "source-host",
            None,
            1001.0,
        );
        let (mut session, _) = locked?;
        assert!(SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?).is_err());
        session.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    session_tree_mandatory_every_engine_starts_its_browser_inside_the_seam,
    "tests/test_session_tree_mandatory.py::test_every_engine_starts_its_browser_inside_the_seam",
    {
        for backend in ["playwright", "patchright", "camoufox", "cloak"] {
            let t = temp("mandatory-engine-owner")?;
            profile(t.path(), "p")?;
            let mut adapter = open_adapter(t.path(), "p", backend)?;
            let before: Vec<Value> =
                serde_json::from_slice(&fs::read(t.path().join("claims-before-open.json"))?)?;
            assert_eq!(before.len(), 1);
            assert_eq!(
                claims::state(&claims::Claim::Record(before[0].clone()), "source-host"),
                claims::State::Busy
            );
            let tree = before[0]["tree"].clone();
            assert!(matches!(tree["kind"].as_str(), Some("pgid" | "job")));
            assert_ne!(before[0]["pid"], std::process::id());
            adapter.finish(Duration::from_secs(2))?;
            assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        }
        Ok(())
    }
);
source_test!(
    session_tree_mandatory_the_seam_takes_any_named_launcher,
    "tests/test_session_tree_mandatory.py::test_the_seam_takes_any_named_launcher",
    {
        let t = temp("arbitrarily-named-native-sdk")?;
        profile(t.path(), "p")?;
        let executable = t.path().join(if cfg!(windows) {
            "different-vendor-launcher.exe"
        } else {
            "different-vendor-launcher"
        });
        fs::copy(fixture()?, &executable)?;
        let mut child = Command::new(&executable);
        child.arg("adapter").arg(t.path());
        let mut adapter = numera_core::browser::adapter::Adapter::spawn(
            t.path(),
            "p",
            "source-host",
            &mut child,
            Duration::from_secs(2),
        )?;
        assert!(!t.path().join("page-created").exists());
        adapter.open(
            "cloak",
            Default::default(),
            Vec::new(),
            Duration::from_secs(2),
        )?;
        assert!(t.path().join("page-created").is_file());
        adapter.finish(Duration::from_secs(2))?;
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    session_tree_mandatory_an_unnamed_launcher_captures_nothing,
    "tests/test_session_tree_mandatory.py::test_an_unnamed_launcher_captures_nothing",
    {
        let t = temp("no-global-spawn-interception")?;
        profile(t.path(), "p")?;
        let mut ordinary = command("sleep")?
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let token = identity::started_at(ordinary.id())?;
        let mut adapter = open_adapter(t.path(), "p", "camoufox")?;
        assert_ne!(adapter_pid(t.path(), "p")?, u64::from(ordinary.id()));
        adapter.finish(Duration::from_secs(2))?;
        assert!(
            identity::alive(ordinary.id(), token)?,
            "session ended an unrelated uncaptured child"
        );
        ordinary.kill()?;
        ordinary.wait()?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_camoufox_body_that_raises_over_a_clean_close_frees_the_profile,
    "tests/test_session_ownership.py::test_a_camoufox_body_that_raises_over_a_clean_close_frees_the_profile",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("body-error-clean-close")?;
        profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let answer: io::Result<()> = registry.with_finite_browser(
            "p",
            "p",
            |session, _| activate_adapter(session, t.path(), "camoufox"),
            |_| Err(io::Error::other("source body failure")),
        );
        assert!(
            answer
                .expect_err("body failure swallowed")
                .to_string()
                .contains("source body failure")
        );
        assert!(!registry.running("p")?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        let _next = Session::admit(t.path(), "p", "source-host")?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_clean_camoufox_session_still_releases,
    "tests/test_session_ownership.py::test_a_clean_camoufox_session_still_releases",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("clean-native-vendor-close")?;
        profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        assert!(
            registry.open_browser("p", "p", |session, _| activate_adapter(
                session,
                t.path(),
                "camoufox"
            ))?
        );
        let browser: u32 = fs::read_to_string(t.path().join("browser-pid"))?.parse()?;
        let token = identity::started_at(browser)?;
        assert!(registry.stop("p", Duration::from_secs(2))?);
        assert!(!process_running(browser, token)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        let _next = Session::admit(t.path(), "p", "source-host")?;
        Ok(())
    }
);
source_test!(
    session_ownership_a_camoufox_close_that_fails_keeps_the_profile,
    "tests/test_session_ownership.py::test_a_camoufox_close_that_fails_keeps_the_profile",
    {
        use numera_core::ownership::registry::Registry;
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Refused;
        impl Backend for Refused {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(false))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled vendor cleanup refusal",
                )))
            }
        }
        let t = temp("failed-native-vendor-close")?;
        profile(t.path(), "p")?;
        fs::write(t.path().join("adapter-mode"), b"fail-close")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        registry.open_browser("p", "p", |session, _| {
            activate_adapter(session, t.path(), "camoufox")
        })?;
        let browser: u32 = fs::read_to_string(t.path().join("browser-pid"))?.parse()?;
        let token = identity::started_at(browser)?;
        let answer = with_backend(Arc::new(Refused), || {
            registry.stop("p", Duration::from_millis(100))
        });
        assert!(answer.is_err());
        assert!(registry.owns("p")?);
        assert!(process_running(browser, token)?);
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        assert!(registry.stop("p", Duration::from_secs(2))?);
        assert!(!process_running(browser, token)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    session_ownership_a_camoufox_exit_that_swallows_the_error_still_ends_the_session,
    "tests/test_session_ownership.py::test_a_camoufox_exit_that_swallows_the_error_still_ends_the_session",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("vendor-close-error-proof")?;
        profile(t.path(), "p")?;
        fs::write(t.path().join("adapter-mode"), b"fail-close")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        registry.open_browser("p", "p", |session, _| {
            activate_adapter(session, t.path(), "camoufox")
        })?;
        let browser: u32 = fs::read_to_string(t.path().join("browser-pid"))?.parse()?;
        let token = identity::started_at(browser)?;
        assert!(registry.stop("p", Duration::from_secs(2))?);
        assert!(!process_running(browser, token)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        assert!(!registry.running("p")?);
        Ok(())
    }
);
source_test!(
    session_ownership_only_one_thread_ever_adopts_a_tree,
    "tests/test_session_ownership.py::test_only_one_thread_ever_adopts_a_tree",
    {
        let t = temp("single-native-session-tree")?;
        let session = Arc::new(std::sync::Mutex::new(Session::admit(
            t.path(),
            "p",
            "source-host",
        )?));
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let session = session.clone();
            let barrier = barrier.clone();
            let mut child = command("sleep")?;
            child.stdout(Stdio::null()).stderr(Stdio::null());
            workers.push(thread::spawn(move || {
                barrier.wait();
                session
                    .lock()
                    .expect("session")
                    .spawn(&mut child, "browser")
                    .map(|tree| tree.pid())
                    .map_err(|error| error.kind())
            }));
        }
        barrier.wait();
        let answers = workers
            .into_iter()
            .map(|worker| worker.join().expect("session spawn thread"))
            .collect::<Vec<_>>();
        assert_eq!(answers.iter().filter(|answer| answer.is_ok()).count(), 1);
        assert!(answers.contains(&Err(io::ErrorKind::AlreadyExists)));
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        session
            .lock()
            .expect("session")
            .finish(Duration::from_secs(2))?;
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    session_ownership_a_deleted_profile_cannot_be_deleted_while_its_browser_runs,
    "tests/test_session_ownership.py::test_a_deleted_profile_cannot_be_deleted_while_its_browser_runs",
    {
        let t = temp("foreign-browser-delete")?;
        profile(t.path(), "p")?;
        let mut tree = spawn_tree(t.path(), true)?;
        claims::take(t.path(), "p", &tree, "source-host", "browser")?;
        let path = t.path().join("sessions/p.json");
        let mut record: Value = serde_json::from_slice(&fs::read(&path)?)?;
        record["host"] = "different-host".into();
        record["pid"] = 424242.into();
        write_json(&path, &record)?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        assert!(repo.delete("p", "source-host", 1001.0).is_err());
        assert!(repo.get("p")?.is_some());
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Unknown
        );
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
