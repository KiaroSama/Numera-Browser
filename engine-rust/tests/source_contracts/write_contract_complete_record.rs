use super::*;
source_test!(
    c000_a_fingerprint_that_is_not_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_fingerprint_that_is_not_one_is_refused_by_name[none]",
    {
        refused_record("fingerprint", Value::Null, "");
    }
);
source_test!(
    c001_a_fingerprint_that_is_not_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_fingerprint_that_is_not_one_is_refused_by_name[list]",
    {
        refused_record("fingerprint", Value::Array(vec![]), "");
    }
);
source_test!(
    c002_a_fingerprint_that_is_not_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_fingerprint_that_is_not_one_is_refused_by_name[int]",
    {
        refused_record("fingerprint", json!(123), "");
    }
);
source_test!(
    c003_a_fingerprint_that_is_not_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_fingerprint_that_is_not_one_is_refused_by_name[str]",
    {
        refused_record("fingerprint", json!("x"), "");
    }
);
source_test!(
    c004_a_fingerprint_that_is_not_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_fingerprint_that_is_not_one_is_refused_by_name[float]",
    {
        refused_record("fingerprint", json!(1.5), "");
    }
);
source_test!(
    c005_a_fingerprint_that_is_not_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_fingerprint_that_is_not_one_is_refused_by_name[bool]",
    {
        refused_record("fingerprint", json!(true), "");
    }
);
source_test!(
    c006_a_fingerprint_that_is_not_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_fingerprint_that_is_not_one_is_refused_by_name[tuple]",
    {
        refused_record("fingerprint", Value::Array(vec![json!("a")]), "");
    }
);
source_test!(
    c000_an_empty_fingerprint_mapping_is_refused_field_by_field,
    "tests/test_write_contract_complete_record.py::test_an_empty_fingerprint_mapping_is_refused_field_by_field",
    {
        let mut p = record();
        p["fingerprint"] = json!({});
        let errors = problems(&p, None);
        assert!(errors.iter().any(|p| p.contains("fingerprint.os")));
        assert!(errors.iter().any(|p| p.contains("fingerprint.seed")));
    }
);
source_test!(
    c000_something_that_is_not_a_profile_at_all_is_named_as_such,
    "tests/test_write_contract_complete_record.py::test_something_that_is_not_a_profile_at_all_is_named_as_such",
    {
        assert_eq!(
            problems(&json!(123), None),
            ["profile: a profile must be a record or a patch, not int. Nothing was written."]
        );
    }
);
source_test!(
    c000_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[screen_width-inf-finite]",
    {
        refused_record("fingerprint.screen_width", number("Infinity"), "finite");
    }
);
source_test!(
    c001_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[screen_height--inf-finite]",
    {
        refused_record("fingerprint.screen_height", number("-Infinity"), "finite");
    }
);
source_test!(
    c002_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[viewport_height--1-between 1 and 6553]",
    {
        refused_record(
            "fingerprint.viewport_height",
            json!(-1),
            "between 1 and 65536",
        );
    }
);
source_test!(
    c003_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[viewport_width-0-between 1 and 6553]",
    {
        refused_record(
            "fingerprint.viewport_width",
            json!(0),
            "between 1 and 65536",
        );
    }
);
source_test!(
    c004_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[timezone-[]-must be text]",
    {
        refused_record("fingerprint.timezone", Value::Array(vec![]), "must be text");
    }
);
source_test!(
    c005_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[user_agent-123-must be text]",
    {
        refused_record("fingerprint.user_agent", json!(123), "must be text");
    }
);
source_test!(
    c006_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[seed-None-required]",
    {
        refused_record("fingerprint.seed", Value::Null, "required");
    }
);
source_test!(
    c007_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[languages-None-must be a list]",
    {
        refused_record("fingerprint.languages", Value::Null, "must be a list");
    }
);
source_test!(
    c008_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[os-None-must be text]",
    {
        refused_record("fingerprint.os", Value::Null, "must be text");
    }
);
source_test!(
    c009_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[chrome_version-None-must be text]",
    {
        refused_record("fingerprint.chrome_version", Value::Null, "must be text");
    }
);
source_test!(
    c010_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[webrtc-[]-must be text]",
    {
        refused_record("fingerprint.webrtc", Value::Array(vec![]), "must be text");
    }
);
source_test!(
    c011_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[canvas-7-must be text]",
    {
        refused_record("fingerprint.canvas", json!(7), "must be text");
    }
);
source_test!(
    c012_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[locale-b'en'-must be text]",
    {
        refused_record(
            "fingerprint.locale",
            Value::Array(vec![json!(101), json!(110)]),
            "must be text",
        );
    }
);
source_test!(
    c013_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[is_mobile-2-true or false]",
    {
        refused_record("fingerprint.is_mobile", json!(2), "true or false");
    }
);
source_test!(
    c014_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[do_not_track-1-true or false]",
    {
        refused_record("fingerprint.do_not_track", json!(1), "true or false");
    }
);
source_test!(
    c015_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[device_memory-3.5-whole number]",
    {
        refused_record("fingerprint.device_memory", json!(3.5), "whole number");
    }
);
source_test!(
    c016_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[hardware_concurren-2.5-whole number]",
    {
        refused_record(
            "fingerprint.hardware_concurrency",
            json!(2.5),
            "whole number",
        );
    }
);
source_test!(
    c017_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[max_touch_points--3-between 0 and 32]",
    {
        refused_record(
            "fingerprint.max_touch_points",
            json!(-3),
            "between 0 and 32",
        );
    }
);
source_test!(
    c018_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[os_version-None-must be text]",
    {
        refused_record("fingerprint.os_version", Value::Null, "must be text");
    }
);
source_test!(
    c019_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[device_model-4-must be text]",
    {
        refused_record("fingerprint.device_model", json!(4), "must be text");
    }
);
source_test!(
    c020_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[seed-inf-finite]",
    {
        refused_record("fingerprint.seed", number("Infinity"), "finite");
    }
);
source_test!(
    c021_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[seed-nan-finite]",
    {
        refused_record("fingerprint.seed", number("NaN"), "finite");
    }
);
source_test!(
    c022_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[seed-100000000000000000-between]",
    {
        refused_record("fingerprint.seed",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"),"between");
    }
);
source_test!(
    c023_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[hardware_concurren-True-must be a number]",
    {
        refused_record(
            "fingerprint.hardware_concurrency",
            json!(true),
            "must be a number",
        );
    }
);
source_test!(
    c024_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[device_pixel_ratio-nan-finite]",
    {
        refused_record("fingerprint.device_pixel_ratio", number("NaN"), "finite");
    }
);
source_test!(
    c025_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[fonts-[1, 2]-every entry]",
    {
        refused_record(
            "fingerprint.fonts",
            Value::Array(vec![json!(1), json!(2)]),
            "every entry",
        );
    }
);
source_test!(
    c026_the_core_write_path_refuses_the_field_and_writes_nothing,
    "tests/test_write_contract_complete_record.py::test_the_core_write_path_refuses_the_field_and_writes_nothing[languages-['en', 3]-every entry]",
    {
        refused_record(
            "fingerprint.languages",
            Value::Array(vec![json!("en"), json!(3)]),
            "every entry",
        );
    }
);
source_test!(
    c000_a_non_finite_dimension_never_becomes_a_non_json_token,
    "tests/test_write_contract_complete_record.py::test_a_non_finite_dimension_never_becomes_a_non_json_token",
    {
        let tmp = common::TempDirectory::new("contract-strict-json").expect("root");
        repo(tmp.path())
            .save(profile("healthy"), 1001.0)
            .expect("save healthy");
        let mut p = record();
        p["fingerprint"]["screen_width"] = number("Infinity");
        assert!(import_file(tmp.path(), p).is_err());
        for (_, bytes) in snapshot(tmp.path()) {
            serde_json::from_slice::<Value>(&bytes).expect("strict stored JSON");
        }
    }
);
source_test!(
    c000_a_malformed_bookmark_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_malformed_bookmark_is_refused_by_name[str]",
    {
        refused_record("bookmarks", json!("https://x.test"), "must be a list");
    }
);
source_test!(
    c001_a_malformed_bookmark_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_malformed_bookmark_is_refused_by_name[int-entry]",
    {
        refused_record("bookmarks", Value::Array(vec![json!(3)]), "{'name', 'url'}");
    }
);
source_test!(
    c002_a_malformed_bookmark_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_malformed_bookmark_is_refused_by_name[str-entry]",
    {
        refused_record(
            "bookmarks",
            Value::Array(vec![json!("https://x.test")]),
            "{'name', 'url'}",
        );
    }
);
source_test!(
    c003_a_malformed_bookmark_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_malformed_bookmark_is_refused_by_name[numeric-name]",
    {
        refused_record(
            "bookmarks",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!(1)),
                ("url".into(), json!("https://x.test")),
            ]))]),
            "bookmarks[].name",
        );
    }
);
source_test!(
    c004_a_malformed_bookmark_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_malformed_bookmark_is_refused_by_name[no-url]",
    {
        refused_record(
            "bookmarks",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                "name".into(),
                json!("ok"),
            )]))]),
            "bookmarks[].url",
        );
    }
);
source_test!(
    c005_a_malformed_bookmark_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_malformed_bookmark_is_refused_by_name[numeric-url]",
    {
        refused_record(
            "bookmarks",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                ("url".into(), json!(7)),
            ]))]),
            "bookmarks[].url",
        );
    }
);
source_test!(
    c000_a_valid_bookmark_list_still_saves,
    "tests/test_write_contract_complete_record.py::test_a_valid_bookmark_list_still_saves",
    {
        let tmp = common::TempDirectory::new("contract-bookmarks").expect("root");
        let mut p = profile("bookmarks");
        p.bookmarks = vec![
            json!({"name":"Docs","url":"https://docs.test"}),
            json!({"name":"","url":"https://x.test"}),
        ];
        let id = p.id.clone();
        repo(tmp.path()).save(p, 1001.0).expect("valid bookmarks");
        assert_eq!(
            read(tmp.path(), "profiles", &id)["bookmarks"],
            json!([{"name":"Docs","url":"https://docs.test"},{"name":"","url":"https://x.test"}])
        );
    }
);
source_test!(
    c000_a_proxy_that_cannot_be_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_proxy_that_cannot_be_one_is_refused_by_name[int]",
    {
        refused_record("proxy", json!(123), "");
    }
);
source_test!(
    c001_a_proxy_that_cannot_be_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_proxy_that_cannot_be_one_is_refused_by_name[list]",
    {
        refused_record("proxy", Value::Array(vec![]), "");
    }
);
source_test!(
    c002_a_proxy_that_cannot_be_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_proxy_that_cannot_be_one_is_refused_by_name[numeric-server]",
    {
        refused_record(
            "proxy",
            Value::Object(serde_json::Map::from_iter([("server".into(), json!(1))])),
            "",
        );
    }
);
source_test!(
    c003_a_proxy_that_cannot_be_one_is_refused_by_name,
    "tests/test_write_contract_complete_record.py::test_a_proxy_that_cannot_be_one_is_refused_by_name[float]",
    {
        refused_record("proxy", json!(1.5), "");
    }
);
source_test!(
    c000_a_refusal_never_quotes_the_password_back,
    "tests/test_write_contract_complete_record.py::test_a_refusal_never_quotes_the_password_back",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","password":["hunter2-the-real-one"]});
        let errors = problems(&p, None).join(";");
        assert!(errors.contains("proxy.password"));
        assert!(!errors.contains("hunter2"));
    }
);
source_test!(
    c000_validating_never_reassigns_the_password,
    "tests/test_write_contract_complete_record.py::test_validating_never_reassigns_the_password",
    {
        proxy_intent();
    }
);
source_test!(
    c000_the_three_shapes_are_told_apart_explicitly,
    "tests/test_write_contract_complete_record.py::test_the_three_shapes_are_told_apart_explicitly",
    {
        let p = profile("p");
        let r = record();
        assert_eq!(
            write_contract::shape(Input::Profile(&p), None).expect("shape"),
            write_contract::ENGINE_OBJECT
        );
        assert_eq!(
            write_contract::shape(Input::Json(&r), None).expect("shape"),
            write_contract::ENGINE_RECORD
        );
        assert_eq!(
            write_contract::shape(Input::Json(&json!({"name":"n","cores":8})), None)
                .expect("shape"),
            write_contract::DASHBOARD_PATCH
        );
    }
);
source_test!(
    c000_a_partial_dashboard_patch_is_not_charged_for_what_it_omits,
    "tests/test_write_contract_complete_record.py::test_a_partial_dashboard_patch_is_not_charged_for_what_it_omits",
    {
        assert!(problems(&json!({"name":"n"}), None).is_empty());
        assert!(problems(&json!({"cores":8,"memory":16}), None).is_empty());
    }
);
source_test!(
    c000_a_complete_record_is_charged_for_what_it_omits,
    "tests/test_write_contract_complete_record.py::test_a_complete_record_is_charged_for_what_it_omits",
    {
        let mut p = record();
        p["fingerprint"]
            .as_object_mut()
            .expect("fingerprint")
            .remove("seed");
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("fingerprint.seed"))
        );
    }
);
source_test!(
    c000_the_merged_patch_is_what_gets_complete_validation,
    "tests/test_write_contract_complete_record.py::test_the_merged_patch_is_what_gets_complete_validation",
    {
        let patch = json!({"notes":"edited"});
        assert!(problems(&patch, None).is_empty());
        let mut stored = record();
        stored
            .as_object_mut()
            .expect("record")
            .extend(patch.as_object().expect("patch").clone());
        assert!(problems(&stored, None).is_empty());
    }
);
source_test!(
    c000_the_packed_resolution_cannot_walk_past_the_dimension_rule,
    "tests/test_write_contract_complete_record.py::test_the_packed_resolution_cannot_walk_past_the_dimension_rule[0x0]",
    {
        assert!(
            problems(&json!({"name":"n","screen":json!("0x0")}), None)
                .iter()
                .any(|p| p.contains("screen"))
        );
    }
);
source_test!(
    c001_the_packed_resolution_cannot_walk_past_the_dimension_rule,
    "tests/test_write_contract_complete_record.py::test_the_packed_resolution_cannot_walk_past_the_dimension_rule[-5x-5]",
    {
        assert!(
            problems(&json!({"name":"n","screen":json!("-5x-5")}), None)
                .iter()
                .any(|p| p.contains("screen"))
        );
    }
);
source_test!(
    c002_the_packed_resolution_cannot_walk_past_the_dimension_rule,
    "tests/test_write_contract_complete_record.py::test_the_packed_resolution_cannot_walk_past_the_dimension_rule[potato]",
    {
        assert!(
            problems(&json!({"name":"n","screen":json!("potato")}), None)
                .iter()
                .any(|p| p.contains("screen"))
        );
    }
);
source_test!(
    c003_the_packed_resolution_cannot_walk_past_the_dimension_rule,
    "tests/test_write_contract_complete_record.py::test_the_packed_resolution_cannot_walk_past_the_dimension_rule[infx1080]",
    {
        assert!(
            problems(&json!({"name":"n","screen":json!("infx1080")}), None)
                .iter()
                .any(|p| p.contains("screen"))
        );
    }
);
source_test!(
    c004_the_packed_resolution_cannot_walk_past_the_dimension_rule,
    "tests/test_write_contract_complete_record.py::test_the_packed_resolution_cannot_walk_past_the_dimension_rule[1920]",
    {
        assert!(
            problems(&json!({"name":"n","screen":json!("1920")}), None)
                .iter()
                .any(|p| p.contains("screen"))
        );
    }
);
source_test!(
    c005_the_packed_resolution_cannot_walk_past_the_dimension_rule,
    "tests/test_write_contract_complete_record.py::test_the_packed_resolution_cannot_walk_past_the_dimension_rule[1920x1080x2]",
    {
        assert!(
            problems(&json!({"name":"n","screen":json!("1920x1080x2")}), None)
                .iter()
                .any(|p| p.contains("screen"))
        );
    }
);
source_test!(
    c006_the_packed_resolution_cannot_walk_past_the_dimension_rule,
    "tests/test_write_contract_complete_record.py::test_the_packed_resolution_cannot_walk_past_the_dimension_rule[99999x10]",
    {
        assert!(
            problems(&json!({"name":"n","screen":json!("99999x10")}), None)
                .iter()
                .any(|p| p.contains("screen"))
        );
    }
);
source_test!(
    c000_an_ordinary_resolution_is_left_alone,
    "tests/test_write_contract_complete_record.py::test_an_ordinary_resolution_is_left_alone[1920x1080]",
    {
        assert!(problems(&json!({"name":"n","screen":json!("1920x1080")}), None).is_empty());
    }
);
source_test!(
    c001_an_ordinary_resolution_is_left_alone,
    "tests/test_write_contract_complete_record.py::test_an_ordinary_resolution_is_left_alone[1920X1080]",
    {
        assert!(problems(&json!({"name":"n","screen":json!("1920X1080")}), None).is_empty());
    }
);
source_test!(
    c002_an_ordinary_resolution_is_left_alone,
    "tests/test_write_contract_complete_record.py::test_an_ordinary_resolution_is_left_alone[800x600]",
    {
        assert!(problems(&json!({"name":"n","screen":json!("800x600")}), None).is_empty());
    }
);
source_test!(
    c000_a_bad_second_entry_leaves_the_first_one_unwritten,
    "tests/test_write_contract_complete_record.py::test_a_bad_second_entry_leaves_the_first_one_unwritten",
    {
        bad_later_complete(1, false, "user_agent");
    }
);
source_test!(
    c000_a_bad_last_entry_is_named_by_its_position,
    "tests/test_write_contract_complete_record.py::test_a_bad_last_entry_is_named_by_its_position",
    {
        bad_later_complete(4, false, "seed");
    }
);
source_test!(
    c000_a_config_change_does_not_commit_when_the_batch_is_refused,
    "tests/test_write_contract_complete_record.py::test_a_config_change_does_not_commit_when_the_batch_is_refused",
    {
        bad_later_complete(0, true, "timezone");
    }
);
source_test!(
    c000_a_healthy_profile_round_trips_unchanged_but_for_server_metadata,
    "tests/test_write_contract_complete_record.py::test_a_healthy_profile_round_trips_unchanged_but_for_server_metadata",
    {
        let tmp = common::TempDirectory::new("contract-roundtrip").expect("root");
        let mut p = profile("Ünïcodé ✓");
        p.notes = "note".into();
        p.tags = vec!["a".into(), "b".into()];
        p.startup_urls = vec!["https://x.test".into()];
        p.bookmarks = vec![json!({"name":"D","url":"https://d.test"})];
        p.proxy = Some(
            Proxy::from_record(
                &json!({"server":"socks5://h:1080","username":"u","country":"NL"}),
                false,
            )
            .expect("proxy"),
        );
        let mut original = serde_json::to_value(&p).expect("record");
        let saved = repo(tmp.path()).save(p, 1001.0).expect("save");
        let mut got = read(tmp.path(), "profiles", &saved.id);
        original["updated_at"] = json!(0);
        got["updated_at"] = json!(0);
        assert_eq!(got, original);
    }
);
source_test!(
    c000_a_fractional_device_pixel_ratio_is_still_accepted,
    "tests/test_write_contract_complete_record.py::test_a_fractional_device_pixel_ratio_is_still_accepted[1.0]",
    {
        accepted_record("device_pixel_ratio", json!(1.0));
    }
);
source_test!(
    c001_a_fractional_device_pixel_ratio_is_still_accepted,
    "tests/test_write_contract_complete_record.py::test_a_fractional_device_pixel_ratio_is_still_accepted[1.5]",
    {
        accepted_record("device_pixel_ratio", json!(1.5));
    }
);
source_test!(
    c002_a_fractional_device_pixel_ratio_is_still_accepted,
    "tests/test_write_contract_complete_record.py::test_a_fractional_device_pixel_ratio_is_still_accepted[2.0]",
    {
        accepted_record("device_pixel_ratio", json!(2.0));
    }
);
source_test!(
    c003_a_fractional_device_pixel_ratio_is_still_accepted,
    "tests/test_write_contract_complete_record.py::test_a_fractional_device_pixel_ratio_is_still_accepted[2.625]",
    {
        accepted_record("device_pixel_ratio", json!(2.625));
    }
);
source_test!(
    c004_a_fractional_device_pixel_ratio_is_still_accepted,
    "tests/test_write_contract_complete_record.py::test_a_fractional_device_pixel_ratio_is_still_accepted[3]",
    {
        accepted_record("device_pixel_ratio", json!(3));
    }
);
source_test!(
    c005_a_fractional_device_pixel_ratio_is_still_accepted,
    "tests/test_write_contract_complete_record.py::test_a_fractional_device_pixel_ratio_is_still_accepted[0.05]",
    {
        accepted_record("device_pixel_ratio", json!(0.05));
    }
);
source_test!(
    c006_a_fractional_device_pixel_ratio_is_still_accepted,
    "tests/test_write_contract_complete_record.py::test_a_fractional_device_pixel_ratio_is_still_accepted[20]",
    {
        accepted_record("device_pixel_ratio", json!(20));
    }
);
source_test!(
    c000_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[seed-0]",
    {
        accepted_record("seed", json!(0));
    }
);
source_test!(
    c001_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[device_memory-0]",
    {
        accepted_record("device_memory", json!(0));
    }
);
source_test!(
    c002_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[max_touch_points-0]",
    {
        accepted_record("max_touch_points", json!(0));
    }
);
source_test!(
    c003_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[cameras-0]",
    {
        accepted_record("cameras", json!(0));
    }
);
source_test!(
    c004_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[microphones-0]",
    {
        accepted_record("microphones", json!(0));
    }
);
source_test!(
    c005_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[speakers-0]",
    {
        accepted_record("speakers", json!(0));
    }
);
source_test!(
    c006_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[os_version-]",
    {
        accepted_record("os_version", json!(""));
    }
);
source_test!(
    c007_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[device_model-]",
    {
        accepted_record("device_model", json!(""));
    }
);
source_test!(
    c008_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[fonts-value8]",
    {
        accepted_record("fonts", Value::Array(vec![]));
    }
);
source_test!(
    c009_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[is_mobile-True]",
    {
        accepted_record("is_mobile", json!(true));
    }
);
source_test!(
    c010_a_legitimate_edge_value_is_not_refused,
    "tests/test_write_contract_complete_record.py::test_a_legitimate_edge_value_is_not_refused[do_not_track-True]",
    {
        accepted_record("do_not_track", json!(true));
    }
);
source_test!(
    c000_an_empty_name_and_no_proxy_are_still_legitimate,
    "tests/test_write_contract_complete_record.py::test_an_empty_name_and_no_proxy_are_still_legitimate",
    {
        let tmp = common::TempDirectory::new("contract-empty-name").expect("root");
        let p = repo(tmp.path())
            .save(profile(""), 1001.0)
            .expect("empty name");
        assert!(p.proxy.is_none());
        assert!(
            repo(tmp.path())
                .resolve(&p.id, 1002.0)
                .expect("read")
                .expect("profile")
                .proxy
                .is_none()
        );
    }
);
