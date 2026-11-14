use super::*;
source_test!(
    a_dashboard_read_cannot_see_a_record_sealed_before_the_descriptor,
    "tests/test_vault_read_isolation.py::test_a_dashboard_read_cannot_see_a_record_sealed_before_the_descriptor",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p", 1, Some("SECRET"));
        let id = prof.id.clone();
        let seen = frozen(root, "dashboard", false, move |root| {
            dash(root, Some(MASTER))
                .get(&id)
                .expect("dashboard get")
                .expect("profile")["proxy"]["pass"]
                .clone()
        });
        assert_eq!(seen, "SECRET");
    }
);
source_test!(
    a_dashboard_listing_cannot_see_a_half_committed_migration,
    "tests/test_vault_read_isolation.py::test_a_dashboard_listing_cannot_see_a_half_committed_migration",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p", 1, Some("SECRET"));
        let _id = prof.id.clone();
        let seen = frozen(root, "dashboard", false, move |root| {
            json!(
                dash(root, Some(MASTER))
                    .list()
                    .expect("listing")
                    .iter()
                    .map(|p| p["proxy"]["pass"].clone())
                    .collect::<Vec<_>>()
            )
        });
        assert_eq!(seen, json!(["SECRET"]));
    }
);
source_test!(
    a_pool_read_cannot_see_a_record_sealed_before_the_descriptor,
    "tests/test_vault_read_isolation.py::test_a_pool_read_cannot_see_a_record_sealed_before_the_descriptor",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        pool(root, None)
            .add_many(vec![pool_entry("1.1.1.1", json!("SECRET"))])
            .expect("pool");
        let seen = frozen(root, "proxies.json", false, move |root| {
            let store = pool(root, Some(MASTER));
            let row = store.list().expect("pool")[0].clone();
            json!({"pass":row["pass"],"withheld":store.secret_withheld(row["id"].as_str().expect("ID")).expect("withheld")})
        });
        assert_eq!(seen, json!({"pass":"SECRET","withheld":false}));
    }
);
source_test!(
    the_reads_still_work_with_no_migration_in_flight,
    "tests/test_vault_read_isolation.py::test_the_reads_still_work_with_no_migration_in_flight",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p", 1, Some("SECRET"));
        pool(root, None)
            .add_many(vec![pool_entry("1.1.1.1", json!("SECRET"))])
            .expect("pool");
        assert_eq!(
            dash(root, None)
                .get(&prof.id)
                .expect("get")
                .expect("profile")["proxy"]["pass"],
            "SECRET"
        );
        assert_eq!(
            dash(root, None).list().expect("list")[0]["proxy"]["pass"],
            "SECRET"
        );
        assert_eq!(
            dash(root, None)
                .raw(&prof.id)
                .expect("raw")
                .expect("profile")["proxy"]["pass"],
            "SECRET"
        );
        let row = pool(root, None).list().expect("pool")[0].clone();
        assert_eq!(row["pass"], "SECRET");
        assert!(
            !pool(root, None)
                .secret_withheld(row["id"].as_str().expect("ID"))
                .expect("not withheld")
        );
    }
);
source_test!(
    the_engine_side_withheld_check_cannot_straddle_the_boundary,
    "tests/test_vault_read_isolation.py::test_the_engine_side_withheld_check_cannot_straddle_the_boundary",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p", 1, Some("SECRET"));
        let id = prof.id.clone();
        let seen = frozen(root, "profiles", false, move |root| {
            json!(
                repo(root, Some(MASTER))
                    .secret_withheld(&id)
                    .expect("withheld check")
            )
        });
        assert_eq!(seen, false);
    }
);
