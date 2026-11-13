use super::*;
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_000,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-bad-verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "bad-verifier", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_001,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-missing-verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "missing-verifier", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_002,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-foreign]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "foreign", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_003,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-false]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "false", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_004,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "list", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_005,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-dict]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "dict", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_006,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-null]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "null", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_007,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[False-absent]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "absent", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_008,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-bad-verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "bad-verifier", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_009,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-missing-verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "missing-verifier", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_010,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-foreign]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "foreign", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_011,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-false]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "false", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_012,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "list", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_013,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-dict]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "dict", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_014,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-null]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "null", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    rotation_never_confirms_or_overwrites_a_damaged_active_descriptor_case_015,
    "tests/test_vault_active_descriptor.py::test_rotation_never_confirms_or_overwrites_a_damaged_active_descriptor[True-absent]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "absent", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_016,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[False-old]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "old", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_017,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[False-foreign]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "foreign", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_018,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[False-bad-verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "bad-verifier", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_019,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[False-false]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "false", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_020,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[False-list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "list", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_021,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[False-dict]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "dict", "new", false);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_022,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[True-old]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "old", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_023,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[True-foreign]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "foreign", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_024,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[True-bad-verifier]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "bad-verifier", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_025,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[True-false]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "false", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_026,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[True-list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "list", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    enable_checks_config_before_both_plaintext_and_cleanup_paths_case_027,
    "tests/test_vault_active_descriptor.py::test_enable_checks_config_before_both_plaintext_and_cleanup_paths[True-dict]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "dict", "new", true);
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_028,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[enable-absent-new-False]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_029,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[enable-absent-new-True]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", true);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_030,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[enable-null-new-True]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "null", "new", true);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_031,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[enable-new-new-False]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_032,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[enable-new-new-True]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", true);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_033,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[rotate-old-old-False]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", false);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_034,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[rotate-old-old-True]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_035,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[rotate-new-new-False]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "new", false);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    valid_transitions_preserve_all_values_and_reopen_twice_case_036,
    "tests/test_vault_active_descriptor.py::test_valid_transitions_preserve_all_values_and_reopen_twice[rotate-new-old-True]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "old", true);
        assert!(recover(root).expect("valid recovery"));
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("idempotent reopen"));
            prove(root, NEW, &active(root));
        }
        assert_eq!(read(root, "profiles/p1.json")["id"], "p1");
        assert_eq!(read(root, "proxies.json")[0]["id"], "pool");
    }
);
source_test!(
    missing_password_preserves_evidence_case_037,
    "tests/test_vault_active_descriptor.py::test_missing_password_preserves_evidence[enable-absent]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", false);
        let before = snapshot(root);
        assert!(recovery::recover(root, None, None).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    missing_password_preserves_evidence_case_038,
    "tests/test_vault_active_descriptor.py::test_missing_password_preserves_evidence[enable-new]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        let before = snapshot(root);
        assert!(recovery::recover(root, None, None).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    missing_password_preserves_evidence_case_039,
    "tests/test_vault_active_descriptor.py::test_missing_password_preserves_evidence[rotate-old]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "new", false);
        let before = snapshot(root);
        assert!(recovery::recover(root, None, None).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    missing_password_preserves_evidence_case_040,
    "tests/test_vault_active_descriptor.py::test_missing_password_preserves_evidence[rotate-new]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "new", false);
        let before = snapshot(root);
        assert!(recovery::recover(root, None, None).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    successful_save_callback_must_really_persist_the_expected_descriptor_case_041,
    "tests/test_vault_active_descriptor.py::test_successful_save_callback_must_really_persist_the_expected_descriptor[noop-enable-absent]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", true);
        let before = snapshot(root);
        let error = testing::with_observer(
            move |event| {
                if event.operation == "before-config" {
                    return Ok(Action::Skip);
                }
                Ok(Action::Continue)
            },
            || recover(root),
        )
        .expect_err("invalid persisted descriptor must refuse");
        assert!(!error.to_string().is_empty());
        assert_eq!(snapshot(root), before);
        assert!(recover(root).expect("retry"));
        prove(root, NEW, &active(root));
    }
);
source_test!(
    successful_save_callback_must_really_persist_the_expected_descriptor_case_042,
    "tests/test_vault_active_descriptor.py::test_successful_save_callback_must_really_persist_the_expected_descriptor[noop-rotate-old]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "new", true);
        let before = snapshot(root);
        let error = testing::with_observer(
            move |event| {
                if event.operation == "before-config" {
                    return Ok(Action::Skip);
                }
                Ok(Action::Continue)
            },
            || recover(root),
        )
        .expect_err("invalid persisted descriptor must refuse");
        assert!(!error.to_string().is_empty());
        assert_eq!(snapshot(root), before);
        assert!(recover(root).expect("retry"));
        prove(root, NEW, &active(root));
    }
);
source_test!(
    successful_save_callback_must_really_persist_the_expected_descriptor_case_043,
    "tests/test_vault_active_descriptor.py::test_successful_save_callback_must_really_persist_the_expected_descriptor[damaged-verifier-enable-absent]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", true);
        let before = snapshot(root);
        let error = testing::with_observer(
            move |event| {
                if event.operation == "after-config" {
                    let mut config: Value = serde_json::from_slice(&fs::read(&event.path)?)
                        .map_err(io::Error::other)?;
                    config["vault"]["verifier"] = "damaged-after-save".into();
                    fs::write(
                        &event.path,
                        serde_json::to_vec(&config).map_err(io::Error::other)?,
                    )?;
                }
                Ok(Action::Continue)
            },
            || recover(root),
        )
        .expect_err("invalid persisted descriptor must refuse");
        assert!(!error.to_string().is_empty());
        assert_eq!(snapshot(root), before);
        assert!(recover(root).expect("retry"));
        prove(root, NEW, &active(root));
    }
);
source_test!(
    successful_save_callback_must_really_persist_the_expected_descriptor_case_044,
    "tests/test_vault_active_descriptor.py::test_successful_save_callback_must_really_persist_the_expected_descriptor[damaged-verifier-rotate-old]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "new", true);
        let before = snapshot(root);
        let error = testing::with_observer(
            move |event| {
                if event.operation == "after-config" {
                    let mut config: Value = serde_json::from_slice(&fs::read(&event.path)?)
                        .map_err(io::Error::other)?;
                    config["vault"]["verifier"] = "damaged-after-save".into();
                    fs::write(
                        &event.path,
                        serde_json::to_vec(&config).map_err(io::Error::other)?,
                    )?;
                }
                Ok(Action::Continue)
            },
            || recover(root),
        )
        .expect_err("invalid persisted descriptor must refuse");
        assert!(!error.to_string().is_empty());
        assert_eq!(snapshot(root), before);
        assert!(recover(root).expect("retry"));
        prove(root, NEW, &active(root));
    }
);
source_test!(
    invalid_utf8_refuses_without_losing_recovery_evidence_case_045,
    "tests/test_vault_active_descriptor.py::test_invalid_utf8_refuses_without_losing_recovery_evidence[vault-migration.json]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "new", false);
        fs::write(root.join("vault-migration.json"), [0xff, 0xfe]).expect("invalid UTF8");
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    invalid_utf8_refuses_without_losing_recovery_evidence_case_046,
    "tests/test_vault_active_descriptor.py::test_invalid_utf8_refuses_without_losing_recovery_evidence[proxies.json]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "new", false);
        fs::write(root.join("proxies.json"), [0xff, 0xfe]).expect("invalid UTF8");
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    journal_unlink_failure_is_not_silently_reported_as_completion,
    "tests/test_vault_active_descriptor.py::test_journal_unlink_failure_is_not_silently_reported_as_completion",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "new", "new", false);
        let error = testing::with_observer(
            error_at("before-cleanup", "vault-migration.json", 13),
            || recover(root),
        )
        .expect_err("cleanup failure");
        assert!(error.to_string().contains("cleanup"), "{error}");
        assert!(root.join("vault-migration.json").exists());
        prove(root, NEW, &active(root));
        assert!(recover(root).expect("retry cleanup"));
        prove(root, NEW, &active(root));
    }
);
source_test!(
    damaged_config_is_not_treated_as_an_empty_fresh_install_case_048,
    "tests/test_vault_active_descriptor.py::test_damaged_config_is_not_treated_as_an_empty_fresh_install[\\xff]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", true);
        fs::write(root.join("config.json"), vec![0xff]).expect("damaged config");
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    damaged_config_is_not_treated_as_an_empty_fresh_install_case_049,
    "tests/test_vault_active_descriptor.py::test_damaged_config_is_not_treated_as_an_empty_fresh_install[{bad]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", true);
        fs::write(root.join("config.json"), b"{bad").expect("damaged config");
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    damaged_config_is_not_treated_as_an_empty_fresh_install_case_050,
    "tests/test_vault_active_descriptor.py::test_damaged_config_is_not_treated_as_an_empty_fresh_install[[]]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", true);
        fs::write(root.join("config.json"), b"[]").expect("damaged config");
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    damaged_config_is_not_treated_as_an_empty_fresh_install_case_051,
    "tests/test_vault_active_descriptor.py::test_damaged_config_is_not_treated_as_an_empty_fresh_install[null]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "absent", "new", true);
        fs::write(root.join("config.json"), b"null").expect("damaged config");
        let before = snapshot(root);
        assert!(recover(root).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_052,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[before-recovery]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "before-recovery");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_053,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[profiles]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "profiles");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_054,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[dashboard]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "dashboard");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_055,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[proxies.json]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "proxies.json");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_056,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[before-config]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "before-config");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_057,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[after-config]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "after-config");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_058,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[before-config-sync]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "before-config-sync");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_059,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[after-config-sync]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "after-config-sync");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_060,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[before-cleanup]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "before-cleanup");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
source_test!(
    abrupt_exit_can_recover_all_three_sites_and_reopen_twice_case_061,
    "tests/test_vault_active_descriptor.py::test_abrupt_exit_can_recover_all_three_sites_and_reopen_twice[after-cleanup]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "rotate", "old", "old", true);
        crash(root, "recover", "after-cleanup");
        recover(root).expect("recover owned abrupt exit");
        prove(root, NEW, &active(root));
        for _ in 0..2 {
            assert!(!recover(root).expect("second reopen"));
            prove(root, NEW, &active(root));
        }
    }
);
