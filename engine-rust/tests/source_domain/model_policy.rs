use super::*;

// Serialization is Rust's schema boundary: use the real model's derived
// Serialize implementation so a model change cannot be hidden by a hand list.
fn model_fields(model: &str) -> BTreeSet<String> {
    let p = profile("n", 1, "windows");
    let value = match model {
        "Profile" => serde_json::to_value(p).expect("serialize Profile"),
        "Fingerprint" => serde_json::to_value(p.fingerprint).expect("serialize Fingerprint"),
        "Proxy" => serde_json::to_value(
            Proxy::from_record(&json!({"server":"http://h:1"}), false).expect("proxy"),
        )
        .expect("serialize Proxy"),
        _ => panic!("unrecognized model"),
    };
    value
        .as_object()
        .expect("serialized model mapping")
        .keys()
        .cloned()
        .collect()
}
fn missing(fields: &BTreeSet<String>, model: &str) -> BTreeSet<String> {
    let p = policy();
    let policed = p["tables"][model]
        .as_object()
        .expect("model table")
        .keys()
        .cloned()
        .collect();
    fields.difference(&policed).cloned().collect()
}
fn covered(model: &str) {
    assert!(
        missing(&model_fields(model), model).is_empty(),
        "{model} has unpoliced fields"
    );
}
fn phantom(model: &str) {
    let p = policy();
    let policed: BTreeSet<String> = p["tables"][model]
        .as_object()
        .expect("model table")
        .keys()
        .cloned()
        .collect();
    assert!(
        policed.difference(&model_fields(model)).next().is_none(),
        "{model} policy names missing fields"
    );
}
fn explained(model: &str) {
    for (field, rule) in policy()["tables"][model].as_object().expect("model table") {
        if [json!("owned"), json!("normalized")].contains(&rule["kind"]) {
            assert!(
                !rule["why"]
                    .as_str()
                    .expect("exemption reason")
                    .trim()
                    .is_empty(),
                "{model}.{field}"
            );
        }
    }
}
source_test!(
    every_model_field_has_a_policy_profile,
    "tests/test_model_policy_coverage.py::test_every_model_field_has_a_policy[Profile]",
    {
        covered("Profile");
    }
);
source_test!(
    every_model_field_has_a_policy_fingerprint,
    "tests/test_model_policy_coverage.py::test_every_model_field_has_a_policy[Fingerprint]",
    {
        covered("Fingerprint");
    }
);
source_test!(
    every_model_field_has_a_policy_proxy,
    "tests/test_model_policy_coverage.py::test_every_model_field_has_a_policy[Proxy]",
    {
        covered("Proxy");
    }
);
source_test!(
    no_policy_names_a_field_that_is_gone_profile,
    "tests/test_model_policy_coverage.py::test_no_policy_names_a_field_that_is_gone[Profile]",
    {
        phantom("Profile");
    }
);
source_test!(
    no_policy_names_a_field_that_is_gone_fingerprint,
    "tests/test_model_policy_coverage.py::test_no_policy_names_a_field_that_is_gone[Fingerprint]",
    {
        phantom("Fingerprint");
    }
);
source_test!(
    no_policy_names_a_field_that_is_gone_proxy,
    "tests/test_model_policy_coverage.py::test_no_policy_names_a_field_that_is_gone[Proxy]",
    {
        phantom("Proxy");
    }
);
source_test!(
    a_field_exempt_from_checking_says_why_profile,
    "tests/test_model_policy_coverage.py::test_a_field_exempt_from_checking_says_why[Profile]",
    {
        explained("Profile");
    }
);
source_test!(
    a_field_exempt_from_checking_says_why_fingerprint,
    "tests/test_model_policy_coverage.py::test_a_field_exempt_from_checking_says_why[Fingerprint]",
    {
        explained("Fingerprint");
    }
);
source_test!(
    a_field_exempt_from_checking_says_why_proxy,
    "tests/test_model_policy_coverage.py::test_a_field_exempt_from_checking_says_why[Proxy]",
    {
        explained("Proxy");
    }
);
source_test!(
    the_coverage_guard_actually_bites,
    "tests/test_model_policy_coverage.py::test_the_coverage_guard_actually_bites",
    {
        let stand_in = BTreeSet::from(["os".into(), "webgpu_adapter".into()]);
        assert_eq!(
            missing(&stand_in, "Fingerprint"),
            BTreeSet::from(["webgpu_adapter".into()])
        );
    }
);
source_test!(
    every_rule_kind_in_the_table_is_one_the_validator_implements,
    "tests/test_model_policy_coverage.py::test_every_rule_kind_in_the_table_is_one_the_validator_implements",
    {
        let p = policy();
        for (model, table) in p["tables"].as_object().expect("policy tables") {
            for field in table.as_object().expect("model table").keys() {
                let mut record = serde_json::to_value(profile("n", 1, "windows")).expect("record");
                record["proxy"] =
                    json!({"server":"http://h:1","username":null,"password":null,"country":null});
                match model.as_str() {
                    "Profile" => record[field] = json!({}),
                    "Fingerprint" => record["fingerprint"][field] = json!({}),
                    "Proxy" => record["proxy"][field] = json!({}),
                    _ => panic!("unknown model policy"),
                }
                let found = problems(&record);
                assert!(
                    !found
                        .iter()
                        .any(|error| error.contains("Unknown field validation policy")),
                    "{model}.{field}: {found:?}"
                );
            }
        }
    }
);
source_test!(
    a_number_rule_resolves_to_exactly_one_set_of_bounds,
    "tests/test_model_policy_coverage.py::test_a_number_rule_resolves_to_exactly_one_set_of_bounds",
    {
        let p = policy();
        let rule = &p["tables"]["Fingerprint"]["hardware_concurrency"];
        assert_eq!(rule["bounds"], p["ranges"]["cores"]);
        assert_eq!(
            rule["bounds"],
            p["ranges"][rule["dash"].as_str().expect("dashboard numeric key")]
        );
        for (sent, accepted) in [(1, true), (512, true), (0, false), (513, false)] {
            let flat = json!({"cores":sent});
            let mut full = serde_json::to_value(profile("n", 1, "windows")).expect("record");
            full["fingerprint"]["hardware_concurrency"] = json!(sent);
            assert_eq!(problems(&flat).is_empty(), accepted);
            assert_eq!(problems(&full).is_empty(), accepted);
        }
    }
);
source_test!(
    the_policy_covers_the_fields_the_serialized_record_carries,
    "tests/test_model_policy_coverage.py::test_the_policy_covers_the_fields_the_serialized_record_carries",
    {
        let record = serde_json::to_value(profile("n", 1, "windows")).expect("record");
        let p = policy();
        for (value, table) in [
            (&record, &p["tables"]["Profile"]),
            (&record["fingerprint"], &p["tables"]["Fingerprint"]),
        ] {
            for key in value.as_object().expect("serialized fields").keys() {
                assert!(table.get(key).is_some(), "unpoliced serialized field {key}");
            }
        }
    }
);
source_test!(
    a_dataclass_field_list_is_what_is_being_diffed,
    "tests/test_model_policy_coverage.py::test_a_dataclass_field_list_is_what_is_being_diffed",
    {
        let p = policy();
        assert_eq!(
            model_fields("Proxy"),
            p["tables"]["Proxy"]
                .as_object()
                .expect("proxy policy")
                .keys()
                .cloned()
                .collect()
        );
        assert!(p["tables"]["Proxy"].get("password").is_some());
    }
);
