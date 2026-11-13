use super::*;
source_test!(
    c000_a_minimal_batch_entry_is_normalised_on_disk,
    "tests/test_write_contracts.py::test_a_minimal_batch_entry_is_normalised_on_disk",
    {
        batch_controls("normal");
    }
);
source_test!(
    c000_a_batch_profile_survives_the_round_trip,
    "tests/test_write_contracts.py::test_a_batch_profile_survives_the_round_trip",
    {
        batch_controls("listing");
    }
);
source_test!(
    c000_two_minimal_entries_get_different_identities,
    "tests/test_write_contracts.py::test_two_minimal_entries_get_different_identities",
    {
        batch_controls("identity");
    }
);
source_test!(
    c000_one_bad_entry_still_writes_none_of_them,
    "tests/test_write_contracts.py::test_one_bad_entry_still_writes_none_of_them",
    {
        refused_field("batch", "tags", json!("not-a-list"));
    }
);
source_test!(
    c000_deleting_a_profile_that_does_not_exist_changes_nothing,
    "tests/test_write_contracts.py::test_deleting_a_profile_that_does_not_exist_changes_nothing",
    {
        let tmp = common::TempDirectory::new("contract-missing-delete").expect("root");
        let p = repo(tmp.path())
            .save(profile("engine-only"), 1001.0)
            .expect("save");
        let s = http(
            tmp.path(),
            "DELETE",
            &format!("/api/profiles/{}", p.id),
            Value::Null,
        )
        .0;
        assert!([200, 404].contains(&s));
        let before = snapshot(tmp.path());
        assert_eq!(
            http(
                tmp.path(),
                "DELETE",
                "/api/profiles/deadbeef1234",
                Value::Null
            )
            .0,
            404
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c000_a_failed_engine_delete_leaves_both_copies,
    "tests/test_write_contracts.py::test_a_failed_engine_delete_leaves_both_copies",
    {
        delete_fault("engine");
    }
);
source_test!(
    c000_the_session_is_detached_before_any_metadata_is_touched,
    "tests/test_write_contracts.py::test_the_session_is_detached_before_any_metadata_is_touched",
    {
        delete_order();
    }
);
source_test!(
    c000_a_held_session_fails_the_delete_without_touching_the_records,
    "tests/test_write_contracts.py::test_a_held_session_fails_the_delete_without_touching_the_records",
    {
        delete_fault("detach");
    }
);
source_test!(
    c000_a_successful_delete_removes_both_copies,
    "tests/test_write_contracts.py::test_a_successful_delete_removes_both_copies",
    {
        let tmp = common::TempDirectory::new("contract-delete").expect("root");
        let p = made(tmp.path(), json!({"name":"gone"}));
        assert_eq!(
            http(
                tmp.path(),
                "DELETE",
                &format!("/api/profiles/{}", p["id"].as_str().expect("id")),
                Value::Null
            )
            .0,
            200
        );
        assert!(ids(tmp.path(), "dashboard").is_empty());
        assert!(ids(tmp.path(), "profiles").is_empty());
        assert_eq!(
            http(tmp.path(), "GET", "/api/profiles", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c000_two_settings_saved_at_once_both_survive,
    "tests/test_write_contracts.py::test_two_settings_saved_at_once_both_survive",
    {
        let tmp = common::TempDirectory::new("contract-settings-race").expect("root");
        thread::scope(|s| {
            let a = s.spawn(|| {
                for _ in 0..40 {
                    config(tmp.path())
                        .set("default_engine", json!("camoufox"))
                        .expect("engine change");
                }
            });
            let b = s.spawn(|| {
                for _ in 0..40 {
                    config(tmp.path())
                        .set("headless_launch", json!(true))
                        .expect("headless change");
                }
            });
            a.join().expect("engine writer");
            b.join().expect("headless writer");
        });
        let cfg = config(tmp.path()).read().expect("config");
        assert_eq!(cfg["default_engine"], "camoufox");
        assert_eq!(cfg["headless_launch"], true);
    }
);
source_test!(
    c000_a_setting_does_not_disturb_the_others,
    "tests/test_write_contracts.py::test_a_setting_does_not_disturb_the_others",
    {
        let tmp = common::TempDirectory::new("contract-kept-settings").expect("root");
        let mut kept = Vec::new();
        for name in ["one", "two"] {
            let path = tmp.path().join(name);
            fs::create_dir(&path).expect("extension directory");
            fs::write(
                path.join("manifest.json"),
                br#"{"manifest_version":3,"name":"x","version":"1.0"}"#,
            )
            .expect("extension manifest");
            kept.push(path.to_string_lossy().into_owned());
        }
        config(tmp.path())
            .set_default_extensions(kept.clone())
            .expect("extensions");
        config(tmp.path())
            .set_default_engine(json!("playwright"))
            .expect("engine");
        config(tmp.path())
            .set_headless_launch(&json!(true))
            .expect("headless");
        let cfg = config(tmp.path()).read().expect("config");
        assert_eq!(cfg["default_extensions"], json!(kept));
        assert_eq!(cfg["default_engine"], "playwright");
        assert_eq!(cfg["headless_launch"], true);
    }
);
source_test!(
    c000_a_rename_that_fails_part_way_moves_nobody,
    "tests/test_write_contracts.py::test_a_rename_that_fails_part_way_moves_nobody",
    {
        folder_change("fail");
    }
);
source_test!(
    c000_a_rename_that_succeeds_moves_every_profile,
    "tests/test_write_contracts.py::test_a_rename_that_succeeds_moves_every_profile",
    {
        folder_change("rename");
    }
);
source_test!(
    c000_deleting_a_folder_reassigns_every_profile,
    "tests/test_write_contracts.py::test_deleting_a_folder_reassigns_every_profile",
    {
        folder_change("delete");
    }
);
source_test!(
    c000_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body0-a string is not a list of proxies]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                json!("not-a-list"),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c001_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body1-no host]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                    "nope".into(),
                    json!(1),
                )]))]),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c002_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body2-an empty host]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                    ("host".into(), json!("")),
                    ("port".into(), json!("80")),
                ]))]),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c003_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body3-a port that is not a number]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                    ("host".into(), json!("h")),
                    ("port".into(), json!("notaport")),
                ]))]),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c004_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body4-a port outside the range]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                    ("host".into(), json!("h")),
                    ("port".into(), json!("0")),
                ]))]),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c005_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body5-a port outside the range]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                    ("host".into(), json!("h")),
                    ("port".into(), json!("70000")),
                ]))]),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c006_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body6-an unsupported type]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                    ("type".into(), json!("GOPHER")),
                    ("host".into(), json!("h")),
                    ("port".into(), json!("80")),
                ]))]),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c007_malformed_structured_input_is_a_4xx_not_a_500,
    "tests/test_write_contracts.py::test_malformed_structured_input_is_a_4xx_not_a_500[body7-an entry that is not an object]",
    {
        let tmp = common::TempDirectory::new("contract-proxy-list").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/proxies",
            Value::Object(serde_json::Map::from_iter([(
                "proxies".into(),
                Value::Array(vec![json!("just a string")]),
            )])),
        );
        assert!([400, 422].contains(&s), "{v}");
        assert_eq!(
            http(tmp.path(), "GET", "/api/proxies", Value::Null).1,
            json!([])
        );
    }
);
source_test!(
    c000_good_structured_input_still_round_trips,
    "tests/test_write_contracts.py::test_good_structured_input_still_round_trips[entry0]",
    {
        proxy_roundtrip(Value::Object(serde_json::Map::from_iter([
            ("type".into(), json!("SOCKS4")),
            ("host".into(), json!("2001:db8::1")),
            ("port".into(), json!("1080")),
            ("user".into(), json!("u")),
        ])));
    }
);
source_test!(
    c001_good_structured_input_still_round_trips,
    "tests/test_write_contracts.py::test_good_structured_input_still_round_trips[entry1]",
    {
        proxy_roundtrip(Value::Object(serde_json::Map::from_iter([
            ("type".into(), json!("SOCKS5")),
            ("host".into(), json!("1.2.3.4")),
            ("port".into(), json!("1080")),
            ("user".into(), json!("u")),
            ("pass".into(), json!("p")),
        ])));
    }
);
source_test!(
    c002_good_structured_input_still_round_trips,
    "tests/test_write_contracts.py::test_good_structured_input_still_round_trips[entry2]",
    {
        proxy_roundtrip(Value::Object(serde_json::Map::from_iter([
            ("type".into(), json!("HTTPS")),
            ("host".into(), json!("example.com")),
            ("port".into(), json!("8443")),
            ("country".into(), json!("de")),
        ])));
    }
);
source_test!(
    c003_good_structured_input_still_round_trips,
    "tests/test_write_contracts.py::test_good_structured_input_still_round_trips[entry3]",
    {
        proxy_roundtrip(Value::Object(serde_json::Map::from_iter([
            ("host".into(), json!("1.2.3.4")),
            ("port".into(), json!(8080)),
        ])));
    }
);
source_test!(
    c000_text_and_structured_input_agree,
    "tests/test_write_contracts.py::test_text_and_structured_input_agree",
    {
        let tmp = common::TempDirectory::new("contract-proxy-dedup").expect("root");
        assert_eq!(
            http(
                tmp.path(),
                "POST",
                "/api/proxies",
                json!({"text":"socks5://u:p@1.2.3.4:1080"})
            )
            .0,
            200
        );
        let first = http(tmp.path(), "GET", "/api/proxies", Value::Null).1;
        assert_eq!(http(tmp.path(),"POST","/api/proxies",json!({"proxies":[{"type":"SOCKS5","host":"1.2.3.4","port":"1080","user":"u","pass":"p"}]})).0,200);
        let entries = http(tmp.path(), "GET", "/api/proxies", Value::Null).1;
        assert_eq!(entries.as_array().expect("proxies").len(), 1);
        assert_eq!(entries[0], first[0]);
    }
);
