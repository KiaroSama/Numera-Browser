use super::*;

source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_9b2d5b46,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[nan]",
    {
        let timeout = configured_timeout(Some("nan"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_d5b592c0,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[NaN]",
    {
        let timeout = configured_timeout(Some("NaN"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_e99270c4,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[inf]",
    {
        let timeout = configured_timeout(Some("inf"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_a079ce0b,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[-inf]",
    {
        let timeout = configured_timeout(Some("-inf"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_1bad6b8c,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[-1]",
    {
        let timeout = configured_timeout(Some("-1"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_ffe616e2,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[-0.1]",
    {
        let timeout = configured_timeout(Some("-0.1"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_56d7ed46,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[1e309]",
    {
        let timeout = configured_timeout(Some("1e309"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_invalid_timeout_cannot_select_an_infinite_or_invalid_wait_f60559e0,
    "tests/test_data_lock_ownership.py::test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait[1e100]",
    {
        let timeout = configured_timeout(Some("1e100"));
        assert_eq!(timeout, Duration::from_secs(30));
        assert!(timeout.as_secs_f64().is_finite());
        Ok(())
    }
);
source_test!(
    data_lock_ownership_valid_timeout_and_existing_invalid_text_fallback_22841ea3,
    "tests/test_data_lock_ownership.py::test_valid_timeout_and_existing_invalid_text_fallback[0-0]",
    {
        assert_eq!(configured_timeout(Some("0")), Duration::from_secs_f64(0.0));
        Ok(())
    }
);
source_test!(
    data_lock_ownership_valid_timeout_and_existing_invalid_text_fallback_3c223f4c,
    "tests/test_data_lock_ownership.py::test_valid_timeout_and_existing_invalid_text_fallback[.02-0.02]",
    {
        assert_eq!(
            configured_timeout(Some(".02")),
            Duration::from_secs_f64(0.02)
        );
        Ok(())
    }
);
source_test!(
    data_lock_ownership_valid_timeout_and_existing_invalid_text_fallback_2b51daa5,
    "tests/test_data_lock_ownership.py::test_valid_timeout_and_existing_invalid_text_fallback[3-3]",
    {
        assert_eq!(configured_timeout(Some("3")), Duration::from_secs_f64(3.0));
        Ok(())
    }
);
source_test!(
    data_lock_ownership_valid_timeout_and_existing_invalid_text_fallback_4cbaf3fb,
    "tests/test_data_lock_ownership.py::test_valid_timeout_and_existing_invalid_text_fallback[-30]",
    {
        assert_eq!(configured_timeout(Some("")), Duration::from_secs_f64(30.0));
        Ok(())
    }
);
source_test!(
    data_lock_ownership_valid_timeout_and_existing_invalid_text_fallback_60d579a0,
    "tests/test_data_lock_ownership.py::test_valid_timeout_and_existing_invalid_text_fallback[invalid-30]",
    {
        assert_eq!(
            configured_timeout(Some("invalid")),
            Duration::from_secs_f64(30.0)
        );
        Ok(())
    }
);
source_test!(
    data_lock_ownership_transaction_predicate_is_about_the_calling_thread,
    "tests/test_data_lock_ownership.py::test_transaction_predicate_is_about_the_calling_thread",
    {
        let t = temp("lock-thread-owner")?;
        let lock = Arc::new(DataLock::transaction(t.path())?);
        assert!(!lock.owned_by_current_thread()?);
        let guard = lock.acquire(Duration::from_secs(1))?;
        assert!(lock.owned_by_current_thread()?);
        {
            let _nested = lock.acquire(Duration::ZERO)?;
            assert!(lock.owned_by_current_thread()?);
        }
        assert!(lock.owned_by_current_thread()?);
        let foreign = Arc::clone(&lock);
        assert!(
            !thread::spawn(move || foreign.owned_by_current_thread())
                .join()
                .expect("foreign")?
        );
        drop(guard);
        assert!(!lock.owned_by_current_thread()?);
        Ok(())
    }
);
source_test!(
    data_lock_ownership_another_threads_transaction_does_not_authorize_a_config_replacement,
    "tests/test_data_lock_ownership.py::test_another_threads_transaction_does_not_authorize_a_config_replacement",
    {
        let t = temp("foreign-config-owner")?;
        let store = Arc::new(ConfigStore::new(t.path().into()));
        store.set("keep", "original".into())?;
        let before = fs::read(t.path().join("config.json"))?;
        let lock = DataLock::transaction(t.path())?;
        let _guard = lock.acquire(Duration::from_secs(1))?;
        let store = Arc::clone(&store);
        let error = thread::spawn(move || {
            store.replace_locked(
                json!({"foreign":"replacement"})
                    .as_object()
                    .expect("map")
                    .clone(),
            )
        })
        .join()
        .expect("foreign writer")
        .expect_err("foreign transaction refused");
        assert_eq!(error.source_kind(), "RuntimeError");
        assert_eq!(fs::read(t.path().join("config.json"))?, before);
        Ok(())
    }
);
source_test!(
    data_lock_ownership_thread_contention_obeys_the_configured_deadline_4813494d,
    "tests/test_data_lock_ownership.py::test_thread_contention_obeys_the_configured_deadline[root]",
    {
        let t = temp("thread-lock-deadline")?;
        let lock = Arc::new(DataLock::transaction(t.path())?);
        let _guard = lock.acquire(Duration::from_secs(1))?;
        let other = Arc::clone(&lock);
        let start = Instant::now();
        let result = thread::spawn(move || {
            other
                .acquire(configured_timeout(Some(".05")))
                .map(|_| ())
                .map_err(|error| error.kind())
        })
        .join()
        .expect("contender");
        assert_eq!(result, Err(io::ErrorKind::TimedOut));
        assert!(start.elapsed() < Duration::from_millis(400));
        Ok(())
    }
);
source_test!(
    data_lock_ownership_thread_contention_obeys_the_configured_deadline_3b9c358f,
    "tests/test_data_lock_ownership.py::test_thread_contention_obeys_the_configured_deadline[file]",
    {
        let t = temp("thread-lock-deadline")?;
        let lock = Arc::new(DataLock::for_file(&t.path().join("x.json"))?);
        let _guard = lock.acquire(Duration::from_secs(1))?;
        let other = Arc::clone(&lock);
        let start = Instant::now();
        let result = thread::spawn(move || {
            other
                .acquire(configured_timeout(Some(".05")))
                .map(|_| ())
                .map_err(|error| error.kind())
        })
        .join()
        .expect("contender");
        assert_eq!(result, Err(io::ErrorKind::TimedOut));
        assert!(start.elapsed() < Duration::from_millis(400));
        Ok(())
    }
);
source_test!(
    data_lock_ownership_reentrant_config_replacement_keeps_root_file_lock_order,
    "tests/test_data_lock_ownership.py::test_reentrant_config_replacement_keeps_root_file_lock_order",
    {
        let t = temp("reentrant-config")?;
        let lock = DataLock::transaction(t.path())?;
        let store = ConfigStore::new(t.path().into());
        let _outer = lock.acquire(Duration::from_secs(1))?;
        store.replace_locked(json!({"a":1}).as_object().expect("map").clone())?;
        {
            let _inner = lock.acquire(Duration::ZERO)?;
            store.replace_locked(json!({"a":1,"b":2}).as_object().expect("map").clone())?;
        }
        assert_eq!(
            store.read()?,
            json!({"a":1,"b":2}).as_object().expect("map").clone()
        );
        Ok(())
    }
);
source_test!(
    data_lock_ownership_concurrent_real_config_updates_preserve_all_keys,
    "tests/test_data_lock_ownership.py::test_concurrent_real_config_updates_preserve_all_keys",
    {
        let t = temp("concurrent-config")?;
        let barrier = Arc::new(Barrier::new(5));
        let mut threads = Vec::new();
        for index in 0..4 {
            let barrier = Arc::clone(&barrier);
            let root = t.path().to_owned();
            threads.push(thread::spawn(move || -> Result<(), String> {
                let store = ConfigStore::new(root);
                barrier.wait();
                for n in 0..8 {
                    store
                        .set(&format!("key-{index}-{n}"), n.into())
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            }));
        }
        barrier.wait();
        for child in threads {
            child.join().expect("config writer").expect("config update");
        }
        assert_eq!(ConfigStore::new(t.path().into()).read()?.len(), 32);
        Ok(())
    }
);
source_test!(
    data_lock_ownership_process_contention_times_out_then_succeeds_without_deleting_lock_file,
    "tests/test_data_lock_ownership.py::test_process_contention_times_out_then_succeeds_without_deleting_lock_file",
    {
        let t = temp("process-lock-retry")?;
        let lock = DataLock::transaction(t.path())?;
        fs::create_dir_all(lock.path().parent().ok_or("lock parent absent")?)?;
        let sentinel = b"source lock identity must survive contention\n";
        fs::write(lock.path(), sentinel)?;
        let before = fs::read(lock.path())?;
        let physical_identity = || -> io::Result<(u64, u64)> {
            // Attribute-only kernel queries also work while Windows denies
            // reads of the held byte range. Compare actual file identity,
            // matching the source's st_ino contract rather than its spelling.
            let stat = numera_core::ownership::file_stat::path(lock.path())?;
            assert!(stat.regular && !stat.reparse);
            Ok((stat.device, stat.inode))
        };
        let identity_before = physical_identity()?;
        let guard = lock.acquire(Duration::from_secs(1))?;
        assert_eq!(physical_identity()?, identity_before);
        let mut child = command("lock")?;
        child.arg(lock.path());
        let output = capture::run(&mut child, limit(Duration::from_secs(5)))?;
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, b"timeout\n");
        drop(guard);
        let output = capture::run(&mut child, limit(Duration::from_secs(5)))?;
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, b"entered\n");
        assert_eq!(physical_identity()?, identity_before);
        assert_eq!(fs::read(lock.path())?, before);
        assert_eq!(before, sentinel);
        Ok(())
    }
);
source_test!(
    data_lock_ownership_timeout_message_never_recommends_removing_the_lock_file,
    "tests/test_data_lock_ownership.py::test_timeout_message_never_recommends_removing_the_lock_file",
    {
        let t = temp("lock-safe-diagnostic")?;
        let lock = Arc::new(DataLock::transaction(t.path())?);
        let _guard = lock.acquire(Duration::from_secs(1))?;
        let other = Arc::clone(&lock);
        let message = thread::spawn(move || {
            other
                .acquire(Duration::ZERO)
                .err()
                .expect("timeout")
                .to_string()
        })
        .join()
        .expect("contender");
        assert!(!message.to_lowercase().contains("delete"));
        assert!(message.contains("Keep the lock file in place"));
        Ok(())
    }
);
source_test!(
    cross_process_locking_a_second_process_waits_for_the_file_lock,
    "tests/test_cross_process_locking.py::test_a_second_process_waits_for_the_file_lock",
    {
        let t = temp("cross-process-file-lock")?;
        let target = t.path().join("guarded.json");
        write_json(&target, &json!({"held":true}))?;
        let lock = DataLock::for_file(&target)?;
        let _guard = lock.acquire(Duration::from_secs(1))?;
        let mut child = command("lock")?;
        child.arg(lock.path());
        let out = capture::run(&mut child, limit(Duration::from_secs(5)))?;
        assert_eq!(out.exit, 0);
        assert_eq!(out.stdout, b"timeout\n");
        Ok(())
    }
);
source_test!(
    cross_process_locking_the_lock_is_released_when_the_holder_exits,
    "tests/test_cross_process_locking.py::test_the_lock_is_released_when_the_holder_exits",
    {
        let t = temp("kernel-owner-release")?;
        let ready = t.path().join("held");
        let release = t.path().join("release");
        let lock = DataLock::for_file(&t.path().join("guarded.json"))?;
        let path = lock.path().to_owned();
        let mut child = command("hold-lock")?;
        child.arg(&path).arg(&ready).arg(&release);
        child.stdout(Stdio::null()).stderr(Stdio::null());
        let mut owner = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let started = Instant::now();
        assert_eq!(
            DataLock::new(path.clone())?
                .acquire(Duration::from_millis(30))
                .err()
                .ok_or("held OS lock acquired")?
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(started.elapsed() < Duration::from_millis(400));
        owner.settle(Duration::from_secs(2))?;
        let next = DataLock::new(path.clone())?;
        let _guard = next.acquire(Duration::from_secs(1))?;
        assert!(path.is_file(), "crash release deleted stable lock identity");
        Ok(())
    }
);
source_test!(
    session_lease_a_killed_holder_releases_the_lease,
    "tests/test_session_lease.py::test_a_killed_holder_releases_the_lease",
    {
        let t = temp("kernel-owner-release")?;
        let ready = t.path().join("held");
        let release = t.path().join("release");
        let path = SessionLease::path_for(t.path(), "p")?;
        let mut child = command("hold-lease")?;
        child.arg(t.path()).arg("p").arg(&ready).arg(&release);
        child.stdout(Stdio::null()).stderr(Stdio::null());
        let mut owner = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let blocked = SessionLease::acquire(&path)
            .err()
            .ok_or("second owner acquired held lease")?;
        assert_eq!(blocked.kind(), io::ErrorKind::WouldBlock);
        owner.settle(Duration::from_secs(2))?;
        let _next = SessionLease::acquire(&path)?;
        assert!(path.is_file(), "crash release deleted stable lock identity");
        Ok(())
    }
);
source_test!(
    session_lease_a_second_process_cannot_take_a_held_lease,
    "tests/test_session_lease.py::test_a_second_process_cannot_take_a_held_lease",
    {
        let t = temp("kernel-owner-release")?;
        let ready = t.path().join("held");
        let release = t.path().join("release");
        let path = SessionLease::path_for(t.path(), "p")?;
        let mut child = command("hold-lease")?;
        child.arg(t.path()).arg("p").arg(&ready).arg(&release);
        child.stdout(Stdio::null()).stderr(Stdio::null());
        let mut owner = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let blocked = SessionLease::acquire(&path)
            .err()
            .ok_or("second owner acquired held lease")?;
        assert_eq!(blocked.kind(), io::ErrorKind::WouldBlock);
        fs::write(&release, b"go")?;
        owner.child_mut().wait()?;
        owner.settle(Duration::from_secs(2))?;
        let _next = SessionLease::acquire(&path)?;
        assert!(path.is_file(), "crash release deleted stable lock identity");
        Ok(())
    }
);
source_test!(
    session_lease_two_processes_racing_one_profile_produce_one_winner,
    "tests/test_session_lease.py::test_two_processes_racing_one_profile_produce_one_winner",
    {
        let t = temp("kernel-owner-release")?;
        let ready = t.path().join("held");
        let release = t.path().join("release");
        let path = SessionLease::path_for(t.path(), "p")?;
        let mut child = command("hold-lease")?;
        child.arg(t.path()).arg("p").arg(&ready).arg(&release);
        child.stdout(Stdio::null()).stderr(Stdio::null());
        let mut owner = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        let blocked = SessionLease::acquire(&path)
            .err()
            .ok_or("second owner acquired held lease")?;
        assert_eq!(blocked.kind(), io::ErrorKind::WouldBlock);
        fs::write(&release, b"go")?;
        owner.child_mut().wait()?;
        owner.settle(Duration::from_secs(2))?;
        let _next = SessionLease::acquire(&path)?;
        assert!(path.is_file(), "crash release deleted stable lock identity");
        Ok(())
    }
);
source_test!(
    data_lock_ownership_acquisition_error_releases_the_thread_lock,
    "tests/test_data_lock_ownership.py::test_acquisition_error_releases_the_thread_lock",
    {
        let t = temp("lock-acquisition-error")?;
        let path = t.path().join("obstruction");
        fs::create_dir(&path)?;
        let lock = Arc::new(DataLock::new(path.clone())?);
        assert!(lock.acquire(Duration::from_millis(50)).is_err());
        fs::remove_dir(&path)?;
        let other = Arc::clone(&lock);
        thread::spawn(move || other.acquire(Duration::from_secs(1)).map(|_| ()))
            .join()
            .expect("retry thread")?;
        Ok(())
    }
);
source_test!(
    session_lease_delete_refuses_while_the_session_is_open,
    "tests/test_session_lease.py::test_delete_refuses_while_the_session_is_open",
    {
        let t = temp("held-lease-delete")?;
        profile(t.path(), "p")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        let data = repo.user_data_path("p")?;
        fs::write(data.join("Cookies"), b"live")?;
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        assert!(repo.delete("p", "source-host", 1001.0).is_err());
        assert!(repo.get("p")?.is_some());
        assert_eq!(fs::read(data.join("Cookies"))?, b"live");
        drop(lease);
        assert!(repo.delete("p", "source-host", 1002.0)?);
        Ok(())
    }
);
source_test!(
    session_lease_backup_and_restore_refuse_while_the_session_is_open,
    "tests/test_session_lease.py::test_backup_and_restore_refuse_while_the_session_is_open",
    {
        let t = temp("held-lease-archive")?;
        profile(t.path(), "p")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        let data = repo.user_data_path("p")?;
        fs::write(data.join("Cookies"), b"live")?;
        let archive = t.path().join("before.numera");
        repo.backup("p", &archive, "source-host")?
            .ok_or("backup absent")?;
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?)?;
        let during = t.path().join("during.numera");
        assert!(repo.backup("p", &during, "source-host").is_err());
        assert!(
            repo.restore(&archive, false, "source-host", 1001.0)
                .is_err()
        );
        assert!(!during.exists());
        drop(lease);
        assert_eq!(
            repo.restore(&archive, false, "source-host", 1002.0)?.id,
            "p"
        );
        Ok(())
    }
);
source_test!(
    session_lease_an_unavailable_lease_observation_is_not_reported_free_95c5007a,
    "tests/test_session_lease.py::test_an_unavailable_lease_observation_is_not_reported_free[mkdir]",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("unobservable-lease")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let obstruction = t.path().join(".locks");
        fs::write(&obstruction, b"not a directory")?;
        assert!(
            registry.running("unobserved")?,
            "unknown lease was reported free"
        );
        fs::remove_file(&obstruction)?;
        assert!(
            !registry.running("unobserved")?,
            "repaired lease remained busy"
        );
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "unobserved")?)?;
        assert!(registry.running("unobserved")?);
        drop(lease);
        assert!(!registry.running("unobserved")?);
        Ok(())
    }
);
source_test!(
    session_lease_an_unavailable_lease_observation_is_not_reported_free_2348f998,
    "tests/test_session_lease.py::test_an_unavailable_lease_observation_is_not_reported_free[open]",
    {
        use numera_core::ownership::registry::Registry;
        let t = temp("unobservable-lease")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        fs::create_dir_all(t.path().join(".locks"))?;
        let obstruction = SessionLease::path_for(t.path(), "unobserved")?;
        fs::create_dir(&obstruction)?;
        assert!(
            registry.running("unobserved")?,
            "unknown lease was reported free"
        );
        fs::remove_dir(&obstruction)?;
        assert!(
            !registry.running("unobserved")?,
            "repaired lease remained busy"
        );
        let lease = SessionLease::acquire(&SessionLease::path_for(t.path(), "unobserved")?)?;
        assert!(registry.running("unobserved")?);
        drop(lease);
        assert!(!registry.running("unobserved")?);
        Ok(())
    }
);
