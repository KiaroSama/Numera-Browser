use super::*;
fn saved(v: Value) -> Vec<String> {
    conversion::to_engine(&json!({"id":"p1","name":"p","startupUrls":v}), 1000.0)
        .expect("browser contract")
        .startup_urls
}
source_test!(
    test_a_url_inside_a_query_value_is_not_a_second_entry,
    "tests/test_startup_urls_stay_whole.py::test_a_url_inside_a_query_value_is_not_a_second_entry",
    {
        let u = "https://example.test/redirect?targets=https://a.test,https://b.test";
        assert_eq!(saved(json!(u)), vec![u]);
    }
);
source_test!(
    test_a_colon_in_a_path_segment_does_not_start_a_new_url,
    "tests/test_startup_urls_stay_whole.py::test_a_colon_in_a_path_segment_does_not_start_a_new_url",
    {
        let u = "https://example.test/red,blue:green";
        assert_eq!(saved(json!(u)), vec![u]);
    }
);
source_test!(
    test_a_comma_in_a_fragment_stays_in_the_fragment,
    "tests/test_startup_urls_stay_whole.py::test_a_comma_in_a_fragment_stays_in_the_fragment",
    {
        let u = "https://example.test/#section,two";
        assert_eq!(saved(json!(u)), vec![u]);
    }
);
source_test!(
    test_an_encoded_comma_is_neither_a_split_point_nor_decoded,
    "tests/test_startup_urls_stay_whole.py::test_an_encoded_comma_is_neither_a_split_point_nor_decoded",
    {
        let u = "https://example.test/s?q=a%2Cb,c%2Cd";
        assert_eq!(saved(json!(u)), vec![u]);
    }
);
source_test!(
    test_one_url_per_line_is_still_the_contract,
    "tests/test_startup_urls_stay_whole.py::test_one_url_per_line_is_still_the_contract",
    {
        assert_eq!(
            saved(json!(
                "https://example.test/redirect?targets=https://a.test,https://b.test\nhttps://example.test/red,blue:green"
            )),
            vec![
                "https://example.test/redirect?targets=https://a.test,https://b.test",
                "https://example.test/red,blue:green"
            ]
        );
    }
);
source_test!(
    test_about_blank_keeps_its_own_scheme_beside_a_comma_line,
    "tests/test_startup_urls_stay_whole.py::test_about_blank_keeps_its_own_scheme_beside_a_comma_line",
    {
        assert_eq!(
            saved(json!(
                "about:blank\nhttps://example.test/redirect?targets=https://a.test,https://b.test"
            )),
            vec![
                "about:blank",
                "https://example.test/redirect?targets=https://a.test,https://b.test"
            ]
        );
    }
);
source_test!(
    test_an_explicit_list_entry_is_never_split,
    "tests/test_startup_urls_stay_whole.py::test_an_explicit_list_entry_is_never_split",
    {
        assert_eq!(
            saved(json!([
                "https://example.test/redirect?targets=https://a.test,https://b.test",
                "https://example.test/red,blue:green"
            ])),
            vec![
                "https://example.test/redirect?targets=https://a.test,https://b.test",
                "https://example.test/red,blue:green"
            ]
        );
    }
);
source_test!(
    test_a_list_entry_holding_the_legacy_comma_form_is_one_entry,
    "tests/test_startup_urls_stay_whole.py::test_a_list_entry_holding_the_legacy_comma_form_is_one_entry",
    {
        assert_eq!(
            saved(json!(["one.test,two.test"])),
            vec!["https://one.test,two.test"]
        );
    }
);
source_test!(
    test_a_list_still_gets_a_missing_scheme_filled_in,
    "tests/test_startup_urls_stay_whole.py::test_a_list_still_gets_a_missing_scheme_filled_in",
    {
        assert_eq!(
            saved(json!([
                "https://example.test/redirect?targets=https://a.test,https://b.test",
                "two.test"
            ])),
            vec![
                "https://example.test/redirect?targets=https://a.test,https://b.test",
                "https://two.test"
            ]
        );
    }
);
source_test!(
    test_legacy_comma_separated_hosts_still_split,
    "tests/test_startup_urls_stay_whole.py::test_legacy_comma_separated_hosts_still_split",
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
    test_a_legacy_split_leaves_the_rest_of_the_line_alone,
    "tests/test_startup_urls_stay_whole.py::test_a_legacy_split_leaves_the_rest_of_the_line_alone",
    {
        assert_eq!(
            saved(json!("one.test,https://example.test/s?q=a%2Cb,c")),
            vec!["https://one.test", "https://example.test/s?q=a%2Cb,c"]
        );
    }
);
source_test!(
    test_a_saved_profile_launches_the_urls_that_were_typed,
    "tests/test_startup_urls_stay_whole.py::test_a_saved_profile_launches_the_urls_that_were_typed",
    {
        let typed = "https://example.test/redirect?targets=https://a.test,https://b.test\nhttps://example.test/red,blue:green\nhttps://example.test/s?q=a%2Cb,c%2Cd";
        let dir = fresh();
        let h = http::Http::new(dir.path());
        let v = h.request(
            "POST",
            "/api/profiles",
            json!({"name":"typed","startupUrls":typed}),
        );
        let id = v["id"].as_str().expect("browser contract");
        let twin: Value = serde_json::from_slice(
            &std::fs::read(dir.path().join(format!("profiles/{id}.json")))
                .expect("browser contract"),
        )
        .expect("browser contract");
        assert_eq!(
            twin["startup_urls"],
            json!(typed.lines().collect::<Vec<_>>())
        );
        let list = h.request("GET", "/api/profiles", Value::Null);
        assert_eq!(
            list.as_array()
                .expect("browser contract")
                .iter()
                .find(|v| v["id"] == id)
                .expect("browser contract")["startupUrls"],
            typed
        );
    }
);
source_test!(
    test_an_unrelated_edit_does_not_re_split_the_urls,
    "tests/test_startup_urls_stay_whole.py::test_an_unrelated_edit_does_not_re_split_the_urls",
    {
        let typed = "https://example.test/redirect?targets=https://a.test,https://b.test\nhttps://example.test/red,blue:green";
        let dir = fresh();
        let h = http::Http::new(dir.path());
        let v = h.request(
            "POST",
            "/api/profiles",
            json!({"name":"edited","startupUrls":typed}),
        );
        let id = v["id"].as_str().expect("browser contract");
        h.request(
            "PUT",
            &format!("/api/profiles/{id}"),
            json!({"notes":"renamed something else"}),
        );
        let twin: Value = serde_json::from_slice(
            &std::fs::read(dir.path().join(format!("profiles/{id}.json")))
                .expect("browser contract"),
        )
        .expect("browser contract");
        assert_eq!(
            twin["startup_urls"],
            json!(typed.lines().collect::<Vec<_>>())
        );
        let list = h.request("GET", "/api/profiles", Value::Null);
        assert_eq!(
            list.as_array()
                .expect("browser contract")
                .iter()
                .find(|v| v["id"] == id)
                .expect("browser contract")["startupUrls"],
            typed
        );
    }
);
source_test!(
    test_a_saved_legacy_profile_still_launches_both_hosts,
    "tests/test_startup_urls_stay_whole.py::test_a_saved_legacy_profile_still_launches_both_hosts",
    {
        let dir = fresh();
        let h = http::Http::new(dir.path());
        let v = h.request(
            "POST",
            "/api/profiles",
            json!({"name":"legacy","startupUrls":"one.test,two.test"}),
        );
        let id = v["id"].as_str().expect("browser contract");
        let twin: Value = serde_json::from_slice(
            &std::fs::read(dir.path().join(format!("profiles/{id}.json")))
                .expect("browser contract"),
        )
        .expect("browser contract");
        assert_eq!(
            twin["startup_urls"],
            json!(["https://one.test", "https://two.test"])
        );
    }
);
