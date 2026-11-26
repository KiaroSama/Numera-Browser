use super::*;
source_test!(
    an_invalid_extension_leaves_the_earlier_config_fields_alone,
    "tests/test_compound_atomicity.py::test_an_invalid_extension_leaves_the_earlier_config_fields_alone",
    {
        let temp = temp();
        let root = temp.path();
        let ext = root.join("does-not-exist");
        let out = settings(root, |settings| {
            settings.config(
                json!({"headless_launch":"on","default_extension":[ext]})
                    .as_object()
                    .expect("args"),
                root,
            )
        })
        .expect("config command");
        assert_eq!(out.exit, 1);
        assert!(
            ConfigStore::new(root.to_owned())
                .read()
                .expect("config")
                .get("headless_launch")
                .is_none()
        );
    }
);
source_test!(
    a_valid_compound_config_still_applies_every_field,
    "tests/test_compound_atomicity.py::test_a_valid_compound_config_still_applies_every_field",
    {
        let temp = temp();
        let root = temp.path();
        let ext = root.join("ext");
        fs::create_dir_all(&ext).expect("extension");
        fs::write(
            ext.join("manifest.json"),
            br#"{"manifest_version":3,"name":"x","version":"1.0"}"#,
        )
        .expect("manifest");
        let out=settings(root,|settings|settings.config(json!({"headless_launch":"on","default_extension":[ext],"default_engine":"playwright"}).as_object().expect("args"),root)).expect("config command");
        assert_eq!(out.exit, 0);
        let cfg = raw(root, "config.json");
        assert_eq!(cfg["headless_launch"], true);
        assert_eq!(cfg["default_engine"], "playwright");
        assert_eq!(cfg["default_extensions"], json!([ext]));
    }
);
source_test!(
    a_failed_create_does_not_move_the_default_engine,
    "tests/test_compound_atomicity.py::test_a_failed_create_does_not_move_the_default_engine",
    {
        let temp = temp();
        let root = temp.path();
        ConfigStore::new(root.to_owned())
            .set_many(
                json!({"default_engine":"playwright"})
                    .as_object()
                    .expect("config")
                    .clone(),
            )
            .expect("config");
        let result = testing::with_observer(nth_failure(1), || {
            cli(root, |commands| {
                commands.create(
                    json!({"name":"boom","engine":"camoufox"})
                        .as_object()
                        .expect("args"),
                    1001.0,
                )
            })
        });
        assert!(result.is_err());
        assert_eq!(raw(root, "config.json")["default_engine"], "playwright");
        assert!(
            store(root)
                .resolve("boom", 1001.0)
                .expect("resolve")
                .is_none()
        );
    }
);
source_test!(
    a_failing_bulk_create_writes_nothing,
    "tests/test_compound_atomicity.py::test_a_failing_bulk_create_writes_nothing",
    {
        let temp = temp();
        let root = temp.path();
        let result = testing::with_observer(nth_failure(5), || {
            cli(root, |commands| {
                commands.bulk_create(
                    json!({"count":4,"prefix":"b"}).as_object().expect("args"),
                    1000.0,
                )
            })
        });
        assert!(result.is_err());
        assert!(store(root).list(1001.0).expect("profiles").is_empty());
    }
);
source_test!(
    a_failing_import_writes_nothing,
    "tests/test_compound_atomicity.py::test_a_failing_import_writes_nothing",
    {
        let temp = temp();
        let root = temp.path();
        let rows = vec![
            json!({"name":"one","os":"Windows"}),
            json!({"name":"two","os":"Windows"}),
            json!({"name":"three","os":"Windows"}),
        ];
        let result = testing::with_observer(nth_failure(4), || {
            cli(root, |commands| {
                commands.import_vendor(&rows, 1000.0, || Ok(1))
            })
        });
        assert!(result.is_err());
        assert!(store(root).list(1001.0).expect("profiles").is_empty());
    }
);
source_test!(
    a_metadata_stamp_does_not_revert_a_concurrent_edit,
    "tests/test_compound_atomicity.py::test_a_metadata_stamp_does_not_revert_a_concurrent_edit",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "stamped");
        let id = p["id"].as_str().expect("id");
        let stale = dash(root).get(id).expect("get").expect("profile");
        assert_eq!(
            http(
                root,
                "PUT",
                &format!("/api/profiles/{id}"),
                json!({"notes":"typed while it ran"})
            )
            .0,
            200
        );
        dash(root)
            .patch(
                id,
                |mut current| {
                    current["_trust"] = json!({"grade":"A"});
                    Ok((current, Default::default()))
                },
                None,
                None,
                1001.0,
            )
            .expect("stamp");
        let final_row = dash(root).get(id).expect("get").expect("profile");
        assert_ne!(final_row["notes"], stale["notes"]);
        assert_eq!(final_row["notes"], "typed while it ran");
        assert_eq!(final_row["_trust"]["grade"], "A");
    }
);
source_test!(
    a_failing_assignment_assigns_nobody,
    "tests/test_compound_atomicity.py::test_a_failing_assignment_assigns_nobody",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"p0"},{"name":"p1"},{"name":"p2"},{"name":"p3"}]}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        assert_eq!(
            http(
                root,
                "POST",
                "/api/proxies",
                json!({"text":"1.1.1.1:8080
2.2.2.2:8080"})
            )
            .0,
            200
        );
        let (status, _body) = testing::with_observer(nth_failure(5), || {
            http(
                root,
                "POST",
                "/api/proxies/assign",
                json!({"profileIds":made["ids"]}),
            )
        });
        assert!(status >= 500, "HTTP response status mismatch");
        assert!(
            http(root, "GET", "/api/profiles", Value::Null)
                .1
                .as_array()
                .expect("profiles")
                .iter()
                .all(|p| p["proxy"].is_null() || p["proxy"] == false)
        );
    }
);
source_test!(
    assignment_survives_a_concurrent_note_edit,
    "tests/test_compound_atomicity.py::test_assignment_survives_a_concurrent_note_edit",
    {
        let temp = temp();
        let root = temp.path();
        let p = create(root, "target");
        let id = p["id"].as_str().expect("id");
        assert_eq!(
            http(root, "POST", "/api/proxies", json!({"text":"1.1.1.1:8080"})).0,
            200
        );
        dash(root)
            .patch(
                id,
                |mut current| {
                    current["notes"] = "edited first".into();
                    Ok((current, Default::default()))
                },
                None,
                None,
                1001.0,
            )
            .expect("note edit");
        let (status, _body) = http(
            root,
            "POST",
            "/api/proxies/assign",
            json!({"profileIds":[id]}),
        );
        assert_eq!(status, 200, "HTTP response status mismatch");
        let final_row = dash(root).get(id).expect("get").expect("profile");
        assert_eq!(final_row["proxy"]["host"], "1.1.1.1");
        assert_eq!(final_row["notes"], "edited first");
    }
);
source_test!(
    an_impossible_warmup_duration_is_refused_everywhere_case_008,
    "tests/test_compound_atomicity.py::test_an_impossible_warmup_duration_is_refused_everywhere[inf]",
    {
        let temp = temp();
        let root = temp.path();
        let minutes = Value::String("Infinity".into());
        assert!(numera_core::browser::warmup_policy::validate(&minutes, None, None).is_err());
        let p = create(root, "w");
        let (status, _body) = http(
            root,
            "POST",
            &format!("/api/profiles/{}/warmup", p["id"].as_str().expect("id")),
            json!({"minutes":minutes}),
        );
        assert_eq!(status, 400, "HTTP response status mismatch");
    }
);
source_test!(
    an_impossible_warmup_duration_is_refused_everywhere_case_009,
    "tests/test_compound_atomicity.py::test_an_impossible_warmup_duration_is_refused_everywhere[nan]",
    {
        let temp = temp();
        let root = temp.path();
        let minutes = Value::String("NaN".into());
        assert!(numera_core::browser::warmup_policy::validate(&minutes, None, None).is_err());
        let p = create(root, "w");
        let (status, _body) = http(
            root,
            "POST",
            &format!("/api/profiles/{}/warmup", p["id"].as_str().expect("id")),
            json!({"minutes":minutes}),
        );
        assert_eq!(status, 400, "HTTP response status mismatch");
    }
);
source_test!(
    an_impossible_warmup_duration_is_refused_everywhere_case_010,
    "tests/test_compound_atomicity.py::test_an_impossible_warmup_duration_is_refused_everywhere[-1]",
    {
        let temp = temp();
        let root = temp.path();
        let minutes = Value::String("-1".into());
        assert!(numera_core::browser::warmup_policy::validate(&minutes, None, None).is_err());
        let p = create(root, "w");
        let (status, _body) = http(
            root,
            "POST",
            &format!("/api/profiles/{}/warmup", p["id"].as_str().expect("id")),
            json!({"minutes":minutes}),
        );
        assert_eq!(status, 400, "HTTP response status mismatch");
    }
);
source_test!(
    an_impossible_warmup_duration_is_refused_everywhere_case_011,
    "tests/test_compound_atomicity.py::test_an_impossible_warmup_duration_is_refused_everywhere[0]",
    {
        let temp = temp();
        let root = temp.path();
        let minutes = Value::String("0".into());
        assert!(numera_core::browser::warmup_policy::validate(&minutes, None, None).is_err());
        let p = create(root, "w");
        let (status, _body) = http(
            root,
            "POST",
            &format!("/api/profiles/{}/warmup", p["id"].as_str().expect("id")),
            json!({"minutes":minutes}),
        );
        assert_eq!(status, 400, "HTTP response status mismatch");
    }
);
source_test!(
    an_impossible_warmup_duration_is_refused_everywhere_case_012,
    "tests/test_compound_atomicity.py::test_an_impossible_warmup_duration_is_refused_everywhere[1000000]",
    {
        let temp = temp();
        let root = temp.path();
        let minutes = Value::String("1000000".into());
        assert!(numera_core::browser::warmup_policy::validate(&minutes, None, None).is_err());
        let p = create(root, "w");
        let (status, _body) = http(
            root,
            "POST",
            &format!("/api/profiles/{}/warmup", p["id"].as_str().expect("id")),
            json!({"minutes":minutes}),
        );
        assert_eq!(status, 400, "HTTP response status mismatch");
    }
);
source_test!(
    an_impossible_warmup_duration_is_refused_everywhere_case_013,
    "tests/test_compound_atomicity.py::test_an_impossible_warmup_duration_is_refused_everywhere[x]",
    {
        let temp = temp();
        let root = temp.path();
        let minutes = Value::String("x".into());
        assert!(numera_core::browser::warmup_policy::validate(&minutes, None, None).is_err());
        let p = create(root, "w");
        let (status, _body) = http(
            root,
            "POST",
            &format!("/api/profiles/{}/warmup", p["id"].as_str().expect("id")),
            json!({"minutes":minutes}),
        );
        assert_eq!(status, 400, "HTTP response status mismatch");
    }
);
source_test!(
    an_unknown_preset_is_refused_before_a_browser_starts,
    "tests/test_compound_atomicity.py::test_an_unknown_preset_is_refused_before_a_browser_starts",
    {
        let temp = temp();
        let root = temp.path();
        assert!(
            numera_core::browser::warmup_policy::validate(
                &json!(5),
                Some(&json!("not-a-preset")),
                None
            )
            .is_err()
        );
        let p = create(root, "w2");
        let (status, _body) = http(
            root,
            "POST",
            &format!("/api/profiles/{}/warmup", p["id"].as_str().expect("id")),
            json!({"minutes":5,"preset":"not-a-preset"}),
        );
        assert_eq!(status, 400, "HTTP response status mismatch");
        assert!(!root.join("sessions").exists() || json_files(root, "sessions").is_empty());
    }
);
source_test!(
    a_schedule_cannot_store_a_duration_a_run_would_refuse,
    "tests/test_compound_atomicity.py::test_a_schedule_cannot_store_a_duration_a_run_would_refuse",
    {
        let temp = temp();
        let root = temp.path();
        let schedules = ScheduleStore::new(root.to_owned());
        assert!(
            schedules
                .add("p", json!({"everyHours":1,"minutes":"Infinity"}), None)
                .is_err()
        );
        assert!(
            schedules
                .add(
                    "p",
                    json!({"everyHours":1,"minutes":5,"preset":"nope"}),
                    None
                )
                .is_err()
        );
        assert!(schedules.list().expect("schedules").is_empty());
    }
);
source_test!(
    two_processes_racing_one_due_occurrence_produce_one_claim,
    "tests/test_compound_atomicity.py::test_two_processes_racing_one_due_occurrence_produce_one_claim",
    {
        let temp = temp();
        let root = temp.path();
        let sched = ScheduleStore::new(root.to_owned());
        let row = sched
            .add("p1", json!({"everyHours":1,"minutes":5}), Some(0.0))
            .expect("schedule");
        let extra = row["id"].as_str().expect("id").to_owned();
        let destination = root.to_owned();
        let test = "compound_atomicity::two_processes_racing_one_due_occurrence_produce_one_claim";
        let a = process::start(test, &destination, "one", "claim", &extra);
        let b = process::start(test, &destination, "two", "claim", &extra);
        for who in ["one", "two"] {
            process::reached(&destination, who, "ready");
        }
        for who in ["one", "two"] {
            process::release(&destination, who);
        }
        process::finish(a);
        process::finish(b);
        assert_eq!(
            ["one", "two"]
                .iter()
                .filter(
                    |who| raw(&destination, &format!("worker-{who}-result.json"))["won"] == true
                )
                .count(),
            1
        );
    }
);
source_test!(
    two_first_runs_import_once,
    "tests/test_compound_atomicity.py::test_two_first_runs_import_once",
    {
        let temp = temp();
        let root = temp.path();
        let src = root.join("legacy");
        let mut original = profile("old", 1);
        original.id = "aaaaaaaaaaaa".into();
        write(
            &src,
            "profiles/aaaaaaaaaaaa.json",
            serde_json::to_value(original).expect("valid legacy profile"),
        );
        let extra = src.to_string_lossy().into_owned();
        let destination = root.join("dest");
        fs::create_dir(&destination).expect("destination");
        let test = "compound_atomicity::two_first_runs_import_once";
        let a = process::start(test, &destination, "one", "first-run", &extra);
        let b = process::start(test, &destination, "two", "first-run", &extra);
        for who in ["one", "two"] {
            process::reached(&destination, who, "ready");
        }
        for who in ["one", "two"] {
            process::release(&destination, who);
        }
        process::finish(a);
        process::finish(b);
        assert_eq!(
            ["one", "two"]
                .iter()
                .filter(
                    |who| raw(&destination, &format!("worker-{who}-result.json"))["won"] == true
                )
                .count(),
            1
        );
        assert_eq!(json_files(&destination, "profiles").len(), 1);
    }
);
