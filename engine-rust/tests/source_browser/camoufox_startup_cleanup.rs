use super::*;
use numera_core::ownership::{
    claims,
    session::Session,
    test_support::{self, Backend},
    tree,
};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
fn suppressed(suppress: bool) {
    let d = fresh();
    let p = profile();
    let error = support::native_open(
        d.path(),
        "playwright",
        if suppress {
            "init-suppress"
        } else {
            "init-fail"
        },
        &p,
    )
    .err()
    .expect("vendor suppression must not yield failed startup");
    assert_eq!(error.kind, "failed");
    assert!(error.message.contains("SDK"));
    let owned: Value = serde_json::from_slice(
        &std::fs::read(d.path().join("driver/close-owned.json")).expect("browser contract"),
    )
    .expect("browser contract");
    assert_eq!(owned["claim"], true, "claim released before context exit");
    assert_eq!(
        owned["lease_busy"], true,
        "lease reusable before context exit"
    );
    assert!(d.path().join("driver/context-closed").exists());
    assert!(d.path().join("driver/driver-stopped").exists());
    assert!(matches!(
        claims::read(d.path(), &p.id),
        claims::Claim::Absent
    ));
    drop(Session::admit(d.path(), &p.id, "source-browser-host").expect("browser contract"));
}
source_test!(
    test_ownership_refusal_exits_entered_backend_before_releasing_false,
    "tests/test_camoufox_startup_cleanup.py::test_ownership_refusal_exits_entered_backend_before_releasing[False]",
    {
        suppressed(false);
    }
);
source_test!(
    test_ownership_refusal_exits_entered_backend_before_releasing_true,
    "tests/test_camoufox_startup_cleanup.py::test_ownership_refusal_exits_entered_backend_before_releasing[True]",
    {
        suppressed(true);
    }
);
source_test!(
    test_ownership_refusal_and_failed_exit_preserve_both_failures_and_claim,
    "tests/test_camoufox_startup_cleanup.py::test_ownership_refusal_and_failed_exit_preserve_both_failures_and_claim",
    {
        struct Refuse(Arc<AtomicBool>);
        impl Backend for Refuse {
            fn signal(&self, _target: i32, _force: bool) -> Option<io::Result<()>> {
                self.0.store(true, Ordering::SeqCst);
                Some(Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "planted backend cleanup refusal",
                )))
            }
        }
        let d = fresh();
        let p = profile();
        let seen = Arc::new(AtomicBool::new(false));
        let error = test_support::with_backend(Arc::new(Refuse(Arc::clone(&seen))), || {
            let error = support::native_open(d.path(), "playwright", "cleanup-hang", &p)
                .err()
                .expect("both failures must surface");
            assert_eq!(error.kind, "unsettled");
            assert!(
                error.message.contains("SDK"),
                "original SDK startup failure lost: {error}"
            );
            assert!(
                error.message.contains("cleanup")
                    && error.message.contains("planted backend cleanup refusal"),
                "cleanup failure lost: {error}"
            );
            assert!(
                Session::admit(d.path(), &p.id, "source-browser-host").is_err(),
                "failed cleanup released profile ownership"
            );
            error
        });
        assert!(seen.load(Ordering::SeqCst));
        let record = match claims::read(d.path(), &p.id) {
            claims::Claim::Record(record) => record,
            _ => panic!("failed cleanup erased claim: {error}"),
        };
        assert!(
            !tree::record_empty(&record).expect("browser contract"),
            "fault fixture was not actually live"
        );
        assert!(numera_core::ownership::capture::retry_retained(Duration::from_secs(3)).is_empty());
        assert!(tree::record_empty(&record).expect("browser contract"));
        assert!(claims::clear(d.path(), &p.id, "source-browser-host").expect("browser contract"));
        drop(Session::admit(d.path(), &p.id, "source-browser-host").expect("browser contract"));
    }
);
