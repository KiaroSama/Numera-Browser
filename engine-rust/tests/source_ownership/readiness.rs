use super::*;

source_test!(
    startup_handshake_a_child_that_reports_ready_is_waited_for,
    "tests/test_startup_handshake.py::test_a_child_that_reports_ready_is_waited_for",
    {
        let t = temp("real-readiness-channel")?;
        let path = t.path().join("ready.json");
        let mut child = command("ready")?;
        child
            .arg(&path)
            .arg("ok")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        let start = Instant::now();
        let result = ready::await_ready(&path, Duration::from_millis(200), || {
            tree.child_mut()
                .try_wait()
                .map(|status| status.map(|status| status.code().unwrap_or(-1)))
        });
        let elapsed = start.elapsed();
        tree.settle(Duration::from_secs(2))?;
        assert!(result.is_ok());
        assert!(path.is_file());
        assert!(elapsed < Duration::from_secs(1));
        Ok(())
    }
);
source_test!(
    startup_handshake_a_refusal_arrives_with_the_reason_that_caused_it,
    "tests/test_startup_handshake.py::test_a_refusal_arrives_with_the_reason_that_caused_it",
    {
        let t = temp("real-readiness-channel")?;
        let path = t.path().join("ready.json");
        let mut child = command("ready")?;
        child
            .arg(&path)
            .arg("locked")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        let start = Instant::now();
        let result = ready::await_ready(&path, Duration::from_millis(200), || {
            tree.child_mut()
                .try_wait()
                .map(|status| status.map(|status| status.code().unwrap_or(-1)))
        });
        let elapsed = start.elapsed();
        tree.settle(Duration::from_secs(2))?;
        let error = result.expect_err("refusal accepted");
        assert_eq!(error.kind, "locked");
        assert!(error.message.contains("vault"));
        assert!(elapsed < Duration::from_secs(1));
        Ok(())
    }
);
source_test!(
    startup_handshake_a_child_that_dies_without_a_word_is_not_ready,
    "tests/test_startup_handshake.py::test_a_child_that_dies_without_a_word_is_not_ready",
    {
        let t = temp("real-readiness-channel")?;
        let path = t.path().join("ready.json");
        let mut child = command("ready")?;
        child
            .arg(&path)
            .arg("silent")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        let start = Instant::now();
        let result = ready::await_ready(&path, Duration::from_millis(200), || {
            tree.child_mut()
                .try_wait()
                .map(|status| status.map(|status| status.code().unwrap_or(-1)))
        });
        let elapsed = start.elapsed();
        tree.settle(Duration::from_secs(2))?;
        let error = result.expect_err("silent exit accepted");
        assert!(error.message.contains("3"));
        assert!(!path.exists());
        assert!(elapsed < Duration::from_secs(1));
        Ok(())
    }
);
source_test!(
    startup_handshake_the_wait_is_bounded,
    "tests/test_startup_handshake.py::test_the_wait_is_bounded",
    {
        let t = temp("real-readiness-channel")?;
        let path = t.path().join("ready.json");
        let mut child = command("ready")?;
        child
            .arg(&path)
            .arg("wait")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut tree = OwnedTree::spawn(&mut child)?;
        let start = Instant::now();
        let result = ready::await_ready(&path, Duration::from_millis(200), || {
            tree.child_mut()
                .try_wait()
                .map(|status| status.map(|status| status.code().unwrap_or(-1)))
        });
        let elapsed = start.elapsed();
        tree.settle(Duration::from_secs(2))?;
        let error = result.expect_err("wedged startup accepted");
        assert_eq!(error.kind, "timeout");
        assert!(elapsed < Duration::from_secs(1));
        assert!(elapsed < Duration::from_secs(1));
        Ok(())
    }
);
source_test!(
    browser_readiness_owning_the_lease_is_not_readiness,
    "tests/test_browser_readiness.py::test_owning_the_lease_is_not_readiness",
    {
        let t = temp("lease-not-readiness")?;
        profile(t.path(), "p")?;
        let path = t.path().join("ready.json");
        let (mut session, current) = numera_core::browser::admission::current(
            t.path(),
            "p",
            "p",
            "source-host",
            None,
            1001.0,
        )?;
        assert_eq!(current.id, "p");
        assert!(!path.exists());
        assert!(SessionLease::acquire(&SessionLease::path_for(t.path(), "p")?).is_err());
        session.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    startup_handshake_a_delete_that_lands_before_the_lease_cannot_launch,
    "tests/test_startup_handshake.py::test_a_delete_that_lands_before_the_lease_cannot_launch",
    {
        let t = temp("delete-before-admission")?;
        let stale = profile(t.path(), "p")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        assert!(repo.delete(&stale.id, "source-host", 1001.0)?);
        let result = numera_core::browser::admission::current(
            t.path(),
            &stale.id,
            "p",
            "source-host",
            None,
            1002.0,
        );
        assert_eq!(result.err().ok_or("deleted profile admitted")?.kind, "gone");
        assert!(!t.path().join("user-data/p").exists());
        Ok(())
    }
);
source_test!(
    startup_handshake_the_gate_yields_the_profile_as_it_is_now,
    "tests/test_startup_handshake.py::test_the_gate_yields_the_profile_as_it_is_now",
    {
        let t = temp("current-admission-profile")?;
        let stale = profile(t.path(), "p")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        repo.edit(
            "p",
            |profile| {
                profile.name = "after".into();
                Ok(None)
            },
            1001.0,
        )?;
        let (mut session, current) = numera_core::browser::admission::current(
            t.path(),
            &stale.id,
            "p",
            "source-host",
            None,
            1002.0,
        )?;
        assert_eq!(stale.name, "p");
        assert_eq!(current.name, "after");
        session.finish(Duration::from_secs(1))?;
        Ok(())
    }
);
source_test!(
    startup_handshake_materialising_a_profile_that_is_gone_says_so,
    "tests/test_startup_handshake.py::test_materialising_a_profile_that_is_gone_says_so",
    {
        let t = temp("materialize-gone")?;
        let repo = numera_core::storage::repo::ProfileRepo::new(t.path().into(), None);
        assert!(repo.materialize("gone", 1000.0)?.is_none());
        assert!(!t.path().join("profiles/gone.json").exists());
        Ok(())
    }
);
source_test!(
    startup_handshake_a_foreign_lease_refuses_the_launch,
    "tests/test_startup_handshake.py::test_a_foreign_lease_refuses_the_launch",
    {
        let t = temp("foreign-admission-lease")?;
        profile(t.path(), "p")?;
        let path = SessionLease::path_for(t.path(), "p")?;
        let _held = SessionLease::acquire(&path)?;
        let result = numera_core::browser::admission::current(
            t.path(),
            "p",
            "p",
            "source-host",
            None,
            1001.0,
        );
        assert_eq!(result.err().ok_or("foreign lease admitted")?.kind, "busy");
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    process_registry_an_ordinary_launch_closing_is_not_a_warm_up_result,
    "tests/test_process_registry.py::test_an_ordinary_launch_closing_is_not_a_warm_up_result",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("registry-kind-finalization")?;
        profile(t.path(), "p")?;
        write_json(
            &t.path().join("schedules.json"),
            &json!([{"id":"one","profileId":"p","lastOutcome":null},{"id":"two","profileId":"p","lastOutcome":null}]),
        )?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let release = t.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .arg("0")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(registry.spawn(
            "p",
            &mut child,
            &ready,
            Kind::Launch,
            None,
            Duration::from_secs(2)
        )?);
        assert!(registry.owns("p")?);
        fs::write(&release, b"go")?;
        let end = Instant::now() + Duration::from_secs(3);
        while registry.running("p")? {
            assert!(Instant::now() < end, "registry failed to reap");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!registry.owns("p")?);
        let rows: Value = serde_json::from_slice(&fs::read(t.path().join("schedules.json"))?)?;
        assert!(rows[1]["lastOutcome"].is_null());
        assert!(rows[0]["lastOutcome"].is_null());
        assert!(rows[0]["lastRun"].is_null());
        Ok(())
    }
);
source_test!(
    process_registry_a_manual_warm_up_is_not_attributed_to_a_schedule,
    "tests/test_process_registry.py::test_a_manual_warm_up_is_not_attributed_to_a_schedule",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("registry-kind-finalization")?;
        profile(t.path(), "p")?;
        write_json(
            &t.path().join("schedules.json"),
            &json!([{"id":"one","profileId":"p","lastOutcome":null},{"id":"two","profileId":"p","lastOutcome":null}]),
        )?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let release = t.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .arg("1")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(registry.spawn(
            "p",
            &mut child,
            &ready,
            Kind::ManualWarmup,
            None,
            Duration::from_secs(2)
        )?);
        assert!(registry.owns("p")?);
        fs::write(&release, b"go")?;
        let end = Instant::now() + Duration::from_secs(3);
        while registry.running("p")? {
            assert!(Instant::now() < end, "registry failed to reap");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!registry.owns("p")?);
        let rows: Value = serde_json::from_slice(&fs::read(t.path().join("schedules.json"))?)?;
        assert!(rows[1]["lastOutcome"].is_null());
        assert!(rows[0]["lastOutcome"].is_null());
        assert!(rows[0]["lastRun"].is_null());
        Ok(())
    }
);
source_test!(
    process_registry_one_scheduled_child_updates_one_occurrence,
    "tests/test_process_registry.py::test_one_scheduled_child_updates_one_occurrence",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("registry-kind-finalization")?;
        profile(t.path(), "p")?;
        write_json(
            &t.path().join("schedules.json"),
            &json!([{"id":"one","profileId":"p","lastOutcome":null},{"id":"two","profileId":"p","lastOutcome":null}]),
        )?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let release = t.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .arg("3")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(registry.spawn(
            "p",
            &mut child,
            &ready,
            Kind::ScheduledWarmup,
            Some("one".into()),
            Duration::from_secs(2)
        )?);
        assert!(registry.owns("p")?);
        fs::write(&release, b"go")?;
        let end = Instant::now() + Duration::from_secs(3);
        while registry.running("p")? {
            assert!(Instant::now() < end, "registry failed to reap");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!registry.owns("p")?);
        let rows: Value = serde_json::from_slice(&fs::read(t.path().join("schedules.json"))?)?;
        assert!(rows[1]["lastOutcome"].is_null());
        assert_eq!(rows[0]["lastOutcome"], "failed");
        Ok(())
    }
);
source_test!(
    process_registry_a_scheduled_warm_up_is_still_recorded,
    "tests/test_process_registry.py::test_a_scheduled_warm_up_is_still_recorded",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("registry-kind-finalization")?;
        profile(t.path(), "p")?;
        write_json(
            &t.path().join("schedules.json"),
            &json!([{"id":"one","profileId":"p","lastOutcome":null},{"id":"two","profileId":"p","lastOutcome":null}]),
        )?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let release = t.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .arg("0")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(registry.spawn(
            "p",
            &mut child,
            &ready,
            Kind::ScheduledWarmup,
            Some("one".into()),
            Duration::from_secs(2)
        )?);
        assert!(registry.owns("p")?);
        fs::write(&release, b"go")?;
        let end = Instant::now() + Duration::from_secs(3);
        while registry.running("p")? {
            assert!(Instant::now() < end, "registry failed to reap");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!registry.owns("p")?);
        let rows: Value = serde_json::from_slice(&fs::read(t.path().join("schedules.json"))?)?;
        assert!(rows[1]["lastOutcome"].is_null());
        assert_eq!(rows[0]["lastOutcome"], "ok");
        Ok(())
    }
);
source_test!(
    process_registry_a_reaped_child_stamps_last_active,
    "tests/test_process_registry.py::test_a_reaped_child_stamps_last_active",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("registry-kind-finalization")?;
        profile(t.path(), "p")?;
        write_json(
            &t.path().join("schedules.json"),
            &json!([{"id":"one","profileId":"p","lastOutcome":null},{"id":"two","profileId":"p","lastOutcome":null}]),
        )?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let release = t.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .arg("0")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(registry.spawn(
            "p",
            &mut child,
            &ready,
            Kind::Launch,
            None,
            Duration::from_secs(2)
        )?);
        assert!(registry.owns("p")?);
        fs::write(&release, b"go")?;
        let end = Instant::now() + Duration::from_secs(3);
        while registry.running("p")? {
            assert!(Instant::now() < end, "registry failed to reap");
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!registry.owns("p")?);
        let rows: Value = serde_json::from_slice(&fs::read(t.path().join("schedules.json"))?)?;
        assert!(rows[1]["lastOutcome"].is_null());
        assert!(rows[0]["lastOutcome"].is_null());
        assert!(rows[0]["lastRun"].is_null());
        let record = numera_core::storage::dashboard::DashboardStore::new(t.path().into(), None)
            .get("p")?
            .ok_or("profile missing")?;
        assert!(record["_last_active"].as_f64().is_some());
        Ok(())
    }
);
source_test!(
    process_registry_stopping_a_child_stamps_last_active_too,
    "tests/test_process_registry.py::test_stopping_a_child_stamps_last_active_too",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("registry-kind-finalization")?;
        profile(t.path(), "p")?;
        write_json(
            &t.path().join("schedules.json"),
            &json!([{"id":"one","profileId":"p","lastOutcome":null},{"id":"two","profileId":"p","lastOutcome":null}]),
        )?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let release = t.path().join("release");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("ok")
            .arg(&release)
            .arg("0")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(registry.spawn(
            "p",
            &mut child,
            &ready,
            Kind::Launch,
            None,
            Duration::from_secs(2)
        )?);
        assert!(registry.owns("p")?);
        assert!(registry.stop("p", Duration::from_secs(2))?);
        assert!(!registry.owns("p")?);
        let rows: Value = serde_json::from_slice(&fs::read(t.path().join("schedules.json"))?)?;
        assert!(rows[1]["lastOutcome"].is_null());
        assert!(rows[0]["lastOutcome"].is_null());
        assert!(rows[0]["lastRun"].is_null());
        let record = numera_core::storage::dashboard::DashboardStore::new(t.path().into(), None)
            .get("p")?
            .ok_or("profile missing")?;
        assert!(record["_last_active"].as_f64().is_some());
        Ok(())
    }
);
source_test!(
    startup_handshake_a_locked_vault_launch_is_never_reported_as_running,
    "tests/test_startup_handshake.py::test_a_locked_vault_launch_is_never_reported_as_running",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("locked-startup-registry")?;
        profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("locked")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let error = registry
            .spawn(
                "p",
                &mut child,
                &ready,
                Kind::Launch,
                None,
                Duration::from_secs(2),
            )
            .expect_err("locked launch accepted");
        assert_eq!(error.kind, "locked");
        assert!(!registry.running("p")?);
        assert!(!registry.owns("p")?);
        Ok(())
    }
);
source_test!(
    startup_handshake_a_child_that_exits_before_readiness_owns_nothing,
    "tests/test_startup_handshake.py::test_a_child_that_exits_before_readiness_owns_nothing",
    {
        use numera_core::ownership::registry::{Kind, Registry};
        let t = temp("stillborn-startup-registry")?;
        profile(t.path(), "p")?;
        let registry = Registry::new(t.path().into(), "source-host".into(), None)?;
        let ready = t.path().join("ready.json");
        let mut child = command("ready")?;
        child
            .arg(&ready)
            .arg("silent")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        assert!(
            registry
                .spawn(
                    "p",
                    &mut child,
                    &ready,
                    Kind::ManualWarmup,
                    None,
                    Duration::from_secs(2)
                )
                .is_err()
        );
        assert!(!registry.running("p")?);
        assert!(!registry.owns("p")?);
        assert!(matches!(claims::read(t.path(), "p"), claims::Claim::Absent));
        Ok(())
    }
);
source_test!(
    parent_ownership_nothing_to_watch_starts_no_thread,
    "tests/test_parent_ownership.py::test_nothing_to_watch_starts_no_thread",
    {
        use numera_core::ownership::parent::Monitor;
        assert!(Monitor::from_text(None)?.is_none());
        assert!(Monitor::from_text(Some(""))?.is_none());
        assert!(Monitor::from_text(Some("not-a-pid"))?.is_none());
        Ok(())
    }
);
source_test!(
    parent_ownership_a_dead_parent_is_not_watched,
    "tests/test_parent_ownership.py::test_a_dead_parent_is_not_watched",
    {
        let monitor = numera_core::ownership::parent::Monitor::from_text(Some("9999999"))?
            .ok_or("configured dead parent lost")?;
        assert!(!monitor.alive()?);
        Ok(())
    }
);
source_test!(
    parent_ownership_liveness_says_what_it_means,
    "tests/test_parent_ownership.py::test_liveness_says_what_it_means",
    {
        assert!(identity::alive(std::process::id(), None)?);
        assert!(!identity::alive(9999999, None)?);
        let monitor = numera_core::ownership::parent::Monitor::from_text(Some(
            &std::process::id().to_string(),
        ))?
        .ok_or("live parent unobserved")?;
        assert!(monitor.alive()?);
        Ok(())
    }
);
source_test!(
    parent_ownership_it_reads_the_environment_the_shell_sets,
    "tests/test_parent_ownership.py::test_it_reads_the_environment_the_shell_sets",
    {
        let t = temp("real-parent-monitor")?;
        let mut middle_command = command("sleep")?;
        middle_command.stdout(Stdio::null()).stderr(Stdio::null());
        let mut middle = OwnedTree::spawn(&mut middle_command)?;
        let ready = t.path().join("watching");
        let release = t.path().join("release");
        let mut child = command("watch")?;
        child
            .arg(&ready)
            .arg(&release)
            .env("NUMERA_PARENT_PID", middle.pid().to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut watched = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        thread::sleep(Duration::from_millis(150));
        assert!(
            watched.child_mut().try_wait()?.is_none(),
            "live parent caused premature exit"
        );
        fs::write(&release, b"go")?;
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = watched.child_mut().try_wait()? {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < end, "released child did not exit");
            thread::sleep(Duration::from_millis(5));
        }
        middle.settle(Duration::from_secs(2))?;
        watched.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    parent_ownership_the_child_exits_when_its_named_parent_dies,
    "tests/test_parent_ownership.py::test_the_child_exits_when_its_named_parent_dies",
    {
        let t = temp("real-parent-monitor")?;
        let mut middle_command = command("sleep")?;
        middle_command.stdout(Stdio::null()).stderr(Stdio::null());
        let mut middle = OwnedTree::spawn(&mut middle_command)?;
        let ready = t.path().join("watching");
        let release = t.path().join("release");
        let mut child = command("watch")?;
        child
            .arg(&ready)
            .arg(&release)
            .env("NUMERA_PARENT_PID", middle.pid().to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut watched = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        middle.settle(Duration::from_secs(2))?;
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = watched.child_mut().try_wait()? {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < end, "orphaned child remained alive");
            thread::sleep(Duration::from_millis(5));
        }
        watched.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
source_test!(
    parent_ownership_the_child_stays_up_while_its_parent_lives,
    "tests/test_parent_ownership.py::test_the_child_stays_up_while_its_parent_lives",
    {
        let t = temp("real-parent-monitor")?;
        let mut middle_command = command("sleep")?;
        middle_command.stdout(Stdio::null()).stderr(Stdio::null());
        let mut middle = OwnedTree::spawn(&mut middle_command)?;
        let ready = t.path().join("watching");
        let release = t.path().join("release");
        let mut child = command("watch")?;
        child
            .arg(&ready)
            .arg(&release)
            .env("NUMERA_PARENT_PID", middle.pid().to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut watched = OwnedTree::spawn(&mut child)?;
        wait_file(&ready)?;
        thread::sleep(Duration::from_millis(150));
        assert!(
            watched.child_mut().try_wait()?.is_none(),
            "live parent caused premature exit"
        );
        fs::write(&release, b"go")?;
        let end = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = watched.child_mut().try_wait()? {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < end, "released child did not exit");
            thread::sleep(Duration::from_millis(5));
        }
        middle.settle(Duration::from_secs(2))?;
        watched.settle(Duration::from_secs(2))?;
        Ok(())
    }
);
