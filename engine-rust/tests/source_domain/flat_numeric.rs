use super::*;
source_test!(
    every_flat_number_has_exactly_one_model_rule,
    "tests/test_flat_numeric_policy.py::test_every_flat_number_has_exactly_one_model_rule",
    {
        let p = policy();
        let table = p["tables"]["Fingerprint"]
            .as_object()
            .expect("fingerprint policy");
        let mut by_dash = BTreeMap::<String, Vec<&Value>>::new();
        for rule in table.values() {
            if let Some(dash) = rule["dash"].as_str().filter(|s| !s.is_empty()) {
                by_dash.entry(dash.into()).or_default().push(rule);
            }
        }
        assert_eq!(
            by_dash.keys().cloned().collect::<BTreeSet<_>>(),
            p["ranges"]
                .as_object()
                .expect("number ranges")
                .keys()
                .cloned()
                .collect()
        );
        for (dash, rules) in by_dash {
            assert_eq!(rules.len(), 1, "{dash}");
            assert_eq!(rules[0]["bounds"], p["ranges"][&dash]);
            let low = p["ranges"][&dash][0].clone();
            let mut input = json!({"name":"n"});
            input[&dash] = low;
            assert!(
                validation::flat_number_problems(&input)
                    .expect("flat validator")
                    .is_empty()
            );
            input[&dash] = json!(true);
            assert!(
                validation::flat_number_problems(&input)
                    .expect("flat validator")
                    .iter()
                    .any(|p| p.contains(&format!("'{dash}'")))
            );
        }
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_001,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[1.5-cores]",
    {
        let input = json!({"name":"n", "cores":1.5});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'cores'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_002,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[1.5-mediaVideo]",
    {
        let input = json!({"name":"n", "mediaVideo":1.5});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'mediaVideo'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_003,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[1.5-mediaAudio]",
    {
        let input = json!({"name":"n", "mediaAudio":1.5});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'mediaAudio'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_004,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[1.5-mediaSpeakers]",
    {
        let input = json!({"name":"n", "mediaSpeakers":1.5});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'mediaSpeakers'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_005,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[1.5-maxTouchPoints]",
    {
        let input = json!({"name":"n", "maxTouchPoints":1.5});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'maxTouchPoints'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_006,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[1.5-seed]",
    {
        let input = json!({"name":"n", "seed":1.5});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'seed'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_007,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[2.25-cores]",
    {
        let input = json!({"name":"n", "cores":2.25});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'cores'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_008,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[2.25-mediaVideo]",
    {
        let input = json!({"name":"n", "mediaVideo":2.25});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'mediaVideo'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_009,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[2.25-mediaAudio]",
    {
        let input = json!({"name":"n", "mediaAudio":2.25});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'mediaAudio'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_010,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[2.25-mediaSpeakers]",
    {
        let input = json!({"name":"n", "mediaSpeakers":2.25});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'mediaSpeakers'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_011,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[2.25-maxTouchPoints]",
    {
        let input = json!({"name":"n", "maxTouchPoints":2.25});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'maxTouchPoints'"),
            "{found:?}"
        );
    }
);
source_test!(
    a_fractional_count_is_refused_before_conversion_case_012,
    "tests/test_flat_numeric_policy.py::test_a_fractional_count_is_refused_before_conversion[2.25-seed]",
    {
        let input = json!({"name":"n", "seed":2.25});
        let found = problems(&input);
        assert!(!found.is_empty());
        assert!(
            found[0].contains("whole number") && found[0].contains("'seed'"),
            "{found:?}"
        );
    }
);
source_test!(
    memory_refuses_a_fraction_no_device_reports,
    "tests/test_flat_numeric_policy.py::test_memory_refuses_a_fraction_no_device_reports",
    {
        assert!(!problems(&json!({"name":"n","memory":3.5})).is_empty());
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_014,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields0]",
    {
        let sent = json!(8);
        let input = json!({"name":"n","os":"windows", "cores":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "cores"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_015,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields1]",
    {
        let sent = json!(2.0);
        let input = json!({"name":"n","os":"windows", "cores":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "cores"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_016,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields2]",
    {
        let sent = json!(7);
        let input = json!({"name":"n","os":"windows", "seed":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "seed"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_017,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields3]",
    {
        let sent = json!(9007199254740992_u64);
        let input = json!({"name":"n","os":"windows", "seed":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "seed"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_018,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields4]",
    {
        let sent = json!(0.25);
        let input = json!({"name":"n","os":"windows", "memory":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "memory"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_019,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields5]",
    {
        let sent = json!(0.5);
        let input = json!({"name":"n","os":"windows", "memory":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "memory"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_020,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields6]",
    {
        let sent = json!(16);
        let input = json!({"name":"n","os":"windows", "memory":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "memory"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_021,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields7]",
    {
        let sent = json!(1.5);
        let input = json!({"name":"n","os":"windows", "devicePixelRatio":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "devicePixelRatio"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_022,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields8]",
    {
        let sent = json!(2.75);
        let input = json!({"name":"n","os":"windows", "devicePixelRatio":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "devicePixelRatio"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_023,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields9]",
    {
        let sent = json!(0);
        let input = json!({"name":"n","os":"windows", "mediaVideo":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "mediaVideo"), &sent);
    }
);
source_test!(
    legal_values_still_pass_and_survive_conversion_case_024,
    "tests/test_flat_numeric_policy.py::test_legal_values_still_pass_and_survive_conversion[fields10]",
    {
        let sent = json!(10);
        let input = json!({"name":"n","os":"windows", "maxTouchPoints":sent});
        assert!(problems(&input).is_empty());
        let fp = convert(input);
        assert_number_eq(&native_number(&fp, "maxTouchPoints"), &sent);
    }
);
source_test!(
    non_numbers_and_out_of_range_stay_refused_case_025,
    "tests/test_flat_numeric_policy.py::test_non_numbers_and_out_of_range_stay_refused[True]",
    {
        assert!(!problems(&json!({"name":"n","cores":json!(true)})).is_empty());
    }
);
source_test!(
    non_numbers_and_out_of_range_stay_refused_case_026,
    "tests/test_flat_numeric_policy.py::test_non_numbers_and_out_of_range_stay_refused[8]",
    {
        assert!(!problems(&json!({"name":"n","cores":json!("8")})).is_empty());
    }
);
source_test!(
    non_numbers_and_out_of_range_stay_refused_case_027,
    "tests/test_flat_numeric_policy.py::test_non_numbers_and_out_of_range_stay_refused[inf]",
    {
        assert!(!problems(&numeric_record("cores", nonfinite("Infinity"))).is_empty());
    }
);
source_test!(
    non_numbers_and_out_of_range_stay_refused_case_028,
    "tests/test_flat_numeric_policy.py::test_non_numbers_and_out_of_range_stay_refused[nan]",
    {
        assert!(!problems(&numeric_record("cores", nonfinite("NaN"))).is_empty());
    }
);
source_test!(
    non_numbers_and_out_of_range_stay_refused_case_029,
    "tests/test_flat_numeric_policy.py::test_non_numbers_and_out_of_range_stay_refused[-1]",
    {
        assert!(!problems(&json!({"name":"n","cores":json!(-1)})).is_empty());
    }
);
source_test!(
    non_numbers_and_out_of_range_stay_refused_case_030,
    "tests/test_flat_numeric_policy.py::test_non_numbers_and_out_of_range_stay_refused[1000000]",
    {
        assert!(!problems(&json!({"name":"n","cores":json!(1000000)})).is_empty());
    }
);
source_test!(
    omitted_and_null_still_mean_not_changing,
    "tests/test_flat_numeric_policy.py::test_omitted_and_null_still_mean_not_changing",
    {
        assert!(problems(&json!({"name":"n"})).is_empty());
        assert!(problems(&json!({"name":"n","cores":null,"mediaVideo":null,"mediaAudio":null,"mediaSpeakers":null,"maxTouchPoints":null,"seed":null,"memory":null,"devicePixelRatio":null})).is_empty());
    }
);
source_test!(
    an_unprintable_integer_is_refused_not_crashed_case_032,
    "tests/test_flat_numeric_policy.py::test_an_unprintable_integer_is_refused_not_crashed[fields0]",
    {
        let input = json!({"name":"n", "cores":huge(5000, false)});
        let error = write_contract::require(Input::Json(&input), "profile", None)
            .expect_err("unprintable integer must be refused");
        assert!(matches!(error, write_contract::Error::Refused(_)));
        assert!(error.to_string().contains("bits"), "{error}");
    }
);
source_test!(
    an_unprintable_integer_is_refused_not_crashed_case_033,
    "tests/test_flat_numeric_policy.py::test_an_unprintable_integer_is_refused_not_crashed[fields1]",
    {
        let input = json!({"name":"n", "seed":huge(5000, true)});
        let error = write_contract::require(Input::Json(&input), "profile", None)
            .expect_err("unprintable integer must be refused");
        assert!(matches!(error, write_contract::Error::Refused(_)));
        assert!(error.to_string().contains("bits"), "{error}");
    }
);
source_test!(
    an_unprintable_integer_is_refused_not_crashed_case_034,
    "tests/test_flat_numeric_policy.py::test_an_unprintable_integer_is_refused_not_crashed[fields2]",
    {
        let input = json!({"name":"n", "name":huge(5000, false)});
        let error = write_contract::require(Input::Json(&input), "profile", None)
            .expect_err("unprintable integer must be refused");
        assert!(matches!(error, write_contract::Error::Refused(_)));
        assert!(error.to_string().contains("bits"), "{error}");
    }
);
source_test!(
    a_secret_is_still_not_quoted_back,
    "tests/test_flat_numeric_policy.py::test_a_secret_is_still_not_quoted_back",
    {
        let input = json!({"name":"n","proxy":{"host":"h","pass":huge(5000, false)}});
        let error = write_contract::require(Input::Json(&input), "profile", None)
            .expect_err("numeric credential must be refused");
        assert!(!error.to_string().contains("bits") && !error.to_string().contains("1000"));
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_036,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[cores-1.25]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "cores":json!(1.25)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("cores"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_037,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[cores-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "cores":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("cores"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_038,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[cores-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "cores":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("cores"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_039,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[cores-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("cores", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("cores"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_040,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[cores-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("cores", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("cores"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_041,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[cores--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "cores":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("cores"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_042,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[memory-1.25]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "memory":json!(1.25)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("memory"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_043,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[memory-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "memory":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("memory"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_044,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[memory-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "memory":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("memory"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_045,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[memory-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("memory", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("memory"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_046,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[memory-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("memory", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("memory"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_047,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[memory--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "memory":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("memory"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_048,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaVideo-1.25]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaVideo":json!(1.25)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaVideo"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_049,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaVideo-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaVideo":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaVideo"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_050,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaVideo-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaVideo":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaVideo"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_051,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaVideo-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("mediaVideo", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaVideo"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_052,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaVideo-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("mediaVideo", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaVideo"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_053,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaVideo--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaVideo":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaVideo"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_054,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaAudio-1.25]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaAudio":json!(1.25)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaAudio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_055,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaAudio-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaAudio":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaAudio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_056,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaAudio-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaAudio":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaAudio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_057,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaAudio-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("mediaAudio", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaAudio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_058,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaAudio-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("mediaAudio", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaAudio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_059,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaAudio--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaAudio":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaAudio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_060,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaSpeakers-1.25]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaSpeakers":json!(1.25)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaSpeakers"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_061,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaSpeakers-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaSpeakers":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaSpeakers"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_062,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaSpeakers-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaSpeakers":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaSpeakers"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_063,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaSpeakers-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("mediaSpeakers", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaSpeakers"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_064,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaSpeakers-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("mediaSpeakers", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaSpeakers"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_065,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[mediaSpeakers--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "mediaSpeakers":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("mediaSpeakers"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_066,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[maxTouchPoints-1.25]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "maxTouchPoints":json!(1.25)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("maxTouchPoints"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_067,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[maxTouchPoints-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "maxTouchPoints":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("maxTouchPoints"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_068,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[maxTouchPoints-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "maxTouchPoints":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("maxTouchPoints"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_069,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[maxTouchPoints-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("maxTouchPoints", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("maxTouchPoints"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_070,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[maxTouchPoints-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("maxTouchPoints", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("maxTouchPoints"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_071,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[maxTouchPoints--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "maxTouchPoints":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("maxTouchPoints"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_072,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[devicePixelRatio-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "devicePixelRatio":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("devicePixelRatio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_073,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[devicePixelRatio-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "devicePixelRatio":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("devicePixelRatio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_074,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[devicePixelRatio-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("devicePixelRatio", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("devicePixelRatio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_075,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[devicePixelRatio-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("devicePixelRatio", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("devicePixelRatio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_076,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[devicePixelRatio--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "devicePixelRatio":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("devicePixelRatio"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_077,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[seed-1.25]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "seed":json!(1.25)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("seed"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_078,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[seed-True]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "seed":json!(true)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("seed"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_079,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[seed-8]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "seed":json!("8")});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("seed"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_080,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[seed-nan]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("seed", nonfinite("NaN"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("seed"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_081,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[seed-inf]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = numeric_record("seed", nonfinite("Infinity"));
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("seed"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_server_conversion_refuses_a_number_it_would_change_case_082,
    "tests/test_flat_numeric_policy.py::test_the_server_conversion_refuses_a_number_it_would_change[seed--1]",
    {
        let temporary =
            common::TempDirectory::new("source-flat-conversion").expect("temporary root");
        let input = json!({"name":"n", "seed":json!(-1)});
        let before = input.clone();
        let store =
            numera_core::storage::dashboard::DashboardStore::new(temporary.path().to_owned(), None);
        let error = store
            .save(input.clone(), Default::default(), None, None, 1000.0)
            .expect_err("conversion door must refuse");
        assert!(error.to_string().contains("seed"), "{error}");
        assert_eq!(input, before, "caller input changed");
        assert!(json_files(temporary.path(), "profiles").is_empty());
        assert!(json_files(temporary.path(), "dashboard").is_empty());
    }
);
source_test!(
    the_core_conversion_stays_tolerant_for_legacy_readers,
    "tests/test_flat_numeric_policy.py::test_the_core_conversion_stays_tolerant_for_legacy_readers",
    {
        let mut input = json!({"name":"legacy","cores":"8","memory":null});
        input["mediaVideo"] = nonfinite("NaN");
        let fp = convert(input).fingerprint;
        assert_eq!(fp.hardware_concurrency, 8);
        assert_eq!(fp.device_memory, json!(16));
        assert_eq!(fp.cameras, 1);
    }
);
