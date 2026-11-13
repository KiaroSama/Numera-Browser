use super::*;

fn race(stage: u8, repeats: usize, repeat_shutdown: bool) -> TestResult {
    use numera_core::ownership::registry::{Kind, Registry};
    use numera_core::ownership::test_support::{Backend, with_backend};
    struct Pause {
        stage: u8,
        arrived: mpsc::Sender<()>,
        release: std::sync::Mutex<mpsc::Receiver<()>>,
        born: std::sync::Mutex<Vec<Value>>,
    }
    impl Pause {
        fn wait(&self) -> io::Result<()> {
            self.arrived.send(()).map_err(io::Error::other)?;
            self.release
                .lock()
                .map_err(|_| io::Error::other("pause lock"))?
                .recv_timeout(Duration::from_secs(5))
                .map_err(io::Error::other)
        }
    }
    impl Backend for Pause {
        fn before_admit(&self, _: &str) -> Option<io::Result<()>> {
            (self.stage == 0).then(|| self.wait())
        }
        fn before_spawn(&self) -> Option<io::Result<()>> {
            (self.stage == 1).then(|| self.wait())
        }
        fn spawned(&self, record: &Value) -> Option<io::Result<()>> {
            self.born.lock().expect("birth record").push(record.clone());
            (self.stage == 2).then(|| self.wait())
        }
    }
    let _compiled = fixture()?;
    for attempt in 0..repeats {
        let t = temp("finite-admission-race")?;
        let registry = Arc::new(Registry::new(t.path().into(), "source-host".into(), None)?);
        let (arrived, observed) = mpsc::channel();
        let (release, waiting) = mpsc::channel();
        let pause = Arc::new(Pause {
            stage,
            arrived,
            release: std::sync::Mutex::new(waiting),
            born: std::sync::Mutex::new(Vec::new()),
        });
        let ready = t.path().join("ready.json");
        let leave = t.path().join("leave");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&leave)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let worker_registry = registry.clone();
        let worker_pause = pause.clone();
        let channel = ready.clone();
        let worker = thread::spawn(move || {
            with_backend(worker_pause, || {
                worker_registry.spawn(
                    "p",
                    &mut child,
                    &channel,
                    Kind::Finite,
                    None,
                    Duration::from_secs(2),
                )
            })
        });
        observed.recv_timeout(Duration::from_secs(3))?;
        let begin = Instant::now();
        let failed = registry.shutdown(Duration::from_millis(30));
        assert!(
            begin.elapsed() < Duration::from_millis(500),
            "shutdown waited outside its budget: {stage}/{attempt}"
        );
        assert!(
            failed.iter().any(|(id, _)| id == "p"),
            "unresolved native admission was silently released"
        );
        if repeat_shutdown {
            for _ in 0..2 {
                assert!(
                    registry
                        .shutdown(Duration::ZERO)
                        .iter()
                        .any(|(id, _)| id == "p")
                );
            }
        }
        if stage == 2 {
            assert_eq!(
                pause.born.lock().expect("born").len(),
                1,
                "post-spawn pause created no child"
            );
        }
        release.send(())?;
        let answer = worker.join().expect("finite start worker");
        assert!(answer.is_err(), "closed registry admitted a late helper");
        for born in pause.born.lock().expect("born").iter() {
            let mut reported = born.clone();
            reported["pidToken"] = reported["token"].take();
            assert!(
                recorded_tree::empty(&reported)?,
                "actual late helper tree survived"
            );
        }
        let lease_result = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)
            .map(drop)
            .map_err(|error| (error.kind(), error.to_string()));
        let after_running = registry.running("p")?;
        let after_claim = match claims::read(t.path(), "p") {
            claims::Claim::Absent => Value::Null,
            claims::Claim::Unreadable => json!({"unreadable":true}),
            claims::Claim::Record(record) => record,
        };
        assert!(
            !after_running,
            "late helper outlived completed startup/shutdown; stage={stage}; answer={answer:?}; claim={after_claim}; lease={lease_result:?}"
        );
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        if stage == 0 {
            assert!(
                pause.born.lock().expect("born").is_empty(),
                "cancelled preflight spawned anyway"
            );
        }
    }
    Ok(())
}

source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_5feceb66,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[0]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_6b86b273,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[1]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_d4735e3a,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[2]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_4e074085,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[3]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_4b227777,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[4]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_ef2d127d,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[5]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_e7f6c011,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[6]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_7902699b,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[7]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_2c624232,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[8]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_19581e27,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[9]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_4a44dc15,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[10]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_4fc82b26,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[11]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_6b51d431,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[12]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_3fdba35f,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[13]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_8527a891,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[14]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_e629fa65,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[15]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_b17ef6d1,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[16]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_4523540f,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[17]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_4ec9599f,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[18]",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_the_startup_shutdown_race_holds_every_time_9400f1b2,
    "tests/test_helper_bounded_lifetime.py::test_the_startup_shutdown_race_holds_every_time[19]",
    { race(0, 1, true) }
);
source_test!(
    helper_admission_a_shutdown_racing_a_start_never_loses_the_outcome_4f471200,
    "tests/test_helper_admission.py::test_a_shutdown_racing_a_start_never_loses_the_outcome[child_env-between admission and the spawn]",
    { race(0, 20, false) }
);
source_test!(
    helper_admission_a_shutdown_racing_a_start_never_loses_the_outcome_9139ec01,
    "tests/test_helper_admission.py::test_a_shutdown_racing_a_start_never_loses_the_outcome[check_open-immediately before the spawn]",
    { race(1, 20, false) }
);
source_test!(
    helper_admission_a_shutdown_racing_a_start_never_loses_the_outcome_595a0d57,
    "tests/test_helper_admission.py::test_a_shutdown_racing_a_start_never_loses_the_outcome[attach-after the spawn, before the record is bound to it]",
    { race(2, 20, false) }
);
source_test!(
    helper_lifecycle_matrix_twenty_controlled_start_shutdown_interleavings_01eb1602,
    "tests/test_helper_lifecycle_matrix.py::test_twenty_controlled_start_shutdown_interleavings[child_env]",
    { race(0, 20, false) }
);
source_test!(
    helper_lifecycle_matrix_twenty_controlled_start_shutdown_interleavings_78d5f3e6,
    "tests/test_helper_lifecycle_matrix.py::test_twenty_controlled_start_shutdown_interleavings[check_open]",
    { race(1, 20, false) }
);
source_test!(
    helper_lifecycle_matrix_twenty_controlled_start_shutdown_interleavings_a9190076,
    "tests/test_helper_lifecycle_matrix.py::test_twenty_controlled_start_shutdown_interleavings[attach]",
    { race(2, 20, false) }
);
source_test!(
    helper_admission_a_shutdown_does_not_release_a_reservation_that_has_not_spawned,
    "tests/test_helper_admission.py::test_a_shutdown_does_not_release_a_reservation_that_has_not_spawned",
    { race(0, 1, true) }
);
source_test!(
    helper_bounded_lifetime_a_second_shutdown_does_not_release_a_still_unresolved_reservation,
    "tests/test_helper_bounded_lifetime.py::test_a_second_shutdown_does_not_release_a_still_unresolved_reservation",
    { race(0, 1, true) }
);
source_test!(
    helper_lifecycle_matrix_cancelled_start_is_retained_across_repeated_shutdown,
    "tests/test_helper_lifecycle_matrix.py::test_cancelled_start_is_retained_across_repeated_shutdown",
    { race(0, 1, true) }
);
source_test!(
    helper_completion_regression_direct_settle_cannot_resolve_an_unstarted_reservation,
    "tests/test_helper_completion_regression.py::test_direct_settle_cannot_resolve_an_unstarted_reservation",
    { race(0, 1, true) }
);
source_test!(
    helper_admission_a_cancelled_reservation_refuses_before_it_spawns,
    "tests/test_helper_admission.py::test_a_cancelled_reservation_refuses_before_it_spawns",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Count(AtomicUsize);
        impl Backend for Count {
            fn before_spawn(&self) -> Option<io::Result<()>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                None
            }
        }
        let t = temp("closed-finite-admission")?;
        let _compiled = fixture()?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        assert!(registry.shutdown(Duration::from_millis(20)).is_empty());
        let ready = t.path().join("ready.json");
        let mut child = command("ready")?;
        child.arg(&ready).arg("ok");
        let observer = Arc::new(Count(AtomicUsize::new(0)));
        let answer = with_backend(observer.clone(), || {
            registry.spawn(
                "p",
                &mut child,
                &ready,
                Kind::Finite,
                None,
                Duration::from_millis(100),
            )
        });
        assert!(answer.is_err());
        assert_eq!(observer.0.load(Ordering::SeqCst), 0);
        assert!(!ready.exists());
        Ok(())
    }
);
source_test!(
    helper_terminal_state_no_helper_may_start_once_the_sweep_has_begun,
    "tests/test_helper_terminal_state.py::test_no_helper_may_start_once_the_sweep_has_begun",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Count(AtomicUsize);
        impl Backend for Count {
            fn before_spawn(&self) -> Option<io::Result<()>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                None
            }
        }
        let t = temp("closed-finite-admission")?;
        let _compiled = fixture()?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        assert!(registry.shutdown(Duration::from_millis(20)).is_empty());
        let ready = t.path().join("ready.json");
        let mut child = command("ready")?;
        child.arg(&ready).arg("ok");
        let observer = Arc::new(Count(AtomicUsize::new(0)));
        let answer = with_backend(observer.clone(), || {
            registry.spawn(
                "p",
                &mut child,
                &ready,
                Kind::Finite,
                None,
                Duration::from_millis(100),
            )
        });
        assert!(answer.is_err());
        assert_eq!(observer.0.load(Ordering::SeqCst), 0);
        assert!(!ready.exists());
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_refused_helper_never_gets_as_far_as_a_process,
    "tests/test_helper_terminal_state.py::test_a_refused_helper_never_gets_as_far_as_a_process",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        use numera_core::ownership::test_support::{Backend, with_backend};
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct Count(AtomicUsize);
        impl Backend for Count {
            fn before_spawn(&self) -> Option<io::Result<()>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                None
            }
        }
        let t = temp("closed-finite-admission")?;
        let _compiled = fixture()?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        assert!(registry.shutdown(Duration::from_millis(20)).is_empty());
        let ready = t.path().join("ready.json");
        let mut child = command("ready")?;
        child.arg(&ready).arg("ok");
        let observer = Arc::new(Count(AtomicUsize::new(0)));
        let answer = with_backend(observer.clone(), || {
            registry.spawn(
                "p",
                &mut child,
                &ready,
                Kind::Finite,
                None,
                Duration::from_millis(100),
            )
        });
        assert!(answer.is_err());
        assert_eq!(observer.0.load(Ordering::SeqCst), 0);
        assert!(!ready.exists());
        Ok(())
    }
);
