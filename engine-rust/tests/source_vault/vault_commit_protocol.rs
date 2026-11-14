use super::*;
source_test!(
    a_sync_failure_after_the_descriptor_lands_does_not_split_the_two,
    "tests/test_vault_commit_protocol.py::test_a_sync_failure_after_the_descriptor_lands_does_not_split_the_two",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let error =
            testing::with_observer(error_at("fsync-file", "config.json", 5), || recover(root))
                .expect_err("actual durability failure");
        assert!(!error.to_string().is_empty());
        let cfg = active(root);
        assert!(cfg == descriptors()[0] || cfg == descriptors()[1]);
        prove(root, if cfg == descriptors()[1] { OLD } else { NEW }, &cfg);
    }
);
source_test!(
    the_retry_after_that_failure_does_not_report_success,
    "tests/test_vault_commit_protocol.py::test_the_retry_after_that_failure_does_not_report_success",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let error =
            testing::with_observer(error_at("fsync-file", "config.json", 5), || recover(root))
                .expect_err("actual durability failure");
        assert!(!error.to_string().is_empty());
        match recovery::recover(root, None, None) {
            Err(_) => assert!(root.join("vault-migration.json").exists()),
            Ok(_) => prove(root, NEW, &active(root)),
        }
    }
);
source_test!(
    two_restarts_converge_on_one_readable_generation,
    "tests/test_vault_commit_protocol.py::test_two_restarts_converge_on_one_readable_generation",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let error =
            testing::with_observer(error_at("fsync-file", "config.json", 5), || recover(root))
                .expect_err("actual durability failure");
        assert!(!error.to_string().is_empty());
        assert!(recover(root).expect("retry after sync failure"));
        recover(root).expect("second restart");
        assert!(!root.join("vault-migration.json").exists());
        prove(root, NEW, &active(root));
        assert_eq!(active(root), descriptors()[0]);
    }
);
source_test!(
    a_clean_resume_moves_every_site,
    "tests/test_vault_commit_protocol.py::test_a_clean_resume_moves_every_site",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        assert!(recover(root).expect("resume"));
        assert_eq!(active(root), descriptors()[0]);
        prove(root, NEW, &active(root));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_half_moved_rotation_finishes_the_rest,
    "tests/test_vault_commit_protocol.py::test_a_half_moved_rotation_finishes_the_rest",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let mut data = read(root, "proxies.json");
        data[0]["pass"] = vault::encrypt(
            NEW,
            descriptors()[0]["salt"].as_str().expect("salt"),
            VALUES[2],
        )
        .expect("already moved pool")
        .into();
        write(root, "proxies.json", data);
        assert!(recover(root).expect("resume"));
        assert_eq!(active(root), descriptors()[0]);
        prove(root, NEW, &active(root));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_wrong_password_refuses_and_keeps_the_record_case_005,
    "tests/test_vault_commit_protocol.py::test_a_wrong_password_refuses_and_keeps_the_record[master-two-wrong-old-moving away from]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let before = snapshot(root);
        let error =
            recovery::recover(root, Some(NEW), Some("wrong-old")).expect_err("wrong password");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::VaultLocked);
        assert!(error.to_string().contains("moving away from"), "{error}");
        assert_eq!(snapshot(root), before);
        prove(root, OLD, &descriptors()[1]);
    }
);
source_test!(
    a_wrong_password_refuses_and_keeps_the_record_case_006,
    "tests/test_vault_commit_protocol.py::test_a_wrong_password_refuses_and_keeps_the_record[wrong-new-master-one-new master password]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let before = snapshot(root);
        let error =
            recovery::recover(root, Some("wrong-new"), Some(OLD)).expect_err("wrong password");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::VaultLocked);
        assert!(error.to_string().contains("new master password"), "{error}");
        assert_eq!(snapshot(root), before);
        prove(root, OLD, &descriptors()[1]);
    }
);
source_test!(
    enable_syncs_the_data_and_the_descriptor_not_only_the_journal,
    "tests/test_vault_commit_protocol.py::test_enable_syncs_the_data_and_the_descriptor_not_only_the_journal",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        minimal(root);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let events = seen.clone();
        testing::with_observer(
            move |e| {
                if e.operation == "fsync-file" {
                    events.lock().expect("sync events").push(
                        e.path
                            .file_name()
                            .expect("file name")
                            .to_string_lossy()
                            .to_string(),
                    );
                }
                Ok(Action::Continue)
            },
            || transaction::enable(root, OLD),
        )
        .expect("durable migration");
        let events = seen.lock().expect("events");
        for name in ["vault-migration.json", "proxies.json", "config.json"] {
            assert!(events.iter().any(|e| e == name), "{events:?}");
        }
        assert!(
            events.iter().position(|e| e == "proxies.json")
                < events.iter().position(|e| e == "config.json")
        );
    }
);
source_test!(
    rotation_syncs_the_data_and_the_descriptor,
    "tests/test_vault_commit_protocol.py::test_rotation_syncs_the_data_and_the_descriptor",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        minimal(root);
        enable(root, OLD);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let events = seen.clone();
        testing::with_observer(
            move |e| {
                if e.operation == "fsync-file" {
                    events.lock().expect("sync events").push(
                        e.path
                            .file_name()
                            .expect("file name")
                            .to_string_lossy()
                            .to_string(),
                    );
                }
                Ok(Action::Continue)
            },
            || transaction::rotate(root, OLD, NEW),
        )
        .expect("durable migration");
        let events = seen.lock().expect("events");
        for name in ["vault-migration.json", "proxies.json", "config.json"] {
            assert!(events.iter().any(|e| e == name), "{events:?}");
        }
        assert!(
            events.iter().position(|e| e == "proxies.json")
                < events.iter().position(|e| e == "config.json")
        );
    }
);
source_test!(
    a_regular_file_sync_failure_is_a_failure_case_009,
    "tests/test_vault_commit_protocol.py::test_a_regular_file_sync_failure_is_a_failure[13]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("f.json"), "{}").expect("sync target");
        let result = testing::with_observer(error_at("fsync-file", "f.json", 13), || {
            vault::durability::sync_file(&root.join("f.json"))
        });
        assert_eq!(
            result.expect_err("regular fsync failure").raw_os_error(),
            Some(13)
        );
    }
);
source_test!(
    a_regular_file_sync_failure_is_a_failure_case_010,
    "tests/test_vault_commit_protocol.py::test_a_regular_file_sync_failure_is_a_failure[9]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("f.json"), "{}").expect("sync target");
        let result = testing::with_observer(error_at("fsync-file", "f.json", 9), || {
            vault::durability::sync_file(&root.join("f.json"))
        });
        assert_eq!(
            result.expect_err("regular fsync failure").raw_os_error(),
            Some(9)
        );
    }
);
source_test!(
    a_regular_file_sync_failure_is_a_failure_case_011,
    "tests/test_vault_commit_protocol.py::test_a_regular_file_sync_failure_is_a_failure[5]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("f.json"), "{}").expect("sync target");
        let result = testing::with_observer(error_at("fsync-file", "f.json", 5), || {
            vault::durability::sync_file(&root.join("f.json"))
        });
        assert_eq!(
            result.expect_err("regular fsync failure").raw_os_error(),
            Some(5)
        );
    }
);
source_test!(
    a_filesystem_that_cannot_sync_this_object_is_tolerated_case_012,
    "tests/test_vault_commit_protocol.py::test_a_filesystem_that_cannot_sync_this_object_is_tolerated[22]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("f.json"), "{}").expect("sync target");
        let result = testing::with_observer(error_at("fsync-file", "f.json", 22), || {
            vault::durability::sync_file(&root.join("f.json"))
        });
        result.expect("unsupported synchronization tolerated");
    }
);
source_test!(
    a_filesystem_that_cannot_sync_this_object_is_tolerated_case_013,
    "tests/test_vault_commit_protocol.py::test_a_filesystem_that_cannot_sync_this_object_is_tolerated[95]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        fs::write(root.join("f.json"), "{}").expect("sync target");
        let result = testing::with_observer(error_at("fsync-file", "f.json", 95), || {
            vault::durability::sync_file(&root.join("f.json"))
        });
        result.expect("unsupported synchronization tolerated");
    }
);
source_test!(
    a_journal_this_build_cannot_trust_is_not_acted_on_case_014,
    "tests/test_vault_commit_protocol.py::test_a_journal_this_build_cannot_trust_is_not_acted_on[<lambda>-a descriptor with no verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let mut record = read(root, "vault-migration.json");
        record["new"]
            .as_object_mut()
            .expect("descriptor")
            .remove("verifier");
        write(root, "vault-migration.json", record);
        assert!(matches!(
            journal::classify(root).0,
            JournalState::Unreadable | JournalState::Unsupported
        ));
        let before = snapshot(root);
        let error = recover(root).expect_err("untrusted journal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_journal_this_build_cannot_trust_is_not_acted_on_case_015,
    "tests/test_vault_commit_protocol.py::test_a_journal_this_build_cannot_trust_is_not_acted_on[<lambda>-a non-string verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let mut record = read(root, "vault-migration.json");
        record["new"]["verifier"] = json!(7);
        write(root, "vault-migration.json", record);
        assert!(matches!(
            journal::classify(root).0,
            JournalState::Unreadable | JournalState::Unsupported
        ));
        let before = snapshot(root);
        let error = recover(root).expect_err("untrusted journal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_journal_this_build_cannot_trust_is_not_acted_on_case_016,
    "tests/test_vault_commit_protocol.py::test_a_journal_this_build_cannot_trust_is_not_acted_on[<lambda>-version 0]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let mut record = read(root, "vault-migration.json");
        record["version"] = json!(0);
        write(root, "vault-migration.json", record);
        assert!(matches!(
            journal::classify(root).0,
            JournalState::Unreadable | JournalState::Unsupported
        ));
        let before = snapshot(root);
        let error = recover(root).expect_err("untrusted journal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_journal_this_build_cannot_trust_is_not_acted_on_case_017,
    "tests/test_vault_commit_protocol.py::test_a_journal_this_build_cannot_trust_is_not_acted_on[<lambda>-a boolean version]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let mut record = read(root, "vault-migration.json");
        record["version"] = json!(true);
        write(root, "vault-migration.json", record);
        assert!(matches!(
            journal::classify(root).0,
            JournalState::Unreadable | JournalState::Unsupported
        ));
        let before = snapshot(root);
        let error = recover(root).expect_err("untrusted journal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_journal_this_build_cannot_trust_is_not_acted_on_case_018,
    "tests/test_vault_commit_protocol.py::test_a_journal_this_build_cannot_trust_is_not_acted_on[<lambda>-a string version]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let mut record = read(root, "vault-migration.json");
        record["version"] = json!("1");
        write(root, "vault-migration.json", record);
        assert!(matches!(
            journal::classify(root).0,
            JournalState::Unreadable | JournalState::Unsupported
        ));
        let before = snapshot(root);
        let error = recover(root).expect_err("untrusted journal");
        assert_eq!(error.kind, recovery::RecoveryErrorKind::JournalUnreadable);
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_secret_file_of_the_wrong_shape_blocks_completion_case_019,
    "tests/test_vault_commit_protocol.py::test_a_secret_file_of_the_wrong_shape_blocks_completion[proxies.json-null]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"null"#).expect("wrong-shaped file"),
        );
        let found =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("completion scan");
        assert!(found.iter().any(|p| p.code == "unreadable_files"));
        let before = snapshot(root);
        let error = recover(root).expect_err("cannot complete damaged site");
        assert!(error.to_string().contains("proxies.json"));
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_secret_file_of_the_wrong_shape_blocks_completion_case_020,
    "tests/test_vault_commit_protocol.py::test_a_secret_file_of_the_wrong_shape_blocks_completion[proxies.json-{\"not\": \"a list\"}]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"{"not":"a list"}"#).expect("wrong-shaped file"),
        );
        let found =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("completion scan");
        assert!(found.iter().any(|p| p.code == "unreadable_files"));
        let before = snapshot(root);
        let error = recover(root).expect_err("cannot complete damaged site");
        assert!(error.to_string().contains("proxies.json"));
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_secret_file_of_the_wrong_shape_blocks_completion_case_021,
    "tests/test_vault_commit_protocol.py::test_a_secret_file_of_the_wrong_shape_blocks_completion[profiles/p1.json-[]]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"[]"#).expect("wrong-shaped file"),
        );
        let found =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("completion scan");
        assert!(found.iter().any(|p| p.code == "unreadable_files"));
        let before = snapshot(root);
        let error = recover(root).expect_err("cannot complete damaged site");
        assert!(error.to_string().contains("p1.json"));
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_secret_file_of_the_wrong_shape_blocks_completion_case_022,
    "tests/test_vault_commit_protocol.py::test_a_secret_file_of_the_wrong_shape_blocks_completion[profiles/p1.json-12]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"12"#).expect("wrong-shaped file"),
        );
        let found =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("completion scan");
        assert!(found.iter().any(|p| p.code == "unreadable_files"));
        let before = snapshot(root);
        let error = recover(root).expect_err("cannot complete damaged site");
        assert!(error.to_string().contains("p1.json"));
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    a_failed_rollback_keeps_the_journal_and_names_the_files,
    "tests/test_vault_commit_protocol.py::test_a_failed_rollback_keeps_the_journal_and_names_the_files",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        let before = snapshot(root);
        let error = testing::with_observer(
            move |e: &Event| {
                if e.operation == "fsync-file" && e.path.ends_with("config.json")
                    || e.operation == "before-restore" && e.path.ends_with("proxies.json")
                {
                    return Err(io::Error::from_raw_os_error(13));
                }
                Ok(Action::Continue)
            },
            || recover(root),
        )
        .expect_err("actual durability failure");
        assert!(!error.to_string().is_empty());
        assert!(error.to_string().contains("proxies.json"), "{error}");
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
    a_migration_that_only_missed_its_cleanup_finishes_quietly,
    "tests/test_vault_commit_protocol.py::test_a_migration_that_only_missed_its_cleanup_finishes_quietly",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        assert!(recover(root).expect("resume"));
        assert_eq!(active(root), descriptors()[0]);
        prove(root, NEW, &active(root));
        assert!(!root.join("vault-migration.json").exists());
        journal(root, "rotate", &descriptors()[0], &descriptors()[1]);
        assert!(recovery::recover(root, Some(NEW), None).expect("cleanup only"));
    }
);
source_test!(
    a_store_opens_normally_after_the_protocol_settles,
    "tests/test_vault_commit_protocol.py::test_a_store_opens_normally_after_the_protocol_settles",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        assert!(recover(root).expect("resume"));
        assert_eq!(active(root), descriptors()[0]);
        prove(root, NEW, &active(root));
        assert!(!root.join("vault-migration.json").exists());
        let store =
            ProfileRepo::open(root.to_owned(), Some(NEW.into()), None).expect("settled open");
        assert!(store.unlocked().expect("unlock"));
        assert_eq!(
            store.vault().expect("descriptor"),
            Some(descriptors()[0].clone())
        );
        assert_no_plain(root, NEW);
    }
);
