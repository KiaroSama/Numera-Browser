use super::*;
source_test!(
    c000_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[seed=inf]",
    {
        refused_record("fingerprint.seed", number("Infinity"), "finite");
    }
);
source_test!(
    c001_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[seed=nan]",
    {
        refused_record("fingerprint.seed", number("NaN"), "finite");
    }
);
source_test!(
    c002_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[seed=10000000000000000000000]",
    {
        refused_record("fingerprint.seed",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"),"between");
    }
);
source_test!(
    c003_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[hardware_concurrency=1000000]",
    {
        refused_record("fingerprint.hardware_concurrency",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"),"between");
    }
);
source_test!(
    c004_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[hardware_concurrency=0]",
    {
        refused_record("fingerprint.hardware_concurrency", json!(0), "between");
    }
);
source_test!(
    c005_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[device_memory=-5]",
    {
        refused_record("fingerprint.device_memory", json!(-5), "between");
    }
);
source_test!(
    c006_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[device_pixel_ratio=-inf]",
    {
        refused_record(
            "fingerprint.device_pixel_ratio",
            number("-Infinity"),
            "finite",
        );
    }
);
source_test!(
    c007_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[device_pixel_ratio=999]",
    {
        refused_record("fingerprint.device_pixel_ratio", json!(999), "between");
    }
);
source_test!(
    c008_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[cameras='two']",
    {
        refused_record("fingerprint.cameras", json!("two"), "must be a number");
    }
);
source_test!(
    c009_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[max_touch_points=True]",
    {
        refused_record(
            "fingerprint.max_touch_points",
            json!(true),
            "must be a number",
        );
    }
);
source_test!(
    c010_a_nested_number_is_refused_before_anything_is_written,
    "tests/test_write_contract_engine_shape.py::test_a_nested_number_is_refused_before_anything_is_written[speakers=1000000000000000000]",
    {
        refused_record("fingerprint.speakers",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"),"between");
    }
);
source_test!(
    c000_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[seed=inf]",
    {
        refused_record("fingerprint.seed", number("Infinity"), "finite");
    }
);
source_test!(
    c001_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[seed=nan]",
    {
        refused_record("fingerprint.seed", number("NaN"), "finite");
    }
);
source_test!(
    c002_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[seed=10000000000000000000000]",
    {
        refused_record("fingerprint.seed",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"),"between");
    }
);
source_test!(
    c003_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[hardware_concurrency=1000000]",
    {
        refused_record("fingerprint.hardware_concurrency",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"),"between");
    }
);
source_test!(
    c004_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[hardware_concurrency=0]",
    {
        refused_record("fingerprint.hardware_concurrency", json!(0), "between");
    }
);
source_test!(
    c005_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[device_memory=-5]",
    {
        refused_record("fingerprint.device_memory", json!(-5), "between");
    }
);
source_test!(
    c006_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[device_pixel_ratio=-inf]",
    {
        refused_record(
            "fingerprint.device_pixel_ratio",
            number("-Infinity"),
            "finite",
        );
    }
);
source_test!(
    c007_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[device_pixel_ratio=999]",
    {
        refused_record("fingerprint.device_pixel_ratio", json!(999), "between");
    }
);
source_test!(
    c008_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[cameras='two']",
    {
        refused_record("fingerprint.cameras", json!("two"), "must be a number");
    }
);
source_test!(
    c009_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[max_touch_points=True]",
    {
        refused_record(
            "fingerprint.max_touch_points",
            json!(true),
            "must be a number",
        );
    }
);
source_test!(
    c010_the_refusal_names_the_field_the_caller_actually_sent,
    "tests/test_write_contract_engine_shape.py::test_the_refusal_names_the_field_the_caller_actually_sent[speakers=1000000000000000000]",
    {
        refused_record("fingerprint.speakers",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"),"between");
    }
);
source_test!(
    c000_a_nested_collection_element_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_nested_collection_element_is_refused[languages=['en-US', 7]]",
    {
        refused_record(
            "fingerprint.languages",
            Value::Array(vec![json!("en-US"), json!(7)]),
            "every entry",
        );
    }
);
source_test!(
    c001_a_nested_collection_element_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_nested_collection_element_is_refused[languages=[None]]",
    {
        refused_record(
            "fingerprint.languages",
            Value::Array(vec![Value::Null]),
            "every entry",
        );
    }
);
source_test!(
    c002_a_nested_collection_element_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_nested_collection_element_is_refused[languages='en-US']",
    {
        refused_record("fingerprint.languages", json!("en-US"), "must be a list");
    }
);
source_test!(
    c003_a_nested_collection_element_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_nested_collection_element_is_refused[fonts=[{}]]",
    {
        refused_record(
            "fingerprint.fonts",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
            "every entry",
        );
    }
);
source_test!(
    c004_a_nested_collection_element_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_nested_collection_element_is_refused[fonts='Arial']",
    {
        refused_record("fingerprint.fonts", json!("Arial"), "must be a list");
    }
);
source_test!(
    c000_a_proxy_field_that_is_not_text_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_field_that_is_not_text_is_refused[server=123]",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","username":"u","password":"p","country":"NL"});
        p["proxy"]["server"] = json!(123);
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("proxy.server") && p.contains("must be text"))
        );
        let tmp = common::TempDirectory::new("contract-proxy-shape").expect("root");
        assert!(import_file(tmp.path(), p).is_err());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c001_a_proxy_field_that_is_not_text_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_field_that_is_not_text_is_refused[server=['http://x:1']",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","username":"u","password":"p","country":"NL"});
        p["proxy"]["server"] = Value::Array(vec![json!("http://x:1")]);
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("proxy.server") && p.contains("must be text"))
        );
        let tmp = common::TempDirectory::new("contract-proxy-shape").expect("root");
        assert!(import_file(tmp.path(), p).is_err());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c002_a_proxy_field_that_is_not_text_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_field_that_is_not_text_is_refused[username=[]]",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","username":"u","password":"p","country":"NL"});
        p["proxy"]["username"] = Value::Array(vec![]);
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("proxy.username") && p.contains("must be text"))
        );
        let tmp = common::TempDirectory::new("contract-proxy-shape").expect("root");
        assert!(import_file(tmp.path(), p).is_err());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c003_a_proxy_field_that_is_not_text_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_field_that_is_not_text_is_refused[username=7]",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","username":"u","password":"p","country":"NL"});
        p["proxy"]["username"] = json!(7);
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("proxy.username") && p.contains("must be text"))
        );
        let tmp = common::TempDirectory::new("contract-proxy-shape").expect("root");
        assert!(import_file(tmp.path(), p).is_err());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c004_a_proxy_field_that_is_not_text_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_field_that_is_not_text_is_refused[password={}]",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","username":"u","password":"p","country":"NL"});
        p["proxy"]["password"] = Value::Object(serde_json::Map::from_iter([]));
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("proxy.password") && p.contains("must be text"))
        );
        let tmp = common::TempDirectory::new("contract-proxy-shape").expect("root");
        assert!(import_file(tmp.path(), p).is_err());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c005_a_proxy_field_that_is_not_text_is_refused,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_field_that_is_not_text_is_refused[country=7]",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","username":"u","password":"p","country":"NL"});
        p["proxy"]["country"] = json!(7);
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("proxy.country") && p.contains("must be text"))
        );
        let tmp = common::TempDirectory::new("contract-proxy-shape").expect("root");
        assert!(import_file(tmp.path(), p).is_err());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c000_a_proxy_that_is_not_a_proxy_is_refused_not_a_traceback,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_that_is_not_a_proxy_is_refused_not_a_traceback",
    {
        refused_record("proxy", json!("http://host:8080"), "");
    }
);
source_test!(
    c000_validating_a_proxy_does_not_decide_what_to_do_with_its_secret,
    "tests/test_write_contract_engine_shape.py::test_validating_a_proxy_does_not_decide_what_to_do_with_its_secret",
    {
        proxy_intent();
    }
);
source_test!(
    c000_an_ordinary_proxy_still_saves,
    "tests/test_write_contract_engine_shape.py::test_an_ordinary_proxy_still_saves",
    {
        ordinary_proxy();
    }
);
source_test!(
    c000_the_serialized_engine_mapping_is_checked_the_same_way,
    "tests/test_write_contract_engine_shape.py::test_the_serialized_engine_mapping_is_checked_the_same_way",
    {
        let mut p = record();
        p["fingerprint"]["seed"] = number("Infinity");
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("fingerprint.seed") && p.contains("finite"))
        );
    }
);
source_test!(
    c000_a_serialized_proxy_is_checked_the_same_way,
    "tests/test_write_contract_engine_shape.py::test_a_serialized_proxy_is_checked_the_same_way",
    {
        let mut p = record();
        p["proxy"] = json!({"server":"http://h:1","username":[]});
        assert!(
            problems(&p, None)
                .iter()
                .any(|p| p.contains("proxy.username"))
        );
    }
);
source_test!(
    c000_the_dashboard_payload_keeps_its_own_flat_names,
    "tests/test_write_contract_engine_shape.py::test_the_dashboard_payload_keeps_its_own_flat_names",
    {
        let mut p = json!({"name":"n"});
        p["cores"] = big(400, false);
        p["seed"] = number("Infinity");
        let errors = problems(&p, None);
        assert!(
            errors
                .iter()
                .any(|p| p.contains("'cores'") && p.contains("between"))
        );
        assert!(
            errors
                .iter()
                .any(|p| p.contains("'seed'") && p.contains("finite"))
        );
    }
);
source_test!(
    c000_a_dashboard_proxy_is_checked_under_its_own_key_names,
    "tests/test_write_contract_engine_shape.py::test_a_dashboard_proxy_is_checked_under_its_own_key_names",
    {
        let errors = problems(
            &json!({"name":"n","proxy":{"type":"HTTP","host":123,"user":[],"port":"8080"}}),
            None,
        );
        assert!(errors.iter().any(|p| p.contains("proxy.host")));
        assert!(errors.iter().any(|p| p.contains("proxy.user")));
        assert!(!errors.iter().any(|p| p.contains("port")));
    }
);
source_test!(
    c000_a_healthy_engine_profile_has_nothing_said_about_it,
    "tests/test_write_contract_engine_shape.py::test_a_healthy_engine_profile_has_nothing_said_about_it",
    {
        let mut p = profile("p");
        p.proxy = Some(
            Proxy::from_record(
                &json!({"server":"http://h:1","username":"u","country":"DE"}),
                true,
            )
            .expect("proxy"),
        );
        assert!(
            write_contract::profile_problems(Input::Profile(&p), "profile", None)
                .expect("object contract")
                .is_empty()
        );
        assert!(problems(&serde_json::to_value(p).expect("record"), None).is_empty());
    }
);
source_test!(
    c000_the_bounds_are_not_duplicated_per_shape,
    "tests/test_write_contract_engine_shape.py::test_the_bounds_are_not_duplicated_per_shape",
    {
        let flat = problems(&json!({"name":"n","cores":513}), None);
        let mut nested = record();
        nested["fingerprint"]["hardware_concurrency"] = json!(513);
        let nested = problems(&nested, None);
        assert!(flat[0].contains("between 1 and 512"));
        assert!(nested[0].contains("between 1 and 512"));
    }
);
source_test!(
    c000_a_batch_with_a_bad_second_entry_writes_nothing,
    "tests/test_write_contract_engine_shape.py::test_a_batch_with_a_bad_second_entry_writes_nothing",
    {
        bad_later_complete(1, false, "device_memory");
    }
);
source_test!(
    c000_the_cli_import_door_refuses_the_same_nested_value_the_api_does,
    "tests/test_write_contract_engine_shape.py::test_the_cli_import_door_refuses_the_same_nested_value_the_api_does",
    {
        cli_refused(
            json!([{"name":"ok","navigator":{"hardwareConcurrency":2000}}]),
            "512",
        );
    }
);
source_test!(
    c000_a_good_vendor_export_still_imports_from_the_cli,
    "tests/test_write_contract_engine_shape.py::test_a_good_vendor_export_still_imports_from_the_cli",
    {
        good_vendor();
    }
);
source_test!(
    c000_an_edit_that_touches_nothing_numeric_still_saves,
    "tests/test_write_contract_engine_shape.py::test_an_edit_that_touches_nothing_numeric_still_saves",
    {
        let tmp = common::TempDirectory::new("contract-core-edit").expect("root");
        let p = repo(tmp.path())
            .save(profile("before"), 1001.0)
            .expect("save");
        let seed = p.fingerprint.seed.clone();
        let updated = repo(tmp.path())
            .edit(
                &p.id,
                |p| {
                    p.name = json!("after");
                    Ok(None)
                },
                1002.0,
            )
            .expect("edit")
            .expect("profile");
        assert_eq!(updated.name, "after");
        assert_eq!(updated.fingerprint.seed, seed);
    }
);
source_test!(
    c000_a_record_with_no_name_key_gets_one_to_render,
    "tests/test_write_contract_engine_shape.py::test_a_record_with_no_name_key_gets_one_to_render",
    {
        let view = read_view::readable(&json!({"id":"x","tags":["a"]}));
        assert_eq!(view["name"], "");
        assert!(view["name"].is_string());
    }
);
source_test!(
    c000_supplying_a_missing_name_does_not_touch_the_stored_record,
    "tests/test_write_contract_engine_shape.py::test_supplying_a_missing_name_does_not_touch_the_stored_record",
    {
        let rec = json!({"id":"x","tags":["a"]});
        let before = rec.clone();
        read_view::readable(&rec);
        assert_eq!(rec, before);
    }
);
source_test!(
    c000_a_healthy_record_is_still_handed_back_by_identity,
    "tests/test_write_contract_engine_shape.py::test_a_healthy_record_is_still_handed_back_by_identity",
    {
        let rec = json!({"id":"x","name":"alpha","tags":["a"]});
        assert_eq!(read_view::readable(&rec), rec);
    }
);
source_test!(
    c000_only_a_field_this_vocabulary_owns_is_supplied,
    "tests/test_write_contract_engine_shape.py::test_only_a_field_this_vocabulary_owns_is_supplied",
    {
        assert!(
            read_view::readable_with(&json!({"id":"x"}), &["notes"], &[])
                .get("name")
                .is_none()
        );
    }
);
source_test!(
    c000_the_api_still_refuses_by_field_and_writes_nothing,
    "tests/test_write_contract_engine_shape.py::test_the_api_still_refuses_by_field_and_writes_nothing[a-cores]",
    {
        let tmp = common::TempDirectory::new("contract-api").expect("root");
        let(s,v)=http(tmp.path(),"POST","/api/profiles",Value::Object(serde_json::Map::from_iter([("name".into(),json!("a")),("cores".into(),serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"))])));
        assert_refused(s, &v, "cores");
        assert!(snapshot(tmp.path()).is_empty());
    }
);
source_test!(
    c001_the_api_still_refuses_by_field_and_writes_nothing,
    "tests/test_write_contract_engine_shape.py::test_the_api_still_refuses_by_field_and_writes_nothing[a-seed]",
    {
        let tmp = common::TempDirectory::new("contract-api").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles",
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("a")),
                ("seed".into(), number("Infinity")),
            ])),
        );
        assert_refused(s, &v, "seed");
        assert!(snapshot(tmp.path()).is_empty());
    }
);
source_test!(
    c002_the_api_still_refuses_by_field_and_writes_nothing,
    "tests/test_write_contract_engine_shape.py::test_the_api_still_refuses_by_field_and_writes_nothing[a-memory]",
    {
        let tmp = common::TempDirectory::new("contract-api").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles",
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("a")),
                ("memory".into(), json!(-5)),
            ])),
        );
        assert_refused(s, &v, "memory");
        assert!(snapshot(tmp.path()).is_empty());
    }
);
source_test!(
    c003_the_api_still_refuses_by_field_and_writes_nothing,
    "tests/test_write_contract_engine_shape.py::test_the_api_still_refuses_by_field_and_writes_nothing[a-tags]",
    {
        let tmp = common::TempDirectory::new("contract-api").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles",
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("a")),
                (
                    "tags".into(),
                    Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
                ),
            ])),
        );
        assert_refused(s, &v, "tags");
        assert!(snapshot(tmp.path()).is_empty());
    }
);
source_test!(
    c000_api_create_and_update_still_work,
    "tests/test_write_contract_engine_shape.py::test_api_create_and_update_still_work",
    {
        let tmp = common::TempDirectory::new("contract-api-edit").expect("root");
        let p = made(
            tmp.path(),
            json!({"name":"control","cores":8,"proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"p","country":"DE"}}),
        );
        let (s, v) = put(tmp.path(), &p["id"], json!({"name":"control renamed"}));
        assert_eq!(s, 200, "{v}");
        assert_eq!(v["name"], "control renamed");
        assert_eq!(v["proxy"]["host"], "1.2.3.4");
    }
);
source_test!(
    c000_a_proxy_the_core_cannot_read_is_refused_over_http,
    "tests/test_write_contract_engine_shape.py::test_a_proxy_the_core_cannot_read_is_refused_over_http",
    {
        refused_field("create", "proxy", json!({"host":["1.2.3.4"]}));
    }
);
source_test!(
    c000_a_bad_later_entry_refuses_the_whole_batch_over_http,
    "tests/test_write_contract_engine_shape.py::test_a_bad_later_entry_refuses_the_whole_batch_over_http",
    {
        refused_field("batch", "seed", number("Infinity"));
    }
);
source_test!(
    c000_every_row_of_the_listing_has_a_name_the_dashboard_can_render,
    "tests/test_write_contract_engine_shape.py::test_every_row_of_the_listing_has_a_name_the_dashboard_can_render",
    {
        legacy_view("/api/profiles");
    }
);
source_test!(
    c000_the_health_page_renders_the_same_legacy_names,
    "tests/test_write_contract_engine_shape.py::test_the_health_page_renders_the_same_legacy_names",
    {
        legacy_view("/api/profiles/health");
    }
);
source_test!(
    c000_a_legacy_record_keeps_its_original_bytes,
    "tests/test_write_contract_engine_shape.py::test_a_legacy_record_keeps_its_original_bytes",
    {
        let tmp = common::TempDirectory::new("contract-legacy-bytes").expect("root");
        legacy(tmp.path());
        let before = snapshot(tmp.path());
        assert_eq!(http(tmp.path(), "GET", "/api/profiles", Value::Null).0, 200);
        assert_eq!(
            http(tmp.path(), "GET", "/api/profiles/health", Value::Null).0,
            200
        );
        assert_listing_preserves_original_files(tmp.path(), &before, true);
    }
);
