use super::*;
source_test!(
    an_unfinished_enable_never_reports_an_enabled_vault_case_000,
    "tests/test_vault_recovery_completeness.py::test_an_unfinished_enable_never_reports_an_enabled_vault[script0-before any secret was written]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "before-migration");
        let desc = read(root, "vault-migration.json")["new"].clone();
        assert!(
            completion::problems(root, &desc, None)
                .expect("scan")
                .iter()
                .any(|p| p.code == "plaintext_left")
        );
        if let Ok(opened) = ProfileRepo::open(root.to_owned(), None, None) {
            assert!(!opened.vault_enabled().expect("vault not complete"));
        }
    }
);
source_test!(
    an_unfinished_enable_never_reports_an_enabled_vault_case_001,
    "tests/test_vault_recovery_completeness.py::test_an_unfinished_enable_never_reports_an_enabled_vault[script1-after one secret file of three]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "profiles");
        let desc = read(root, "vault-migration.json")["new"].clone();
        assert!(
            completion::problems(root, &desc, None)
                .expect("scan")
                .iter()
                .any(|p| p.code == "plaintext_left")
        );
        if let Ok(opened) = ProfileRepo::open(root.to_owned(), None, None) {
            assert!(!opened.vault_enabled().expect("vault not complete"));
        }
    }
);
source_test!(
    an_unfinished_enable_is_refused_rather_than_served_case_002,
    "tests/test_vault_recovery_completeness.py::test_an_unfinished_enable_is_refused_rather_than_served[script0]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "before-migration");
        let error = ProfileRepo::open(root.to_owned(), None, None)
            .err()
            .expect("must refuse incomplete enable");
        assert!(error.to_string().to_lowercase().contains("password"));
    }
);
source_test!(
    an_unfinished_enable_is_refused_rather_than_served_case_003,
    "tests/test_vault_recovery_completeness.py::test_an_unfinished_enable_is_refused_rather_than_served[script1]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "profiles");
        let error = ProfileRepo::open(root.to_owned(), None, None)
            .err()
            .expect("must refuse incomplete enable");
        assert!(error.to_string().to_lowercase().contains("password"));
    }
);
source_test!(
    the_password_finishes_an_interrupted_enable_case_004,
    "tests/test_vault_recovery_completeness.py::test_the_password_finishes_an_interrupted_enable[script0]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "before-migration");
        let opened = ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None)
            .expect("finish interrupted enable");
        assert!(opened.vault_enabled().expect("enabled"));
        assert_no_plain(root, MASTER);
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    the_password_finishes_an_interrupted_enable_case_005,
    "tests/test_vault_recovery_completeness.py::test_the_password_finishes_an_interrupted_enable[script1]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "profiles");
        let opened = ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None)
            .expect("finish interrupted enable");
        assert!(opened.vault_enabled().expect("enabled"));
        assert_no_plain(root, MASTER);
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    finishing_an_interrupted_enable_is_idempotent,
    "tests/test_vault_recovery_completeness.py::test_finishing_an_interrupted_enable_is_idempotent",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "profiles");
        for _ in 0..2 {
            let opened = ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None)
                .expect("idempotent recovery");
            assert_eq!(
                secret(&opened.get(&prof.id).expect("get").expect("profile")),
                "secret-one"
            );
        }
        assert_no_plain(root, MASTER);
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_rotation_records_a_journal_at_all,
    "tests/test_vault_recovery_completeness.py::test_a_rotation_records_a_journal_at_all",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        enable(root, MASTER);
        let seen = Arc::new(Mutex::new(false));
        let observed = seen.clone();
        testing::with_observer(
            move |e| {
                if e.operation == "fsync-file" && e.path.ends_with("vault-migration.json") {
                    let record: Value =
                        serde_json::from_slice(&fs::read(&e.path)?).map_err(io::Error::other)?;
                    assert_eq!(record["op"], "rotate");
                    *observed.lock().expect("flag") = true;
                }
                Ok(Action::Continue)
            },
            || transaction::rotate(root, MASTER, NEW),
        )
        .expect("rotation");
        assert!(*seen.lock().expect("flag"));
    }
);
source_test!(
    an_interrupted_rotation_is_not_discarded,
    "tests/test_vault_recovery_completeness.py::test_an_interrupted_rotation_is_not_discarded",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        enable(root, MASTER);
        crash(root, "rotate", "profiles");
        assert!(root.join("vault-migration.json").exists());
        let _ = ProfileRepo::open(root.to_owned(), None, None);
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    an_interrupted_rotation_can_be_finished,
    "tests/test_vault_recovery_completeness.py::test_an_interrupted_rotation_can_be_finished",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        enable(root, MASTER);
        crash(root, "rotate", "profiles");
        assert!(root.join("vault-migration.json").exists());
        ProfileRepo::resume_vault(root.to_owned(), MASTER, NEW).expect("resume rotation");
        let reopened = ProfileRepo::open(root.to_owned(), Some(NEW.into()), None).expect("new key");
        assert_eq!(
            secret(&reopened.get(&prof.id).expect("get").expect("profile")),
            "secret-one"
        );
        assert!(!root.join("vault-migration.json").exists());
        assert!(
            !repo(root, Some(MASTER))
                .unlocked()
                .expect("old key retired")
        );
    }
);
source_test!(
    a_crash_during_recovery_is_itself_recoverable,
    "tests/test_vault_recovery_completeness.py::test_a_crash_during_recovery_is_itself_recoverable",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "profiles");
        crash(root, "recover-master", "dashboard");
        let finished = ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None)
            .expect("recover a second crash");
        assert!(finished.vault_enabled().expect("enabled"));
        assert_no_plain(root, MASTER);
        assert_eq!(
            secret(&finished.get(&prof.id).expect("get").expect("profile")),
            "secret-one"
        );
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_crash_after_the_commit_only_clears_the_journal,
    "tests/test_vault_recovery_completeness.py::test_a_crash_after_the_commit_only_clears_the_journal",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        enable(root, MASTER);
        let before = active(root);
        journal(root, "enable", &before, &Value::Null);
        let reopened = ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None)
            .expect("cleanup committed migration");
        assert!(!root.join("vault-migration.json").exists());
        assert_eq!(active(root), before);
        assert!(reopened.vault_enabled().expect("enabled"));
    }
);
source_test!(
    a_failed_rollback_keeps_its_journal,
    "tests/test_vault_recovery_completeness.py::test_a_failed_rollback_keeps_its_journal",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let before = snapshot(root);
        let error = testing::with_observer(
            move |e| {
                if e.operation == "before-config"
                    || e.operation == "before-restore" && e.path.ends_with("proxies.json")
                {
                    return Err(io::Error::from_raw_os_error(13));
                }
                Ok(Action::Continue)
            },
            || recover(root),
        )
        .expect_err("failed rollback");
        assert!(error.to_string().contains("could not be restored"));
        assert_rollback_failure(&error, "proxies.json", 13);
        for (path, bytes) in before {
            if path != "proxies.json" {
                assert_eq!(
                    fs::read(root.join(path)).expect("other rollback paths restored"),
                    bytes
                );
            }
        }
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_failed_resume_keeps_its_journal_even_when_rollback_works,
    "tests/test_vault_recovery_completeness.py::test_a_failed_resume_keeps_its_journal_even_when_rollback_works",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "profiles");
        fs::write(root.join("proxies.json"), "{not json").expect("damage remaining site");
        assert!(ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None).is_err());
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    no_journal_holds_a_password_or_a_plaintext_secret_case_014,
    "tests/test_vault_recovery_completeness.py::test_no_journal_holds_a_password_or_a_plaintext_secret[script0-False]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "before-migration");
        let blob = fs::read_to_string(root.join("vault-migration.json")).expect("journal");
        for text in [MASTER, NEW, "secret-one", "secret-two", "secret-three"] {
            assert!(!blob.contains(text), "{text} leaked");
        }
    }
);
source_test!(
    no_journal_holds_a_password_or_a_plaintext_secret_case_015,
    "tests/test_vault_recovery_completeness.py::test_no_journal_holds_a_password_or_a_plaintext_secret[script1-False]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        crash(root, "enable", "profiles");
        let blob = fs::read_to_string(root.join("vault-migration.json")).expect("journal");
        for text in [MASTER, NEW, "secret-one", "secret-two", "secret-three"] {
            assert!(!blob.contains(text), "{text} leaked");
        }
    }
);
source_test!(
    no_journal_holds_a_password_or_a_plaintext_secret_case_016,
    "tests/test_vault_recovery_completeness.py::test_no_journal_holds_a_password_or_a_plaintext_secret[script2-True]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        enable(root, MASTER);
        crash(root, "rotate", "profiles");
        let blob = fs::read_to_string(root.join("vault-migration.json")).expect("journal");
        for text in [MASTER, NEW, "secret-one", "secret-two", "secret-three"] {
            assert!(!blob.contains(text), "{text} leaked");
        }
    }
);
source_test!(
    a_clean_enable_and_rotation_still_leave_nothing_behind,
    "tests/test_vault_recovery_completeness.py::test_a_clean_enable_and_rotation_still_leave_nothing_behind",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "alpha", 1, Some("secret-one"));
        write(
            root,
            "dashboard/alpha.json",
            json!({"id":"alpha","proxy":{"pass":"secret-two"}}),
        );
        write(
            root,
            "proxies.json",
            json!([{"id":"pool","pass":"secret-three"}]),
        );
        enable(root, MASTER);
        assert!(!root.join("vault-migration.json").exists());
        transaction::rotate(root, MASTER, NEW).expect("rotate");
        assert!(!root.join("vault-migration.json").exists());
        assert_eq!(
            secret(
                &repo(root, Some(NEW))
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            "secret-one"
        );
        assert_no_plain(root, NEW);
    }
);
