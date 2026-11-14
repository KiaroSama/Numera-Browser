use super::*;
source_test!(
    a_profile_write_cannot_land_inside_enable,
    "tests/test_vault_transaction.py::test_a_profile_write_cannot_land_inside_enable",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        for i in 0..6 {
            saved(root, &format!("p{i}"), i, Some("SECRET"));
        }
        let mut cached = repo(root, None)
            .get("source-0")
            .expect("cached read")
            .expect("profile");
        cached.notes = "edited".into();
        let seen = frozen(root, "profiles", false, move |root| {
            json!(repo(root, None).save(cached, 1001.0).is_ok())
        });
        assert_eq!(seen, true);
        assert_no_plain(root, MASTER);
        for i in 0..6 {
            assert_eq!(
                secret(
                    &repo(root, Some(MASTER))
                        .resolve(&format!("p{i}"), 1002.0)
                        .expect("resolve")
                        .expect("profile")
                ),
                "SECRET"
            );
        }
    }
);
source_test!(
    a_secret_write_cannot_land_inside_a_password_rotation,
    "tests/test_vault_transaction.py::test_a_secret_write_cannot_land_inside_a_password_rotation",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        for i in 0..6 {
            saved(root, &format!("p{i}"), i, Some("SECRET"));
        }
        enable(root, MASTER);
        let mut cached = repo(root, Some(MASTER))
            .get("source-0")
            .expect("cached read")
            .expect("profile");
        cached.notes = "edited".into();
        let seen = frozen(root, "profiles", true, move |root| {
            json!(repo(root, Some(MASTER)).save(cached, 1001.0).is_ok())
        });
        assert_eq!(seen, true);
        assert_no_plain(root, NEW);
        for i in 0..6 {
            assert_eq!(
                secret(
                    &repo(root, Some(NEW))
                        .resolve(&format!("p{i}"), 1002.0)
                        .expect("resolve")
                        .expect("profile")
                ),
                "SECRET"
            );
        }
    }
);
source_test!(
    a_proxy_credential_write_cannot_land_inside_a_migration,
    "tests/test_vault_transaction.py::test_a_proxy_credential_write_cannot_land_inside_a_migration",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        pool(root, None)
            .add_many(vec![pool_entry("1.1.1.1", json!("SECRET"))])
            .expect("pool");
        enable(root, MASTER);
        let seen = frozen(root, "proxies.json", true, move |root| {
            json!(
                pool(root, Some(MASTER))
                    .add_many(vec![pool_entry("9.9.9.9", json!("SECRET"))])
                    .is_err()
            )
        });
        assert_eq!(seen, true);
        assert_no_plain(root, NEW);
        for row in pool(root, Some(NEW)).list().expect("pool") {
            assert_eq!(row["pass"], "SECRET");
            assert_ne!(row["host"], "9.9.9.9");
        }
    }
);
source_test!(
    two_simultaneous_enables_cannot_race_two_salts,
    "tests/test_vault_transaction.py::test_two_simultaneous_enables_cannot_race_two_salts",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        for i in 0..6 {
            saved(root, &format!("p{i}"), i, Some("SECRET"));
        }
        let seen = frozen(root, "profiles", false, move |root| {
            json!(transaction::enable(root, "second").is_err())
        });
        assert_eq!(seen, true);
        assert!(
            repo(root, Some(MASTER))
                .unlocked()
                .expect("winning password")
        );
        assert_no_plain(root, MASTER);
        for i in 0..6 {
            assert_eq!(
                secret(
                    &repo(root, Some(MASTER))
                        .resolve(&format!("p{i}"), 1002.0)
                        .expect("resolve")
                        .expect("profile")
                ),
                "SECRET"
            );
        }
    }
);
