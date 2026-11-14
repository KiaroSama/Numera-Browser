use super::*;
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_000,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[{not json at all-invalid JSON]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), r#"{not json at all"#)
            .expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_001,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[[]-a list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), r#"[]"#).expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_002,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[{}-an empty object]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), r#"{}"#).expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_003,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[null-a null]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), r#"null"#).expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_004,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[payload4-no salt]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), r#"{"op":"enable"}"#)
            .expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_005,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[payload5-a non-string salt]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(
            root.join("vault-migration.json"),
            r#"{"op":"enable","salt":17}"#,
        )
        .expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_006,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[payload6-an empty salt]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(
            root.join("vault-migration.json"),
            r#"{"op":"enable","salt":""}"#,
        )
        .expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_damaged_journal_is_unreadable_not_absent_case_007,
    "tests/test_vault_recovery_integrity.py::test_a_damaged_journal_is_unreadable_not_absent[payload7-a rotation with no old key]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"),r#"{"op":"rotate","salt":"c2FsdA==","new":{"salt":"c2FsdA==","verifier":"v"},"old":null}"#).expect("damaged journal");
        assert_eq!(journal::classify(root).0, JournalState::Unreadable);
        assert!(recovery::needs_recovery(root).expect("recovery required"));
    }
);
source_test!(
    a_newer_journal_is_unsupported_not_absent_case_008,
    "tests/test_vault_recovery_integrity.py::test_a_newer_journal_is_unsupported_not_absent[payload0]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        write(
            root,
            "vault-migration.json",
            json!({"version":99,"op":"enable","new":json!({"salt":"c2FsdA==","verifier":"v"})}),
        );
        assert_eq!(journal::classify(root).0, JournalState::Unsupported);
        assert!(recovery::needs_recovery(root).expect("unsupported requires recovery"));
    }
);
source_test!(
    a_newer_journal_is_unsupported_not_absent_case_009,
    "tests/test_vault_recovery_integrity.py::test_a_newer_journal_is_unsupported_not_absent[payload1]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        write(
            root,
            "vault-migration.json",
            json!({"version":1,"op":"reshard","new":json!({"salt":"c2FsdA==","verifier":"v"})}),
        );
        assert_eq!(journal::classify(root).0, JournalState::Unsupported);
        assert!(recovery::needs_recovery(root).expect("unsupported requires recovery"));
    }
);
source_test!(
    a_valid_journal_still_classifies_as_valid,
    "tests/test_vault_recovery_integrity.py::test_a_valid_journal_still_classifies_as_valid",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        journal(
            root,
            "enable",
            &json!({"salt":"c2FsdA==","verifier":"v"}),
            &Value::Null,
        );
        assert_eq!(journal::classify(root).0, JournalState::Valid);
    }
);
source_test!(
    no_journal_is_still_absent,
    "tests/test_vault_recovery_integrity.py::test_no_journal_is_still_absent",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        assert_eq!(journal::classify(root), (JournalState::Absent, None));
        assert!(!recovery::needs_recovery(root).expect("fresh root"));
    }
);
source_test!(
    recovery_refuses_a_damaged_journal_and_keeps_it_case_012,
    "tests/test_vault_recovery_integrity.py::test_recovery_refuses_a_damaged_journal_and_keeps_it[{not json]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), "{not json").expect("journal");
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("damaged journal refusal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert!(
            error.to_string().contains("backup") || error.to_string().contains("restore"),
            "{error}"
        );
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    recovery_refuses_a_damaged_journal_and_keeps_it_case_013,
    "tests/test_vault_recovery_integrity.py::test_recovery_refuses_a_damaged_journal_and_keeps_it[[]]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), "[]").expect("journal");
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("damaged journal refusal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert!(
            error.to_string().contains("backup") || error.to_string().contains("restore"),
            "{error}"
        );
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    recovery_refuses_a_damaged_journal_and_keeps_it_case_014,
    "tests/test_vault_recovery_integrity.py::test_recovery_refuses_a_damaged_journal_and_keeps_it[{}]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), "{}").expect("journal");
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("damaged journal refusal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert!(
            error.to_string().contains("backup") || error.to_string().contains("restore"),
            "{error}"
        );
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    classification_never_writes,
    "tests/test_vault_recovery_integrity.py::test_classification_never_writes",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("vault-migration.json"), "{not json").expect("journal");
        let before = snapshot(root);
        journal::classify(root);
        recovery::needs_recovery(root).expect("recovery verdict");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    an_unreadable_secret_file_blocks_completion,
    "tests/test_vault_recovery_integrity.py::test_an_unreadable_secret_file_blocks_completion",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        write(
            root,
            "profiles/a.json",
            json!({"id":"a","proxy":{"password":"enc:v2:abc"}}),
        );
        fs::write(root.join("proxies.json"), "{truncated").expect("bad pool");
        journal(
            root,
            "enable",
            &json!({"salt":"c2FsdA==","verifier":"v"}),
            &Value::Null,
        );
        let found = completion::problems(root, &json!({"salt":"c2FsdA==","verifier":"v"}), None)
            .expect("inspect");
        assert!(!found.iter().any(|p| p.code == "plaintext_left"));
        assert!(
            found
                .iter()
                .any(|p| p.code == "unreadable_files" && p.detail.contains("proxies.json"))
        );
        let error = recovery::recover(root, None, None).expect_err("unreadable site");
        assert!(error.to_string().contains("proxies.json"));
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    an_already_committed_descriptor_is_not_a_receipt_without_a_key,
    "tests/test_vault_recovery_integrity.py::test_an_already_committed_descriptor_is_not_a_receipt_without_a_key",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        write(
            root,
            "profiles/a.json",
            json!({"id":"a","proxy":{"password":"enc:v2:abc"}}),
        );
        journal(
            root,
            "enable",
            &json!({"salt":"c2FsdA==","verifier":"v"}),
            &Value::Null,
        );
        write(
            root,
            "config.json",
            json!({"vault":json!({"salt":"c2FsdA==","verifier":"v"})}),
        );
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("no authenticated receipt");
        code(&error, "no_password");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_config_that_disagrees_with_the_journal_is_named_not_guessed,
    "tests/test_vault_recovery_integrity.py::test_a_config_that_disagrees_with_the_journal_is_named_not_guessed",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        write(
            root,
            "profiles/a.json",
            json!({"id":"a","proxy":{"password":"enc:v2:abc"}}),
        );
        journal(
            root,
            "enable",
            &json!({"salt":"c2FsdA==","verifier":"v"}),
            &Value::Null,
        );
        write(
            root,
            "config.json",
            json!({"vault":{"salt":"c2FsdA==","verifier":"enc:v2:opens-nothing"}}),
        );
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("no authenticated receipt");
        code(&error, "descriptor_disagreement");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    an_uncommitted_descriptor_is_not_adopted_without_a_key,
    "tests/test_vault_recovery_integrity.py::test_an_uncommitted_descriptor_is_not_adopted_without_a_key",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        write(
            root,
            "profiles/a.json",
            json!({"id":"a","proxy":{"password":"enc:v2:abc"}}),
        );
        journal(
            root,
            "enable",
            &json!({"salt":"c2FsdA==","verifier":"v"}),
            &Value::Null,
        );
        write(root, "config.json", json!({}));
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("no authenticated receipt");
        code(&error, "no_password");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_real_sync_error_propagates,
    "tests/test_vault_recovery_integrity.py::test_a_real_sync_error_propagates",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("f.json"), "{}").expect("sync target");
        let result = testing::with_observer(error_at("fsync-file", "f.json", 28), || {
            vault::durability::sync_file(&root.join("f.json"))
        });
        assert_eq!(result.expect_err("disk full").raw_os_error(), Some(28));
    }
);
source_test!(
    an_unsupported_sync_is_tolerated,
    "tests/test_vault_recovery_integrity.py::test_an_unsupported_sync_is_tolerated",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("f.json"), "{}").expect("sync target");
        let result = testing::with_observer(error_at("fsync-file", "f.json", 22), || {
            vault::durability::sync_file(&root.join("f.json"))
        });
        result.expect("unsupported fsync tolerated");
    }
);
source_test!(
    beginning_a_journal_syncs_it,
    "tests/test_vault_recovery_integrity.py::test_beginning_a_journal_syncs_it",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let events = seen.clone();
        testing::with_observer(
            move |e| {
                events
                    .lock()
                    .expect("events")
                    .push((e.operation, e.path.clone()));
                Ok(Action::Continue)
            },
            || transaction::begin(root, &json!({"salt":"c2FsdA==","verifier":"v"}), None),
        )
        .expect("begin durable journal");
        assert!(
            seen.lock()
                .expect("events")
                .iter()
                .any(|(op, p)| *op == "fsync-file" && p.ends_with("vault-migration.json"))
        );
        let record = read(root, "vault-migration.json");
        assert_eq!(record["version"], 1);
        assert_eq!(record["op"], "enable");
    }
);
source_test!(
    the_journal_records_its_format_version,
    "tests/test_vault_recovery_integrity.py::test_the_journal_records_its_format_version",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let events = seen.clone();
        testing::with_observer(
            move |e| {
                events
                    .lock()
                    .expect("events")
                    .push((e.operation, e.path.clone()));
                Ok(Action::Continue)
            },
            || transaction::begin(root, &json!({"salt":"c2FsdA==","verifier":"v"}), None),
        )
        .expect("begin durable journal");
        assert!(
            seen.lock()
                .expect("events")
                .iter()
                .any(|(op, p)| *op == "fsync-file" && p.ends_with("vault-migration.json"))
        );
        let record = read(root, "vault-migration.json");
        assert_eq!(record["version"], 1);
        assert_eq!(record["op"], "enable");
    }
);
source_test!(
    a_store_that_is_already_open_refuses_after_a_migration_dies,
    "tests/test_vault_recovery_integrity.py::test_a_store_that_is_already_open_refuses_after_a_migration_dies",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let store = repo(root, None);
        assert!(store.vault().expect("fresh vault").is_none());
        journal(
            root,
            "enable",
            &json!({"salt":"c2FsdA==","verifier":"v"}),
            &Value::Null,
        );
        assert!(store.vault().is_err());
    }
);
