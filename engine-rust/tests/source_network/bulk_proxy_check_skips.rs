use super::*;
source_test!(
    c000_a_wholly_locked_selection_reports_skipped_instead_of_failing,
    "tests/test_bulk_proxy_check_skips.py::test_a_wholly_locked_selection_reports_skipped_instead_of_failing",
    {
        let tmp = common::TempDirectory::new("source-net-locked-sweep").expect("root");
        let ids = pool_sealed(tmp.path(), &["a.example", "b.example"]);
        let (s, body, checked) = pool_sweep(tmp.path(), json!({"ids":ids}));
        assert_eq!(s, 200, "{body}");
        assert_eq!(body["skipped"], 2);
        assert_eq!(body["tested"], 2);
        assert!(
            body["results"]
                .as_array()
                .expect("results")
                .iter()
                .all(|r| r["status"] == "skipped")
        );
        assert_eq!(
            body["results"]
                .as_array()
                .expect("results")
                .iter()
                .map(|r| r["id"].clone())
                .collect::<Vec<_>>(),
            ids
        );
        assert!(
            checked.is_empty(),
            "withheld credential reached health checker"
        );
    }
);
source_test!(
    c000_a_locked_sweep_leaves_every_health_field_alone,
    "tests/test_bulk_proxy_check_skips.py::test_a_locked_sweep_leaves_every_health_field_alone",
    {
        let tmp = common::TempDirectory::new("source-net-locked-health").expect("root");
        pool_sealed(tmp.path(), &["a.example", "b.example"]);
        let before = fs::read(tmp.path().join("proxies.json")).expect("original protected pool");
        let (s, body, checked) = pool_sweep(tmp.path(), json!({}));
        assert_eq!(s, 200, "{body}");
        assert!(checked.is_empty());
        assert_eq!(
            fs::read(tmp.path().join("proxies.json")).expect("preserved protected pool"),
            before
        );
    }
);
source_test!(
    c000_a_mixed_selection_tests_what_it_can_and_skips_the_rest,
    "tests/test_bulk_proxy_check_skips.py::test_a_mixed_selection_tests_what_it_can_and_skips_the_rest",
    {
        let tmp = common::TempDirectory::new("source-net-mixed-health").expect("root");
        let locked = pool_added(tmp.path(), &["locked.example"], "secret");
        let open = pool_added(tmp.path(), &["open.example"], "");
        repository(tmp.path())
            .enable_vault("master")
            .expect("vault seal");
        let original = pool_raw(tmp.path())
            .as_array()
            .expect("pool")
            .iter()
            .find(|p| p["host"] == "locked.example")
            .expect("locked record")
            .clone();
        let mut ids = locked.clone();
        ids.extend(open.clone());
        let (s, body, checked) = pool_sweep(tmp.path(), json!({"ids":ids}));
        assert_eq!(s, 200, "{body}");
        assert_eq!(checked, ["open.example"]);
        let results = body["results"].as_array().expect("results");
        assert_eq!(
            results
                .iter()
                .find(|r| r["id"] == open[0])
                .expect("open result")["status"],
            "ok"
        );
        assert_eq!(
            results
                .iter()
                .find(|r| r["id"] == locked[0])
                .expect("locked result")["status"],
            "skipped"
        );
        assert_eq!(body["skipped"], 1);
        assert_eq!(body["tested"], 2);
        let stored = pool_raw(tmp.path());
        let records = stored.as_array().expect("pool");
        assert_eq!(
            records
                .iter()
                .find(|p| p["host"] == "open.example")
                .expect("open record")["status"],
            "ok"
        );
        assert_eq!(
            records
                .iter()
                .find(|p| p["host"] == "locked.example")
                .expect("locked record"),
            &original
        );
    }
);
source_test!(
    c000_an_all_readable_sweep_is_unchanged,
    "tests/test_bulk_proxy_check_skips.py::test_an_all_readable_sweep_is_unchanged",
    {
        let tmp = common::TempDirectory::new("source-net-readable-health").expect("root");
        let ids = pool_added(tmp.path(), &["one.example", "two.example"], "secret");
        let (s, body, mut checked) = pool_sweep(tmp.path(), json!({"ids":ids}));
        assert_eq!(s, 200, "{body}");
        assert_eq!(body["tested"], 2);
        assert_eq!(body["ok"], 2);
        assert_eq!(body["skipped"], 0);
        checked.sort();
        assert_eq!(checked, ["one.example", "two.example"]);
        assert!(
            pool_raw(tmp.path())
                .as_array()
                .expect("pool")
                .iter()
                .all(|p| p["status"] == "ok")
        );
    }
);
source_test!(
    c000_ids_nothing_in_the_pool_matches_are_refused,
    "tests/test_bulk_proxy_check_skips.py::test_ids_nothing_in_the_pool_matches_are_refused",
    {
        let tmp = common::TempDirectory::new("source-net-unmatched-health").expect("root");
        pool_added(tmp.path(), &["one.example"], "secret");
        let (s, body, checked) = pool_sweep(tmp.path(), json!({"ids":["no-such-proxy"]}));
        assert_eq!(s, 400, "{body}");
        assert!(checked.is_empty());
    }
);
source_test!(
    c000_a_missing_id_beside_a_locked_one_still_reports_the_locked_one,
    "tests/test_bulk_proxy_check_skips.py::test_a_missing_id_beside_a_locked_one_still_reports_the_locked_one",
    {
        let tmp = common::TempDirectory::new("source-net-missing-locked-health").expect("root");
        let ids = pool_sealed(tmp.path(), &["a.example"]);
        let (s, body, checked) =
            pool_sweep(tmp.path(), json!({"ids":[ids[0],"gone-from-the-pool"]}));
        assert_eq!(s, 200, "{body}");
        assert_eq!(
            body["results"]
                .as_array()
                .expect("results")
                .iter()
                .map(|r| r["id"].clone())
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(body["skipped"], 1);
        assert!(checked.is_empty());
    }
);
source_test!(
    c000_an_empty_pool_is_refused_rather_than_swept,
    "tests/test_bulk_proxy_check_skips.py::test_an_empty_pool_is_refused_rather_than_swept",
    {
        let tmp = common::TempDirectory::new("source-net-empty-health").expect("root");
        let (s, body, checked) = pool_sweep(tmp.path(), json!({"ids":[]}));
        assert_eq!(s, 400, "{body}");
        assert!(checked.is_empty());
    }
);
