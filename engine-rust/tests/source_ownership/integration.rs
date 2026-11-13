use super::*;

fn owned_claim(root: &Path, id: &str) -> io::Result<Value> {
    match claims::read(root, id) {
        claims::Claim::Record(mut record) => {
            record["pidToken"] = record["token"].take();
            Ok(record)
        }
        _ => Err(io::Error::other("owned claim absent")),
    }
}
fn runtime(root: &Path) -> io::Result<numera_core::browser::runtime::Runtime> {
    Ok(numera_core::browser::runtime::Runtime {
        adapter: fixture()?,
        cwd: root.into(),
        home: root.join("home"),
        logs: root.join("logs"),
        country: None,
        relay: None,
        binary_options: Default::default(),
        registry: None,
        deployment: None,
    })
}
fn server(root: &Path) -> io::Result<numera_core::http::server::Server> {
    numera_core::http::server::Server::start(numera_core::http::server::Options {
        root: root.into(),
        address: "127.0.0.1:0".parse().expect("address"),
        token: String::new(),
        origins: String::new(),
        insecure_override: String::new(),
        password: None,
        host: "source-host".into(),
        installed: std::collections::BTreeMap::from([("playwright".into(), true)]),
        browser: runtime(root)?,
        dashboard: None,
    })
}
fn http(address: std::net::SocketAddr, method: &str, path: &str) -> io::Result<(u16, String)> {
    use std::io::Read;
    let mut stream = socket_fixture::connect(address, Duration::from_secs(2), |_, _| Ok(()))?;
    stream.set_read_timeout(Some(Duration::from_secs(4)))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{{}}"
    )?;
    let mut response = String::new();
    stream.take(1_048_576).read_to_string(&mut response)?;
    let code = response
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| io::Error::other("HTTP status absent"))?;
    Ok((code, response))
}
fn network_guard_control() -> TestResult {
    if std::env::var_os("NUMERA_SOCKET_FIXTURE_CONTROL").is_none() {
        let mut isolated = Command::new(std::env::current_exe()?);
        isolated
            .args([
                "--exact",
                "integration::network_guard_denied_connection_closes_the_socket_it_created",
                "--nocapture",
            ])
            .env("NUMERA_SOCKET_FIXTURE_CONTROL", "1");
        let result = capture::run(&mut isolated, limit(Duration::from_secs(3)))?;
        assert_eq!(
            result.exit,
            0,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        return Ok(());
    }

    use std::io::Read;
    let mut allocated = None;
    let mut attempts = 0;
    let error = socket_fixture::connect(
        "203.0.113.1:80".parse()?,
        Duration::from_secs(1),
        |socket, address| {
            attempts += 1;
            allocated = Some(socket_fixture::raw(socket));
            assert!(!address.ip().is_loopback());
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "external connection denied by explicit fixture scope",
            ))
        },
    )
    .expect_err("denied external connect was attempted");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(attempts, 1);
    assert!(socket_fixture::closed(
        allocated.ok_or("no socket allocated")?
    ));
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let accepted = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("loopback accepted");
        stream.write_all(b"OK").expect("loopback reply");
    });
    let mut stream = socket_fixture::connect(address, Duration::from_secs(1), |_, address| {
        assert!(address.ip().is_loopback());
        Ok(())
    })?;
    let mut bytes = [0; 2];
    stream.read_exact(&mut bytes)?;
    assert_eq!(&bytes, b"OK");
    accepted.join().expect("loopback fixture");
    Ok(())
}

source_test!(
    driver_capture_concurrency_paused_preflight_does_not_block_another_profiles_driver,
    "tests/test_driver_capture_concurrency.py::test_paused_preflight_does_not_block_another_profiles_driver",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("independent-driver-preflight")?;
        profile(t.path(), "p")?;
        profile(t.path(), "q")?;
        let registry = Arc::new(Registry::new(t.path().into(), "source-host".into(), None)?);
        let (paused, seen) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let first = registry.clone();
        let root = t.path().to_owned();
        let a = thread::spawn(move || {
            first.open_browser("p", "p", |session, profile| {
                paused.send(()).expect("pause");
                wait.recv_timeout(Duration::from_secs(5)).expect("resume");
                super::sessions::activate_adapter(session, &root, &profile.engine)
            })
        });
        seen.recv_timeout(Duration::from_secs(2))?;
        let second = registry.open_browser("q", "q", |session, profile| {
            super::sessions::activate_adapter(session, t.path(), &profile.engine)
        });
        release.send(())?;
        assert!(second?);
        assert!(a.join().expect("first driver")?);
        let p = owned_claim(t.path(), "p")?;
        let q = owned_claim(t.path(), "q")?;
        assert_ne!(p["tree"]["group"], q["tree"]["group"]);
        assert!(registry.shutdown(Duration::from_secs(2)).is_empty());
        assert!(recorded_tree::empty(&p)?);
        assert!(recorded_tree::empty(&q)?);
        Ok(())
    }
);
source_test!(
    driver_capture_concurrency_actual_spawns_and_their_record_callbacks_remain_serialized,
    "tests/test_driver_capture_concurrency.py::test_actual_spawns_and_their_record_callbacks_remain_serialized",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Paused {
            arrived: mpsc::Sender<()>,
            release: std::sync::Mutex<mpsc::Receiver<()>>,
            records: std::sync::Mutex<Vec<Value>>,
        }
        impl Backend for Paused {
            fn spawned(&self, record: &Value) -> Option<io::Result<()>> {
                let first = {
                    let mut rows = self.records.lock().expect("records");
                    rows.push(record.clone());
                    rows.len() == 1
                };
                if first {
                    self.arrived.send(()).expect("recording");
                    self.release
                        .lock()
                        .expect("release")
                        .recv_timeout(Duration::from_secs(5))
                        .expect("release recording");
                }
                None
            }
        }
        let t = temp("serialized-spawn-publication")?;
        let mut p = Session::admit(t.path(), "p", "source-host")?;
        let mut q = Session::admit(t.path(), "q", "source-host")?;
        let (arrived, seen) = mpsc::channel();
        let (release, waiting) = mpsc::channel();
        let hook = Arc::new(Paused {
            arrived,
            release: std::sync::Mutex::new(waiting),
            records: std::sync::Mutex::new(Vec::new()),
        });
        let h = hook.clone();
        let mut first_command = command("sleep")?;
        first_command.stdout(Stdio::null()).stderr(Stdio::null());
        let a = thread::spawn(move || {
            with_backend(h, || {
                p.spawn(&mut first_command, "browser")?;
                Ok::<_, io::Error>(p)
            })
        });
        seen.recv_timeout(Duration::from_secs(2))?;
        let (entered, observed) = mpsc::channel();
        let h = hook.clone();
        let mut next_command = command("sleep")?;
        next_command.stdout(Stdio::null()).stderr(Stdio::null());
        let b = thread::spawn(move || {
            with_backend(h, || {
                entered.send(()).expect("next preflight");
                q.spawn(&mut next_command, "browser")?;
                Ok::<_, io::Error>(q)
            })
        });
        observed.recv_timeout(Duration::from_secs(2))?;
        thread::sleep(Duration::from_millis(40));
        assert_eq!(
            hook.records.lock().expect("records").len(),
            1,
            "second spawn escaped durable publication serialization"
        );
        assert!(matches!(claims::read(t.path(), "q"), claims::Claim::Absent));
        release.send(())?;
        let mut p = a.join().expect("first")?;
        let mut q = b.join().expect("second")?;
        let records = hook.records.lock().expect("records").clone();
        assert_eq!(records.len(), 2);
        assert_ne!(records[0]["tree"]["group"], records[1]["tree"]["group"]);
        assert_eq!(owned_claim(t.path(), "p")?["pid"], records[0]["pid"]);
        assert_eq!(owned_claim(t.path(), "q")?["pid"], records[1]["pid"]);
        p.finish(Duration::from_secs(1))?;
        q.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    driver_capture_concurrency_matching_spawn_on_an_unregistered_thread_is_not_captured,
    "tests/test_driver_capture_concurrency.py::test_matching_spawn_on_an_unregistered_thread_is_not_captured",
    {
        let t = temp("sibling-spawn-not-adopted")?;
        let mut session = Session::admit(t.path(), "p", "source-host")?;
        let mut sibling = command("sleep")?;
        sibling.stdout(Stdio::null()).stderr(Stdio::null());
        let mut stranger = super::orphans::RawChild(
            thread::spawn(move || sibling.spawn())
                .join()
                .expect("sibling")?,
        );
        assert!(session.tree_record().is_none());
        let mut own = command("sleep")?;
        own.stdout(Stdio::null()).stderr(Stdio::null());
        session.spawn(&mut own, "browser")?;
        assert_ne!(
            session.tree_record().ok_or("owner absent")?["pid"],
            stranger.0.id()
        );
        session.finish(Duration::from_secs(1))?;
        assert!(
            stranger.0.try_wait()?.is_none(),
            "explicit owner captured another thread's child"
        );
        Ok(())
    }
);
source_test!(
    driver_capture_concurrency_failed_preflight_leaves_no_claim_or_interception,
    "tests/test_driver_capture_concurrency.py::test_failed_preflight_leaves_no_claim_or_interception",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("failed-preflight-no-interception")?;
        profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let failed = registry.open_browser("p", "p", |_session, _profile| {
            Err(ready::StartupFailed {
                kind: "failed".into(),
                message: "controlled preflight failed".into(),
            })
        });
        assert!(
            failed
                .expect_err("preflight accepted")
                .message
                .contains("controlled preflight failed")
        );
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        assert!(!t.path().join("page-created").exists());
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        drop(lease);
        assert!(registry.open_browser("p", "p", |session, profile| {
            super::sessions::activate_adapter(session, t.path(), &profile.engine)
        })?);
        assert!(registry.stop("p", Duration::from_secs(1))?);
        Ok(())
    }
);
source_test!(
    startup_handshake_two_simultaneous_launches_leave_exactly_one_owner,
    "tests/test_startup_handshake.py::test_two_simultaneous_launches_leave_exactly_one_owner",
    {
        use numera_core::ownership::registry::Registry;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let t = temp("simultaneous-native-browser-open")?;
        profile(t.path(), "p")?;
        let registry = Arc::new(Registry::new(t.path().into(), "source-host".into(), None)?);
        let barrier = Arc::new(Barrier::new(3));
        let spawned = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let root = t.path().to_owned();
            let registry = registry.clone();
            let barrier = barrier.clone();
            let spawned = spawned.clone();
            threads.push(thread::spawn(move || {
                barrier.wait();
                registry.open_browser("p", "p", |session, profile| {
                    spawned.fetch_add(1, Ordering::SeqCst);
                    super::sessions::activate_adapter(session, &root, &profile.engine)
                })
            }));
        }
        barrier.wait();
        let mut answers = Vec::new();
        for thread in threads {
            answers.push(thread.join().expect("launch")?);
        }
        answers.sort();
        assert_eq!(answers, vec![false, true]);
        assert_eq!(spawned.load(Ordering::SeqCst), 1);
        let record = owned_claim(t.path(), "p")?;
        assert!(registry.stop("p", Duration::from_secs(2))?);
        assert!(recorded_tree::empty(&record)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    process_registry_the_kind_of_a_child_is_not_guessable,
    "tests/test_process_registry.py::test_the_kind_of_a_child_is_not_guessable",
    {
        use numera_core::ownership::registry::Kind;
        assert_eq!(
            [
                Kind::Launch.name(),
                Kind::Attach.name(),
                Kind::ManualWarmup.name(),
                Kind::ScheduledWarmup.name()
            ],
            ["launch", "attach", "manual_warmup", "scheduled_warmup"]
        );
        let t = temp("invalid-native-owner-kind")?;
        let source = t.path().join("invalid_kind.rs");
        fs::write(
            &source,
            r#"use numera_core::ownership::registry::Registry;fn invalid(r:&Registry,command:&mut std::process::Command,path:&std::path::Path){let _=r.spawn("p",command,path,"warmup",None,std::time::Duration::ZERO);}"#,
        )?;
        let deps = std::env::current_exe()?
            .parent()
            .ok_or("test deps")?
            .to_owned();
        let mut compiler =
            Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        compiler
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg(source)
            .arg("--extern")
            .arg(format!(
                "numera_core={}",
                library(&deps, "numera_core")?.display()
            ))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()))
            .arg("-o")
            .arg(t.path().join("invalid.rmeta"));
        let answer = capture::run(&mut compiler, limit(Duration::from_secs(30)))?;
        assert_ne!(answer.exit, 0, "unknown string kind reached runtime");
        let diagnostic = String::from_utf8_lossy(&answer.stderr);
        assert!(
            diagnostic.contains("expected `Kind`") && diagnostic.contains("found `&str`"),
            "unrelated compilation error: {diagnostic}"
        );
        assert!(!t.path().join("sessions").exists());
        Ok(())
    }
);
source_test!(
    helper_lifecycle_shutdown_is_wired_into_the_app,
    "tests/test_helper_lifecycle.py::test_shutdown_is_wired_into_the_app",
    {
        let t = temp("http-owned-shutdown")?;
        profile(t.path(), "p")?;
        let mut server = server(t.path())?;
        server.wait_ready(Duration::from_secs(3))?;
        let address = server.address();
        let response = http(address, "POST", "/api/profiles/p/launch")?;
        assert_eq!(response.0, 200, "{}", response.1);
        let browser: u32 = fs::read_to_string(t.path().join("browser-pid"))?.parse()?;
        let token = identity::started_at(browser)?;
        assert!(process_running(browser, token)?);
        server.stop(Duration::from_secs(3))?;
        assert!(!process_running(browser, token)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        assert!(
            std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err()
        );
        Ok(())
    }
);
source_test!(
    helper_terminal_state_the_shutdown_dispatcher_surfaces_the_list,
    "tests/test_helper_terminal_state.py::test_the_shutdown_dispatcher_surfaces_the_list",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Refuse;
        impl Backend for Refuse {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(false))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled shutdown refusal",
                )))
            }
        }
        let t = temp("http-shutdown-surfaces-live-owner")?;
        profile(t.path(), "p")?;
        let mut server = server(t.path())?;
        server.wait_ready(Duration::from_secs(3))?;
        assert_eq!(
            http(server.address(), "POST", "/api/profiles/p/launch")?.0,
            200
        );
        fs::write(t.path().join("adapter-mode"), b"fail-close")?;
        let failure = with_backend(Arc::new(Refuse), || server.stop(Duration::from_secs(12)))
            .expect_err("shutdown discarded unsettled owners");
        assert!(
            failure
                .to_string()
                .contains("HTTP browser cleanup remains unsettled"),
            "{failure}"
        );
        assert!(
            matches!(claims::read(t.path(), "p"), claims::Claim::Record(_)),
            "failed shutdown erased its claim"
        );
        server.stop(Duration::from_secs(3))?;
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
#[cfg(unix)]
source_test!(
    shell_death_cleanup_a_killed_shell_closes_the_browsers_the_engine_launched,
    "tests/test_shell_death_cleanup.py::test_a_killed_shell_closes_the_browsers_the_engine_launched",
    {
        let t = temp("real-shell-death-shutdown")?;
        profile(t.path(), "p")?;
        fs::write(t.path().join("adapter-mode"), b"lock-browser")?;
        let mut shell_command = command("sleep")?;
        shell_command.stdout(Stdio::null()).stderr(Stdio::null());
        let mut shell = super::orphans::RawChild(shell_command.spawn()?);
        let mut engine_command = command("native-serve")?;
        engine_command
            .arg(t.path())
            .env("NUMERA_PARENT_PID", shell.0.id().to_string())
            .env_remove("NUMERA_API_TOKEN")
            .env_remove("NUMERA_UPDATE_TOKEN")
            .env_remove("NUMERA_UPDATE_TICKET")
            .env_remove("NUMERA_UPDATE_ACTIVATION")
            .current_dir(t.path())
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(
                t.path().join("engine-stderr"),
            )?));
        let mut engine = OwnedTree::spawn(&mut engine_command)?;
        let ready = t.path().join("server-address");
        if let Err(error) = wait_file(&ready) {
            return Err(format!(
                "{error}; native helper: {}",
                fs::read_to_string(t.path().join("engine-stderr"))?
            )
            .into());
        }
        let address = fs::read_to_string(ready)?.parse()?;
        assert_eq!(http(address, "POST", "/api/profiles/p/launch")?.0, 200);
        wait_file(&t.path().join("renderer-pid"))?;
        for path in ["browser.lock", "renderer.lock"] {
            let lock = DataLock::new(t.path().join(path))?;
            assert_eq!(
                lock.acquire(Duration::from_millis(40))
                    .err()
                    .ok_or("live browser failed to hold OS lock")?
                    .kind(),
                io::ErrorKind::TimedOut
            );
        }
        shell.0.kill()?;
        shell.0.wait()?;
        let end = Instant::now() + Duration::from_secs(12);
        let status = loop {
            if let Some(status) = engine.child_mut().try_wait()? {
                break status;
            }
            assert!(
                Instant::now() < end,
                "engine did not exit after actual parent death"
            );
            thread::sleep(Duration::from_millis(10));
        };
        assert!(status.success(), "native engine exited with {status}");
        for path in ["browser.lock", "renderer.lock"] {
            let lock = DataLock::new(t.path().join(path))?;
            let _guard = lock.acquire(Duration::from_secs(1))?;
        }
        engine.settle(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    network_guard_denied_connection_closes_the_socket_it_created,
    "tests/test_network_guard.py::test_denied_connection_closes_the_socket_it_created",
    { network_guard_control() }
);
