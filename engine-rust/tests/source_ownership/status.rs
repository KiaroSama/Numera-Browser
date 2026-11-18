use super::*;

pub(super) fn cli_stop(
    root: &Path,
    id: &str,
) -> Result<numera_core::cli::profile_commands::Outcome, numera_core::storage::repo::RepoError> {
    let repo = numera_core::storage::repo::ProfileRepo::new(root.into(), None);
    let config = ConfigStore::new(root.into());
    let installed = Default::default();
    numera_core::cli::profile_commands::Profiles {
        repo: &repo,
        config: &config,
        installed: &installed,
        host: "source-host",
    }
    .stop(root, id, 1001.0)
}
struct StatusHttp {
    runtime: tokio::runtime::Runtime,
    router: axum::Router,
}
impl StatusHttp {
    fn new(root: &Path) -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let router = numera_core::http::profile_routes::router(root.into(), "", "source-host")?;
        Ok(Self { runtime, router })
    }
    fn request(
        &self,
        method: &str,
        path: &str,
    ) -> Result<(u16, Value), Box<dyn std::error::Error>> {
        use tower::ServiceExt;
        self.runtime.block_on(async {
            let request = axum::http::Request::builder()
                .method(method)
                .uri(path)
                .body(axum::body::Body::empty())?;
            let response = self.router.clone().oneshot(request).await?;
            let status = response.status().as_u16();
            let bytes = axum::body::to_bytes(response.into_body(), 1_048_576).await?;
            let body = serde_json::from_slice(&bytes)?;
            Ok((status, body))
        })
    }
}

source_test!(
    proved_dead_an_unobservable_claim_is_not_reported_free_b8422b10,
    "tests/test_proved_dead.py::test_an_unobservable_claim_is_not_reported_free[stat-PermissionError]",
    {
        use numera_core::vault::testing::with_observer;
        let t = temp("claim-io-uncertainty")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let claim = t.path().join("sessions/unobserved.json");
        write_json(
            &claim,
            &json!({"session":"preserved","what":"source session"}),
        )?;
        let before = fs::read(&claim)?;
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = count.clone();
        let target = claim.clone();
        with_observer(
            move |observed: &numera_core::vault::testing::Event| {
                let event = observed.operation;
                let path = observed.path.as_path();
                if event == "before-path-probe" && path == target {
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "controlled claim observation refusal",
                    ));
                }
                Ok(numera_core::vault::testing::Action::Continue)
            },
            || {
                assert!(matches!(
                    claims::read(t.path(), "unobserved"),
                    claims::Claim::Unreadable
                ));
                assert_eq!(
                    claims::state(&claims::read(t.path(), "unobserved"), "source-host"),
                    claims::State::Unknown
                );
                assert!(
                    claims::take(t.path(), "unobserved", &tree, "source-host", "browser").is_err()
                );
            },
        );
        assert!(count.load(std::sync::atomic::Ordering::SeqCst) >= 3);
        assert_eq!(fs::read(&claim)?, before);
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Record(_)
        ));
        fs::remove_file(&claim)?;
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Absent
        ));
        assert!(matches!(
            claims::read(t.path(), "../invalid"),
            claims::Claim::Absent
        ));
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_an_unobservable_claim_is_not_reported_free_1c015f35,
    "tests/test_proved_dead.py::test_an_unobservable_claim_is_not_reported_free[stat-OSError]",
    {
        use numera_core::vault::testing::with_observer;
        let t = temp("claim-io-uncertainty")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let claim = t.path().join("sessions/unobserved.json");
        write_json(
            &claim,
            &json!({"session":"preserved","what":"source session"}),
        )?;
        let before = fs::read(&claim)?;
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = count.clone();
        let target = claim.clone();
        with_observer(
            move |observed: &numera_core::vault::testing::Event| {
                let event = observed.operation;
                let path = observed.path.as_path();
                if event == "before-path-probe" && path == target {
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return Err(io::Error::other("controlled claim observation refusal"));
                }
                Ok(numera_core::vault::testing::Action::Continue)
            },
            || {
                assert!(matches!(
                    claims::read(t.path(), "unobserved"),
                    claims::Claim::Unreadable
                ));
                assert_eq!(
                    claims::state(&claims::read(t.path(), "unobserved"), "source-host"),
                    claims::State::Unknown
                );
                assert!(
                    claims::take(t.path(), "unobserved", &tree, "source-host", "browser").is_err()
                );
            },
        );
        assert!(count.load(std::sync::atomic::Ordering::SeqCst) >= 3);
        assert_eq!(fs::read(&claim)?, before);
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Record(_)
        ));
        fs::remove_file(&claim)?;
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Absent
        ));
        assert!(matches!(
            claims::read(t.path(), "../invalid"),
            claims::Claim::Absent
        ));
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_an_unobservable_claim_is_not_reported_free_1ee14cbe,
    "tests/test_proved_dead.py::test_an_unobservable_claim_is_not_reported_free[read-PermissionError]",
    {
        use numera_core::vault::testing::with_observer;
        let t = temp("claim-io-uncertainty")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let claim = t.path().join("sessions/unobserved.json");
        write_json(
            &claim,
            &json!({"session":"preserved","what":"source session"}),
        )?;
        let before = fs::read(&claim)?;
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = count.clone();
        let target = claim.clone();
        with_observer(
            move |observed: &numera_core::vault::testing::Event| {
                let event = observed.operation;
                let path = observed.path.as_path();
                if event == "before-claim-read" && path == target {
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "controlled claim observation refusal",
                    ));
                }
                Ok(numera_core::vault::testing::Action::Continue)
            },
            || {
                assert!(matches!(
                    claims::read(t.path(), "unobserved"),
                    claims::Claim::Unreadable
                ));
                assert_eq!(
                    claims::state(&claims::read(t.path(), "unobserved"), "source-host"),
                    claims::State::Unknown
                );
                assert!(
                    claims::take(t.path(), "unobserved", &tree, "source-host", "browser").is_err()
                );
            },
        );
        assert!(count.load(std::sync::atomic::Ordering::SeqCst) >= 3);
        assert_eq!(fs::read(&claim)?, before);
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Record(_)
        ));
        fs::remove_file(&claim)?;
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Absent
        ));
        assert!(matches!(
            claims::read(t.path(), "../invalid"),
            claims::Claim::Absent
        ));
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_an_unobservable_claim_is_not_reported_free_4a751b1c,
    "tests/test_proved_dead.py::test_an_unobservable_claim_is_not_reported_free[read-OSError]",
    {
        use numera_core::vault::testing::with_observer;
        let t = temp("claim-io-uncertainty")?;
        let mut tree = spawn_tree(t.path(), true)?;
        let claim = t.path().join("sessions/unobserved.json");
        write_json(
            &claim,
            &json!({"session":"preserved","what":"source session"}),
        )?;
        let before = fs::read(&claim)?;
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = count.clone();
        let target = claim.clone();
        with_observer(
            move |observed: &numera_core::vault::testing::Event| {
                let event = observed.operation;
                let path = observed.path.as_path();
                if event == "before-claim-read" && path == target {
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return Err(io::Error::other("controlled claim observation refusal"));
                }
                Ok(numera_core::vault::testing::Action::Continue)
            },
            || {
                assert!(matches!(
                    claims::read(t.path(), "unobserved"),
                    claims::Claim::Unreadable
                ));
                assert_eq!(
                    claims::state(&claims::read(t.path(), "unobserved"), "source-host"),
                    claims::State::Unknown
                );
                assert!(
                    claims::take(t.path(), "unobserved", &tree, "source-host", "browser").is_err()
                );
            },
        );
        assert!(count.load(std::sync::atomic::Ordering::SeqCst) >= 3);
        assert_eq!(fs::read(&claim)?, before);
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Record(_)
        ));
        fs::remove_file(&claim)?;
        assert!(matches!(
            claims::read(t.path(), "unobserved"),
            claims::Claim::Absent
        ));
        assert!(matches!(
            claims::read(t.path(), "../invalid"),
            claims::Claim::Absent
        ));
        tree.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    proved_dead_missing_directory_observation_is_not_a_missing_claim,
    "tests/test_proved_dead.py::test_missing_directory_observation_is_not_a_missing_claim",
    {
        use numera_core::vault::testing::with_observer;
        let t = temp("claim-directory-unknown")?;
        let claim = t.path().join("sessions/p.json");
        write_json(&claim, &json!({"session":"preserved"}))?;
        let before = fs::read(&claim)?;
        let target = claim.clone();
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = count.clone();
        with_observer(
            move |observed: &numera_core::vault::testing::Event| {
                let event = observed.operation;
                let path = observed.path.as_path();
                if event == "before-path-probe" && path == target {
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "controlled directory observation failure",
                    ));
                }
                Ok(numera_core::vault::testing::Action::Continue)
            },
            || {
                assert!(matches!(
                    claims::read(t.path(), "p"),
                    claims::Claim::Unreadable
                ))
            },
        );
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(fs::read(&claim)?, before);
        Ok(())
    }
);
source_test!(
    proved_dead_unreadable_ownership_cannot_report_a_profile_stopped_a37df942,
    "tests/test_proved_dead.py::test_unreadable_ownership_cannot_report_a_profile_stopped[lease-cli]",
    {
        let t = temp("uncertain-owner-stop")?;
        profile(t.path(), "p")?;
        let http = StatusHttp::new(t.path())?;
        let path = SessionLease::path_for(t.path(), "p")?;
        fs::create_dir_all(path.parent().ok_or("lease parent")?)?;
        fs::create_dir(&path)?;
        assert!(
            cli_stop(t.path(), "p").is_err(),
            "unreadable lease reported a successful stop"
        );
        fs::remove_dir(&path)?;
        assert_eq!(http.request("POST", "/api/profiles/p/stop")?.0, 200);
        Ok(())
    }
);
source_test!(
    proved_dead_unreadable_ownership_cannot_report_a_profile_stopped_b099916e,
    "tests/test_proved_dead.py::test_unreadable_ownership_cannot_report_a_profile_stopped[lease-http]",
    {
        let t = temp("uncertain-owner-stop")?;
        profile(t.path(), "p")?;
        let http = StatusHttp::new(t.path())?;
        let path = SessionLease::path_for(t.path(), "p")?;
        fs::create_dir_all(path.parent().ok_or("lease parent")?)?;
        fs::create_dir(&path)?;
        let reply = http.request("POST", "/api/profiles/p/stop")?;
        assert_eq!(reply.0, 409);
        assert!(
            reply.1["detail"]
                .as_str()
                .is_some_and(|message| message.contains("another process"))
        );
        let listing = http.request("GET", "/api/profiles")?;
        assert_eq!(listing.1[0]["status"], "running");
        fs::remove_dir(&path)?;
        assert_eq!(http.request("POST", "/api/profiles/p/stop")?.0, 200);
        Ok(())
    }
);
source_test!(
    proved_dead_unreadable_ownership_cannot_report_a_profile_stopped_87f60e3f,
    "tests/test_proved_dead.py::test_unreadable_ownership_cannot_report_a_profile_stopped[claim-cli]",
    {
        let t = temp("uncertain-owner-stop")?;
        profile(t.path(), "p")?;
        let http = StatusHttp::new(t.path())?;
        let path = t.path().join("sessions/p.json");
        fs::create_dir_all(&path)?;
        let outcome = cli_stop(t.path(), "p")?;
        assert_eq!(outcome.exit, 1);
        assert_eq!(outcome.payload["ok"], false);
        assert!(
            outcome.payload["stopped"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
        fs::remove_dir(&path)?;
        assert_eq!(http.request("POST", "/api/profiles/p/stop")?.0, 200);
        Ok(())
    }
);
source_test!(
    proved_dead_unreadable_ownership_cannot_report_a_profile_stopped_5b47eed1,
    "tests/test_proved_dead.py::test_unreadable_ownership_cannot_report_a_profile_stopped[claim-http]",
    {
        let t = temp("uncertain-owner-stop")?;
        profile(t.path(), "p")?;
        let http = StatusHttp::new(t.path())?;
        let path = t.path().join("sessions/p.json");
        fs::create_dir_all(&path)?;
        let reply = http.request("POST", "/api/profiles/p/stop")?;
        assert_eq!(reply.0, 409);
        let listing = http.request("GET", "/api/profiles")?;
        assert_eq!(listing.1[0]["status"], "running");
        fs::remove_dir(&path)?;
        assert_eq!(http.request("POST", "/api/profiles/p/stop")?.0, 200);
        Ok(())
    }
);
source_test!(
    process_registry_stop_refuses_to_claim_it_stopped_a_foreign_owner,
    "tests/test_process_registry.py::test_stop_refuses_to_claim_it_stopped_a_foreign_owner",
    {
        let t = temp("foreign-stop-refusal")?;
        profile(t.path(), "p")?;
        let http = StatusHttp::new(t.path())?;
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        let reply = http.request("POST", "/api/profiles/p/stop")?;
        assert_eq!(reply.0, 409);
        assert!(
            reply.1["detail"]
                .as_str()
                .is_some_and(|message| message.contains("cannot stop"))
        );
        let outcome = cli_stop(t.path(), "p")?;
        assert_eq!(outcome.exit, 1);
        assert_eq!(outcome.payload["ok"], false);
        drop(lease);
        assert_eq!(http.request("POST", "/api/profiles/p/stop")?.0, 200);
        Ok(())
    }
);
source_test!(
    proved_dead_stopping_a_profile_nobody_here_owns_says_so,
    "tests/test_proved_dead.py::test_stopping_a_profile_nobody_here_owns_says_so",
    {
        let t = temp("foreign-stop-refusal")?;
        profile(t.path(), "p")?;
        let http = StatusHttp::new(t.path())?;
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        let reply = http.request("POST", "/api/profiles/p/stop")?;
        assert_eq!(reply.0, 409);
        assert!(
            reply.1["detail"]
                .as_str()
                .is_some_and(|message| message.contains("cannot stop"))
        );
        let outcome = cli_stop(t.path(), "p")?;
        assert_eq!(outcome.exit, 1);
        assert_eq!(outcome.payload["ok"], false);
        drop(lease);
        assert_eq!(http.request("POST", "/api/profiles/p/stop")?.0, 200);
        Ok(())
    }
);
source_test!(
    process_registry_stopping_a_profile_nobody_is_running_is_still_fine,
    "tests/test_process_registry.py::test_stopping_a_profile_nobody_is_running_is_still_fine",
    {
        let t = temp("idle-stop-control")?;
        profile(t.path(), "p")?;
        let http = StatusHttp::new(t.path())?;
        let reply = http.request("POST", "/api/profiles/p/stop")?;
        assert_eq!(reply.0, 200);
        assert_eq!(reply.1["status"], "stopped");
        let outcome = cli_stop(t.path(), "p")?;
        assert_eq!(outcome.exit, 0);
        assert_eq!(outcome.payload["ok"], true);
        Ok(())
    }
);
source_test!(
    process_registry_stopping_a_profile_that_does_not_exist_is_still_a_404,
    "tests/test_process_registry.py::test_stopping_a_profile_that_does_not_exist_is_still_a_404",
    {
        let t = temp("missing-stop-control")?;
        let http = StatusHttp::new(t.path())?;
        let reply = http.request("POST", "/api/profiles/missing/stop")?;
        assert_eq!(reply.0, 404);
        assert_eq!(reply.1["detail"], "profile not found");
        assert_ne!(cli_stop(t.path(), "missing")?.exit, 0);
        Ok(())
    }
);
