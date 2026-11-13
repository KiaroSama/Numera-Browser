use super::*;
source_test!(
    a_journal_exists_before_the_first_destructive_write,
    "tests/test_vault_crash_recovery.py::test_a_journal_exists_before_the_first_destructive_write",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        crash(root, "enable", "before-config");
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    the_journal_never_holds_the_password_or_a_plaintext_secret,
    "tests/test_vault_crash_recovery.py::test_the_journal_never_holds_the_password_or_a_plaintext_secret",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        crash(root, "enable", "before-config");
        let blob = fs::read_to_string(root.join("vault-migration.json")).expect("journal");
        assert!(!blob.contains(MASTER) && !blob.contains("secret-one"));
    }
);
source_test!(
    the_secret_is_recoverable_after_a_hard_kill,
    "tests/test_vault_crash_recovery.py::test_the_secret_is_recoverable_after_a_hard_kill",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret-one"));
        crash(root, "enable", "before-config");
        let opened = ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None)
            .expect("recover hard kill");
        assert_eq!(
            secret(&opened.get(&prof.id).expect("get").expect("profile")),
            "secret-one"
        );
    }
);
source_test!(
    recovery_leaves_one_consistent_generation,
    "tests/test_vault_crash_recovery.py::test_recovery_leaves_one_consistent_generation",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret-one"));
        crash(root, "enable", "before-config");
        let opened = ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None)
            .expect("recover hard kill");
        assert!(opened.vault_enabled().expect("enabled"));
        let (engine, dashboard) = raw_pair(root, &prof.id);
        assert!(vault::is_envelope(
            engine["proxy"]["password"].as_str().expect("engine secret")
        ));
        assert!(vault::is_envelope(
            dashboard["proxy"]["pass"]
                .as_str()
                .expect("dashboard secret")
        ));
    }
);
source_test!(
    the_journal_is_cleaned_up_after_recovery,
    "tests/test_vault_crash_recovery.py::test_the_journal_is_cleaned_up_after_recovery",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        crash(root, "enable", "before-config");
        ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None).expect("recover hard kill");
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_clean_enable_leaves_no_journal,
    "tests/test_vault_crash_recovery.py::test_a_clean_enable_leaves_no_journal",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        enable(root, MASTER);
        assert!(!root.join("vault-migration.json").exists());
        assert!(repo(root, Some(MASTER)).vault_enabled().expect("enabled"));
    }
);
source_test!(
    ordinary_failure_still_rolls_back_and_leaves_no_journal,
    "tests/test_vault_crash_recovery.py::test_ordinary_failure_still_rolls_back_and_leaves_no_journal",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret-one"));
        let before = snapshot(root);
        assert!(
            testing::with_observer(error_at("before-config", "config.json", 5), || {
                transaction::enable(root, MASTER)
            })
            .is_err()
        );
        assert!(!root.join("vault-migration.json").exists());
        assert!(!repo(root, None).vault_enabled().expect("not enabled"));
        assert_eq!(snapshot(root), before);
        assert_eq!(
            secret(
                &repo(root, None)
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            "secret-one"
        );
    }
);
