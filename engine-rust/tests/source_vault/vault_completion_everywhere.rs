use super::*;
source_test!(
    same_salt_foreign_ciphertext_is_refused_with_the_password,
    "tests/test_vault_completion_everywhere.py::test_same_salt_foreign_ciphertext_is_refused_with_the_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        reseal(root, "foreign", &descriptors()[0], false);
        let before = snapshot(root);
        let error = recovery::recover(root, Some(NEW), None).expect_err("foreign generation");
        code(&error, "wrong_generation");
        assert_eq!(snapshot(root), before);
        prove(root, "foreign", &descriptors()[0]);
    }
);
source_test!(
    same_salt_foreign_ciphertext_is_refused_without_a_password,
    "tests/test_vault_completion_everywhere.py::test_same_salt_foreign_ciphertext_is_refused_without_a_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        reseal(root, "foreign", &descriptors()[0], false);
        let before = snapshot(root);
        let error = recovery::recover(root, None, None).expect_err("foreign generation");
        code(&error, "no_password");
        assert_eq!(snapshot(root), before);
        prove(root, "foreign", &descriptors()[0]);
    }
);
source_test!(
    a_damaged_config_verifier_is_not_hidden_by_a_matching_salt,
    "tests/test_vault_completion_everywhere.py::test_a_damaged_config_verifier_is_not_hidden_by_a_matching_salt",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        let mut cfg = config(root);
        cfg["vault"]["verifier"] = "enc:v2:opens-nothing".into();
        write(root, "config.json", cfg);
        let before = snapshot(root);
        let error = recover(root).expect_err("descriptor disagreement");
        code(&error, "descriptor_disagreement");
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    the_ordinary_already_committed_case_completes_with_the_key,
    "tests/test_vault_completion_everywhere.py::test_the_ordinary_already_committed_case_completes_with_the_key",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        let good = active(root);
        assert!(recover(root).expect("already committed recovery"));
        assert!(!root.join("vault-migration.json").exists());
        assert_eq!(active(root), good);
        prove(root, NEW, &good);
        for _ in 0..2 {
            assert_eq!(
                ProfileRepo::open(root.to_owned(), None, None)
                    .expect("settled store")
                    .vault()
                    .expect("vault"),
                Some(good.clone())
            );
        }
    }
);
source_test!(
    an_uninspectable_pool_entry_blocks_completion_case_004,
    "tests/test_vault_completion_everywhere.py::test_an_uninspectable_pool_entry_blocks_completion[[1, 2, \"x\"]-a pool of non-records]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"[1,2,"x"]"#).expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(observed.iter().any(|p| p.code == "unreadable_files"));
        let error = recover(root).expect_err("uninspectable secret holder");
        code(&error, "unreadable_files");
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    an_uninspectable_pool_entry_blocks_completion_case_005,
    "tests/test_vault_completion_everywhere.py::test_an_uninspectable_pool_entry_blocks_completion[[{\"id\": \"x\", \"pass\": 5}]-a secret that is not a string]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"[{"id":"x","pass":5}]"#).expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(observed.iter().any(|p| p.code == "unreadable_files"));
        let error = recover(root).expect_err("uninspectable secret holder");
        code(&error, "unreadable_files");
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    an_uninspectable_pool_entry_blocks_completion_case_006,
    "tests/test_vault_completion_everywhere.py::test_an_uninspectable_pool_entry_blocks_completion[[{\"id\": \"x\", \"pass\": {\"nested\": \"no\"}}]-a secret that is an object]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"[{"id":"x","pass":{"nested":"no"}}]"#)
                .expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(observed.iter().any(|p| p.code == "unreadable_files"));
        let error = recover(root).expect_err("uninspectable secret holder");
        code(&error, "unreadable_files");
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_legitimately_empty_pool_still_completes_case_007,
    "tests/test_vault_completion_everywhere.py::test_a_legitimately_empty_pool_still_completes[[]-an empty pool]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"[]"#).expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(!observed.iter().any(|p| p.code == "unreadable_files"));
        assert!(recover(root).expect("legitimate absent credential"));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_legitimately_empty_pool_still_completes_case_008,
    "tests/test_vault_completion_everywhere.py::test_a_legitimately_empty_pool_still_completes[[{\"id\": \"x\", \"host\": \"h\"}]-a pool entry with no secret at all]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"[{"id":"x","host":"h"}]"#).expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(!observed.iter().any(|p| p.code == "unreadable_files"));
        assert!(recover(root).expect("legitimate absent credential"));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_legitimately_empty_pool_still_completes_case_009,
    "tests/test_vault_completion_everywhere.py::test_a_legitimately_empty_pool_still_completes[[{\"id\": \"x\", \"pass\": \"\"}]-an explicitly empty secret]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "proxies.json",
            serde_json::from_str(r#"[{"id":"x","pass":""}]"#).expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(!observed.iter().any(|p| p.code == "unreadable_files"));
        assert!(recover(root).expect("legitimate absent credential"));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    an_uninspectable_profile_node_blocks_completion_case_010,
    "tests/test_vault_completion_everywhere.py::test_an_uninspectable_profile_node_blocks_completion[{\"id\": \"p1\", \"proxy\": 7}-a proxy node that is not an object]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"{"id":"p1","proxy":7}"#).expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(observed.iter().any(|p| p.code == "unreadable_files"));
        let error = recover(root).expect_err("uninspectable secret holder");
        code(&error, "unreadable_files");
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    an_uninspectable_profile_node_blocks_completion_case_011,
    "tests/test_vault_completion_everywhere.py::test_an_uninspectable_profile_node_blocks_completion[{\"id\": \"p1\", \"proxy\": {\"password\": 5}}-a secret that is not a string]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"{"id":"p1","proxy":{"password":5}}"#)
                .expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(observed.iter().any(|p| p.code == "unreadable_files"));
        let error = recover(root).expect_err("uninspectable secret holder");
        code(&error, "unreadable_files");
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    an_uninspectable_profile_node_blocks_completion_case_012,
    "tests/test_vault_completion_everywhere.py::test_an_uninspectable_profile_node_blocks_completion[{\"id\": \"p1\", \"proxy\": {\"password\": [\"a\"]}}-a secret that is a list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"{"id":"p1","proxy":{"password":["a"]}}"#)
                .expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(observed.iter().any(|p| p.code == "unreadable_files"));
        let error = recover(root).expect_err("uninspectable secret holder");
        code(&error, "unreadable_files");
        assert!(root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_profile_with_nothing_to_protect_still_completes_case_013,
    "tests/test_vault_completion_everywhere.py::test_a_profile_with_nothing_to_protect_still_completes[{\"id\": \"p1\", \"name\": \"one\"}-a profile with no proxy at all]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"{"id":"p1","name":"one"}"#).expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(!observed.iter().any(|p| p.code == "unreadable_files"));
        assert!(recover(root).expect("legitimate absent credential"));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_profile_with_nothing_to_protect_still_completes_case_014,
    "tests/test_vault_completion_everywhere.py::test_a_profile_with_nothing_to_protect_still_completes[{\"id\": \"p1\", \"proxy\": {\"host\": \"h\", \"port\": 1}}-a proxy with no password]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"{"id":"p1","proxy":{"host":"h","port":1}}"#)
                .expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(!observed.iter().any(|p| p.code == "unreadable_files"));
        assert!(recover(root).expect("legitimate absent credential"));
        assert!(!root.join("vault-migration.json").exists());
    }
);
source_test!(
    a_profile_with_nothing_to_protect_still_completes_case_015,
    "tests/test_vault_completion_everywhere.py::test_a_profile_with_nothing_to_protect_still_completes[{\"id\": \"p1\", \"proxy\": {\"password\": \"\"}}-an explicitly empty password]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        setup(root, "enable", "new", "new", false);
        write(
            root,
            "profiles/p1.json",
            serde_json::from_str(r#"{"id":"p1","proxy":{"password":""}}"#)
                .expect("original holder shape"),
        );
        let observed =
            completion::problems(root, &descriptors()[0], Some(NEW)).expect("inspect secret sites");
        assert!(!observed.iter().any(|p| p.code == "unreadable_files"));
        assert!(recover(root).expect("legitimate absent credential"));
        assert!(!root.join("vault-migration.json").exists());
    }
);
