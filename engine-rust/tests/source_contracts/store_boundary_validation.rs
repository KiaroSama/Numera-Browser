use super::*;
source_test!(
    c000_an_invalid_later_field_leaves_the_config_untouched,
    "tests/test_store_boundary_validation.py::test_an_invalid_later_field_leaves_the_config_untouched",
    {
        let tmp = common::TempDirectory::new("contract-config-validation").expect("root");
        assert_eq!(
            http(
                tmp.path(),
                "PUT",
                "/api/config",
                json!({"default_engine":"playwright"})
            )
            .0,
            200
        );
        let before = snapshot(tmp.path());
        let (s, v) = http(
            tmp.path(),
            "PUT",
            "/api/config",
            json!({"default_engine":"camoufox","default_extensions":[tmp.path().join("no-such-extension").to_string_lossy()]}),
        );
        assert_eq!(s, 400, "{v}");
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c000_a_string_is_not_a_boolean,
    "tests/test_store_boundary_validation.py::test_a_string_is_not_a_boolean",
    {
        let tmp = common::TempDirectory::new("contract-config-bool").expect("root");
        let (s, v) = http(
            tmp.path(),
            "PUT",
            "/api/config",
            json!({"headless_launch":"false"}),
        );
        assert_eq!(s, 400);
        assert!(
            v["detail"]
                .as_str()
                .expect("detail")
                .contains("true or false")
        );
        assert!(
            !config(tmp.path())
                .read()
                .expect("config")
                .contains_key("headless_launch")
        );
    }
);
source_test!(
    c000_an_unknown_config_key_is_refused,
    "tests/test_store_boundary_validation.py::test_an_unknown_config_key_is_refused",
    {
        let tmp = common::TempDirectory::new("contract-config-key").expect("root");
        let (s, v) = http(
            tmp.path(),
            "PUT",
            "/api/config",
            json!({"root":"/somewhere/else"}),
        );
        assert_eq!(s, 400);
        assert!(
            v["detail"]
                .as_str()
                .expect("detail")
                .contains("unknown config key")
        );
    }
);
source_test!(
    c000_a_valid_multi_field_request_still_applies,
    "tests/test_store_boundary_validation.py::test_a_valid_multi_field_request_still_applies",
    {
        let tmp = common::TempDirectory::new("contract-config-success").expect("root");
        assert_eq!(
            http(
                tmp.path(),
                "PUT",
                "/api/config",
                json!({"default_engine":"camoufox","headless_launch":true})
            )
            .0,
            200
        );
        let cfg = config(tmp.path()).read().expect("config");
        assert_eq!(cfg["default_engine"], "camoufox");
        assert_eq!(cfg["headless_launch"], true);
    }
);
source_test!(
    c000_the_store_refuses_a_schedule_it_cannot_serialise,
    "tests/test_store_boundary_validation.py::test_the_store_refuses_a_schedule_it_cannot_serialise[nan]",
    {
        let tmp = common::TempDirectory::new("contract-schedule").expect("root");
        let mut request = json!({"everyHours":6});
        request["minutes"] = number("NaN");
        assert!(
            numera_core::storage::schedules::ScheduleStore::new(tmp.path().to_owned())
                .add("p", request, Some(1000.0))
                .is_err()
        );
        assert!(ids(tmp.path(), "profiles").is_empty());
        assert!(!tmp.path().join("schedules.json").exists());
    }
);
source_test!(
    c001_the_store_refuses_a_schedule_it_cannot_serialise,
    "tests/test_store_boundary_validation.py::test_the_store_refuses_a_schedule_it_cannot_serialise[inf]",
    {
        let tmp = common::TempDirectory::new("contract-schedule").expect("root");
        let mut request = json!({"everyHours":6});
        request["minutes"] = number("Infinity");
        assert!(
            numera_core::storage::schedules::ScheduleStore::new(tmp.path().to_owned())
                .add("p", request, Some(1000.0))
                .is_err()
        );
        assert!(ids(tmp.path(), "profiles").is_empty());
        assert!(!tmp.path().join("schedules.json").exists());
    }
);
source_test!(
    c002_the_store_refuses_a_schedule_it_cannot_serialise,
    "tests/test_store_boundary_validation.py::test_the_store_refuses_a_schedule_it_cannot_serialise[-inf]",
    {
        let tmp = common::TempDirectory::new("contract-schedule").expect("root");
        let mut request = json!({"everyHours":6});
        request["minutes"] = number("-Infinity");
        assert!(
            numera_core::storage::schedules::ScheduleStore::new(tmp.path().to_owned())
                .add("p", request, Some(1000.0))
                .is_err()
        );
        assert!(ids(tmp.path(), "profiles").is_empty());
        assert!(!tmp.path().join("schedules.json").exists());
    }
);
source_test!(
    c003_the_store_refuses_a_schedule_it_cannot_serialise,
    "tests/test_store_boundary_validation.py::test_the_store_refuses_a_schedule_it_cannot_serialise[-5]",
    {
        let tmp = common::TempDirectory::new("contract-schedule").expect("root");
        let mut request = json!({"everyHours":6});
        request["minutes"] = json!(-5);
        assert!(
            numera_core::storage::schedules::ScheduleStore::new(tmp.path().to_owned())
                .add("p", request, Some(1000.0))
                .is_err()
        );
        assert!(ids(tmp.path(), "profiles").is_empty());
        assert!(!tmp.path().join("schedules.json").exists());
    }
);
source_test!(
    c004_the_store_refuses_a_schedule_it_cannot_serialise,
    "tests/test_store_boundary_validation.py::test_the_store_refuses_a_schedule_it_cannot_serialise[0]",
    {
        let tmp = common::TempDirectory::new("contract-schedule").expect("root");
        let mut request = json!({"everyHours":6});
        request["minutes"] = json!(0);
        assert!(
            numera_core::storage::schedules::ScheduleStore::new(tmp.path().to_owned())
                .add("p", request, Some(1000.0))
                .is_err()
        );
        assert!(ids(tmp.path(), "profiles").is_empty());
        assert!(!tmp.path().join("schedules.json").exists());
    }
);
source_test!(
    c000_no_non_finite_number_reaches_the_file,
    "tests/test_store_boundary_validation.py::test_no_non_finite_number_reaches_the_file",
    {
        let tmp = common::TempDirectory::new("contract-schedule-finite").expect("root");
        let store = numera_core::storage::schedules::ScheduleStore::new(tmp.path().to_owned());
        store
            .add("p", json!({"everyHours":6,"minutes":5}), Some(1000.0))
            .expect("schedule");
        for row in store.list().expect("schedules") {
            for key in ["everyHours", "minutes"] {
                assert!(row[key].as_f64().expect("schedule number").is_finite());
            }
        }
    }
);
source_test!(
    c000_the_cli_cannot_write_a_non_finite_schedule,
    "tests/test_store_boundary_validation.py::test_the_cli_cannot_write_a_non_finite_schedule",
    {
        cli_bad_schedule();
    }
);
source_test!(
    c000_a_small_interval_is_still_clamped_not_refused,
    "tests/test_store_boundary_validation.py::test_a_small_interval_is_still_clamped_not_refused",
    {
        let tmp = common::TempDirectory::new("contract-schedule-small").expect("root");
        let row = numera_core::storage::schedules::ScheduleStore::new(tmp.path().to_owned())
            .add("p", json!({"everyHours":0.001,"minutes":5}), Some(1000.0))
            .expect("small interval");
        assert!(row["everyHours"].as_f64().expect("interval") > 0.0);
    }
);
source_test!(
    c000_a_structured_create_keeps_the_rotate_url,
    "tests/test_store_boundary_validation.py::test_a_structured_create_keeps_the_rotate_url",
    {
        let tmp = common::TempDirectory::new("contract-rotation-url").expect("root");
        let url = "https://api.example.test/rotate?key=abc";
        assert_eq!(
            http(
                tmp.path(),
                "POST",
                "/api/proxies",
                json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080","rotateUrl":url}]})
            )
            .0,
            200
        );
        let rows = http(tmp.path(), "GET", "/api/proxies", Value::Null).1;
        assert_eq!(rows[0]["rotateUrl"], url);
    }
);
source_test!(
    c000_a_malformed_rotate_url_is_refused_not_a_traceback,
    "tests/test_store_boundary_validation.py::test_a_malformed_rotate_url_is_refused_not_a_traceback[ftp://example.test/rotate]",
    {
        let tmp = common::TempDirectory::new("contract-rotate").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080","rotateUrl":json!("ftp://example.test/rotate")}]}),
        );
        assert!((400..500).contains(&s), "{v}");
    }
);
source_test!(
    c001_a_malformed_rotate_url_is_refused_not_a_traceback,
    "tests/test_store_boundary_validation.py::test_a_malformed_rotate_url_is_refused_not_a_traceback[http://example.test:99999/rotate]",
    {
        let tmp = common::TempDirectory::new("contract-rotate").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080","rotateUrl":json!("http://example.test:99999/rotate")}]}),
        );
        assert!((400..500).contains(&s), "{v}");
    }
);
source_test!(
    c002_a_malformed_rotate_url_is_refused_not_a_traceback,
    "tests/test_store_boundary_validation.py::test_a_malformed_rotate_url_is_refused_not_a_traceback[not-a-url]",
    {
        let tmp = common::TempDirectory::new("contract-rotate").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080","rotateUrl":json!("not-a-url")}]}),
        );
        assert!((400..500).contains(&s), "{v}");
    }
);
source_test!(
    c003_a_malformed_rotate_url_is_refused_not_a_traceback,
    "tests/test_store_boundary_validation.py::test_a_malformed_rotate_url_is_refused_not_a_traceback[http:///no-host]",
    {
        let tmp = common::TempDirectory::new("contract-rotate").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080","rotateUrl":json!("http:///no-host")}]}),
        );
        assert!((400..500).contains(&s), "{v}");
    }
);
source_test!(
    c000_the_canonical_shape_round_trips_through_normalize,
    "tests/test_store_boundary_validation.py::test_the_canonical_shape_round_trips_through_normalize",
    {
        let row = json!({"type":"HTTP","host":"1.2.3.4","port":"8080","rotateUrl":"https://api.example.test/rotate"});
        let once = numera_core::pool::normalize(&row).expect("normalize");
        assert_eq!(once["rotateUrl"], row["rotateUrl"]);
        assert_eq!(
            numera_core::pool::normalize(&once).expect("normalize twice"),
            once
        );
    }
);
