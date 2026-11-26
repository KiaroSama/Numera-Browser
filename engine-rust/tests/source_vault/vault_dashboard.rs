use super::*;
source_test!(
    without_a_vault_nothing_changes,
    "tests/test_vault_dashboard.py::test_without_a_vault_nothing_changes",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let saved=dash(root,None).save(json!({"name":"a","os":"Windows","proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"hunter2-the-proxy-password"}}),Default::default(),None,None,1000.0).expect("dashboard save");
        let text = fs::read_to_string(
            root.join("dashboard")
                .join(format!("{}.json", saved["id"].as_str().expect("ID"))),
        )
        .expect("dashboard bytes");
        assert!(text.contains("hunter2-the-proxy-password"));
    }
);
source_test!(
    an_unlocked_vault_encrypts_the_dashboard_copy_too,
    "tests/test_vault_dashboard.py::test_an_unlocked_vault_encrypts_the_dashboard_copy_too",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, "pw");
        let saved=dash(root,Some("pw")).save(json!({"name":"a","os":"Windows","proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"hunter2-the-proxy-password"}}),Default::default(),None,None,1000.0).expect("dashboard save");
        assert_ne!(saved["proxy"]["pass"], "hunter2-the-proxy-password");
        assert!(
            !read(
                root,
                &format!("dashboard/{}.json", saved["id"].as_str().expect("ID"))
            )
            .to_string()
            .contains("hunter2-the-proxy-password")
        );
    }
);
source_test!(
    a_locked_vault_refuses_a_new_secret_rather_than_writing_it,
    "tests/test_vault_dashboard.py::test_a_locked_vault_refuses_a_new_secret_rather_than_writing_it",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, "pw");
        assert!(!repo(root, None).unlocked().expect("locked"));
        let error=dash(root,None).save(json!({"name":"a","os":"Windows","proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"hunter2-the-proxy-password"}}),Default::default(),None,None,1000.0).expect_err("locked secret");
        assert!(error.to_string().contains("locked"));
    }
);
source_test!(
    editing_a_non_secret_field_while_locked_keeps_the_password,
    "tests/test_vault_dashboard.py::test_editing_a_non_secret_field_while_locked_keeps_the_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, "pw");
        let mut saved=dash(root,Some("pw")).save(json!({"name":"a","os":"Windows","proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"hunter2-the-proxy-password"}}),Default::default(),None,None,1000.0).expect("dashboard save");
        let ciphertext = saved["proxy"]["pass"].clone();
        saved["name"] = "renamed while locked".into();
        let again = dash(root, None)
            .save(saved, Default::default(), None, None, 1001.0)
            .expect("locked edit");
        assert_eq!(again["name"], "renamed while locked");
        assert_eq!(again["proxy"]["pass"], ciphertext);
        assert!(!again.to_string().contains("hunter2-the-proxy-password"));
    }
);
source_test!(
    an_already_encrypted_value_is_not_encrypted_twice,
    "tests/test_vault_dashboard.py::test_an_already_encrypted_value_is_not_encrypted_twice",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, "pw");
        let saved=dash(root,Some("pw")).save(json!({"name":"a","os":"Windows","proxy":{"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"hunter2-the-proxy-password"}}),Default::default(),None,None,1000.0).expect("dashboard save");
        let again = dash(root, Some("pw"))
            .save(saved.clone(), Default::default(), None, None, 1000.0)
            .expect("dashboard save");
        assert_eq!(again["proxy"]["pass"], saved["proxy"]["pass"]);
    }
);
source_test!(
    a_profile_with_no_proxy_password_is_untouched,
    "tests/test_vault_dashboard.py::test_a_profile_with_no_proxy_password_is_untouched",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, "pw");
        let saved = dash(root, Some("pw"))
            .save(
                json!({"name":"no proxy","os":"Windows"}),
                Default::default(),
                None,
                None,
                1000.0,
            )
            .expect("dashboard save");
        assert_eq!(saved["name"], "no proxy");
    }
);
