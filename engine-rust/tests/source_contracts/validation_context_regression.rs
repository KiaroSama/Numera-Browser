use super::*;
source_test!(
    c000_context_deserialized_from_json_has_the_same_semantics,
    "tests/test_validation_context_regression.py::test_context_deserialized_from_json_has_the_same_semantics[engine record]",
    {
        let kind = serde_json::from_str::<String>("\"engine record\"").expect("decoded context");
        let value = if kind == write_contract::ENGINE_RECORD {
            record()
        } else {
            json!({"notes":"ok"})
        };
        assert!(problems(&value, Some(&kind)).is_empty());
        write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect("decoded context valid");
    }
);
source_test!(
    c001_context_deserialized_from_json_has_the_same_semantics,
    "tests/test_validation_context_regression.py::test_context_deserialized_from_json_has_the_same_semantics[dashboard patch]",
    {
        let kind = serde_json::from_str::<String>("\"dashboard patch\"").expect("decoded context");
        let value = if kind == write_contract::ENGINE_RECORD {
            record()
        } else {
            json!({"notes":"ok"})
        };
        assert!(problems(&value, Some(&kind)).is_empty());
        write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect("decoded context valid");
    }
);
source_test!(
    c000_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-None-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Null;
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c001_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-None-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Null;
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c002_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-None-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Null;
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c003_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-17-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!(17);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c004_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-17-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!(17);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c005_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-17-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!(17);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c006_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-not a record-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!("not a record");
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c007_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-not a record-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!("not a record");
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c008_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-not a record-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!("not a record");
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c009_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-value3-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c010_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-value3-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c011_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-value3-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c012_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-value4-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![json!(1)]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c013_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-value4-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![json!(1)]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c014_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[False-value4-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if false {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![json!(1)]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c015_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-None-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Null;
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c016_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-None-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Null;
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c017_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-None-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Null;
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c018_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-17-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!(17);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c019_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-17-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!(17);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c020_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-17-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!(17);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c021_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-not a record-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!("not a record");
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c022_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-not a record-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!("not a record");
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c023_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-not a record-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = json!("not a record");
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c024_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-value3-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c025_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-value3-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c026_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-value3-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c027_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-value4-engine record]",
    {
        let kind = "engine record".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![json!(1)]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c028_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-value4-dashboard patch]",
    {
        let kind = "dashboard patch".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![json!(1)]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c029_wrong_declared_container_is_a_write_refusal,
    "tests/test_validation_context_regression.py::test_wrong_declared_container_is_a_write_refusal[True-value4-engine object]",
    {
        let kind = "engine object".to_owned();
        let kind = if true {
            serde_json::from_str::<String>(&serde_json::to_string(&kind).expect("encode"))
                .expect("decode")
        } else {
            kind
        };
        let value = Value::Array(vec![json!(1)]);
        let e = write_contract::require(Input::Json(&value), "profile", Some(&kind))
            .expect_err("wrong container");
        assert!(e.to_string().contains("profile"));
    }
);
source_test!(
    c000_complete_mappings_cannot_become_partial_patches,
    "tests/test_validation_context_regression.py::test_complete_mappings_cannot_become_partial_patches[None]",
    {
        let mut p = record();
        p["fingerprint"] = Value::Null;
        let before = p.clone();
        assert!(
            write_contract::require(
                Input::Json(&p),
                "profile",
                Some(write_contract::ENGINE_RECORD)
            )
            .expect_err("bad fingerprint")
            .to_string()
            .contains("fingerprint")
        );
        assert_eq!(p, before);
    }
);
source_test!(
    c001_complete_mappings_cannot_become_partial_patches,
    "tests/test_validation_context_regression.py::test_complete_mappings_cannot_become_partial_patches[17]",
    {
        let mut p = record();
        p["fingerprint"] = json!(17);
        let before = p.clone();
        assert!(
            write_contract::require(
                Input::Json(&p),
                "profile",
                Some(write_contract::ENGINE_RECORD)
            )
            .expect_err("bad fingerprint")
            .to_string()
            .contains("fingerprint")
        );
        assert_eq!(p, before);
    }
);
source_test!(
    c002_complete_mappings_cannot_become_partial_patches,
    "tests/test_validation_context_regression.py::test_complete_mappings_cannot_become_partial_patches[bad2]",
    {
        let mut p = record();
        p["fingerprint"] = Value::Array(vec![]);
        let before = p.clone();
        assert!(
            write_contract::require(
                Input::Json(&p),
                "profile",
                Some(write_contract::ENGINE_RECORD)
            )
            .expect_err("bad fingerprint")
            .to_string()
            .contains("fingerprint")
        );
        assert_eq!(p, before);
    }
);
source_test!(
    c003_complete_mappings_cannot_become_partial_patches,
    "tests/test_validation_context_regression.py::test_complete_mappings_cannot_become_partial_patches[fingerprint]",
    {
        let mut p = record();
        p["fingerprint"] = json!("fingerprint");
        let before = p.clone();
        assert!(
            write_contract::require(
                Input::Json(&p),
                "profile",
                Some(write_contract::ENGINE_RECORD)
            )
            .expect_err("bad fingerprint")
            .to_string()
            .contains("fingerprint")
        );
        assert_eq!(p, before);
    }
);
source_test!(
    c000_missing_fingerprint_is_refused_in_complete_context,
    "tests/test_validation_context_regression.py::test_missing_fingerprint_is_refused_in_complete_context",
    {
        let mut p = record();
        p.as_object_mut().expect("record").remove("fingerprint");
        assert!(
            write_contract::require(
                Input::Json(&p),
                "profile",
                Some(write_contract::ENGINE_RECORD)
            )
            .expect_err("missing fingerprint")
            .to_string()
            .contains("fingerprint")
        );
    }
);
source_test!(
    c000_mapping_subclasses_are_supported,
    "tests/test_validation_context_regression.py::test_mapping_subclasses_are_supported",
    {
        let pairs = record()
            .as_object()
            .expect("record")
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();
        let mapping = Value::Object(serde_json::Map::from_iter(pairs));
        write_contract::require(
            Input::Json(&mapping),
            "profile",
            Some(write_contract::ENGINE_RECORD),
        )
        .expect("mapping adapter");
        write_contract::require(
            Input::Json(&json!({"notes":"ok"})),
            "profile",
            Some(write_contract::DASHBOARD_PATCH),
        )
        .expect("patch mapping");
    }
);
source_test!(
    c000_fractional_compatibility_is_preserved,
    "tests/test_validation_context_regression.py::test_fractional_compatibility_is_preserved[0.25]",
    {
        let p = conversion::to_engine(&json!({"name":"memory","memory":json!(0.25)}), 1000.0)
            .expect("fractional memory");
        write_contract::require(
            Input::Profile(&p),
            "profile",
            Some(write_contract::ENGINE_OBJECT),
        )
        .expect("valid object");
        let rec = serde_json::to_value(&p).expect("record");
        write_contract::require(
            Input::Json(&rec),
            "profile",
            Some(write_contract::ENGINE_RECORD),
        )
        .expect("valid record");
        let restored =
            conversion::to_engine(&conversion::to_dashboard(&p), 1001.0).expect("round trip");
        assert_eq!(
            restored.fingerprint.device_memory.as_f64(),
            json!(0.25).as_f64()
        );
    }
);
source_test!(
    c001_fractional_compatibility_is_preserved,
    "tests/test_validation_context_regression.py::test_fractional_compatibility_is_preserved[0.5]",
    {
        let p = conversion::to_engine(&json!({"name":"memory","memory":json!(0.5)}), 1000.0)
            .expect("fractional memory");
        write_contract::require(
            Input::Profile(&p),
            "profile",
            Some(write_contract::ENGINE_OBJECT),
        )
        .expect("valid object");
        let rec = serde_json::to_value(&p).expect("record");
        write_contract::require(
            Input::Json(&rec),
            "profile",
            Some(write_contract::ENGINE_RECORD),
        )
        .expect("valid record");
        let restored =
            conversion::to_engine(&conversion::to_dashboard(&p), 1001.0).expect("round trip");
        assert_eq!(
            restored.fingerprint.device_memory.as_f64(),
            json!(0.5).as_f64()
        );
    }
);
source_test!(
    c002_fractional_compatibility_is_preserved,
    "tests/test_validation_context_regression.py::test_fractional_compatibility_is_preserved[1]",
    {
        let p = conversion::to_engine(&json!({"name":"memory","memory":json!(1)}), 1000.0)
            .expect("fractional memory");
        write_contract::require(
            Input::Profile(&p),
            "profile",
            Some(write_contract::ENGINE_OBJECT),
        )
        .expect("valid object");
        let rec = serde_json::to_value(&p).expect("record");
        write_contract::require(
            Input::Json(&rec),
            "profile",
            Some(write_contract::ENGINE_RECORD),
        )
        .expect("valid record");
        let restored =
            conversion::to_engine(&conversion::to_dashboard(&p), 1001.0).expect("round trip");
        assert_eq!(
            restored.fingerprint.device_memory.as_f64(),
            json!(1).as_f64()
        );
    }
);
source_test!(
    c003_fractional_compatibility_is_preserved,
    "tests/test_validation_context_regression.py::test_fractional_compatibility_is_preserved[16]",
    {
        let p = conversion::to_engine(&json!({"name":"memory","memory":json!(16)}), 1000.0)
            .expect("fractional memory");
        write_contract::require(
            Input::Profile(&p),
            "profile",
            Some(write_contract::ENGINE_OBJECT),
        )
        .expect("valid object");
        let rec = serde_json::to_value(&p).expect("record");
        write_contract::require(
            Input::Json(&rec),
            "profile",
            Some(write_contract::ENGINE_RECORD),
        )
        .expect("valid record");
        let restored =
            conversion::to_engine(&conversion::to_dashboard(&p), 1001.0).expect("round trip");
        assert_eq!(
            restored.fingerprint.device_memory.as_f64(),
            json!(16).as_f64()
        );
    }
);
source_test!(
    c000_patch_omission_is_not_a_complete_record_omission,
    "tests/test_validation_context_regression.py::test_patch_omission_is_not_a_complete_record_omission",
    {
        assert!(
            write_contract::require(
                Input::Json(&json!({})),
                "profile",
                Some(write_contract::DASHBOARD_PATCH)
            )
            .is_ok()
        );
        assert!(
            write_contract::require(
                Input::Json(&json!({})),
                "profile",
                Some(write_contract::ENGINE_RECORD)
            )
            .expect_err("complete omission")
            .to_string()
            .contains("fingerprint")
        );
    }
);
source_test!(
    c000_invalid_context_still_rejects_programmer_mistakes,
    "tests/test_validation_context_regression.py::test_invalid_context_still_rejects_programmer_mistakes",
    {
        assert!(matches!(
            write_contract::require(Input::Json(&json!({})), "profile", Some("arbitrary")),
            Err(write_contract::Error::UnknownShape(_))
        ));
    }
);
