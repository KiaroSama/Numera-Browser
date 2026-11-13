use super::*;

use numera_core::ownership::registry::{Kind, Registry};
fn start_worker(
    registry: &Registry,
    root: &Path,
    id: &str,
    kind: Kind,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ready = root.join(format!("ready-{id}"));
    let leave = root.join(format!("leave-{id}"));
    let mut child = command("ready")?;
    child
        .arg(&ready)
        .arg("ok")
        .arg(&leave)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    assert!(registry.spawn(id, &mut child, &ready, kind, None, Duration::from_secs(2))?);
    match claims::read(root, id) {
        claims::Claim::Record(mut record) => {
            record["pidToken"] = record["token"].take();
            Ok(record)
        }
        _ => Err("registered worker has no durable claim".into()),
    }
}
fn overlapping_stop() -> TestResult {
    use numera_core::ownership::test_support::{Backend, with_backend};
    struct Pause {
        arrived: mpsc::Sender<()>,
        release: std::sync::Mutex<mpsc::Receiver<()>>,
        signals: std::sync::atomic::AtomicUsize,
    }
    impl Backend for Pause {
        fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
            self.signals
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.arrived.send(()).expect("settle arrived");
            self.release
                .lock()
                .expect("release")
                .recv_timeout(Duration::from_secs(3))
                .expect("settle released");
            None
        }
    }
    let t = temp("overlapping-owner-settlement")?;
    let registry = Arc::new(Registry::new(t.path().into(), "source-host".into(), None)?);
    let record = start_worker(&registry, t.path(), "p", Kind::Finite)?;
    let (arrived, observed) = mpsc::channel();
    let (release, waiting) = mpsc::channel();
    let pause = Arc::new(Pause {
        arrived,
        release: std::sync::Mutex::new(waiting),
        signals: std::sync::atomic::AtomicUsize::new(0),
    });
    let first_registry = registry.clone();
    let first_pause = pause.clone();
    let first = thread::spawn(move || {
        with_backend(first_pause, || {
            first_registry.stop("p", Duration::from_secs(2))
        })
    });
    observed.recv_timeout(Duration::from_secs(2))?;
    let (sent, received) = mpsc::channel();
    let second_registry = registry.clone();
    let second = thread::spawn(move || {
        let result = second_registry.stop("p", Duration::from_secs(2));
        sent.send(result).expect("second settle");
    });
    assert!(
        matches!(
            received.recv_timeout(Duration::from_millis(30)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ),
        "second settlement bypassed the owner decision"
    );
    release.send(())?;
    assert!(first.join().expect("first settlement")?);
    assert!(!received.recv_timeout(Duration::from_secs(2))??);
    second.join().expect("second settlement");
    assert_eq!(pause.signals.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(!registry.running("p")?);
    assert!(recorded_tree::empty(&record)?);
    assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
    Ok(())
}

source_test!(
    helper_deadline_the_module_states_a_finite_deadline,
    "tests/test_helper_deadline.py::test_the_module_states_a_finite_deadline",
    {
        let configured = limit(Duration::from_secs(numera_core::mcp::CALL_TIMEOUT_SECONDS));
        assert!(!configured.wall.is_zero());
        assert!(configured.wall <= Duration::from_secs(1800));
        assert!(!configured.idle.is_zero());
        assert!(!configured.cleanup.is_zero());
        Ok(())
    }
);
source_test!(
    helper_lifecycle_a_second_settle_arriving_mid_settle_does_not_strand_the_helper,
    "tests/test_helper_lifecycle.py::test_a_second_settle_arriving_mid_settle_does_not_strand_the_helper",
    { overlapping_stop() }
);
source_test!(
    helper_lifecycle_matrix_settlement_overlap_has_one_terminal_result,
    "tests/test_helper_lifecycle_matrix.py::test_settlement_overlap_has_one_terminal_result",
    { overlapping_stop() }
);
source_test!(
    helper_admission_shutting_down_twice_is_not_a_different_answer,
    "tests/test_helper_admission.py::test_shutting_down_twice_is_not_a_different_answer",
    {
        let t = temp("repeat-shutdown")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        start_worker(&registry, t.path(), "p", Kind::Finite)?;
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(!registry.running("p")?);
        let ready = t.path().join("late-ready");
        let mut late = command("ready")?;
        late.arg(&ready).arg("ok");
        assert!(
            registry
                .spawn(
                    "q",
                    &mut late,
                    &ready,
                    Kind::Finite,
                    None,
                    Duration::from_secs(1)
                )
                .is_err()
        );
        assert!(!ready.exists());
        Ok(())
    }
);
source_test!(
    helper_admission_concurrent_shutdowns_do_not_race_each_other,
    "tests/test_helper_admission.py::test_concurrent_shutdowns_do_not_race_each_other",
    {
        let t = temp("concurrent-shutdown")?;
        let registry = Arc::new(Registry::new(t.path().into(), "source-host".into(), None)?);
        let record = start_worker(&registry, t.path(), "p", Kind::Finite)?;
        let barrier = Arc::new(Barrier::new(3));
        let (send, receive) = mpsc::channel();
        let mut workers = Vec::new();
        for _ in 0..2 {
            let registry = registry.clone();
            let barrier = barrier.clone();
            let send = send.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                send.send(registry.shutdown(Duration::from_secs(1)))
                    .expect("shutdown result");
            }));
        }
        barrier.wait();
        for _ in 0..2 {
            assert!(receive.recv_timeout(Duration::from_secs(3))?.is_empty());
        }
        for worker in workers {
            worker.join().expect("shutdown joined");
        }
        assert!(recorded_tree::empty(&record)?);
        assert!(!registry.running("p")?);
        Ok(())
    }
);
source_test!(
    helper_lifecycle_matrix_concurrent_managers_do_not_share_admission,
    "tests/test_helper_lifecycle_matrix.py::test_concurrent_managers_do_not_share_admission",
    {
        let t = temp("separate-manager-admission")?;
        let a = Registry::new(t.path().into(), "source-host".into(), None)?;
        let b = Registry::new(t.path().into(), "source-host".into(), None)?;
        assert!(a.shutdown(Duration::from_millis(50)).is_empty());
        let record = start_worker(&b, t.path(), "p", Kind::Finite)?;
        assert!(b.running("p")?);
        let ready = t.path().join("refused");
        let mut child = command("ready")?;
        child.arg(&ready).arg("ok");
        assert!(
            a.spawn(
                "q",
                &mut child,
                &ready,
                Kind::Finite,
                None,
                Duration::from_millis(100)
            )
            .is_err()
        );
        assert!(!ready.exists());
        assert!(!recorded_tree::empty(&record)?);
        assert!(b.shutdown(Duration::from_secs(1)).is_empty());
        Ok(())
    }
);
source_test!(
    helper_admission_one_app_s_shutdown_leaves_another_app_s_helper_alone,
    "tests/test_helper_admission.py::test_one_app_s_shutdown_leaves_another_app_s_helper_alone",
    {
        let t = temp("registry-lifetime-isolation")?;
        let a = Registry::new(t.path().into(), "source-host".into(), None)?;
        let first = start_worker(&a, t.path(), "p", Kind::Finite)?;
        let b = Registry::new(t.path().into(), "source-host".into(), None)?;
        let second = start_worker(&b, t.path(), "q", Kind::Finite)?;
        assert!(a.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&first)?);
        assert!(b.running("q")?);
        assert!(
            !recorded_tree::empty(&second)?,
            "another registry swept this helper"
        );
        assert!(b.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&second)?);
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_new_app_is_not_closed_by_another_app_s_shutdown,
    "tests/test_helper_terminal_state.py::test_a_new_app_is_not_closed_by_another_app_s_shutdown",
    {
        let t = temp("registry-lifetime-isolation")?;
        let a = Registry::new(t.path().into(), "source-host".into(), None)?;
        assert!(a.shutdown(Duration::from_secs(1)).is_empty());
        let b = Registry::new(t.path().into(), "source-host".into(), None)?;
        let second = start_worker(&b, t.path(), "q", Kind::Finite)?;
        assert!(b.running("q")?);
        assert!(
            !recorded_tree::empty(&second)?,
            "another registry swept this helper"
        );
        assert!(b.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&second)?);
        Ok(())
    }
);
source_test!(
    helper_terminal_state_one_app_s_sweep_does_not_reach_another_app_s_helpers,
    "tests/test_helper_terminal_state.py::test_one_app_s_sweep_does_not_reach_another_app_s_helpers",
    {
        let t = temp("registry-lifetime-isolation")?;
        let a = Registry::new(t.path().into(), "source-host".into(), None)?;
        let first = start_worker(&a, t.path(), "p", Kind::Finite)?;
        let b = Registry::new(t.path().into(), "source-host".into(), None)?;
        let second = start_worker(&b, t.path(), "q", Kind::Finite)?;
        assert!(a.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&first)?);
        assert!(b.running("q")?);
        assert!(
            !recorded_tree::empty(&second)?,
            "another registry swept this helper"
        );
        assert!(b.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&second)?);
        Ok(())
    }
);
source_test!(
    helper_terminal_state_an_app_owns_its_own_manager,
    "tests/test_helper_terminal_state.py::test_an_app_owns_its_own_manager",
    {
        let t = temp("registry-lifetime-isolation")?;
        let a = Registry::new(t.path().into(), "source-host".into(), None)?;
        let first = start_worker(&a, t.path(), "p", Kind::Finite)?;
        let b = Registry::new(t.path().into(), "source-host".into(), None)?;
        let second = start_worker(&b, t.path(), "q", Kind::Finite)?;
        assert!(a.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&first)?);
        assert!(b.running("q")?);
        assert!(
            !recorded_tree::empty(&second)?,
            "another registry swept this helper"
        );
        assert!(b.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&second)?);
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_successful_shutdown_reports_nothing,
    "tests/test_helper_terminal_state.py::test_a_successful_shutdown_reports_nothing",
    {
        let t = temp("registered-helper-settlement")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Finite)?;
        assert!(registry.running("p")?);
        assert!(!recorded_tree::empty(&record)?);
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&record)?);
        assert!(!registry.running("p")?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    helper_lifecycle_shutdown_ends_a_helper_that_is_still_running,
    "tests/test_helper_lifecycle.py::test_shutdown_ends_a_helper_that_is_still_running",
    {
        let t = temp("registered-helper-settlement")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Finite)?;
        assert!(registry.running("p")?);
        assert!(!recorded_tree::empty(&record)?);
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&record)?);
        assert!(!registry.running("p")?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    helper_bounded_lifetime_a_resolved_reservation_is_still_settled_normally,
    "tests/test_helper_bounded_lifetime.py::test_a_resolved_reservation_is_still_settled_normally",
    {
        let t = temp("registered-helper-settlement")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Finite)?;
        assert!(registry.running("p")?);
        assert!(!recorded_tree::empty(&record)?);
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&record)?);
        assert!(!registry.running("p")?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    helper_lifecycle_a_running_helper_is_tracked_and_released,
    "tests/test_helper_lifecycle.py::test_a_running_helper_is_tracked_and_released",
    {
        let t = temp("running-helper-owner")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Finite)?;
        assert!(registry.running("p")?);
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        fs::write(t.path().join("leave-p"), b"go")?;
        let end = Instant::now() + Duration::from_secs(2);
        while registry.running("p")? {
            assert!(Instant::now() < end, "completed helper remained registered");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(recorded_tree::empty(&record)?);
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_plain_helper_leaves_nothing_behind,
    "tests/test_helper_terminal_state.py::test_a_plain_helper_leaves_nothing_behind",
    {
        let t = temp("running-helper-owner")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Finite)?;
        assert!(registry.running("p")?);
        assert_eq!(
            claims::state(&claims::read(t.path(), "p"), "source-host"),
            claims::State::Busy
        );
        fs::write(t.path().join("leave-p"), b"go")?;
        let end = Instant::now() + Duration::from_secs(2);
        while registry.running("p")? {
            assert!(Instant::now() < end, "completed helper remained registered");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(recorded_tree::empty(&record)?);
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    helper_lifecycle_a_timed_out_helper_leaves_the_registry,
    "tests/test_helper_lifecycle.py::test_a_timed_out_helper_leaves_the_registry",
    {
        let t = temp("timed-out-owner-removal")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("never-ready");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("wait")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let failure = registry
            .spawn(
                "p",
                &mut child,
                &ready,
                Kind::Finite,
                None,
                Duration::from_millis(150),
            )
            .expect_err("silent helper accepted");
        assert_eq!(failure.kind, "timeout");
        assert!(!registry.running("p")?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        Ok(())
    }
);
source_test!(
    helper_lifecycle_the_finite_deadline_does_not_reach_a_persistent_session,
    "tests/test_helper_lifecycle.py::test_the_finite_deadline_does_not_reach_a_persistent_session",
    {
        let t = temp("finite-vs-persistent-owner")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let persistent = start_worker(&registry, t.path(), "p", Kind::Launch)?;
        let output = emit("finite-completed", 0)?;
        assert_eq!(output.stdout, b"finite-completed");
        assert!(registry.running("p")?);
        assert!(registry.owns("p")?);
        assert!(
            !recorded_tree::empty(&persistent)?,
            "finite capture ended a persistent session"
        );
        assert!(registry.stop("p", Duration::from_secs(1))?);
        assert!(recorded_tree::empty(&persistent)?);
        Ok(())
    }
);
source_test!(
    helper_completion_regression_end_all_uses_one_budget_across_all_helpers,
    "tests/test_helper_completion_regression.py::test_end_all_uses_one_budget_across_all_helpers",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Pending;
        impl Backend for Pending {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(false))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                Some(Ok(()))
            }
        }
        let t = temp("one-global-shutdown-budget")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let records = (0..3)
            .map(|index| start_worker(&registry, t.path(), &format!("p{index}"), Kind::Finite))
            .collect::<Result<Vec<_>, _>>()?;
        let begin = Instant::now();
        let failed = with_backend(Arc::new(Pending), || {
            registry.shutdown(Duration::from_millis(150))
        });
        let elapsed = begin.elapsed();
        assert_eq!(failed.len(), 3);
        assert!(
            elapsed < Duration::from_millis(300),
            "each helper received a fresh cleanup budget: {elapsed:?}"
        );
        for record in &records {
            assert!(
                !recorded_tree::empty(record)?,
                "fixture secretly killed the survivor before acceptance"
            );
        }
        assert!(registry.shutdown(Duration::from_secs(2)).is_empty());
        for record in records {
            assert!(recorded_tree::empty(&record)?);
        }
        Ok(())
    }
);
source_test!(
    helper_terminal_state_a_tree_that_cannot_be_proved_empty_stays_in_the_registry,
    "tests/test_helper_terminal_state.py::test_a_tree_that_cannot_be_proved_empty_stays_in_the_registry",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Refused;
        impl Backend for Refused {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(false))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled helper signal refusal",
                )))
            }
        }
        let t = temp("retained-unproved-owner")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Launch)?;
        let failed = with_backend(Arc::new(Refused), || {
            registry.shutdown(Duration::from_millis(100))
        });
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, "p");
        assert!(registry.owns("p")?);
        assert!(!recorded_tree::empty(&record)?);
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&record)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    helper_terminal_state_end_all_reports_what_it_could_not_end,
    "tests/test_helper_terminal_state.py::test_end_all_reports_what_it_could_not_end",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Refused;
        impl Backend for Refused {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(false))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled helper signal refusal",
                )))
            }
        }
        let t = temp("retained-unproved-owner")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Launch)?;
        let failed = with_backend(Arc::new(Refused), || {
            registry.shutdown(Duration::from_millis(100))
        });
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, "p");
        assert!(registry.owns("p")?);
        assert!(!recorded_tree::empty(&record)?);
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&record)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    proved_dead_stop_session_keeps_a_child_it_could_not_kill,
    "tests/test_proved_dead.py::test_stop_session_keeps_a_child_it_could_not_kill",
    {
        use numera_core::ownership::test_support::{Backend, with_backend};
        struct Refused;
        impl Backend for Refused {
            fn group_empty(&self, _: i32) -> Option<io::Result<bool>> {
                Some(Ok(false))
            }
            fn signal(&self, _: i32, _: bool) -> Option<io::Result<()>> {
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "controlled helper signal refusal",
                )))
            }
        }
        let t = temp("retained-unproved-owner")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let record = start_worker(&registry, t.path(), "p", Kind::Launch)?;
        let failed = with_backend(Arc::new(Refused), || {
            registry.shutdown(Duration::from_millis(100))
        });
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, "p");
        assert!(registry.owns("p")?);
        assert!(!recorded_tree::empty(&record)?);
        assert!(Session::admit(t.path(), "p", "source-host").is_err());
        assert!(registry.shutdown(Duration::from_secs(1)).is_empty());
        assert!(recorded_tree::empty(&record)?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
