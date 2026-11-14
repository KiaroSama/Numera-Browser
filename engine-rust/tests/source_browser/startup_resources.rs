use super::*;
use numera_core::ownership::{
    claims,
    registry::Registry,
    session::Session,
    test_support::{self, Backend},
    tree::OwnedTree,
};
use std::{
    io,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
fn fail_after(mode: &str, backend: &str, context: bool) {
    let d = fresh();
    let p = profile();
    let result = support::native_open(d.path(), backend, mode, &p);
    assert!(result.is_err(), "fault did not fail launch");
    let driver = d.path().join("driver");
    assert_eq!(driver.join("context-created").exists(), context);
    if context {
        assert!(
            driver.join("context-closed").exists(),
            "live context was not closed"
        );
    }
    support::wait(&driver.join("driver-stopped"));
    assert!(matches!(
        claims::read(d.path(), &p.id),
        claims::Claim::Absent
    ));
    let lease = numera_core::storage::locks::SessionLease::acquire(
        &numera_core::storage::locks::SessionLease::path_for(d.path(), &p.id)
            .expect("browser contract"),
    )
    .expect("browser contract");
    drop(lease);
}
fn fixture_tree(mode: &str, extra: Option<&str>) -> OwnedTree {
    let mut c = Command::new(support::fixture());
    c.arg(mode);
    if let Some(extra) = extra {
        c.arg(extra);
    }
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    OwnedTree::spawn(&mut c).expect("browser contract")
}
fn scheduler(fail_commit: bool) {
    let d = fresh();
    let h = http::Http::new(d.path());
    let p = h.request(
        "POST",
        "/api/profiles",
        json!({"name":"scheduled","seed":1}),
    );
    let id = p["id"].as_str().expect("browser contract");
    let schedules = numera_core::storage::schedules::ScheduleStore::new(d.path().into());
    let row = schedules
        .add(
            id,
            json!({"everyHours":1,"minutes":1,"preset":"general"}),
            Some(0.0),
        )
        .expect("browser contract");
    let sid = row["id"].as_str().expect("browser contract");
    let registry = Arc::new(
        Registry::new(d.path().into(), "source-browser-host".into(), None)
            .expect("browser contract"),
    );
    let scheduler = numera_core::http::scheduler::Scheduler::new(
        d.path().into(),
        None,
        "source-browser-host".into(),
        Arc::clone(&registry),
        support::runtime(d.path()),
    )
    .expect("browser contract");
    let root = d.path().to_owned();
    let once = Arc::new(AtomicBool::new(false));
    let observed = Arc::clone(&once);
    numera_core::vault::testing::with_observer(
        move |event| {
            if fail_commit
                && event.operation == "before-atomic-write"
                && event.path == root.join("schedules.json")
                && root.join("driver/page-created").exists()
                && !observed.swap(true, Ordering::SeqCst)
            {
                return Err(io::Error::new(
                    io::ErrorKind::StorageFull,
                    "no space left on device",
                ));
            }
            Ok(numera_core::vault::testing::Action::Continue)
        },
        || scheduler.tick().expect("browser contract"),
    );
    let row = schedules
        .get(sid)
        .expect("browser contract")
        .expect("browser contract");
    assert!(
        d.path().join("driver/context-created").exists(),
        "scheduler never reached browser"
    );
    if fail_commit {
        assert!(once.load(Ordering::SeqCst));
        assert!(row["lastRun"].is_null());
        assert_eq!(row["lastOutcome"], "failed");
        assert!(
            !registry.running(id).expect("browser contract") || !row["claimedBy"].is_null(),
            "live browser against due unclaimed schedule"
        );
        support::wait(&d.path().join("driver/driver-stopped"));
    } else {
        assert_eq!(row["lastOutcome"], "started");
        assert!(!row["lastRun"].is_null());
        assert!(registry.running(id).expect("browser contract"));
        assert!(!d.path().join("driver/driver-stopped").exists());
    }
    assert!(registry.shutdown(Duration::from_secs(3)).is_empty());
}
source_test!(
    test_playwright_releases_everything_when_startup_fails_after_the_context,
    "tests/test_startup_resources.py::test_playwright_releases_everything_when_startup_fails_after_the_context",
    {
        fail_after("init-fail", "playwright", true);
    }
);
source_test!(
    test_playwright_stops_the_driver_when_the_context_itself_fails,
    "tests/test_startup_resources.py::test_playwright_stops_the_driver_when_the_context_itself_fails",
    {
        fail_after("launch-fail", "playwright", false);
    }
);
source_test!(
    test_cloak_releases_everything_when_startup_fails_after_the_context,
    "tests/test_startup_resources.py::test_cloak_releases_everything_when_startup_fails_after_the_context",
    {
        fail_after("page-fail", "cloak", true);
    }
);
source_test!(
    test_the_lease_is_free_again_after_a_failed_open,
    "tests/test_startup_resources.py::test_the_lease_is_free_again_after_a_failed_open",
    {
        let d = fresh();
        let p = profile();
        assert!(support::native_open(d.path(), "playwright", "launch-fail", &p).is_err());
        let next = Session::admit(d.path(), &p.id, "source-browser-host")
            .expect("failed open released lease");
        drop(next);
        assert!(matches!(
            claims::read(d.path(), &p.id),
            claims::Claim::Absent
        ));
    }
);
source_test!(
    test_end_child_returns_a_real_code_and_only_once_the_child_is_gone,
    "tests/test_startup_resources.py::test_end_child_returns_a_real_code_and_only_once_the_child_is_gone",
    {
        let mut tree = fixture_tree("leaf", None);
        assert!(
            tree.child_mut()
                .try_wait()
                .expect("browser contract")
                .is_none()
        );
        tree.settle(Duration::from_secs(2))
            .expect("browser contract");
        assert!(
            tree.child_mut()
                .try_wait()
                .expect("browser contract")
                .is_some()
        );
        assert!(tree.empty().expect("browser contract"));
    }
);
source_test!(
    test_end_child_waits_after_the_kill_escalation,
    "tests/test_startup_resources.py::test_end_child_waits_after_the_kill_escalation",
    {
        struct Observe(Arc<AtomicBool>);
        impl Backend for Observe {
            fn signal(&self, _target: i32, force: bool) -> Option<io::Result<()>> {
                if force {
                    self.0.store(true, Ordering::SeqCst);
                }
                None
            }
        }
        let mut tree = fixture_tree("leaf", None);
        let force = Arc::new(AtomicBool::new(false));
        test_support::with_backend(Arc::new(Observe(Arc::clone(&force))), || {
            tree.settle(Duration::from_secs(2))
                .expect("browser contract")
        });
        assert!(force.load(Ordering::SeqCst));
        assert!(
            tree.child_mut()
                .try_wait()
                .expect("browser contract")
                .is_some()
        );
        assert!(tree.empty().expect("browser contract"));
    }
);
source_test!(
    test_end_child_escalates_only_when_the_polite_ask_is_ignored,
    "tests/test_startup_resources.py::test_end_child_escalates_only_when_the_polite_ask_is_ignored",
    {
        struct Observe(Arc<AtomicBool>);
        impl Backend for Observe {
            fn signal(&self, _target: i32, _force: bool) -> Option<io::Result<()>> {
                self.0.store(true, Ordering::SeqCst);
                None
            }
        }
        let d = fresh();
        let mut p = profile();
        p.fingerprint = fp("windows", 3);
        let mut adapter =
            support::native_open(d.path(), "playwright", "normal", &p).expect("browser contract");
        assert_eq!(
            adapter
                .call("close", serde_json::Map::new(), Duration::from_secs(5))
                .expect("browser contract")["closing"],
            true
        );
        support::wait(&d.path().join("driver/driver-stopped"));
        let end = std::time::Instant::now() + Duration::from_secs(3);
        while adapter.settled_exit().expect("browser contract").is_none() {
            assert!(std::time::Instant::now() < end);
            std::thread::park_timeout(Duration::from_millis(2));
        }
        let forced = Arc::new(AtomicBool::new(false));
        test_support::with_backend(Arc::new(Observe(Arc::clone(&forced))), || drop(adapter));
        assert!(
            !forced.load(Ordering::SeqCst),
            "graceful SDK close was killed again"
        );
    }
);
source_test!(
    test_end_child_on_a_dead_child_reports_its_real_code,
    "tests/test_startup_resources.py::test_end_child_on_a_dead_child_reports_its_real_code",
    {
        let mut c = Command::new(support::fixture());
        c.arg("emit").arg("").arg("7");
        let mut tree = OwnedTree::spawn(&mut c).expect("browser contract");
        assert_eq!(
            tree.child_mut().wait().expect("browser contract").code(),
            Some(7)
        );
        tree.settle(Duration::from_secs(2))
            .expect("browser contract");
        assert_eq!(
            tree.child_mut()
                .try_wait()
                .expect("browser contract")
                .expect("browser contract")
                .code(),
            Some(7)
        );
    }
);
source_test!(
    test_a_bookkeeping_failure_after_ready_stops_the_browser_it_opened,
    "tests/test_startup_resources.py::test_a_bookkeeping_failure_after_ready_stops_the_browser_it_opened",
    {
        scheduler(true);
    }
);
source_test!(
    test_a_confirmed_start_still_records_normally,
    "tests/test_startup_resources.py::test_a_confirmed_start_still_records_normally",
    {
        scheduler(false);
    }
);
source_test!(
    test_the_relay_is_closed_when_the_driver_will_not_even_start,
    "tests/test_startup_resources.py::test_the_relay_is_closed_when_the_driver_will_not_even_start",
    {
        let d = fresh();
        let mut c = Command::new(support::fixture());
        c.arg("relay-failure").arg(d.path());
        let out =
            numera_core::ownership::capture::run(&mut c, support::limit(Duration::from_secs(10)))
                .expect("browser contract");
        assert_eq!(out.exit, 0, "{}", String::from_utf8_lossy(&out.stderr));
    }
);
source_test!(
    test_a_cleanup_that_fails_is_surfaced_not_swallowed,
    "tests/test_startup_resources.py::test_a_cleanup_that_fails_is_surfaced_not_swallowed",
    {
        let d = fresh();
        let p = profile();
        let result = support::native_open(d.path(), "playwright", "cleanup-fail", &p);
        let error = result
            .err()
            .expect("startup and cleanup failures must surface");
        assert!(
            error.message.contains("SDK") && error.message.contains("cleanup"),
            "both failures must be surfaced: {error}"
        );
        let driver = d.path().join("driver");
        let req = support::requests(&driver);
        assert!(req.iter().any(|r| r["method"] == "close"));
        support::wait(&driver.join("driver-stopped"));
        let log = support::logs(d.path());
        assert!(
            log.contains("cleanup") && log.contains("failed"),
            "cleanup failure was not surfaced"
        );
        assert!(
            matches!(claims::read(d.path(), &p.id), claims::Claim::Absent),
            "owner did not force-settle after vendor close failure"
        );
    }
);
