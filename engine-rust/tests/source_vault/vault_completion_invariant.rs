use super::*;
source_test!(
    ciphertext_from_a_foreign_descriptor_is_refused,
    "tests/test_vault_completion_invariant.py::test_ciphertext_from_a_foreign_descriptor_is_refused",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "old", false);
        reseal(root, NEW, &descriptors()[0], false);
        journal(root, "enable", &descriptors()[1], &Value::Null);
        let before = snapshot(root);
        let error = recovery::recover(root, Some(OLD), None).expect_err("foreign descriptor");
        assert!(
            error.to_string().contains("does not open")
                && error.to_string().contains("proxies.json")
        );
        assert_eq!(snapshot(root), before);
        prove(root, NEW, &descriptors()[0]);
    }
);
source_test!(
    the_foreign_data_is_still_recoverable_afterwards,
    "tests/test_vault_completion_invariant.py::test_the_foreign_data_is_still_recoverable_afterwards",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "old", false);
        reseal(root, NEW, &descriptors()[0], false);
        journal(root, "enable", &descriptors()[1], &Value::Null);
        let before = snapshot(root);
        let error = recovery::recover(root, Some(OLD), None).expect_err("foreign descriptor");
        assert!(
            error.to_string().contains("does not open")
                && error.to_string().contains("proxies.json")
        );
        assert_eq!(snapshot(root), before);
        prove(root, NEW, &descriptors()[0]);
    }
);
source_test!(
    an_enable_whose_data_really_is_ours_completes,
    "tests/test_vault_completion_invariant.py::test_an_enable_whose_data_really_is_ours_completes",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        assert!(recover(root).expect("owned encrypted generation"));
        prove(root, NEW, &active(root));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    without_a_password_an_interrupted_enable_refuses,
    "tests/test_vault_completion_invariant.py::test_without_a_password_an_interrupted_enable_refuses",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("password required");
        code(&error, "no_password");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_damaged_verifier_is_never_committed,
    "tests/test_vault_completion_invariant.py::test_a_damaged_verifier_is_never_committed",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        let mut bad = descriptors()[0].clone();
        bad["verifier"] = descriptors()[1]["verifier"].clone();
        assert!(!vault::check_password(
            NEW,
            bad["salt"].as_str().expect("salt"),
            bad["verifier"].as_str().expect("verifier")
        ));
        journal(root, "enable", &bad, &Value::Null);
        let before = snapshot(root);
        let error = recover(root).expect_err("damaged verifier");
        code(&error, "verifier_mismatch");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    an_unusable_descriptor_is_refused_case_005,
    "tests/test_vault_completion_invariant.py::test_an_unusable_descriptor_is_refused[<lambda>-an empty verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        let mut bad = descriptors()[0].clone();
        bad["verifier"] = "".into();
        journal(root, "enable", &bad, &Value::Null);
        let before = snapshot(root);
        let error = recover(root).expect_err("unusable journal descriptor");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
        let found = completion::problems(root, &bad, Some(NEW)).expect("completion guard");
        assert_eq!(
            found.iter().map(|p| p.code).collect::<BTreeSet<_>>(),
            BTreeSet::from(["descriptor_unusable"])
        );
    }
);
source_test!(
    an_unusable_descriptor_is_refused_case_006,
    "tests/test_vault_completion_invariant.py::test_an_unusable_descriptor_is_refused[<lambda>-a null verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        let mut bad = descriptors()[0].clone();
        bad["verifier"] = Value::Null;
        journal(root, "enable", &bad, &Value::Null);
        let before = snapshot(root);
        let error = recover(root).expect_err("unusable journal descriptor");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
        let found = completion::problems(root, &bad, Some(NEW)).expect("completion guard");
        assert_eq!(
            found.iter().map(|p| p.code).collect::<BTreeSet<_>>(),
            BTreeSet::from(["descriptor_unusable"])
        );
    }
);
source_test!(
    an_unusable_descriptor_is_refused_case_007,
    "tests/test_vault_completion_invariant.py::test_an_unusable_descriptor_is_refused[<lambda>-a non-string verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        let mut bad = descriptors()[0].clone();
        bad["verifier"] = json!(17);
        journal(root, "enable", &bad, &Value::Null);
        let before = snapshot(root);
        let error = recover(root).expect_err("unusable journal descriptor");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
        let found = completion::problems(root, &bad, Some(NEW)).expect("completion guard");
        assert_eq!(
            found.iter().map(|p| p.code).collect::<BTreeSet<_>>(),
            BTreeSet::from(["descriptor_unusable"])
        );
    }
);
source_test!(
    an_unusable_descriptor_is_refused_case_008,
    "tests/test_vault_completion_invariant.py::test_an_unusable_descriptor_is_refused[<lambda>-a salt that is not base64]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        let mut bad = descriptors()[0].clone();
        bad["salt"] = "not-base64!!".into();
        journal(root, "enable", &bad, &Value::Null);
        let before = snapshot(root);
        let error = recover(root).expect_err("unusable journal descriptor");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
        let found = completion::problems(root, &bad, Some(NEW)).expect("completion guard");
        assert_eq!(
            found.iter().map(|p| p.code).collect::<BTreeSet<_>>(),
            BTreeSet::from(["descriptor_unusable"])
        );
    }
);
source_test!(
    a_resume_does_not_finish_over_a_readable_credential_case_009,
    "tests/test_vault_completion_invariant.py::test_a_resume_does_not_finish_over_a_readable_credential[ordinary resume]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        assert!(recover(root).expect("resume must seal plaintext"));
        prove(root, NEW, &active(root));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_resume_does_not_finish_over_a_readable_credential_case_010,
    "tests/test_vault_completion_invariant.py::test_a_resume_does_not_finish_over_a_readable_credential[config already names the new key]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "old", true);
        assert!(recover(root).expect("resume must seal plaintext"));
        prove(root, NEW, &active(root));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_clean_resume_moves_every_site,
    "tests/test_vault_completion_invariant.py::test_a_clean_resume_moves_every_site",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        assert!(recover(root).expect("resume rotation"));
        prove(root, NEW, &active(root));
    }
);
source_test!(
    a_wrong_password_refuses_and_changes_nothing_case_012,
    "tests/test_vault_completion_invariant.py::test_a_wrong_password_refuses_and_changes_nothing[master-bravo-wrong-old-old]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let before = snapshot(root);
        let error =
            recovery::recover(root, Some(NEW), Some("wrong-old")).expect_err("wrong password");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::VaultLocked);
        assert_eq!(error.role, Some("old"));
        assert_eq!(snapshot(root), before);
        prove(root, OLD, &descriptors()[1]);
    }
);
source_test!(
    a_wrong_password_refuses_and_changes_nothing_case_013,
    "tests/test_vault_completion_invariant.py::test_a_wrong_password_refuses_and_changes_nothing[wrong-new-master-alpha-new]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let before = snapshot(root);
        let error =
            recovery::recover(root, Some("wrong-new"), Some(OLD)).expect_err("wrong password");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::VaultLocked);
        assert_eq!(error.role, Some("new"));
        assert_eq!(snapshot(root), before);
        prove(root, OLD, &descriptors()[1]);
    }
);
source_test!(
    a_resume_with_no_passwords_refuses_and_keeps_the_salt,
    "tests/test_vault_completion_invariant.py::test_a_resume_with_no_passwords_refuses_and_keeps_the_salt",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("passwords required");
        code(&error, "needs_both_passwords");
        assert!(error.to_string().contains("resume"));
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_new_config_over_old_data_is_not_a_finished_rotation,
    "tests/test_vault_completion_invariant.py::test_a_new_config_over_old_data_is_not_a_finished_rotation",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "old", false);
        assert!(recover(root).expect("resume rotation"));
        prove(root, NEW, &active(root));
    }
);
source_test!(
    a_malformed_holder_container_blocks_completion_case_016,
    "tests/test_vault_completion_invariant.py::test_a_malformed_holder_container_blocks_completion[proxies.json-null]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"null"#).expect("holder"),
        );
        let before = snapshot(root);
        let error = recover(root).expect_err("malformed container");
        code(&error, "unreadable_files");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_malformed_holder_container_blocks_completion_case_017,
    "tests/test_vault_completion_invariant.py::test_a_malformed_holder_container_blocks_completion[proxies.json-{\"not\": \"a list\"}]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"{"not":"a list"}"#).expect("holder"),
        );
        let before = snapshot(root);
        let error = recover(root).expect_err("malformed container");
        code(&error, "unreadable_files");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_malformed_holder_container_blocks_completion_case_018,
    "tests/test_vault_completion_invariant.py::test_a_malformed_holder_container_blocks_completion[profiles/p1.json-[]]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"[]"#).expect("holder"),
        );
        let before = snapshot(root);
        let error = recover(root).expect_err("malformed container");
        code(&error, "unreadable_files");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_root_with_no_secrets_at_all_completes,
    "tests/test_vault_completion_invariant.py::test_a_root_with_no_secrets_at_all_completes",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        write(root, "config.json", json!({}));
        journal(root, "enable", &descriptors()[0], &Value::Null);
        assert!(recover(root).expect("no secrets to migrate"));
        assert_eq!(active(root), descriptors()[0]);
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    the_store_opens_twice_after_the_protocol_settles,
    "tests/test_vault_completion_invariant.py::test_the_store_opens_twice_after_the_protocol_settles",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        assert!(recover(root).expect("resume rotation"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            let opened =
                ProfileRepo::open(root.to_owned(), Some(NEW.into()), None).expect("reopen");
            assert!(opened.unlocked().expect("unlock"));
            assert_eq!(
                opened.vault().expect("vault"),
                Some(descriptors()[0].clone())
            );
        }
    }
);
