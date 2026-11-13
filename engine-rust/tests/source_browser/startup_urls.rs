use super::*;
fn saved(v: Value) -> Vec<String> {
    conversion::to_engine(&json!({"id":"p1","name":"p","startupUrls":v}), 1000.0)
        .expect("browser contract")
        .startup_urls
}
source_test!(
    test_a_comma_in_a_query_string_is_not_a_separator,
    "tests/test_startup_urls.py::test_a_comma_in_a_query_string_is_not_a_separator",
    {
        let u = "https://example.test/search?q=red,blue";
        assert_eq!(saved(json!(u)), vec![u]);
    }
);
source_test!(
    test_a_comma_in_a_path_is_not_a_separator,
    "tests/test_startup_urls.py::test_a_comma_in_a_path_is_not_a_separator",
    {
        let u = "https://example.test/maps/@52.5,13.4,12z";
        assert_eq!(saved(json!(u)), vec![u]);
    }
);
source_test!(
    test_an_encoded_comma_is_left_exactly_as_written,
    "tests/test_startup_urls.py::test_an_encoded_comma_is_left_exactly_as_written",
    {
        let u = "https://example.test/search?q=red%2Cblue";
        assert_eq!(saved(json!(u)), vec![u]);
    }
);
source_test!(
    test_one_url_per_line_is_the_contract,
    "tests/test_startup_urls.py::test_one_url_per_line_is_the_contract",
    {
        assert_eq!(
            saved(json!("https://one.test\nhttps://two.test/s?q=a,b")),
            vec!["https://one.test", "https://two.test/s?q=a,b"]
        );
    }
);
source_test!(
    test_a_list_is_taken_entry_by_entry,
    "tests/test_startup_urls.py::test_a_list_is_taken_entry_by_entry",
    {
        assert_eq!(
            saved(json!(["https://one.test/a,b", "two.test"])),
            vec!["https://one.test/a,b", "https://two.test"]
        );
    }
);
source_test!(
    test_a_missing_scheme_is_filled_in,
    "tests/test_startup_urls.py::test_a_missing_scheme_is_filled_in",
    {
        assert_eq!(
            saved(json!("example.test/path")),
            vec!["https://example.test/path"]
        );
    }
);
source_test!(
    test_about_blank_is_kept_as_a_scheme_of_its_own,
    "tests/test_startup_urls.py::test_about_blank_is_kept_as_a_scheme_of_its_own",
    {
        assert_eq!(
            saved(json!("about:blank\nhttps://one.test")),
            vec!["about:blank", "https://one.test"]
        );
    }
);
source_test!(
    test_legacy_comma_separated_entries_still_split_where_they_cannot_be_one_url,
    "tests/test_startup_urls.py::test_legacy_comma_separated_entries_still_split_where_they_cannot_be_one_url",
    {
        for v in ["https://one.test,https://two.test", "one.test,two.test"] {
            assert_eq!(
                saved(json!(v)),
                vec!["https://one.test", "https://two.test"]
            );
        }
    }
);
source_test!(
    test_nothing_in_means_nothing_out,
    "tests/test_startup_urls.py::test_nothing_in_means_nothing_out",
    {
        assert!(saved(json!("")).is_empty());
        assert!(saved(Value::Null).is_empty());
    }
);
