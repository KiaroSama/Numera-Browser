use super::*;
source_test!(
    a_malformed_engine_record_stays_a_complete_record_case_000,
    "tests/test_write_contract_context.py::test_a_malformed_engine_record_stays_a_complete_record[record0-a null fingerprint]",
    {
        let record = json!({"id":"p1","name":"one","fingerprint":Value::Null});
        assert_eq!(
            write_contract::shape(Input::Json(&record), None).expect("shape"),
            write_contract::ENGINE_RECORD
        );
        assert!(!problems(&record).is_empty());
    }
);
source_test!(
    a_malformed_engine_record_stays_a_complete_record_case_001,
    "tests/test_write_contract_context.py::test_a_malformed_engine_record_stays_a_complete_record[record1-a numeric fingerprint]",
    {
        let record = json!({"id":"p1","name":"one","fingerprint":json!(123)});
        assert_eq!(
            write_contract::shape(Input::Json(&record), None).expect("shape"),
            write_contract::ENGINE_RECORD
        );
        assert!(!problems(&record).is_empty());
    }
);
source_test!(
    a_malformed_engine_record_stays_a_complete_record_case_002,
    "tests/test_write_contract_context.py::test_a_malformed_engine_record_stays_a_complete_record[record2-a text fingerprint]",
    {
        let record = json!({"id":"p1","name":"one","fingerprint":json!("text")});
        assert_eq!(
            write_contract::shape(Input::Json(&record), None).expect("shape"),
            write_contract::ENGINE_RECORD
        );
        assert!(!problems(&record).is_empty());
    }
);
source_test!(
    a_malformed_engine_record_stays_a_complete_record_case_003,
    "tests/test_write_contract_context.py::test_a_malformed_engine_record_stays_a_complete_record[record3-a list fingerprint]",
    {
        let record = json!({"id":"p1","name":"one","fingerprint":json!([])});
        assert_eq!(
            write_contract::shape(Input::Json(&record), None).expect("shape"),
            write_contract::ENGINE_RECORD
        );
        assert!(!problems(&record).is_empty());
    }
);
source_test!(
    an_entry_point_may_declare_the_shape_outright,
    "tests/test_write_contract_context.py::test_an_entry_point_may_declare_the_shape_outright",
    {
        let record = json!({"id":"p1","name":"one"});
        assert_eq!(
            write_contract::shape(Input::Json(&record), None).expect("shape"),
            write_contract::DASHBOARD_PATCH
        );
        assert!(problems(&record).is_empty());
        assert!(
            !write_contract::profile_problems(
                Input::Json(&record),
                "profile",
                Some(write_contract::ENGINE_RECORD)
            )
            .expect("complete guard")
            .is_empty()
        );
    }
);
source_test!(
    an_unknown_declared_shape_is_refused_not_guessed,
    "tests/test_write_contract_context.py::test_an_unknown_declared_shape_is_refused_not_guessed",
    {
        let record = json!({"name":"n"});
        let error = write_contract::shape(Input::Json(&record), Some("something-else"))
            .expect_err("unknown shape");
        assert!(matches!(error, write_contract::Error::UnknownShape(_)));
        assert!(error.to_string().contains("unknown profile shape"));
    }
);
source_test!(
    an_ordinary_patch_is_still_a_patch,
    "tests/test_write_contract_context.py::test_an_ordinary_patch_is_still_a_patch",
    {
        let record = json!({"name":"renamed"});
        assert_eq!(
            write_contract::shape(Input::Json(&record), None).expect("shape"),
            write_contract::DASHBOARD_PATCH
        );
        assert!(problems(&record).is_empty());
    }
);
source_test!(
    a_profile_object_is_unaffected,
    "tests/test_write_contract_context.py::test_a_profile_object_is_unaffected",
    {
        let p = convert(json!({"name":"one","os":"windows"}));
        assert_eq!(
            write_contract::shape(Input::Profile(&p), None).expect("shape"),
            write_contract::ENGINE_OBJECT
        );
        assert!(
            write_contract::profile_problems(Input::Profile(&p), "profile", None)
                .expect("object guard")
                .is_empty()
        );
    }
);
source_test!(
    a_fractional_device_memory_survives_conversion_case_008,
    "tests/test_write_contract_context.py::test_a_fractional_device_memory_survives_conversion[0.25]",
    {
        assert_eq!(
            convert(json!({"name":"n","os":"windows","memory":0.25}))
                .fingerprint
                .device_memory,
            json!(0.25)
        );
    }
);
source_test!(
    a_fractional_device_memory_survives_conversion_case_009,
    "tests/test_write_contract_context.py::test_a_fractional_device_memory_survives_conversion[0.5]",
    {
        assert_eq!(
            convert(json!({"name":"n","os":"windows","memory":0.5}))
                .fingerprint
                .device_memory,
            json!(0.5)
        );
    }
);
source_test!(
    a_whole_device_memory_stays_a_whole_number_case_010,
    "tests/test_write_contract_context.py::test_a_whole_device_memory_stays_a_whole_number[16-16]",
    {
        let memory = convert(json!({"name":"n","os":"windows","memory":16}))
            .fingerprint
            .device_memory;
        assert_eq!(memory, json!(16));
        assert!(
            memory.as_i64().is_some(),
            "whole memory must remain an integer"
        );
    }
);
source_test!(
    a_whole_device_memory_stays_a_whole_number_case_011,
    "tests/test_write_contract_context.py::test_a_whole_device_memory_stays_a_whole_number[8-8]",
    {
        let memory = convert(json!({"name":"n","os":"windows","memory":"8"}))
            .fingerprint
            .device_memory;
        assert_eq!(memory, json!(8));
        assert!(
            memory.as_i64().is_some(),
            "whole memory must remain an integer"
        );
    }
);
source_test!(
    a_whole_device_memory_stays_a_whole_number_case_012,
    "tests/test_write_contract_context.py::test_a_whole_device_memory_stays_a_whole_number[4.0-4]",
    {
        let memory = convert(json!({"name":"n","os":"windows","memory":4.0}))
            .fingerprint
            .device_memory;
        assert_eq!(memory, json!(4));
        assert!(
            memory.as_i64().is_some(),
            "whole memory must remain an integer"
        );
    }
);
source_test!(
    a_fractional_device_memory_round_trips_through_a_real_store_case_013,
    "tests/test_write_contract_context.py::test_a_fractional_device_memory_round_trips_through_a_real_store[0.25]",
    {
        let temporary =
            common::TempDirectory::new("source-fraction-store").expect("temporary root");
        let repo = ProfileRepo::new(temporary.path().to_owned(), None);
        let p = convert(json!({"name":"small-device","os":"windows","memory":0.25}));
        repo.save(p, 1000.0).expect("save fractional memory");
        let paths = json_files(temporary.path(), "profiles");
        assert_eq!(paths.len(), 1);
        let bytes: Value = serde_json::from_slice(&fs::read(&paths[0]).expect("stored bytes"))
            .expect("stored JSON");
        assert_eq!(bytes["fingerprint"]["device_memory"], json!(0.25));
        assert_eq!(
            repo.list(1001.0).expect("read store")[0]
                .fingerprint
                .device_memory,
            json!(0.25)
        );
    }
);
source_test!(
    a_fractional_device_memory_round_trips_through_a_real_store_case_014,
    "tests/test_write_contract_context.py::test_a_fractional_device_memory_round_trips_through_a_real_store[0.5]",
    {
        let temporary =
            common::TempDirectory::new("source-fraction-store").expect("temporary root");
        let repo = ProfileRepo::new(temporary.path().to_owned(), None);
        let p = convert(json!({"name":"small-device","os":"windows","memory":0.5}));
        repo.save(p, 1000.0).expect("save fractional memory");
        let paths = json_files(temporary.path(), "profiles");
        assert_eq!(paths.len(), 1);
        let bytes: Value = serde_json::from_slice(&fs::read(&paths[0]).expect("stored bytes"))
            .expect("stored JSON");
        assert_eq!(bytes["fingerprint"]["device_memory"], json!(0.5));
        assert_eq!(
            repo.list(1001.0).expect("read store")[0]
                .fingerprint
                .device_memory,
            json!(0.5)
        );
    }
);
source_test!(
    the_dashboard_guard_and_the_complete_guard_agree_case_015,
    "tests/test_write_contract_context.py::test_the_dashboard_guard_and_the_complete_guard_agree[0.25]",
    {
        let record = json!({"name":"n","os":"windows","memory":0.25});
        assert!(problems(&record).is_empty());
        let p = convert(record);
        assert!(
            write_contract::profile_problems(Input::Profile(&p), "profile", None)
                .expect("complete guard")
                .is_empty()
        );
    }
);
source_test!(
    the_dashboard_guard_and_the_complete_guard_agree_case_016,
    "tests/test_write_contract_context.py::test_the_dashboard_guard_and_the_complete_guard_agree[0.5]",
    {
        let record = json!({"name":"n","os":"windows","memory":0.5});
        assert!(problems(&record).is_empty());
        let p = convert(record);
        assert!(
            write_contract::profile_problems(Input::Profile(&p), "profile", None)
                .expect("complete guard")
                .is_empty()
        );
    }
);
source_test!(
    the_dashboard_guard_and_the_complete_guard_agree_case_017,
    "tests/test_write_contract_context.py::test_the_dashboard_guard_and_the_complete_guard_agree[16]",
    {
        let record = json!({"name":"n","os":"windows","memory":16});
        assert!(problems(&record).is_empty());
        let p = convert(record);
        assert!(
            write_contract::profile_problems(Input::Profile(&p), "profile", None)
                .expect("complete guard")
                .is_empty()
        );
    }
);
source_test!(
    device_memory_allows_exactly_the_reported_fractions,
    "tests/test_write_contract_context.py::test_device_memory_allows_exactly_the_reported_fractions",
    {
        let p = policy();
        assert_eq!(p["tables"]["Fingerprint"]["device_memory"]["kind"], "int");
        assert_eq!(
            p["tables"]["Fingerprint"]["device_memory"]["fractions"],
            json!([0.25, 0.5])
        );
        for value in [0.25, 0.5] {
            assert!(problems(&json!({"memory":value})).is_empty());
        }
        for value in [0.125, 0.75, 3.5] {
            assert!(!problems(&json!({"memory":value})).is_empty());
        }
    }
);
source_test!(
    counts_are_still_whole_case_019,
    "tests/test_write_contract_context.py::test_counts_are_still_whole[hardware_concurrency]",
    {
        assert_eq!(
            policy()["tables"]["Fingerprint"]["hardware_concurrency"]["kind"],
            "int"
        );
        let mut record = serde_json::to_value(profile("n", 1, "windows")).expect("complete record");
        record["fingerprint"]["hardware_concurrency"] = json!(1.5);
        assert!(
            problems(&record)
                .iter()
                .any(|p| p.contains("whole number") && p.contains("hardware_concurrency"))
        );
    }
);
source_test!(
    counts_are_still_whole_case_020,
    "tests/test_write_contract_context.py::test_counts_are_still_whole[screen_width]",
    {
        assert_eq!(
            policy()["tables"]["Fingerprint"]["screen_width"]["kind"],
            "int"
        );
        let mut record = serde_json::to_value(profile("n", 1, "windows")).expect("complete record");
        record["fingerprint"]["screen_width"] = json!(1.5);
        assert!(
            problems(&record)
                .iter()
                .any(|p| p.contains("whole number") && p.contains("screen_width"))
        );
    }
);
source_test!(
    counts_are_still_whole_case_021,
    "tests/test_write_contract_context.py::test_counts_are_still_whole[cameras]",
    {
        assert_eq!(policy()["tables"]["Fingerprint"]["cameras"]["kind"], "int");
        let mut record = serde_json::to_value(profile("n", 1, "windows")).expect("complete record");
        record["fingerprint"]["cameras"] = json!(1.5);
        assert!(
            problems(&record)
                .iter()
                .any(|p| p.contains("whole number") && p.contains("cameras"))
        );
    }
);
source_test!(
    counts_are_still_whole_case_022,
    "tests/test_write_contract_context.py::test_counts_are_still_whole[max_touch_points]",
    {
        assert_eq!(
            policy()["tables"]["Fingerprint"]["max_touch_points"]["kind"],
            "int"
        );
        let mut record = serde_json::to_value(profile("n", 1, "windows")).expect("complete record");
        record["fingerprint"]["max_touch_points"] = json!(1.5);
        assert!(
            problems(&record)
                .iter()
                .any(|p| p.contains("whole number") && p.contains("max_touch_points"))
        );
    }
);
source_test!(
    an_unusable_memory_still_falls_back_case_023,
    "tests/test_write_contract_context.py::test_an_unusable_memory_still_falls_back[inf]",
    {
        assert_eq!(
            convert(numeric_record("memory", nonfinite("Infinity")))
                .fingerprint
                .device_memory,
            json!(16)
        );
    }
);
source_test!(
    an_unusable_memory_still_falls_back_case_024,
    "tests/test_write_contract_context.py::test_an_unusable_memory_still_falls_back[nan]",
    {
        assert_eq!(
            convert(numeric_record("memory", nonfinite("NaN")))
                .fingerprint
                .device_memory,
            json!(16)
        );
    }
);
source_test!(
    an_unusable_memory_still_falls_back_case_025,
    "tests/test_write_contract_context.py::test_an_unusable_memory_still_falls_back[lots]",
    {
        assert_eq!(
            convert(json!({"name":"n","os":"windows","memory":json!("lots")}))
                .fingerprint
                .device_memory,
            json!(16)
        );
    }
);
source_test!(
    an_unusable_memory_still_falls_back_case_026,
    "tests/test_write_contract_context.py::test_an_unusable_memory_still_falls_back[None]",
    {
        assert_eq!(
            convert(json!({"name":"n","os":"windows","memory":Value::Null}))
                .fingerprint
                .device_memory,
            json!(16)
        );
    }
);
