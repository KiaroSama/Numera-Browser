use super::*;
source_test!(
    add_and_list_round_trip,
    "tests/test_scheduler.py::test_add_and_list_round_trip",
    {
        let temp = temp();
        let root = temp.path();
        let st = ScheduleStore::new(root.to_owned());
        let e = st
            .add(
                "p1",
                json!({"everyHours":12,"minutes":7,"preset":"ads"}),
                Some(1000.0),
            )
            .expect("add");
        assert_eq!(e["profileId"], "p1");
        assert_eq!(e["preset"], "ads");
        assert_eq!(st.list().expect("list")[0]["id"], e["id"]);
        assert_eq!(
            st.get(e["id"].as_str().expect("id"))
                .expect("get")
                .expect("entry")["minutes"],
            7.0
        );
    }
);
source_test!(
    interval_is_floored,
    "tests/test_scheduler.py::test_interval_is_floored",
    {
        let temp = temp();
        let root = temp.path();
        let st = ScheduleStore::new(root.to_owned());
        let e = st
            .add("p1", json!({"everyHours":0.0001}), Some(0.0))
            .expect("tiny interval");
        assert_eq!(e["everyHours"], 0.25);
        assert!(st.add("p1", json!({"everyHours":0}), Some(0.0)).is_err());
    }
);
source_test!(delete, "tests/test_scheduler.py::test_delete", {
    let temp = temp();
    let root = temp.path();
    let st = ScheduleStore::new(root.to_owned());
    let e = st
        .add("p1", json!({"everyHours":1}), Some(0.0))
        .expect("add");
    let id = e["id"].as_str().expect("id");
    assert!(st.delete(id).expect("delete"));
    assert!(st.list().expect("list").is_empty());
    assert!(!st.delete("nope").expect("missing"));
});
source_test!(
    never_run_is_due_at_creation,
    "tests/test_scheduler.py::test_never_run_is_due_at_creation",
    {
        let s = json!({"id": "x", "profileId": "p", "everyHours": 12, "enabled": true, "lastRun": null, "createdAt": 5000});
        assert_eq!(schedules::next_run_at(&s).expect("next"), 5000.0);
        assert_eq!(
            schedules::due_schedules(std::slice::from_ref(&s), 5000.0)
                .expect("due")
                .as_slice(),
            std::slice::from_ref(&s)
        );
        assert!(
            schedules::due_schedules(&[s], 4999.0)
                .expect("before")
                .is_empty()
        );
    }
);
source_test!(
    next_run_is_last_plus_interval,
    "tests/test_scheduler.py::test_next_run_is_last_plus_interval",
    {
        let s = json!({"id": "x", "profileId": "p", "everyHours": 12, "enabled": true, "lastRun": 10000, "createdAt": 0});
        assert_eq!(schedules::next_run_at(&s).expect("next"), 53200.0);
        assert_eq!(
            schedules::due_schedules(std::slice::from_ref(&s), 53200.0)
                .expect("due")
                .as_slice(),
            std::slice::from_ref(&s)
        );
        assert!(
            schedules::due_schedules(&[s], 49600.0)
                .expect("before")
                .is_empty()
        );
    }
);
source_test!(
    disabled_never_due,
    "tests/test_scheduler.py::test_disabled_never_due",
    {
        let s = json!({"id": "x", "profileId": "p", "everyHours": 1, "enabled": false, "lastRun": null, "createdAt": 0});
        assert!(
            schedules::due_schedules(&[s], 1e9)
                .expect("disabled")
                .is_empty()
        );
    }
);
source_test!(
    mark_run_moves_next_due,
    "tests/test_scheduler.py::test_mark_run_moves_next_due",
    {
        let temp = temp();
        let root = temp.path();
        let st = ScheduleStore::new(root.to_owned());
        let e = st
            .add("p1", json!({"everyHours":6}), Some(0.0))
            .expect("add");
        assert_eq!(
            schedules::due_schedules(&st.list().expect("rows"), 1.0).expect("due"),
            st.list().expect("rows")
        );
        st.mark_run(e["id"].as_str().expect("id"), 1000.0)
            .expect("mark");
        assert!(
            schedules::due_schedules(&st.list().expect("rows"), 1000.0)
                .expect("not due")
                .is_empty()
        );
        assert_eq!(
            schedules::due_schedules(&st.list().expect("rows"), 22600.0).expect("due again"),
            st.list().expect("rows")
        );
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_015,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad0]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"everyHours": "soon"}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_016,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad1]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"everyHours": -4}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_017,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad2]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"everyHours": 0}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_018,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad3]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"everyHours": 1000000000.0}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_019,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad4]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"minutes": -10}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_020,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad5]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"minutes": 0}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_021,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad6]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"minutes": "lots"}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_022,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad7]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"enabled": "yes"}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_schedule_is_refused_not_stored_case_023,
    "tests/test_scheduler.py::test_a_malformed_schedule_is_refused_not_stored[bad8]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(
            root,
            "POST",
            "/api/schedules",
            json!({"profileId":p["id"],"everyHours":12,"minutes":5,"preset":"general"}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!(
            "/api/schedules/{}",
            row["id"].as_str().expect("schedule id")
        );
        let before = http(root, "GET", "/api/schedules", Value::Null).1;
        let (status, _body) = http(root, "PUT", &uri, json!({"preset": 5}));
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert_eq!(http(root, "GET", "/api/schedules", Value::Null).1, before);
    }
);
source_test!(
    a_malformed_create_is_a_4xx_not_a_500_case_024,
    "tests/test_scheduler.py::test_a_malformed_create_is_a_4xx_not_a_500[bad0]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let mut body = json!({"everyHours": "soon", "profileId": null});
        body["profileId"] = p["id"].clone();
        let (status, _value) = http(root, "POST", "/api/schedules", body);
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert!(
            ScheduleStore::new(root.to_owned())
                .list()
                .expect("rows")
                .is_empty()
        );
    }
);
source_test!(
    a_malformed_create_is_a_4xx_not_a_500_case_025,
    "tests/test_scheduler.py::test_a_malformed_create_is_a_4xx_not_a_500[bad1]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let mut body = json!({"minutes": -1, "profileId": null});
        body["profileId"] = p["id"].clone();
        let (status, _value) = http(root, "POST", "/api/schedules", body);
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert!(
            ScheduleStore::new(root.to_owned())
                .list()
                .expect("rows")
                .is_empty()
        );
    }
);
source_test!(
    a_malformed_create_is_a_4xx_not_a_500_case_026,
    "tests/test_scheduler.py::test_a_malformed_create_is_a_4xx_not_a_500[bad2]",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let mut body = json!({"everyHours": 100000, "profileId": null});
        body["profileId"] = p["id"].clone();
        let (status, _value) = http(root, "POST", "/api/schedules", body);
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert!(
            ScheduleStore::new(root.to_owned())
                .list()
                .expect("rows")
                .is_empty()
        );
    }
);
source_test!(
    scheduler_owned_fields_cannot_be_patched,
    "tests/test_scheduler.py::test_scheduler_owned_fields_cannot_be_patched",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(root, "POST", "/api/schedules", json!({"profileId":p["id"]}));
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!("/api/schedules/{}", row["id"].as_str().expect("id"));
        for key in ["lastRun", "lastOutcome", "profileId"] {
            let mut patch = json!({});
            patch[key] = 1.into();
            assert_eq!(http(root, "PUT", &uri, patch).0, 400, "{key}");
        }
    }
);
source_test!(
    a_valid_update_still_works,
    "tests/test_scheduler.py::test_a_valid_update_still_works",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "s");
        let (status, row) = http(root, "POST", "/api/schedules", json!({"profileId":p["id"]}));
        assert_eq!(status, 200, "HTTP response status mismatch");
        let uri = format!("/api/schedules/{}", row["id"].as_str().expect("id"));
        let (status, changed) = http(root, "PUT", &uri, json!({"everyHours":6,"enabled":false}));
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(changed["everyHours"], 6.0);
        assert_eq!(changed["enabled"], false);
    }
);
source_test!(
    a_non_finite_interval_is_refused,
    "tests/test_scheduler.py::test_a_non_finite_interval_is_refused",
    {
        for text in ["Infinity", "-Infinity", "NaN"] {
            let value = Value::Number(serde_json::Number::from_string_unchecked(text.to_owned()));
            let error = {
                let mut patch = json!({});
                patch["everyHours"] = value;
                schedules::validate_schedule(&patch, true)
            }
            .expect_err("nonfinite interval");
            assert!(!error.to_string().is_empty());
        }
    }
);
source_test!(
    a_tiny_interval_is_clamped_not_refused,
    "tests/test_scheduler.py::test_a_tiny_interval_is_clamped_not_refused",
    {
        assert_eq!(
            schedules::validate_schedule(&json!({"everyHours":0.0001}), true).expect("clamp")["everyHours"],
            0.25
        );
    }
);
source_test!(
    one_unusable_schedule_does_not_stop_the_others,
    "tests/test_scheduler.py::test_one_unusable_schedule_does_not_stop_the_others",
    {
        let temp = temp();
        let root = temp.path();
        let (bad, badrow) = lifecycle::schedule(root, "bad", 1);
        let (good, _) = lifecycle::schedule(root, "good", 2);
        let (scheduler, registry) = lifecycle::scheduler(root);
        let target = format!("{}.json", bad.id);
        let reached = Arc::new(Mutex::new(false));
        let seen = reached.clone();
        testing::with_observer(
            move |event| {
                if event.operation == "before-profile-materialize"
                    && event.path.file_name().is_some_and(|p| p == target.as_str())
                {
                    *seen.lock().expect("fault reached") = true;
                    return Err(io::Error::other("unconvertible"));
                }
                Ok(Action::Continue)
            },
            || scheduler.tick(),
        )
        .expect("tick");
        assert!(*reached.lock().expect("fault reached"));
        assert!(
            registry.running(&good.id).expect("good running"),
            "row after unusable profile activated"
        );
        let failed = lifecycle::row(root, badrow["id"].as_str().expect("id"));
        assert_eq!(failed["lastOutcome"], "failed");
        assert!(!failed["lastError"].as_str().expect("error").is_empty());
        lifecycle::shutdown(&registry);
    }
);
source_test!(
    a_warm_up_that_dies_immediately_is_not_recorded_as_run,
    "tests/test_scheduler.py::test_a_warm_up_that_dies_immediately_is_not_recorded_as_run",
    {
        let temp = temp();
        let root = temp.path();
        let (_, row) = lifecycle::schedule(root, "dies", 1);
        fs::write(root.join("adapter-mode"), b"exit-immediate").expect("exit mode");
        let (scheduler, registry) = lifecycle::scheduler(root);
        scheduler.tick().expect("tick");
        let failed = lifecycle::row(root, row["id"].as_str().expect("id"));
        assert_eq!(failed["lastOutcome"], "failed");
        assert!(failed["lastRun"].is_null());
        assert!(failed["claimedBy"].is_null());
        lifecycle::shutdown(&registry);
        assert!(!root.join("page-created").exists());
    }
);
source_test!(
    the_exit_code_of_a_finished_warm_up_is_recorded,
    "tests/test_scheduler.py::test_the_exit_code_of_a_finished_warm_up_is_recorded",
    {
        let temp = temp();
        let root = temp.path();
        let (p, row) = lifecycle::schedule(root, "finishes", 1);
        let (scheduler, registry) = lifecycle::scheduler(root);
        scheduler.tick().expect("tick");
        let id = row["id"].as_str().expect("id");
        assert_eq!(lifecycle::row(root, id)["lastOutcome"], "started");
        let adapter_pid = fs::read_to_string(root.join("adapter-pid"))
            .expect("actual adapter pid")
            .parse::<u32>()
            .expect("native pid");
        let adapter_token = process_identity::started_at(adapter_pid)
            .expect("adapter birth")
            .expect("live adapter");
        let browser_pid = fs::read_to_string(root.join("browser-pid"))
            .expect("actual browser leaf pid")
            .parse::<u32>()
            .expect("native pid");
        let browser_token = process_identity::started_at(browser_pid)
            .expect("browser birth")
            .expect("live browser leaf");
        assert!(
            process_identity::alive(browser_pid, Some(browser_token)).expect("live leaf control")
        );
        let tree = raw(root, &format!("sessions/{}.json", p.id));
        assert!(!numera_core::ownership::recorded_tree::empty(&tree).expect("live owned tree"));
        fs::write(root.join("adapter-exit"), b"3").expect("actual adapter exit");
        process::wait(&root.join("adapter-exiting"));
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            registry.reap().expect("reap");
            if !registry.running(&p.id).expect("running") {
                break;
            }
            assert!(Instant::now() < end, "warmup never settled");
            thread::sleep(Duration::from_millis(10));
        }
        let outcome = lifecycle::row(root, id);
        assert!(
            !process_identity::alive(adapter_pid, Some(adapter_token)).expect("exited adapter")
        );
        // A reparented Unix zombie has a PID but cannot execute. The same
        // kernel tree observation used by the owner proves every live leaf
        // ended; Windows observes the captured named Job instead of PID zero.
        assert!(numera_core::ownership::recorded_tree::empty(&tree).expect("settled whole tree"));
        assert_eq!(outcome["lastOutcome"], "failed");
        assert!(
            outcome["lastError"]
                .as_str()
                .expect("exit error")
                .contains('3'),
            "{outcome}"
        );
        lifecycle::shutdown(&registry);
    }
);
source_test!(
    shutdown_terminates_the_children_it_started,
    "tests/test_scheduler.py::test_shutdown_terminates_the_children_it_started",
    {
        let temp = temp();
        let root = temp.path();
        let (p, _) = lifecycle::schedule(root, "child", 1);
        let (scheduler, registry) = lifecycle::scheduler(root);
        scheduler.tick().expect("tick");
        assert!(registry.running(&p.id).expect("running"));
        let pid = fs::read_to_string(root.join("browser-pid"))
            .expect("actual browser pid")
            .parse::<u32>()
            .expect("pid");
        let token = process_identity::started_at(pid)
            .expect("browser birth")
            .expect("live browser");
        lifecycle::shutdown(&registry);
        assert!(!process_identity::alive(pid, Some(token)).expect("browser observation"));
        assert!(!registry.running(&p.id).expect("registry settled"));
    }
);
source_test!(
    a_failed_materialisation_releases_the_claim,
    "tests/test_scheduler.py::test_a_failed_materialisation_releases_the_claim",
    {
        let temp = temp();
        let root = temp.path();
        let (_p, row) = lifecycle::schedule(root, "unconvertible", 1);
        let (scheduler, registry) = lifecycle::scheduler(root);
        testing::with_observer(error_at("before-profile-materialize", "", 13), || {
            scheduler.tick()
        })
        .expect("tick");
        let id = row["id"].as_str().expect("id");
        let failed = lifecycle::row(root, id);
        assert!(failed["claimedBy"].is_null());
        assert_eq!(failed["lastOutcome"], "failed");
        assert!(failed["lastRun"].is_null());
        assert!(!root.join("page-created").exists());
        scheduler.tick().expect("retry");
        assert_eq!(lifecycle::row(root, id)["lastOutcome"], "started");
        lifecycle::shutdown(&registry);
    }
);
source_test!(
    a_spawn_that_cannot_start_releases_the_claim,
    "tests/test_scheduler.py::test_a_spawn_that_cannot_start_releases_the_claim",
    {
        let temp = temp();
        let root = temp.path();
        let (_, row) = lifecycle::schedule(root, "unspawnable", 1);
        let registry = Arc::new(
            Registry::new(root.to_owned(), "source-storage-host".into(), None).expect("registry"),
        );
        let mut runtime = lifecycle::runtime(root);
        runtime.adapter = root.join("no-such-executable");
        let scheduler = numera_core::http::scheduler::Scheduler::new(
            root.to_owned(),
            None,
            "source-storage-host".into(),
            registry.clone(),
            runtime,
        )
        .expect("scheduler");
        scheduler.tick().expect("tick");
        let failed = lifecycle::row(root, row["id"].as_str().expect("id"));
        assert!(failed["claimedBy"].is_null());
        assert_eq!(failed["lastOutcome"], "failed");
        assert!(failed["lastRun"].is_null());
        lifecycle::shutdown(&registry);
    }
);
source_test!(
    a_child_that_never_reports_ready_releases_the_claim,
    "tests/test_scheduler.py::test_a_child_that_never_reports_ready_releases_the_claim",
    {
        let temp = temp();
        let root = temp.path();
        let (_, row) = lifecycle::schedule(root, "dies", 1);
        fs::write(root.join("adapter-mode"), b"exit-immediate").expect("exit mode");
        let (scheduler, registry) = lifecycle::scheduler(root);
        scheduler.tick().expect("tick");
        let failed = lifecycle::row(root, row["id"].as_str().expect("id"));
        assert_eq!(failed["lastOutcome"], "failed");
        assert!(failed["lastRun"].is_null());
        assert!(failed["claimedBy"].is_null());
        lifecycle::shutdown(&registry);
        assert!(!root.join("page-created").exists());
    }
);
source_test!(
    only_a_confirmed_start_advances_the_occurrence,
    "tests/test_scheduler.py::test_only_a_confirmed_start_advances_the_occurrence",
    {
        let temp = temp();
        let root = temp.path();
        let (_, row) = lifecycle::schedule(root, "starts", 1);
        let (scheduler, registry) = lifecycle::scheduler(root);
        scheduler.tick().expect("tick");
        let started = lifecycle::row(root, row["id"].as_str().expect("id"));
        assert_eq!(started["lastOutcome"], "started");
        assert!(!started["lastRun"].is_null());
        assert!(started["claimedBy"].is_null());
        assert!(root.join("page-created").is_file());
        lifecycle::shutdown(&registry);
    }
);
