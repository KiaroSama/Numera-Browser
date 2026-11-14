use super::*;
source_test!(
    c000_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[tags-value0]",
    {
        refused_field(
            "create",
            "tags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
        );
    }
);
source_test!(
    c001_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[tags-value1]",
    {
        refused_field("create", "tags", Value::Array(vec![json!("ok"), json!(17)]));
    }
);
source_test!(
    c002_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[tags-work]",
    {
        refused_field("create", "tags", json!("work"));
    }
);
source_test!(
    c003_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[ipBased-value3]",
    {
        refused_field(
            "create",
            "ipBased",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                "a".into(),
                json!(1),
            )]))]),
        );
    }
);
source_test!(
    c004_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[clearOnStart-value4]",
    {
        refused_field("create", "clearOnStart", Value::Array(vec![Value::Null]));
    }
);
source_test!(
    c005_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[switchedOff-value5]",
    {
        refused_field(
            "create",
            "switchedOff",
            Value::Array(vec![Value::Array(vec![json!("nested")])]),
        );
    }
);
source_test!(
    c006_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[fontList-17]",
    {
        refused_field("create", "fontList", json!(17));
    }
);
source_test!(
    c007_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[fontList-value7]",
    {
        refused_field(
            "create",
            "fontList",
            Value::Object(serde_json::Map::from_iter([("Arial".into(), json!(true))])),
        );
    }
);
source_test!(
    c008_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[cores-nan]",
    {
        refused_field("create", "cores", number("NaN"));
    }
);
source_test!(
    c009_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[cores-inf]",
    {
        refused_field("create", "cores", number("Infinity"));
    }
);
source_test!(
    c010_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[cores--4]",
    {
        refused_field("create", "cores", json!(-4));
    }
);
source_test!(
    c011_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[memory--inf]",
    {
        refused_field("create", "memory", number("-Infinity"));
    }
);
source_test!(
    c012_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[seed-nan]",
    {
        refused_field("create", "seed", number("NaN"));
    }
);
source_test!(
    c013_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[mediaVideo-inf]",
    {
        refused_field("create", "mediaVideo", number("Infinity"));
    }
);
source_test!(
    c014_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[devicePixelRatio-big]",
    {
        refused_field("create", "devicePixelRatio", json!("big"));
    }
);
source_test!(
    c015_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[devicePixelRatio-0]",
    {
        refused_field("create", "devicePixelRatio", json!(0));
    }
);
source_test!(
    c016_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[maxTouchPoints-value16]",
    {
        refused_field("create", "maxTouchPoints", Value::Array(vec![]));
    }
);
source_test!(
    c017_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[canvas-5]",
    {
        refused_field("create", "canvas", json!(5));
    }
);
source_test!(
    c018_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[webglRenderer-value18]",
    {
        refused_field(
            "create",
            "webglRenderer",
            Value::Object(serde_json::Map::from_iter([])),
        );
    }
);
source_test!(
    c019_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[osVersion-value19]",
    {
        refused_field("create", "osVersion", Value::Array(vec![]));
    }
);
source_test!(
    c020_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[deviceModel-12]",
    {
        refused_field("create", "deviceModel", json!(12));
    }
);
source_test!(
    c021_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[browser-value21]",
    {
        refused_field("create", "browser", Value::Array(vec![json!("Chrome")]));
    }
);
source_test!(
    c022_a_malformed_value_is_refused_on_create,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_create[proxy-1.2.3.4:8080]",
    {
        refused_field("create", "proxy", json!("1.2.3.4:8080"));
    }
);
source_test!(
    c000_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[tags-value0]",
    {
        refused_field(
            "update",
            "tags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
        );
    }
);
source_test!(
    c001_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[tags-value1]",
    {
        refused_field("update", "tags", Value::Array(vec![json!("ok"), json!(17)]));
    }
);
source_test!(
    c002_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[tags-work]",
    {
        refused_field("update", "tags", json!("work"));
    }
);
source_test!(
    c003_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[ipBased-value3]",
    {
        refused_field(
            "update",
            "ipBased",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                "a".into(),
                json!(1),
            )]))]),
        );
    }
);
source_test!(
    c004_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[clearOnStart-value4]",
    {
        refused_field("update", "clearOnStart", Value::Array(vec![Value::Null]));
    }
);
source_test!(
    c005_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[switchedOff-value5]",
    {
        refused_field(
            "update",
            "switchedOff",
            Value::Array(vec![Value::Array(vec![json!("nested")])]),
        );
    }
);
source_test!(
    c006_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[fontList-17]",
    {
        refused_field("update", "fontList", json!(17));
    }
);
source_test!(
    c007_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[fontList-value7]",
    {
        refused_field(
            "update",
            "fontList",
            Value::Object(serde_json::Map::from_iter([("Arial".into(), json!(true))])),
        );
    }
);
source_test!(
    c008_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[cores-nan]",
    {
        refused_field("update", "cores", number("NaN"));
    }
);
source_test!(
    c009_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[cores-inf]",
    {
        refused_field("update", "cores", number("Infinity"));
    }
);
source_test!(
    c010_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[cores--4]",
    {
        refused_field("update", "cores", json!(-4));
    }
);
source_test!(
    c011_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[memory--inf]",
    {
        refused_field("update", "memory", number("-Infinity"));
    }
);
source_test!(
    c012_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[seed-nan]",
    {
        refused_field("update", "seed", number("NaN"));
    }
);
source_test!(
    c013_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[mediaVideo-inf]",
    {
        refused_field("update", "mediaVideo", number("Infinity"));
    }
);
source_test!(
    c014_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[devicePixelRatio-big]",
    {
        refused_field("update", "devicePixelRatio", json!("big"));
    }
);
source_test!(
    c015_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[devicePixelRatio-0]",
    {
        refused_field("update", "devicePixelRatio", json!(0));
    }
);
source_test!(
    c016_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[maxTouchPoints-value16]",
    {
        refused_field("update", "maxTouchPoints", Value::Array(vec![]));
    }
);
source_test!(
    c017_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[canvas-5]",
    {
        refused_field("update", "canvas", json!(5));
    }
);
source_test!(
    c018_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[webglRenderer-value18]",
    {
        refused_field(
            "update",
            "webglRenderer",
            Value::Object(serde_json::Map::from_iter([])),
        );
    }
);
source_test!(
    c019_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[osVersion-value19]",
    {
        refused_field("update", "osVersion", Value::Array(vec![]));
    }
);
source_test!(
    c020_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[deviceModel-12]",
    {
        refused_field("update", "deviceModel", json!(12));
    }
);
source_test!(
    c021_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[browser-value21]",
    {
        refused_field("update", "browser", Value::Array(vec![json!("Chrome")]));
    }
);
source_test!(
    c022_a_malformed_value_is_refused_on_update,
    "tests/test_profile_schema.py::test_a_malformed_value_is_refused_on_update[proxy-1.2.3.4:8080]",
    {
        refused_field("update", "proxy", json!("1.2.3.4:8080"));
    }
);
source_test!(
    c000_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[tags-value0]",
    {
        refused_field(
            "batch",
            "tags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
        );
    }
);
source_test!(
    c001_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[tags-value1]",
    {
        refused_field("batch", "tags", Value::Array(vec![json!("ok"), json!(17)]));
    }
);
source_test!(
    c002_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[tags-work]",
    {
        refused_field("batch", "tags", json!("work"));
    }
);
source_test!(
    c003_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[ipBased-value3]",
    {
        refused_field(
            "batch",
            "ipBased",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                "a".into(),
                json!(1),
            )]))]),
        );
    }
);
source_test!(
    c004_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[clearOnStart-value4]",
    {
        refused_field("batch", "clearOnStart", Value::Array(vec![Value::Null]));
    }
);
source_test!(
    c005_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[switchedOff-value5]",
    {
        refused_field(
            "batch",
            "switchedOff",
            Value::Array(vec![Value::Array(vec![json!("nested")])]),
        );
    }
);
source_test!(
    c006_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[fontList-17]",
    {
        refused_field("batch", "fontList", json!(17));
    }
);
source_test!(
    c007_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[fontList-value7]",
    {
        refused_field(
            "batch",
            "fontList",
            Value::Object(serde_json::Map::from_iter([("Arial".into(), json!(true))])),
        );
    }
);
source_test!(
    c008_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[cores-nan]",
    {
        refused_field("batch", "cores", number("NaN"));
    }
);
source_test!(
    c009_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[cores-inf]",
    {
        refused_field("batch", "cores", number("Infinity"));
    }
);
source_test!(
    c010_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[cores--4]",
    {
        refused_field("batch", "cores", json!(-4));
    }
);
source_test!(
    c011_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[memory--inf]",
    {
        refused_field("batch", "memory", number("-Infinity"));
    }
);
source_test!(
    c012_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[seed-nan]",
    {
        refused_field("batch", "seed", number("NaN"));
    }
);
source_test!(
    c013_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[mediaVideo-inf]",
    {
        refused_field("batch", "mediaVideo", number("Infinity"));
    }
);
source_test!(
    c014_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[devicePixelRatio-big]",
    {
        refused_field("batch", "devicePixelRatio", json!("big"));
    }
);
source_test!(
    c015_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[devicePixelRatio-0]",
    {
        refused_field("batch", "devicePixelRatio", json!(0));
    }
);
source_test!(
    c016_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[maxTouchPoints-value16]",
    {
        refused_field("batch", "maxTouchPoints", Value::Array(vec![]));
    }
);
source_test!(
    c017_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[canvas-5]",
    {
        refused_field("batch", "canvas", json!(5));
    }
);
source_test!(
    c018_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[webglRenderer-value18]",
    {
        refused_field(
            "batch",
            "webglRenderer",
            Value::Object(serde_json::Map::from_iter([])),
        );
    }
);
source_test!(
    c019_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[osVersion-value19]",
    {
        refused_field("batch", "osVersion", Value::Array(vec![]));
    }
);
source_test!(
    c020_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[deviceModel-12]",
    {
        refused_field("batch", "deviceModel", json!(12));
    }
);
source_test!(
    c021_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[browser-value21]",
    {
        refused_field("batch", "browser", Value::Array(vec![json!("Chrome")]));
    }
);
source_test!(
    c022_a_malformed_entry_refuses_the_whole_batch,
    "tests/test_profile_schema.py::test_a_malformed_entry_refuses_the_whole_batch[proxy-1.2.3.4:8080]",
    {
        refused_field("batch", "proxy", json!("1.2.3.4:8080"));
    }
);
source_test!(
    c000_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[tags-value0]",
    {
        refused_field(
            "bulk",
            "tags",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([]))]),
        );
    }
);
source_test!(
    c001_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[tags-value1]",
    {
        refused_field("bulk", "tags", Value::Array(vec![json!("ok"), json!(17)]));
    }
);
source_test!(
    c002_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[tags-work]",
    {
        refused_field("bulk", "tags", json!("work"));
    }
);
source_test!(
    c003_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[ipBased-value3]",
    {
        refused_field(
            "bulk",
            "ipBased",
            Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                "a".into(),
                json!(1),
            )]))]),
        );
    }
);
source_test!(
    c004_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[clearOnStart-value4]",
    {
        refused_field("bulk", "clearOnStart", Value::Array(vec![Value::Null]));
    }
);
source_test!(
    c005_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[switchedOff-value5]",
    {
        refused_field(
            "bulk",
            "switchedOff",
            Value::Array(vec![Value::Array(vec![json!("nested")])]),
        );
    }
);
source_test!(
    c006_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[fontList-17]",
    {
        refused_field("bulk", "fontList", json!(17));
    }
);
source_test!(
    c007_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[fontList-value7]",
    {
        refused_field(
            "bulk",
            "fontList",
            Value::Object(serde_json::Map::from_iter([("Arial".into(), json!(true))])),
        );
    }
);
source_test!(
    c008_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[cores-nan]",
    {
        refused_field("bulk", "cores", number("NaN"));
    }
);
source_test!(
    c009_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[cores-inf]",
    {
        refused_field("bulk", "cores", number("Infinity"));
    }
);
source_test!(
    c010_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[cores--4]",
    {
        refused_field("bulk", "cores", json!(-4));
    }
);
source_test!(
    c011_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[memory--inf]",
    {
        refused_field("bulk", "memory", number("-Infinity"));
    }
);
source_test!(
    c012_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[seed-nan]",
    {
        refused_field("bulk", "seed", number("NaN"));
    }
);
source_test!(
    c013_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[mediaVideo-inf]",
    {
        refused_field("bulk", "mediaVideo", number("Infinity"));
    }
);
source_test!(
    c014_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[devicePixelRatio-big]",
    {
        refused_field("bulk", "devicePixelRatio", json!("big"));
    }
);
source_test!(
    c015_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[devicePixelRatio-0]",
    {
        refused_field("bulk", "devicePixelRatio", json!(0));
    }
);
source_test!(
    c016_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[maxTouchPoints-value16]",
    {
        refused_field("bulk", "maxTouchPoints", Value::Array(vec![]));
    }
);
source_test!(
    c017_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[canvas-5]",
    {
        refused_field("bulk", "canvas", json!(5));
    }
);
source_test!(
    c018_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[webglRenderer-value18]",
    {
        refused_field(
            "bulk",
            "webglRenderer",
            Value::Object(serde_json::Map::from_iter([])),
        );
    }
);
source_test!(
    c019_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[osVersion-value19]",
    {
        refused_field("bulk", "osVersion", Value::Array(vec![]));
    }
);
source_test!(
    c020_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[deviceModel-12]",
    {
        refused_field("bulk", "deviceModel", json!(12));
    }
);
source_test!(
    c021_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[browser-value21]",
    {
        refused_field("bulk", "browser", Value::Array(vec![json!("Chrome")]));
    }
);
source_test!(
    c022_a_malformed_bulk_patch_reaches_no_profile,
    "tests/test_profile_schema.py::test_a_malformed_bulk_patch_reaches_no_profile[proxy-1.2.3.4:8080]",
    {
        refused_field("bulk", "proxy", json!("1.2.3.4:8080"));
    }
);
source_test!(
    c000_a_non_string_tag_never_reaches_the_rendered_listing,
    "tests/test_profile_schema.py::test_a_non_string_tag_never_reaches_the_rendered_listing",
    {
        let tmp = common::TempDirectory::new("contract-tags-list").expect("root");
        http(
            tmp.path(),
            "POST",
            "/api/profiles",
            json!({"name":"poisoned","tags":[{},17,null]}),
        );
        let (s, rows) = http(tmp.path(), "GET", "/api/profiles", Value::Null);
        assert_eq!(s, 200);
        for row in rows.as_array().expect("listing") {
            assert!(
                row["tags"]
                    .as_array()
                    .expect("tags")
                    .iter()
                    .all(Value::is_string)
            );
        }
    }
);
source_test!(
    c000_no_non_finite_number_is_ever_stored,
    "tests/test_profile_schema.py::test_no_non_finite_number_is_ever_stored",
    {
        let tmp = common::TempDirectory::new("contract-finite").expect("root");
        for field in ["cores", "memory", "seed", "mediaVideo", "mediaAudio"] {
            for bad in ["NaN", "Infinity", "-Infinity"] {
                let mut body = json!({"name":format!("{field}-{bad}")});
                body[field] = number(bad);
                let (s, _) = http(tmp.path(), "POST", "/api/profiles", body);
                assert!((400..500).contains(&s));
            }
        }
        assert!(ids(tmp.path(), "dashboard").is_empty());
        assert!(ids(tmp.path(), "profiles").is_empty());
    }
);
source_test!(
    c000_a_profile_the_engine_cannot_model_is_refused_not_half_written,
    "tests/test_profile_schema.py::test_a_profile_the_engine_cannot_model_is_refused_not_half_written",
    {
        refused_field("create", "fontList", json!(17));
    }
);
source_test!(
    c000_a_conversion_failure_refuses_the_write_rather_than_dropping_the_twin,
    "tests/test_profile_schema.py::test_a_conversion_failure_refuses_the_write_rather_than_dropping_the_twin",
    {
        conversion_failure(false);
    }
);
source_test!(
    c000_an_import_that_cannot_be_mirrored_is_refused_as_a_client_error,
    "tests/test_profile_schema.py::test_an_import_that_cannot_be_mirrored_is_refused_as_a_client_error",
    {
        conversion_failure(true);
    }
);
source_test!(
    c000_every_stored_profile_has_its_engine_twin,
    "tests/test_profile_schema.py::test_every_stored_profile_has_its_engine_twin[/api/profiles-body0]",
    {
        let tmp = common::TempDirectory::new("contract-twins").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles",
            Value::Object(serde_json::Map::from_iter([(
                "name".into(),
                json!("single"),
            )])),
        );
        assert_eq!(s, 200, "{v}");
        valid_twin(tmp.path());
    }
);
source_test!(
    c001_every_stored_profile_has_its_engine_twin,
    "tests/test_profile_schema.py::test_every_stored_profile_has_its_engine_twin[/api/profiles/batch-body1]",
    {
        let tmp = common::TempDirectory::new("contract-twins").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles/batch",
            Value::Object(serde_json::Map::from_iter([(
                "profiles".into(),
                Value::Array(vec![Value::Object(serde_json::Map::from_iter([(
                    "name".into(),
                    json!("batched"),
                )]))]),
            )])),
        );
        assert_eq!(s, 200, "{v}");
        valid_twin(tmp.path());
    }
);
source_test!(
    c000_a_minimal_payload_is_still_a_valid_profile,
    "tests/test_profile_schema.py::test_a_minimal_payload_is_still_a_valid_profile",
    {
        let tmp = common::TempDirectory::new("contract-minimal").expect("root");
        let p = made(tmp.path(), json!({"name":"minimal"}));
        assert!(p["tags"].is_array());
        assert!(p["startupUrls"].is_string());
        valid_twin(tmp.path());
    }
);
source_test!(
    c000_a_fully_populated_profile_round_trips,
    "tests/test_profile_schema.py::test_a_fully_populated_profile_round_trips",
    {
        full_roundtrip();
    }
);
source_test!(
    c000_an_empty_list_and_an_empty_string_are_still_valid,
    "tests/test_profile_schema.py::test_an_empty_list_and_an_empty_string_are_still_valid",
    {
        let tmp = common::TempDirectory::new("contract-empties").expect("root");
        made(
            tmp.path(),
            json!({"name":"empties","tags":[],"notes":"","fontList":"","ipBased":[],"startupUrls":""}),
        );
        valid_twin(tmp.path());
    }
);
source_test!(
    c000_an_explicit_null_is_still_a_missing_field_not_a_bad_one,
    "tests/test_profile_schema.py::test_an_explicit_null_is_still_a_missing_field_not_a_bad_one",
    {
        let tmp = common::TempDirectory::new("contract-nulls").expect("root");
        made(
            tmp.path(),
            json!({"name":"nulls","proxy":null,"notes":null,"tags":null}),
        );
        valid_twin(tmp.path());
    }
);
source_test!(
    c000_a_corrupt_record_does_not_take_an_unrelated_view_down,
    "tests/test_profile_schema.py::test_a_corrupt_record_does_not_take_an_unrelated_view_down[/api/profiles]",
    {
        corrupt_view("/api/profiles");
    }
);
source_test!(
    c001_a_corrupt_record_does_not_take_an_unrelated_view_down,
    "tests/test_profile_schema.py::test_a_corrupt_record_does_not_take_an_unrelated_view_down[/api/profiles/health]",
    {
        corrupt_view("/api/profiles/health");
    }
);
source_test!(
    c002_a_corrupt_record_does_not_take_an_unrelated_view_down,
    "tests/test_profile_schema.py::test_a_corrupt_record_does_not_take_an_unrelated_view_down[/api/profiles/collisions]",
    {
        corrupt_view("/api/profiles/collisions");
    }
);
source_test!(
    c000_writing_a_corrupt_record_back_says_which_field_is_wrong,
    "tests/test_profile_schema.py::test_writing_a_corrupt_record_back_says_which_field_is_wrong",
    {
        let tmp = common::TempDirectory::new("contract-corrupt-edit").expect("root");
        let id = corrupt(tmp.path());
        let before = snapshot(tmp.path());
        let (s, v) = put(tmp.path(), &json!(id), json!({"name":"renamed"}));
        assert!((400..500).contains(&s), "{v}");
        assert!(
            ["fontList", "tags", "cores"]
                .iter()
                .any(|f| v.to_string().contains(f))
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
);
source_test!(
    c000_a_corrupt_record_can_be_repaired_by_sending_good_values,
    "tests/test_profile_schema.py::test_a_corrupt_record_can_be_repaired_by_sending_good_values",
    {
        let tmp = common::TempDirectory::new("contract-corrupt-repair").expect("root");
        let id = corrupt(tmp.path());
        let (s, v) = put(
            tmp.path(),
            &json!(id),
            json!({"name":"repaired","fontList":"Arial","tags":["work"],"cores":8}),
        );
        assert_eq!(s, 200, "{v}");
        assert_eq!(read(tmp.path(), "dashboard", &id)["cores"], 8);
        valid_twin(tmp.path());
    }
);
source_test!(
    c000_the_revision_conflict_still_refuses_a_stale_write,
    "tests/test_profile_schema.py::test_the_revision_conflict_still_refuses_a_stale_write",
    {
        revision(true);
    }
);
source_test!(
    c000_an_unrelated_edit_still_preserves_the_proxy_password,
    "tests/test_profile_schema.py::test_an_unrelated_edit_still_preserves_the_proxy_password",
    {
        password_edit("preserve");
    }
);
source_test!(
    c000_an_empty_password_still_clears_it,
    "tests/test_profile_schema.py::test_an_empty_password_still_clears_it",
    {
        password_edit("clear");
    }
);
source_test!(
    c000_a_new_password_still_replaces_it,
    "tests/test_profile_schema.py::test_a_new_password_still_replaces_it",
    {
        password_edit("replace");
    }
);
source_test!(
    c000_a_null_proxy_still_removes_it,
    "tests/test_profile_schema.py::test_a_null_proxy_still_removes_it",
    {
        password_edit("remove");
    }
);
source_test!(
    c000_the_schema_covers_every_writable_field,
    "tests/test_profile_schema.py::test_the_schema_covers_every_writable_field",
    {
        let schema: Value = serde_json::from_str(include_str!("../../src/http/field_schema.json"))
            .expect("native field schema");
        let client = schema["client_fields"].as_array().expect("client fields");
        for field in client {
            assert!(
                schema["fields"]
                    .get(field.as_str().expect("field"))
                    .is_some()
                    || schema["server_owned"]
                        .as_array()
                        .expect("server owned")
                        .contains(field),
                "writable field lacks schema: {field}"
            );
        }
        let emitted = conversion::to_dashboard(&profile("schema"));
        for field in emitted.as_object().expect("actual converter fields").keys() {
            assert!(
                schema["fields"].get(field).is_some()
                    || schema["server_owned"]
                        .as_array()
                        .expect("owned fields")
                        .contains(&json!(field)),
                "converter exposes an untyped writable field: {field}"
            );
        }
    }
);
source_test!(
    c000_an_unknown_field_is_still_refused,
    "tests/test_profile_schema.py::test_an_unknown_field_is_still_refused",
    {
        let tmp = common::TempDirectory::new("contract-unknown").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles",
            json!({"name":"a","nonsuch":1}),
        );
        assert_eq!(s, 422);
        assert!(v.to_string().contains("nonsuch"));
    }
);
source_test!(
    c000_an_unknown_engine_is_still_refused,
    "tests/test_profile_schema.py::test_an_unknown_engine_is_still_refused",
    {
        let tmp = common::TempDirectory::new("contract-engine").expect("root");
        let (s, v) = http(
            tmp.path(),
            "POST",
            "/api/profiles",
            json!({"name":"a","engine":"bogus"}),
        );
        assert_eq!(s, 422);
        assert!(v.to_string().contains("bogus"));
    }
);
