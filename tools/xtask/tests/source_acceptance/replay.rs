use super::*;
source_test!(
    test_complete_replay_comparison_ignores_only_object_key_order,
    "tests/test_replay_contracts.py::test_complete_replay_comparison_ignores_only_object_key_order",
    {
        let huge = Value::Number(serde_json::Number::from_string_unchecked("9".repeat(400)));
        require_equal(&huge, &huge, "large integer")?;
        let expected = json!({"id":"real-generated-id","nested":{"enabled":true,"count":1},"items":["first","second"],"secret":"synthetic"});
        require_equal(
            &json!({"secret":"synthetic","items":["first","second"],"nested":{"count":1,"enabled":true},"id":"real-generated-id"}),
            &expected,
            "fixture",
        )?;
        let mut candidates = Vec::new();
        let mut v = expected.clone();
        v["id"] = json!("different-id");
        candidates.push(v);
        let mut v = expected.clone();
        v["nested"]["enabled"] = json!(1);
        candidates.push(v);
        let mut v = expected.clone();
        v["nested"]["count"] = json!(1.0);
        candidates.push(v);
        let mut v = expected.clone();
        v["items"] = json!(["second", "first"]);
        candidates.push(v);
        let mut v = expected.clone();
        v["secret"] = Value::Null;
        candidates.push(v);
        let mut v = expected.clone();
        v.as_object_mut()
            .ok_or("expected object missing")?
            .remove("nested");
        candidates.push(v);
        let mut v = expected.clone();
        v["unexpected"] = json!(true);
        candidates.push(v);
        for observed in candidates {
            assert!(
                require_equal(&observed, &expected, "fixture")
                    .err()
                    .ok_or("replay change accepted")?
                    .to_string()
                    .contains("Cross-runtime fixture differs")
            );
        }

        Ok(())
    }
);
source_test!(
    test_replay_comparison_refuses_nonfinite_values_without_printing_payloads,
    "tests/test_replay_contracts.py::test_replay_comparison_refuses_nonfinite_values_without_printing_payloads",
    {
        for raw in ["NaN", "Infinity", "-Infinity"] {
            let value = Value::Number(serde_json::Number::from_string_unchecked(raw.into()));
            assert!(
                require_equal(
                    &Value::Object(serde_json::Map::from_iter([(
                        "number".into(),
                        value.clone()
                    )])),
                    &Value::Object(serde_json::Map::from_iter([(
                        "number".into(),
                        value.clone()
                    )])),
                    "fixture"
                )
                .is_err()
            );
        }
        let error = require_equal(
            &json!({"password":"synthetic-secret"}),
            &json!({}),
            "fixture",
        )
        .err()
        .ok_or("secret mismatch accepted")?;
        assert!(!error.to_string().contains("synthetic-secret"));

        Ok(())
    }
);
