use super::*;
source_test!(
    a_failed_migration_leaves_no_vault_and_no_half_encrypted_store,
    "tests/test_failure_atomicity.py::test_a_failed_migration_leaves_no_vault_and_no_half_encrypted_store",
    {
        let temp = temp();
        let root = temp.path();
        let ids = (0..4)
            .map(|i| secret_profile(root, &format!("p{i}"), i + 1, Some(&format!("secret-{i}"))).id)
            .collect::<Vec<_>>();
        let before = ids
            .iter()
            .map(|id| raw(root, &format!("profiles/{id}.json"))["proxy"]["password"].clone())
            .collect::<Vec<_>>();
        let mut count = 0;
        let result = testing::with_observer(
            move |event| {
                if event.operation == "encrypt" {
                    count += 1;
                    if count == 3 {
                        return Err(io::Error::other("disk full"));
                    }
                }
                Ok(Action::Continue)
            },
            || store(root).enable_vault("master"),
        );
        assert!(result.is_err());
        assert!(!store(root).vault_enabled().expect("vault disabled"));
        for (i, id) in ids.iter().enumerate() {
            assert_eq!(
                raw(root, &format!("profiles/{id}.json"))["proxy"]["password"],
                before[i]
            );
        }
    }
);
source_test!(
    a_successful_migration_converts_everything_and_then_switches_on,
    "tests/test_failure_atomicity.py::test_a_successful_migration_converts_everything_and_then_switches_on",
    {
        let temp = temp();
        let root = temp.path();
        let ids = (0..3)
            .map(|i| secret_profile(root, &format!("p{i}"), i + 1, Some(&format!("secret-{i}"))).id)
            .collect::<Vec<_>>();
        let plain = saved(root, "no-secret", 9);
        assert_eq!(store(root).enable_vault("master").expect("migration"), 6);
        assert!(store(root).vault_enabled().expect("enabled"));
        for id in ids {
            assert!(numera_core::vault::is_envelope(
                raw(root, &format!("profiles/{id}.json"))["proxy"]["password"]
                    .as_str()
                    .expect("envelope")
            ));
        }
        assert!(raw(root, &format!("profiles/{}.json", plain.id))["proxy"].is_null());
    }
);
source_test!(
    a_failed_engine_write_does_not_leave_the_dashboard_copy_ahead,
    "tests/test_failure_atomicity.py::test_a_failed_engine_write_does_not_leave_the_dashboard_copy_ahead",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"before"}));
        let id = p["id"].as_str().expect("id");
        let mut changed = p.clone();
        changed["name"] = "after".into();
        let target = format!("{id}.json");
        let result = testing::with_observer(
            move |event| {
                if event.operation == "before-batch-write"
                    && event.path.parent().is_some_and(|p| p.ends_with("profiles"))
                    && event.path.file_name().is_some_and(|p| p == target.as_str())
                {
                    return Err(io::Error::other("engine store is read-only"));
                }
                Ok(Action::Continue)
            },
            || dash(root).save(changed, Default::default(), None, None, 1001.0),
        );
        assert!(result.is_err());
        assert_eq!(raw(root, &format!("dashboard/{id}.json"))["name"], "before");
    }
);
source_test!(
    a_profile_the_engine_cannot_model_is_refused_not_half_written,
    "tests/test_failure_atomicity.py::test_a_profile_the_engine_cannot_model_is_refused_not_half_written",
    {
        let temp = temp();
        let root = temp.path();
        let result =
            testing::with_observer(error_at("before-dashboard-conversion", "", 13), || {
                dash(root).save(
                    json!({"name":"odd"}),
                    Default::default(),
                    None,
                    None,
                    1000.0,
                )
            });
        assert!(matches!(
            result,
            Err(numera_core::storage::repo::RepoError::UnconvertibleProfile(
                _
            ))
        ));
        assert!(json_files(root, "dashboard").is_empty());
        assert!(json_files(root, "profiles").is_empty());
    }
);
source_test!(
    a_restore_that_cannot_finish_leaves_the_original_intact,
    "tests/test_failure_atomicity.py::test_a_restore_that_cannot_finish_leaves_the_original_intact",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, zip) = restored_fixture(root);
        store(root)
            .edit(
                &p.id,
                |p| {
                    p.name = "renamed since".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("rename");
        fs::write(ud.join("Cookies"), b"live session").expect("live");
        let result = testing::with_observer(error_at("before-restore-transfer", "", 13), || {
            store(root).restore(&zip, false, "source-storage-host", 1002.0)
        });
        assert!(result.is_err());
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("live session"),
            b"live session"
        );
        assert_eq!(
            store(root).get(&p.id).expect("get").expect("profile").name,
            "renamed since"
        );
        assert!(directories(&ud, "restore").is_empty());
        assert!(directories(&root.join("user-data"), ".replaced-").is_empty());
    }
);
source_test!(
    a_restore_that_succeeds_replaces_the_session_and_the_metadata,
    "tests/test_failure_atomicity.py::test_a_restore_that_succeeds_replaces_the_session_and_the_metadata",
    {
        let temp = temp();
        let root = temp.path();
        let (p, ud, zip) = restored_fixture(root);
        store(root)
            .edit(
                &p.id,
                |p| {
                    p.name = "renamed since".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("rename");
        fs::write(ud.join("Cookies"), b"live session").expect("live");
        fs::write(ud.join("Extra"), b"not in backup").expect("extra");
        store(root)
            .restore(&zip, false, "source-storage-host", 1002.0)
            .expect("restore");
        assert_eq!(
            fs::read(ud.join("Cookies")).expect("restored"),
            b"the live session"
        );
        assert!(!ud.join("Extra").exists());
        assert_eq!(
            store(root).get(&p.id).expect("get").expect("profile").name,
            "alpha"
        );
        assert!(directories(&root.join("user-data"), ".replaced-").is_empty());
    }
);
source_test!(
    an_interrupted_write_leaves_the_previous_content_case_006,
    "tests/test_failure_atomicity.py::test_an_interrupted_write_leaves_the_previous_content[profile]",
    {
        let temp = temp();
        let root = temp.path();
        let p = saved(root, "first", 1);
        let target = root.join(format!("profiles/{}.json", p.id));
        let before = fs::read(&target).expect("previous bytes");
        let result = testing::with_observer(
            |event| {
                if event.operation == "before-atomic-replace" {
                    return Err(io::Error::other("crash"));
                }
                Ok(Action::Continue)
            },
            || {
                store(root)
                    .edit(
                        &p.id,
                        |p| {
                            p.name = "second".into();
                            Ok(None)
                        },
                        1001.0,
                    )
                    .map(|_| ())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&target).expect("after"), before);
        assert!(serde_json::from_slice::<Value>(&before).is_ok());
        assert!(
            fs::read_dir(target.parent().expect("parent"))
                .expect("files")
                .all(|e| !e
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp"))
        );
    }
);
source_test!(
    an_interrupted_write_leaves_the_previous_content_case_007,
    "tests/test_failure_atomicity.py::test_an_interrupted_write_leaves_the_previous_content[config]",
    {
        let temp = temp();
        let root = temp.path();
        let config = ConfigStore::new(root.to_owned());
        config
            .set_many(
                json!({"default_engine":"playwright"})
                    .as_object()
                    .expect("config")
                    .clone(),
            )
            .expect("initial config");
        let target = root.join("config.json");
        let before = fs::read(&target).expect("previous bytes");
        let result = testing::with_observer(
            |event| {
                if event.operation == "before-atomic-replace" {
                    return Err(io::Error::other("crash"));
                }
                Ok(Action::Continue)
            },
            || {
                config
                    .set_many(
                        json!({"default_engine":"camoufox"})
                            .as_object()
                            .expect("config")
                            .clone(),
                    )
                    .map_err(numera_core::storage::repo::RepoError::from)
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&target).expect("after"), before);
        assert!(serde_json::from_slice::<Value>(&before).is_ok());
        assert!(
            fs::read_dir(target.parent().expect("parent"))
                .expect("files")
                .all(|e| !e
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp"))
        );
    }
);
source_test!(
    an_interrupted_write_leaves_the_previous_content_case_008,
    "tests/test_failure_atomicity.py::test_an_interrupted_write_leaves_the_previous_content[schedules]",
    {
        let temp = temp();
        let root = temp.path();
        let schedules = ScheduleStore::new(root.to_owned());
        schedules
            .add("p1", json!({"everyHours":2}), None)
            .expect("first schedule");
        let target = root.join("schedules.json");
        let before = fs::read(&target).expect("previous bytes");
        let result = testing::with_observer(
            |event| {
                if event.operation == "before-atomic-replace" {
                    return Err(io::Error::other("crash"));
                }
                Ok(Action::Continue)
            },
            || {
                schedules
                    .add("p2", json!({"everyHours":3}), None)
                    .map(|_| ())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&target).expect("after"), before);
        assert!(serde_json::from_slice::<Value>(&before).is_ok());
        assert!(
            fs::read_dir(target.parent().expect("parent"))
                .expect("files")
                .all(|e| !e
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp"))
        );
    }
);
source_test!(
    a_fast_child_keeps_its_outcome,
    "tests/test_failure_atomicity.py::test_a_fast_child_keeps_its_outcome",
    {
        let temp = temp();
        let root = temp.path();
        let jobs = numera_core::jobs::JobStore::new(root.to_owned());
        let record = json!({"id":"abc123","pid":std::process::id(),"pidToken":process_identity::started_at(std::process::id()).expect("birth"),"started":1000.0});
        jobs.record_exit("abc123", 0, 1001.0).expect("early exit");
        write(root, "jobs/abc123.json", record);
        let got = jobs.get("abc123", 1002.0).expect("get").expect("job");
        assert_eq!(got["state"], "done");
        assert_eq!(got["exit"], 0);
        assert_eq!(got["pid"], std::process::id());
        assert_eq!(got["running"], false);
    }
);
source_test!(
    a_slow_child_also_keeps_both_halves,
    "tests/test_failure_atomicity.py::test_a_slow_child_also_keeps_both_halves",
    {
        let temp = temp();
        let root = temp.path();
        let jobs = numera_core::jobs::JobStore::new(root.to_owned());
        let record = json!({"id":"abc123","pid":std::process::id(),"pidToken":process_identity::started_at(std::process::id()).expect("birth"),"started":1000.0});
        write(root, "jobs/abc123.json", record);
        jobs.record_exit("abc123", 3, 1001.0).expect("late exit");
        let got = jobs.get("abc123", 1002.0).expect("get").expect("job");
        assert_eq!(got["state"], "failed");
        assert_eq!(got["exit"], 3);
        assert_eq!(got["pid"], std::process::id());
    }
);
source_test!(
    the_outcome_file_is_not_listed_as_a_second_job,
    "tests/test_failure_atomicity.py::test_the_outcome_file_is_not_listed_as_a_second_job",
    {
        let temp = temp();
        let root = temp.path();
        let jobs = numera_core::jobs::JobStore::new(root.to_owned());
        let record = json!({"id":"abc123","pid":std::process::id(),"pidToken":process_identity::started_at(std::process::id()).expect("birth"),"started":1000.0});
        write(root, "jobs/abc123.json", record);
        jobs.record_exit("abc123", 0, 1001.0).expect("exit");
        assert_eq!(
            jobs.listing(1002.0)
                .expect("listing")
                .iter()
                .map(|r| r["id"].as_str().expect("id"))
                .collect::<Vec<_>>(),
            ["abc123"]
        );
    }
);
source_test!(
    pruning_removes_both_halves,
    "tests/test_failure_atomicity.py::test_pruning_removes_both_halves",
    {
        let temp = temp();
        let root = temp.path();
        let jobs = numera_core::jobs::JobStore::new(root.to_owned());
        let record = json!({"id":"abc123","pid":std::process::id(),"pidToken":process_identity::started_at(std::process::id()).expect("birth"),"started":1000.0});
        write(root, "jobs/abc123.json", record);
        jobs.record_exit("abc123", 0, 1001.0).expect("exit");
        assert_eq!(jobs.prune(1002.0).expect("prune"), 1);
        assert!(fs::read_dir(root.join("jobs")).expect("jobs").all(|e| {
            !e.expect("entry")
                .file_name()
                .to_string_lossy()
                .starts_with("abc123")
        }));
    }
);
source_test!(
    the_store_refuses_an_invalid_schedule_patch,
    "tests/test_failure_atomicity.py::test_the_store_refuses_an_invalid_schedule_patch",
    {
        let temp = temp();
        let root = temp.path();
        let sched = ScheduleStore::new(root.to_owned());
        let entry = sched
            .add("p1", json!({"everyHours":2}), Some(0.0))
            .expect("schedule");
        let id = entry["id"].as_str().expect("id");
        let before = fs::read(root.join("schedules.json")).expect("before");
        for bad in [
            json!({"everyHours":"soon"}),
            json!({"everyHours":-5}),
            json!({"minutes":0}),
            json!({"enabled":"yes"}),
        ] {
            assert!(sched.update(id, &bad).is_err());
        }
        assert_eq!(
            fs::read(root.join("schedules.json")).expect("after"),
            before
        );
        assert!(
            schedules::next_run_at(&sched.get(id).expect("get").expect("row"))
                .expect("next time")
                .is_finite()
        );
    }
);
source_test!(
    a_client_cannot_move_the_next_run,
    "tests/test_failure_atomicity.py::test_a_client_cannot_move_the_next_run",
    {
        let temp = temp();
        let root = temp.path();
        let sched = ScheduleStore::new(root.to_owned());
        let entry = sched
            .add("p1", json!({"everyHours":2}), Some(0.0))
            .expect("schedule");
        let id = entry["id"].as_str().expect("id");
        assert!(sched.update(id, &json!({"lastRun":0})).is_err());
    }
);
source_test!(
    the_scheduler_can_still_write_its_own_outcome,
    "tests/test_failure_atomicity.py::test_the_scheduler_can_still_write_its_own_outcome",
    {
        let temp = temp();
        let root = temp.path();
        let sched = ScheduleStore::new(root.to_owned());
        let entry = sched
            .add("p1", json!({"everyHours":2}), Some(0.0))
            .expect("schedule");
        let id = entry["id"].as_str().expect("id");
        sched.mark_run(id, 1234.0).expect("mark");
        sched
            .record(
                id,
                json!({"lastOutcome":"ok","lastError":null})
                    .as_object()
                    .expect("outcome")
                    .clone(),
            )
            .expect("record");
        let row = sched.get(id).expect("get").expect("row");
        assert_eq!(row["lastRun"], 1234.0);
        assert_eq!(row["lastOutcome"], "ok");
        let error = sched
            .record(
                id,
                json!({"everyHours":99})
                    .as_object()
                    .expect("bad outcome")
                    .clone(),
            )
            .expect_err("client field refused");
        assert!(error.to_string().contains("not a scheduler-owned field"));
    }
);
source_test!(
    a_valid_patch_is_normalised_not_just_accepted,
    "tests/test_failure_atomicity.py::test_a_valid_patch_is_normalised_not_just_accepted",
    {
        let temp = temp();
        let root = temp.path();
        let sched = ScheduleStore::new(root.to_owned());
        let entry = sched
            .add("p1", json!({"everyHours":2}), Some(0.0))
            .expect("schedule");
        let id = entry["id"].as_str().expect("id");
        assert_eq!(
            sched
                .update(id, &json!({"everyHours":0.01}))
                .expect("update")
                .expect("row")["everyHours"],
            0.25
        );
    }
);
source_test!(
    concurrent_schedule_writes_do_not_lose_each_other,
    "tests/test_failure_atomicity.py::test_concurrent_schedule_writes_do_not_lose_each_other",
    {
        let temp = temp();
        let root = temp.path();
        let sched = ScheduleStore::new(root.to_owned());
        let rows = (0..8)
            .map(|i| {
                sched
                    .add(&format!("p{i}"), json!({"everyHours":2}), Some(0.0))
                    .expect("schedule")["id"]
                    .as_str()
                    .expect("id")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        let mut workers = Vec::new();
        for (i, id) in rows.iter().cloned().enumerate() {
            let root = root.to_owned();
            workers.push(thread::spawn(move || {
                let sched = ScheduleStore::new(root);
                for _ in 0..15 {
                    sched
                        .update(&id, &json!({"minutes":(i+1)as f64}))
                        .expect("minutes");
                    sched.mark_run(&id, i as f64).expect("last run");
                }
            }));
        }
        for worker in workers {
            worker.join().expect("writer");
        }
        let final_rows = sched.list().expect("schedules");
        assert_eq!(final_rows.len(), 8);
        for (i, id) in rows.iter().enumerate() {
            let row = final_rows
                .iter()
                .find(|r| r["id"] == id.as_str())
                .expect("row survived");
            assert_eq!(row["minutes"], (i + 1) as f64);
            assert_eq!(row["lastRun"], i as f64);
        }
    }
);
