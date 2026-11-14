use super::*;
source_test!(
    a_recycled_pid_is_not_treated_as_the_original_process,
    "tests/test_jobs.py::test_a_recycled_pid_is_not_treated_as_the_original_process",
    {
        let me = std::process::id();
        let token = process_identity::started_at(me)
            .expect("own birth")
            .expect("token");
        assert!(token > 0);
        assert!(process_identity::alive(me, Some(token)).expect("alive"));
        assert!(!process_identity::alive(me, Some(token + 1)).expect("recycled identity"));
    }
);
source_test!(
    terminate_never_signals_a_pid_that_is_no_longer_ours,
    "tests/test_jobs.py::test_terminate_never_signals_a_pid_that_is_no_longer_ours",
    {
        struct Observe(Arc<Mutex<usize>>);
        impl numera_core::ownership::test_support::Backend for Observe {
            fn signal(&self, _target: i32, _force: bool) -> Option<io::Result<()>> {
                *self.0.lock().expect("signals") += 1;
                Some(Ok(()))
            }
        }
        let me = std::process::id();
        let token = process_identity::started_at(me)
            .expect("birth")
            .expect("token");
        let signalled = Arc::new(Mutex::new(0));
        numera_core::ownership::test_support::with_backend(
            Arc::new(Observe(signalled.clone())),
            || {
                assert!(
                    numera_core::ownership::recorded_tree::settle(
                        &json!({"pid":me,"pidToken":token+1}),
                        Duration::from_secs(1)
                    )
                    .expect("stale tree already empty")
                );
            },
        );
        assert_eq!(*signalled.lock().expect("signals"), 0);
    }
);
source_test!(
    a_dead_pid_is_dead_with_or_without_a_token,
    "tests/test_jobs.py::test_a_dead_pid_is_dead_with_or_without_a_token",
    {
        let temp = temp();
        let root = temp.path();
        let child = process::start(
            "jobs::a_dead_pid_is_dead_with_or_without_a_token",
            root,
            "exited",
            "dead-pid",
            "",
        );
        process::reached(root, "exited", "ready");
        let record = raw(root, "worker-exited-result.json");
        let pid =
            u32::try_from(record["pid"].as_u64().expect("real child pid")).expect("native pid");
        let token = record["token"].as_u64().expect("real child birth");
        assert!(process_identity::alive(pid, Some(token)).expect("live control"));
        process::release(root, "exited");
        process::finish(child);
        assert!(!process_identity::alive(pid, Some(token)).expect("dead pid with identity"));
        assert!(!process_identity::alive(pid, None).expect("dead pid without identity"));
        assert!(!process_identity::alive(0, None).expect("zero pid"));
    }
);
source_test!(
    a_job_record_stores_the_identity_beside_the_pid,
    "tests/test_jobs.py::test_a_job_record_stores_the_identity_beside_the_pid",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "p", 1);
        let jobs = numera_core::jobs::JobStore::new(root.to_owned());
        let argv = vec!["show".into(), p.id.clone(), "--json".into()];
        let rec = jobs
            .spawn_detached(numera_core::jobs::SpawnOptions {
                executable: Path::new(env!("CARGO_BIN_EXE_numera-engine")),
                argv: &argv,
                command: "show",
                profile: Some(&p.id),
                password: None,
            })
            .expect("actual detached job");
        let id = rec["id"].as_str().expect("job id");
        let record = jobs.record(id).expect("record").expect("job");
        assert!(record["pidToken"].as_u64().is_some_and(|v| v > 0));
        assert!(record["pid"].as_u64().is_some_and(|v| v > 0));
        wait_job(&jobs, id);
    }
);
source_test!(
    the_fallback_identity_is_stable_across_processes,
    "tests/test_jobs.py::test_the_fallback_identity_is_stable_across_processes",
    {
        let temp = temp();
        let root = temp.path();
        let test = "jobs::the_fallback_identity_is_stable_across_processes";
        let me = std::process::id().to_string();
        let mut seen = BTreeSet::new();
        for who in ["one", "two", "three"] {
            process::finish(process::start(test, root, who, "birth", &me));
            seen.insert(
                raw(root, &format!("worker-{who}-result.json"))["token"]
                    .as_u64()
                    .expect("kernel token"),
            );
        }
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen.first().copied(),
            process_identity::started_at(std::process::id()).expect("own birth")
        );
        struct Missing;
        impl numera_core::ownership::test_support::Backend for Missing {
            fn birth(&self, _pid: u32) -> Option<io::Result<Option<u64>>> {
                Some(Err(io::Error::other("kernel birth unavailable")))
            }
        }
        numera_core::ownership::test_support::with_backend(Arc::new(Missing), || {
            assert!(
                process_identity::started_at(std::process::id()).is_err(),
                "unverified identity fails closed"
            )
        });
    }
);
