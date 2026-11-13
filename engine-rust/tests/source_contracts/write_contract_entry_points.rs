use super::*;
source_test!(
    c000_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[cores]",
    {
        refused_field("create","cores",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c001_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[cores-neg]",
    {
        refused_field("create","cores",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c002_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[memory]",
    {
        refused_field("create","memory",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c003_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[memory-neg]",
    {
        refused_field("create","memory",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c004_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[mediaVideo]",
    {
        refused_field("create","mediaVideo",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c005_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[mediaVideo-neg]",
    {
        refused_field("create","mediaVideo",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c006_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[mediaAudio]",
    {
        refused_field("create","mediaAudio",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c007_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[mediaAudio-neg]",
    {
        refused_field("create","mediaAudio",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c008_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[mediaSpeakers]",
    {
        refused_field("create","mediaSpeakers",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c009_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[mediaSpeakers-neg]",
    {
        refused_field("create","mediaSpeakers",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c010_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[maxTouchPoints]",
    {
        refused_field("create","maxTouchPoints",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c011_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[maxTouchPoints-neg]",
    {
        refused_field("create","maxTouchPoints",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c012_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[devicePixelRatio]",
    {
        refused_field("create","devicePixelRatio",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c013_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[devicePixelRatio-neg]",
    {
        refused_field("create","devicePixelRatio",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c014_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[seed]",
    {
        refused_field("create","seed",serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c015_an_integer_too_large_for_a_float_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_integer_too_large_for_a_float_is_refused_by_field[seed-neg]",
    {
        refused_field("create","seed",serde_json::from_str::<Value>("-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"));
    }
);
source_test!(
    c000_the_refusal_does_not_echo_the_whole_number_back,
    "tests/test_write_contract_entry_points.py::test_the_refusal_does_not_echo_the_whole_number_back",
    {
        let tmp = common::TempDirectory::new("contract-huge-message").expect("root");
        let mut payload = json!({"name":"huge"});
        payload["cores"] = big(4000, false);
        let (s, v) = http(tmp.path(), "POST", "/api/profiles", payload);
        assert_refused(s, &v, "cores");
        assert!(v.to_string().len() < 500);
    }
);
source_test!(
    c000_an_integer_past_the_decoder_limit_is_a_bad_request_not_a_crash,
    "tests/test_write_contract_entry_points.py::test_an_integer_past_the_decoder_limit_is_a_bad_request_not_a_crash",
    {
        let tmp = common::TempDirectory::new("contract-decoder-limit").expect("root");
        let body = format!(r#"{{"name":"huge","cores":1{}}}"#, "0".repeat(5000));
        let (s, v) = http_raw(tmp.path(), "POST", "/api/profiles", body.into_bytes());
        assert!((400..500).contains(&s), "{v}");
        assert!(ids(tmp.path(), "dashboard").is_empty());
    }
);
source_test!(
    c000_a_huge_integer_is_refused_on_every_write_route,
    "tests/test_write_contract_entry_points.py::test_a_huge_integer_is_refused_on_every_write_route[/api/profiles/batch-body0]",
    {
        let tmp = common::TempDirectory::new("contract-huge").expect("root");
        let(s,v)=http(tmp.path(),"POST","/api/profiles/batch",Value::Object(serde_json::Map::from_iter([("profiles".into(),Value::Array(vec![Value::Object(serde_json::Map::from_iter([("name".into(),json!("a"))])),Value::Object(serde_json::Map::from_iter([("name".into(),json!("b")),("cores".into(),serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"))]))]))])));
        assert_refused(s, &v, "cores");
        assert!(ids(tmp.path(), "dashboard").is_empty());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c001_a_huge_integer_is_refused_on_every_write_route,
    "tests/test_write_contract_entry_points.py::test_a_huge_integer_is_refused_on_every_write_route[/api/profiles/bulk-body1]",
    {
        let tmp = common::TempDirectory::new("contract-huge").expect("root");
        let(s,v)=http(tmp.path(),"POST","/api/profiles/bulk",Value::Object(serde_json::Map::from_iter([("ids".into(),Value::Array(vec![])),("patch".into(),Value::Object(serde_json::Map::from_iter([("cores".into(),serde_json::from_str::<Value>("10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000").expect("large integer"))])))])));
        assert_refused(s, &v, "cores");
        assert!(ids(tmp.path(), "dashboard").is_empty());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c000_a_huge_integer_in_a_native_import_is_refused_too,
    "tests/test_write_contract_entry_points.py::test_a_huge_integer_in_a_native_import_is_refused_too",
    {
        let tmp = common::TempDirectory::new("contract-native-huge").expect("root");
        let mut p = json!({"name":"huge"});
        p["seed"] = big(400, false);
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(native(Value::Array(vec![p]))),
        );
        assert_refused(s, &v, "seed");
        assert!(ids(tmp.path(), "dashboard").is_empty());
    }
);
source_test!(
    c000_an_integer_inside_the_range_is_still_accepted,
    "tests/test_write_contract_entry_points.py::test_an_integer_inside_the_range_is_still_accepted",
    {
        let tmp = common::TempDirectory::new("contract-in-range").expect("root");
        let p = made(
            tmp.path(),
            json!({"name":"fine","cores":16,"memory":32,"seed":4503599627370496i64,"maxTouchPoints":0}),
        );
        assert_eq!(
            read(tmp.path(), "dashboard", p["id"].as_str().expect("id"))["seed"],
            4503599627370496i64
        );
    }
);
source_test!(
    c000_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[{}]-addTags]",
    {
        refused_field(
            "bulk",
            "addTags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
        );
    }
);
source_test!(
    c001_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[{}]-removeTags]",
    {
        refused_field(
            "bulk",
            "removeTags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
        );
    }
);
source_test!(
    c002_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[{'name': 'work'}]-addTags]",
    {
        refused_field(
            "bulk",
            "addTags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                "name".into(),
                json!("work"),
            )]))]),
        );
    }
);
source_test!(
    c003_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[{'name': 'work'}]-removeTags]",
    {
        refused_field(
            "bulk",
            "removeTags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                "name".into(),
                json!("work"),
            )]))]),
        );
    }
);
source_test!(
    c004_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[['ok', 17]-addTags]",
    {
        refused_field(
            "bulk",
            "addTags",
            Value::Array(vec![json!("ok"), json!(17)]),
        );
    }
);
source_test!(
    c005_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[['ok', 17]-removeTags]",
    {
        refused_field(
            "bulk",
            "removeTags",
            Value::Array(vec![json!("ok"), json!(17)]),
        );
    }
);
source_test!(
    c006_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[None]-addTags]",
    {
        refused_field("bulk", "addTags", Value::Array(vec![Value::Null]));
    }
);
source_test!(
    c007_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[None]-removeTags]",
    {
        refused_field("bulk", "removeTags", Value::Array(vec![Value::Null]));
    }
);
source_test!(
    c008_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[['nested']]-addTags]",
    {
        refused_field(
            "bulk",
            "addTags",
            Value::Array(vec![Value::Array(vec![json!("nested")])]),
        );
    }
);
source_test!(
    c009_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[[['nested']]-removeTags]",
    {
        refused_field(
            "bulk",
            "removeTags",
            Value::Array(vec![Value::Array(vec![json!("nested")])]),
        );
    }
);
source_test!(
    c010_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[['ok', None]-addTags]",
    {
        refused_field(
            "bulk",
            "addTags",
            Value::Array(vec![json!("ok"), Value::Null]),
        );
    }
);
source_test!(
    c011_a_non_string_tag_operand_is_refused_not_raised,
    "tests/test_write_contract_entry_points.py::test_a_non_string_tag_operand_is_refused_not_raised[['ok', None]-removeTags]",
    {
        refused_field(
            "bulk",
            "removeTags",
            Value::Array(vec![json!("ok"), Value::Null]),
        );
    }
);
source_test!(
    c000_a_tag_operand_that_is_not_a_list_is_still_refused,
    "tests/test_write_contract_entry_points.py::test_a_tag_operand_that_is_not_a_list_is_still_refused",
    {
        refused_field("bulk", "addTags", json!("work"));
    }
);
source_test!(
    c000_string_tag_operands_still_add_and_remove,
    "tests/test_write_contract_entry_points.py::test_string_tag_operands_still_add_and_remove",
    {
        tag_operands(
            json!({"addTags":["new"],"removeTags":["old"]}),
            json!(["new"]),
        );
    }
);
source_test!(
    c000_an_empty_tag_operand_is_not_an_error,
    "tests/test_write_contract_entry_points.py::test_an_empty_tag_operand_is_not_an_error",
    {
        tag_operands(json!({"addTags":[]}), json!(["old"]));
    }
);
source_test!(
    c000_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[numeric name]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([("name".into(), json!(123))])),
            "name",
            None,
        );
    }
);
source_test!(
    c001_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[list name]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([(
                "name".into(),
                Value::Array(vec![json!("a")]),
            )])),
            "name",
            None,
        );
    }
);
source_test!(
    c002_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[numeric notes]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                ("notes".into(), json!(5)),
            ])),
            "notes",
            None,
        );
    }
);
source_test!(
    c003_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[object timezone]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "timezone".into(),
                    Value::Object(serde_json::Map::from_iter([(
                        "zone".into(),
                        json!("Europe/Berlin"),
                    )])),
                ),
            ])),
            "timezone",
            None,
        );
    }
);
source_test!(
    c004_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[numeric webgl vendor]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                ("webgl_vendor".into(), json!(7)),
            ])),
            "webglVendor",
            None,
        );
    }
);
source_test!(
    c005_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[object in the tag list]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "tags".into(),
                    Value::Array(vec![
                        Value::Object(serde_json::Map::from_iter([])),
                        json!("fine"),
                    ]),
                ),
            ])),
            "tags",
            None,
        );
    }
);
source_test!(
    c006_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[number in the tag list]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                ("tags".into(), Value::Array(vec![json!("fine"), json!(17)])),
            ])),
            "tags",
            None,
        );
    }
);
source_test!(
    c007_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[object proxy user]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "proxy".into(),
                    Value::Object(serde_json::Map::from_iter([
                        ("type".into(), json!("http")),
                        ("host".into(), json!("1.2.3.4")),
                        ("port".into(), json!(8080)),
                        (
                            "user".into(),
                            Value::Object(serde_json::Map::from_iter([("k".into(), json!(1))])),
                        ),
                    ])),
                ),
            ])),
            "proxy",
            None,
        );
    }
);
source_test!(
    c008_an_unrenderable_vendor_value_is_refused_by_field,
    "tests/test_write_contract_entry_points.py::test_an_unrenderable_vendor_value_is_refused_by_field[list proxy country]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "proxy".into(),
                    Value::Object(serde_json::Map::from_iter([
                        ("type".into(), json!("http")),
                        ("host".into(), json!("1.2.3.4")),
                        ("port".into(), json!(8080)),
                        ("country".into(), Value::Array(vec![json!(1)])),
                    ])),
                ),
            ])),
            "proxy",
            None,
        );
    }
);
source_test!(
    c000_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[numeric name]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([("name".into(), json!(123))])),
            "name",
            Some(2),
        );
    }
);
source_test!(
    c001_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[list name]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([(
                "name".into(),
                Value::Array(vec![json!("a")]),
            )])),
            "name",
            Some(2),
        );
    }
);
source_test!(
    c002_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[numeric notes]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                ("notes".into(), json!(5)),
            ])),
            "notes",
            Some(2),
        );
    }
);
source_test!(
    c003_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[object timezone]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "timezone".into(),
                    Value::Object(serde_json::Map::from_iter([(
                        "zone".into(),
                        json!("Europe/Berlin"),
                    )])),
                ),
            ])),
            "timezone",
            Some(2),
        );
    }
);
source_test!(
    c004_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[numeric webgl vendor]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                ("webgl_vendor".into(), json!(7)),
            ])),
            "webglVendor",
            Some(2),
        );
    }
);
source_test!(
    c005_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[object in the tag list]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "tags".into(),
                    Value::Array(vec![
                        Value::Object(serde_json::Map::from_iter([])),
                        json!("fine"),
                    ]),
                ),
            ])),
            "tags",
            Some(2),
        );
    }
);
source_test!(
    c006_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[number in the tag list]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                ("tags".into(), Value::Array(vec![json!("fine"), json!(17)])),
            ])),
            "tags",
            Some(2),
        );
    }
);
source_test!(
    c007_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[object proxy user]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "proxy".into(),
                    Value::Object(serde_json::Map::from_iter([
                        ("type".into(), json!("http")),
                        ("host".into(), json!("1.2.3.4")),
                        ("port".into(), json!(8080)),
                        (
                            "user".into(),
                            Value::Object(serde_json::Map::from_iter([("k".into(), json!(1))])),
                        ),
                    ])),
                ),
            ])),
            "proxy",
            Some(2),
        );
    }
);
source_test!(
    c008_a_bad_entry_after_the_first_still_refuses_the_whole_import,
    "tests/test_write_contract_entry_points.py::test_a_bad_entry_after_the_first_still_refuses_the_whole_import[list proxy country]",
    {
        vendor_refused(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("ok")),
                (
                    "proxy".into(),
                    Value::Object(serde_json::Map::from_iter([
                        ("type".into(), json!("http")),
                        ("host".into(), json!("1.2.3.4")),
                        ("port".into(), json!(8080)),
                        ("country".into(), Value::Array(vec![json!(1)])),
                    ])),
                ),
            ])),
            "proxy",
            Some(2),
        );
    }
);
source_test!(
    c000_a_vendor_value_the_importer_cannot_read_is_never_a_traceback,
    "tests/test_write_contract_entry_points.py::test_a_vendor_value_the_importer_cannot_read_is_never_a_traceback[tag list that is not a list]",
    {
        vendor_unreadable(Value::Object(serde_json::Map::from_iter([
            ("name".into(), json!("ok")),
            ("tags".into(), json!(17)),
        ])));
    }
);
source_test!(
    c001_a_vendor_value_the_importer_cannot_read_is_never_a_traceback,
    "tests/test_write_contract_entry_points.py::test_a_vendor_value_the_importer_cannot_read_is_never_a_traceback[user agent that is not text]",
    {
        vendor_unreadable(Value::Object(serde_json::Map::from_iter([
            ("name".into(), json!("ok")),
            ("userAgent".into(), json!(99)),
        ])));
    }
);
source_test!(
    c002_a_vendor_value_the_importer_cannot_read_is_never_a_traceback,
    "tests/test_write_contract_entry_points.py::test_a_vendor_value_the_importer_cannot_read_is_never_a_traceback[language that is not text]",
    {
        vendor_unreadable(Value::Object(serde_json::Map::from_iter([
            ("name".into(), json!("ok")),
            (
                "language".into(),
                Value::Object(serde_json::Map::from_iter([(
                    "code".into(),
                    json!("en-US"),
                )])),
            ),
        ])));
    }
);
source_test!(
    c000_every_imported_profile_passes_the_same_schema_a_created_one_does,
    "tests/test_write_contract_entry_points.py::test_every_imported_profile_passes_the_same_schema_a_created_one_does",
    {
        let tmp = common::TempDirectory::new("contract-import-schema").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(
                json!([{"name":"gologin one","os":"Windows","tags":"ads, client","navigator":{"userAgent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36","hardwareConcurrency":"8 cores","resolution":"1920x1080"},"proxy":{"mode":"socks5","host":"1.2.3.4","port":1080,"username":"u","password":"p","country":"DE"}},{"profile_name":"adspower two","os_type":"macOS","group_name":"Client work","description":"second"}]),
            ),
        );
        assert_eq!(s, 200, "{v}");
        valid_twin(tmp.path());
        for id in ids(tmp.path(), "dashboard") {
            let mut rec = read(tmp.path(), "dashboard", &id);
            numera_core::http::profile_fields::strip_server_owned(
                rec.as_object_mut().expect("record"),
            )
            .expect("strip owned");
            let errors = numera_core::http::profile_fields::check(&rec, "imported", &[])
                .expect("same HTTP schema");
            assert!(errors.is_empty(), "{errors:?}");
        }
    }
);
source_test!(
    c000_a_vendor_number_outside_the_contract_is_refused_with_its_bounds,
    "tests/test_write_contract_entry_points.py::test_a_vendor_number_outside_the_contract_is_refused_with_its_bounds",
    {
        vendor_refused(
            json!({"name":"ok","navigator":{"hardwareConcurrency":2000}}),
            "512",
            None,
        );
    }
);
source_test!(
    c000_an_imported_profile_can_still_be_edited_afterwards,
    "tests/test_write_contract_entry_points.py::test_an_imported_profile_can_still_be_edited_afterwards",
    {
        let tmp = common::TempDirectory::new("contract-import-edit").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(json!([{"name":"vendor"}])),
        );
        assert_eq!(s, 200, "{v}");
        let id = ids(tmp.path(), "dashboard").into_iter().next().expect("id");
        let (s, v) = put(tmp.path(), &json!(id), json!({"name":"renamed"}));
        assert_eq!(s, 200, "{v}");
    }
);
source_test!(
    c000_a_vendor_name_is_rendered_as_text_by_every_reader,
    "tests/test_write_contract_entry_points.py::test_a_vendor_name_is_rendered_as_text_by_every_reader",
    {
        let tmp = common::TempDirectory::new("contract-import-name").expect("root");
        http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(json!([{"name":123}])),
        );
        made(tmp.path(), json!({"name":"innocent"}));
        let (s, rows) = http(tmp.path(), "GET", "/api/profiles", Value::Null);
        assert_eq!(s, 200);
        for p in rows.as_array().expect("rows") {
            assert!(p["name"].is_string());
        }
    }
);
source_test!(
    c000_the_native_branch_holds_the_same_line,
    "tests/test_write_contract_entry_points.py::test_the_native_branch_holds_the_same_line",
    {
        let tmp = common::TempDirectory::new("contract-native-name").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(native(
                json!([{"name":"one"},{"name":123},{"name":"three"}]),
            )),
        );
        assert_refused(s, &v, "name");
        assert!(v.to_string().contains("profiles[1]"));
        assert!(snapshot(tmp.path()).is_empty());
    }
);
source_test!(
    c000_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[name]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([(
                "profile_name".into(),
                json!("by profile_name"),
            )])),
            "name",
            json!("by profile_name"),
        );
    }
);
source_test!(
    c001_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[title]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([(
                "title".into(),
                json!("by title"),
            )])),
            "name",
            json!("by title"),
        );
    }
);
source_test!(
    c002_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[notes]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("description".into(), json!("by description")),
            ])),
            "notes",
            json!("by description"),
        );
    }
);
source_test!(
    c003_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[nested user agent]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                (
                    "fingerprint".into(),
                    Value::Object(serde_json::Map::from_iter([(
                        "ua".into(),
                        json!(
                            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
                        ),
                    )])),
                ),
            ])),
            "browserVersion",
            json!("126.0.0.0"),
        );
    }
);
source_test!(
    c004_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[os_type]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("os_type".into(), json!("macOS")),
            ])),
            "os",
            json!("macOS"),
        );
    }
);
source_test!(
    c005_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[screen_resolution]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("screen_resolution".into(), json!("2560 x 1440")),
            ])),
            "screen",
            json!("2560x1440"),
        );
    }
);
source_test!(
    c006_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[nested locale]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                (
                    "navigator".into(),
                    Value::Object(serde_json::Map::from_iter([(
                        "language".into(),
                        json!("de-DE"),
                    )])),
                ),
            ])),
            "locale",
            json!("de-DE"),
        );
    }
);
source_test!(
    c007_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[nested timezone]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                (
                    "timezone".into(),
                    Value::Object(serde_json::Map::from_iter([(
                        "timezone".into(),
                        json!("Europe/Berlin"),
                    )])),
                ),
            ])),
            "timezone",
            json!("Europe/Berlin"),
        );
    }
);
source_test!(
    c008_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[hardware_concurrency]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("hardware_concurrency".into(), json!(12)),
            ])),
            "cores",
            json!(12),
        );
    }
);
source_test!(
    c009_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[device_memory]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("device_memory".into(), json!("16")),
            ])),
            "memory",
            json!(16),
        );
    }
);
source_test!(
    c010_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[unmaskedVendor]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("unmaskedVendor".into(), json!("Google Inc. (NVIDIA)")),
            ])),
            "webglVendor",
            json!("Google Inc. (NVIDIA)"),
        );
    }
);
source_test!(
    c011_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[comma-separated tags]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("tags".into(), json!("ads, client")),
            ])),
            "tags",
            Value::Array(vec![json!("ads"), json!("client")]),
        );
    }
);
source_test!(
    c012_the_importer_is_still_tolerant_about_the_name_a_vendor_uses,
    "tests/test_write_contract_entry_points.py::test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses[group_name becomes a tag]",
    {
        vendor_alias(
            Value::Object(serde_json::Map::from_iter([
                ("name".into(), json!("n")),
                ("group_name".into(), json!("Client work")),
            ])),
            "tags",
            Value::Array(vec![json!("Client work")]),
        );
    }
);
source_test!(
    c000_a_missing_field_is_still_filled_in_coherently,
    "tests/test_write_contract_entry_points.py::test_a_missing_field_is_still_filled_in_coherently",
    {
        let tmp = common::TempDirectory::new("contract-sparse").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(json!([{"name":"sparse"}])),
        );
        assert_eq!(s, 200, "{v}");
        assert_eq!(
            v,
            json!({"imported":1,"format":"vendor","results":[{"name":"sparse","issues":[]}]})
        );
        let id = ids(tmp.path(), "dashboard").into_iter().next().expect("id");
        let rec = read(tmp.path(), "dashboard", &id);
        for f in ["userAgent", "locale", "timezone"] {
            assert!(!rec[f].as_str().expect("text").is_empty());
        }
        assert!(rec["tags"].is_array());
        assert!(rec["seed"].is_i64());
    }
);
source_test!(
    c000_a_proxy_a_vendor_actually_exports_still_arrives_whole,
    "tests/test_write_contract_entry_points.py::test_a_proxy_a_vendor_actually_exports_still_arrives_whole",
    {
        let tmp = common::TempDirectory::new("contract-import-proxy").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/import",
            importing(
                json!([{"name":"with proxy","proxy":{"proxy_type":"socks4","ip":"203.0.113.9","proxy_port":"1080","login":"u","proxy_password":"p","geo":"NL"}}]),
            ),
        );
        assert_eq!(s, 200, "{v}");
        let rows = http(tmp.path(), "GET", "/api/profiles", Value::Null).1;
        let px = &rows[0]["proxy"];
        for (k, w) in [
            ("type", "SOCKS4"),
            ("host", "203.0.113.9"),
            ("port", "1080"),
            ("user", "u"),
            ("country", "NL"),
        ] {
            assert_eq!(px[k], w);
        }
    }
);
