use super::*;
fn refuse_core_field(field: &str, value: Value, fragment: &str) {
    let temp = common::TempDirectory::new("source-core-refusal").expect("temporary root");
    let mut record = serde_json::to_value(profile("alpha", 1, "windows")).expect("record");
    record[field] = value;
    let error = write_contract::require(
        Input::Json(&record),
        "profile",
        Some(write_contract::ENGINE_RECORD),
    )
    .expect_err("complete-record contract must refuse the original uncoerced field");
    assert!(error.to_string().contains(fragment), "{error}");
    // The dynamically malformed Python object cannot be constructed with the
    // typed Rust model. Exercise the actual raw complete-record import door,
    // including its deserialization and real repository write attempt.
    let path = temp.path().join("profile-import.json");
    fs::write(&path, serde_json::to_vec(&record).expect("input JSON")).expect("input file");
    let repo = ProfileRepo::new(temp.path().to_owned(), None);
    assert!(
        repo.import_file(&path, 1000.0).is_err(),
        "malformed complete record reached disk"
    );
    assert!(json_files(temp.path(), "profiles").is_empty());
    assert!(json_files(temp.path(), "dashboard").is_empty());
}
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_000,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[name-123-must be text]",
    {
        refuse_core_field("name", json!(123), "must be text");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_001,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[name-None-must be text]",
    {
        refuse_core_field("name", Value::Null, "must be text");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_002,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[name-value2-must be text]",
    {
        refuse_core_field("name", json!(["a"]), "must be text");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_003,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[notes-7-must be text]",
    {
        refuse_core_field("notes", json!(7), "must be text");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_004,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[engine-value4-must be text]",
    {
        refuse_core_field("engine", json!({}), "must be text");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_005,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[tags-value5-every entry]",
    {
        refuse_core_field("tags", json!([{}]), "every entry");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_006,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[tags-value6-every entry]",
    {
        refuse_core_field("tags", json!(["ok", 17]), "every entry");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_007,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[tags-notalist-must be a list]",
    {
        refuse_core_field("tags", json!("notalist"), "must be a list");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_008,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[startup_urls-value8-every entry]",
    {
        refuse_core_field("startup_urls", json!([null]), "every entry");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_009,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[extensions-value9-every entry]",
    {
        refuse_core_field("extensions", json!([1]), "every entry");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_010,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[launch_args-value10-every entry]",
    {
        refuse_core_field("launch_args", json!([[]]), "every entry");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_011,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[ip_based-value11-every entry]",
    {
        refuse_core_field("ip_based", json!([{}]), "every entry");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_012,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[clear_on_start-value12-every entry]",
    {
        refuse_core_field("clear_on_start", json!([2.5]), "every entry");
    }
);
source_test!(
    the_core_write_path_refuses_what_the_api_refuses_case_013,
    "tests/test_write_contract_core.py::test_the_core_write_path_refuses_what_the_api_refuses[switched_off-value13-every entry]",
    {
        refuse_core_field("switched_off", json!([[120]]), "every entry");
    }
);
source_test!(
    a_batch_names_the_entry_that_failed,
    "tests/test_write_contract_core.py::test_a_batch_names_the_entry_that_failed",
    {
        use numera_core::cli::profile_commands::Profiles;
        let temp = common::TempDirectory::new("source-core-batch-refusal").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let config = ConfigStore::new(temp.path().to_owned());
        let installed = BTreeMap::new();
        let commands = Profiles {
            repo: &repo,
            config: &config,
            installed: &installed,
            host: "source-domain-host",
        };
        let rows = [json!({"name":"good"}), json!({"name":"bad","tags":[{}]})];
        let error = commands
            .import_vendor(&rows, 1000.0, || Ok(7))
            .err()
            .expect("batch must refuse malformed tags");
        assert!(error.to_string().contains("profiles[1]"), "{error}");
        assert!(repo.list(1001.0).expect("list").is_empty());
        assert!(json_files(temp.path(), "dashboard").is_empty());
    }
);
source_test!(
    an_ordinary_profile_is_untouched,
    "tests/test_write_contract_core.py::test_an_ordinary_profile_is_untouched",
    {
        let temp = common::TempDirectory::new("source-core-good-profile").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let mut p = profile("Ünïcodé ✓", 1, "windows");
        p.notes = "".into();
        p.tags = vec!["a".into(), "b".into()];
        p.startup_urls = vec!["https://x.test".into()];
        let saved = repo.save(p, 1000.0).expect("save healthy profile");
        assert_eq!(saved.name, "Ünïcodé ✓");
        assert_eq!(repo.list(1001.0).expect("list")[0].tags, ["a", "b"]);
        temp.cleanup().expect("remove owned temporary root");
    }
);
source_test!(
    an_empty_list_and_an_empty_string_are_fine,
    "tests/test_write_contract_core.py::test_an_empty_list_and_an_empty_string_are_fine",
    {
        let temp = common::TempDirectory::new("source-core-empty-profile").expect("temporary root");
        let repo = ProfileRepo::new(temp.path().to_owned(), None);
        let mut p = profile("", 1, "windows");
        p.tags.clear();
        p.notes.clear();
        assert_eq!(repo.save(p, 1000.0).expect("save empty values").name, "");
    }
);
source_test!(
    an_enormous_integer_is_refused_not_an_overflow_case_017,
    "tests/test_write_contract_core.py::test_an_enormous_integer_is_refused_not_an_overflow[10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000]",
    {
        let found = problems(&json!({"name":"n","cores":huge(400, false)}));
        assert!(found.iter().any(|p| p.contains("between")), "{found:?}");
    }
);
source_test!(
    an_enormous_integer_is_refused_not_an_overflow_case_018,
    "tests/test_write_contract_core.py::test_an_enormous_integer_is_refused_not_an_overflow[-10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000]",
    {
        let found = problems(&json!({"name":"n","cores":huge(400, true)}));
        assert!(found.iter().any(|p| p.contains("between")), "{found:?}");
    }
);
source_test!(
    an_enormous_integer_is_refused_not_an_overflow_case_019,
    "tests/test_write_contract_core.py::test_an_enormous_integer_is_refused_not_an_overflow[1606938044258990275541962092341162602522202993782792835301376]",
    {
        let found = problems(
            &json!({"name":"n","cores":serde_json::from_str::<Value>("1606938044258990275541962092341162602522202993782792835301376").expect("large integer")}),
        );
        assert!(found.iter().any(|p| p.contains("between")), "{found:?}");
    }
);
source_test!(
    a_non_finite_float_is_refused_case_020,
    "tests/test_write_contract_core.py::test_a_non_finite_float_is_refused[nan]",
    {
        let found = problems(&numeric_record("memory", nonfinite("NaN")));
        assert!(found.iter().any(|p| p.contains("finite")), "{found:?}");
    }
);
source_test!(
    a_non_finite_float_is_refused_case_021,
    "tests/test_write_contract_core.py::test_a_non_finite_float_is_refused[inf]",
    {
        let found = problems(&numeric_record("memory", nonfinite("Infinity")));
        assert!(found.iter().any(|p| p.contains("finite")), "{found:?}");
    }
);
source_test!(
    a_non_finite_float_is_refused_case_022,
    "tests/test_write_contract_core.py::test_a_non_finite_float_is_refused[-inf]",
    {
        let found = problems(&numeric_record("memory", nonfinite("-Infinity")));
        assert!(found.iter().any(|p| p.contains("finite")), "{found:?}");
    }
);
source_test!(
    a_number_inside_the_range_is_accepted,
    "tests/test_write_contract_core.py::test_a_number_inside_the_range_is_accepted",
    {
        assert!(problems(&json!({"name":"n","cores":8,"memory":16})).is_empty());
    }
);
source_test!(
    a_boolean_is_not_a_number,
    "tests/test_write_contract_core.py::test_a_boolean_is_not_a_number",
    {
        let found = problems(&json!({"name":"n","cores":true}));
        assert!(
            found.iter().any(|p| p.contains("must be a number")),
            "{found:?}"
        );
    }
);
source_test!(
    a_legacy_name_renders_as_text_case_025,
    "tests/test_write_contract_core.py::test_a_legacy_name_renders_as_text[123-123]",
    {
        let view = read_view::readable(&json!({"id":"x","name":json!(123)}));
        assert_eq!(view["name"], "123");
        assert!(view["name"].is_string());
    }
);
source_test!(
    a_legacy_name_renders_as_text_case_026,
    "tests/test_write_contract_core.py::test_a_legacy_name_renders_as_text[None-]",
    {
        let view = read_view::readable(&json!({"id":"x","name":Value::Null}));
        assert_eq!(view["name"], "");
        assert!(view["name"].is_string());
    }
);
source_test!(
    a_legacy_name_renders_as_text_case_027,
    "tests/test_write_contract_core.py::test_a_legacy_name_renders_as_text[12.5-12.5]",
    {
        let view = read_view::readable(&json!({"id":"x","name":json!(12.5)}));
        assert_eq!(view["name"], "12.5");
        assert!(view["name"].is_string());
    }
);
source_test!(
    a_legacy_name_renders_as_text_case_028,
    "tests/test_write_contract_core.py::test_a_legacy_name_renders_as_text[True-true]",
    {
        let view = read_view::readable(&json!({"id":"x","name":json!(true)}));
        assert_eq!(view["name"], "true");
        assert!(view["name"].is_string());
    }
);
source_test!(
    a_legacy_name_renders_as_text_case_029,
    "tests/test_write_contract_core.py::test_a_legacy_name_renders_as_text[stored4-]",
    {
        let view = read_view::readable(&json!({"id":"x","name":json!({})}));
        assert_eq!(view["name"], "");
        assert!(view["name"].is_string());
    }
);
source_test!(
    a_legacy_name_renders_as_text_case_030,
    "tests/test_write_contract_core.py::test_a_legacy_name_renders_as_text[stored5-]",
    {
        let view = read_view::readable(&json!({"id":"x","name":json!([])}));
        assert_eq!(view["name"], "");
        assert!(view["name"].is_string());
    }
);
source_test!(
    the_read_view_never_touches_the_stored_record,
    "tests/test_write_contract_core.py::test_the_read_view_never_touches_the_stored_record",
    {
        let record = json!({"id":"x","name":123,"tags":["a",{}]});
        let before = serde_json::to_vec(&record).expect("before");
        read_view::readable(&record);
        assert_eq!(serde_json::to_vec(&record).expect("after"), before);
    }
);
source_test!(
    a_healthy_record_passes_through_unchanged,
    "tests/test_write_contract_core.py::test_a_healthy_record_passes_through_unchanged",
    {
        let record = json!({"id":"x","name":"alpha","tags":["a"]});
        assert_eq!(read_view::readable(&record), record);
        assert_eq!(read_view::readable(&record)["tags"], record["tags"]);
    }
);
source_test!(
    the_dashboard_view_uses_its_own_field_names,
    "tests/test_write_contract_core.py::test_the_dashboard_view_uses_its_own_field_names",
    {
        let view = read_view::readable_with(
            &json!({"id":"x","name":null,"folder":7,"startupUrls":5,"tags":["a",{}]}),
            &["name", "folder", "startupUrls"],
            &["tags"],
        );
        assert_eq!(view["name"], "");
        assert_eq!(view["folder"], "7");
        assert_eq!(view["startupUrls"], "5");
        assert_eq!(view["tags"], json!(["a"]));
    }
);
source_test!(
    a_bad_record_does_not_hide_the_healthy_ones,
    "tests/test_write_contract_core.py::test_a_bad_record_does_not_hide_the_healthy_ones",
    {
        let rows = [
            json!({"id":"a","name":123}),
            json!({"id":"b","name":"fine"}),
        ];
        let view = rows.iter().map(read_view::readable).collect::<Vec<_>>();
        assert_eq!(
            view.iter()
                .map(|p| p["id"].as_str().expect("ID"))
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(view[0]["name"], "123");
    }
);
