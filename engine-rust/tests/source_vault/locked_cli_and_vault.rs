use super::*;
source_test!(
    locked_pool_assign_refuses_and_changes_nothing,
    "tests/test_locked_cli_and_vault.py::test_locked_pool_assign_refuses_and_changes_nothing",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _id = pool_vault(root);
        let before = snapshot(root);
        cli(root, None, |_, commands| {
            let result = commands
                .assign(json!({"all":true}).as_object().expect("args"), 1001.0)
                .expect("CLI assign");
            assert_eq!(result.exit, 1);
            assert!(
                result.payload["error"]
                    .as_str()
                    .expect("error")
                    .contains("master password")
            );
        });
        assert_eq!(snapshot(root), before);
        assert!(
            repo(root, Some(MASTER))
                .list(1002.0)
                .expect("profiles")
                .iter()
                .all(|p| p.proxy.is_none())
        );
    }
);
source_test!(
    unlocked_pool_assign_carries_the_real_credential,
    "tests/test_locked_cli_and_vault.py::test_unlocked_pool_assign_carries_the_real_credential",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _id = pool_vault(root);
        cli(root, Some(MASTER), |_, commands| {
            assert_eq!(
                commands
                    .assign(json!({"all":true}).as_object().expect("args"), 1001.0)
                    .expect("CLI assign")
                    .exit,
                0
            )
        });
        let profiles = repo(root, Some(MASTER)).list(1002.0).expect("profiles");
        assert!(!profiles.is_empty());
        for p in profiles {
            assert!(p.proxy.is_some());
            assert_eq!(secret(&p), "PROXY-PASSWORD");
        }
    }
);
source_test!(
    a_locked_plaintext_replacement_is_refused,
    "tests/test_locked_cli_and_vault.py::test_a_locked_plaintext_replacement_is_refused",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _id = pool_vault(root);
        let before = read(root, "proxies.json");
        let cipher = before[0]["pass"].as_str().expect("ciphertext");
        assert!(vault::is_envelope(cipher));
        assert!(
            vault::policy::write_secret(
                vault::policy::VaultState::Locked,
                Some("NEW-PASSWORD"),
                Some(cipher),
                |value| vault::encrypt(MASTER, active(root)["salt"].as_str().expect("salt"), value)
            )
            .is_err()
        );
        assert_eq!(read(root, "proxies.json"), before);
    }
);
source_test!(
    a_withheld_value_preserves_the_secret_and_an_empty_one_clears,
    "tests/test_locked_cli_and_vault.py::test_a_withheld_value_preserves_the_secret_and_an_empty_one_clears",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _id = pool_vault(root);
        let cipher = read(root, "proxies.json")[0]["pass"]
            .as_str()
            .expect("ciphertext")
            .to_owned();
        let seal =
            |v: &str| vault::encrypt(MASTER, active(root)["salt"].as_str().expect("salt"), v);
        use vault::policy::{VaultState, write_secret};
        assert_eq!(
            write_secret(VaultState::Locked, None, Some(&cipher), seal).expect("preserve"),
            Some(cipher.clone())
        );
        assert!(write_secret(VaultState::Locked, Some(""), Some(&cipher), seal).is_err());
        assert_eq!(
            write_secret(VaultState::Locked, Some(""), None, seal).expect("empty clear"),
            Some("".into())
        );
        assert_eq!(
            write_secret(VaultState::Unlocked, Some(""), Some(&cipher), seal)
                .expect("unlocked clear"),
            Some("".into())
        );
    }
);
source_test!(
    a_locked_non_secret_edit_still_works,
    "tests/test_locked_cli_and_vault.py::test_a_locked_non_secret_edit_still_works",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = pool_vault(root);
        pool(root, None)
            .update(
                &id,
                json!({"country":"DE"}).as_object().expect("patch").clone(),
            )
            .expect("country edit");
        assert_eq!(
            pool(root, None).get(&id).expect("get").expect("proxy")["country"],
            "DE"
        );
        assert!(vault::is_envelope(
            read(root, "proxies.json")[0]["pass"]
                .as_str()
                .expect("ciphertext")
        ));
    }
);
source_test!(
    an_unreadable_envelope_is_never_handed_out_as_a_password,
    "tests/test_locked_cli_and_vault.py::test_an_unreadable_envelope_is_never_handed_out_as_a_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = pool_vault(root);
        let future = format!("enc:v9:{}", "Z".repeat(40));
        let mut raw = read(root, "proxies.json");
        raw[0]["pass"] = future.clone().into();
        write(root, "proxies.json", raw);
        assert!(vault::is_envelope(&future));
        let store = pool(root, Some(MASTER));
        assert!(store.secret_withheld(&id).expect("withheld"));
        assert_ne!(
            store.get(&id).expect("get").expect("proxy")["pass"],
            json!(future)
        );
    }
);
source_test!(
    corrupt_ciphertext_counts_as_withheld_not_absent,
    "tests/test_locked_cli_and_vault.py::test_corrupt_ciphertext_counts_as_withheld_not_absent",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = pool_vault(root);
        let future = "enc:v2:not-valid-base64-!!!".to_owned();
        let mut raw = read(root, "proxies.json");
        raw[0]["pass"] = future.clone().into();
        write(root, "proxies.json", raw);
        assert!(vault::is_envelope(&future));
        let store = pool(root, Some(MASTER));
        assert!(store.secret_withheld(&id).expect("withheld"));
        assert_ne!(
            store.get(&id).expect("get").expect("proxy")["pass"],
            json!(future)
        );
    }
);
source_test!(
    cli_export_refuses_when_one_secret_is_unreadable,
    "tests/test_locked_cli_and_vault.py::test_cli_export_refuses_when_one_secret_is_unreadable",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _id = pool_vault(root);
        let future = format!("enc:v9:{}", "Z".repeat(40));
        let mut raw = read(root, "proxies.json");
        raw[0]["pass"] = future.clone().into();
        write(root, "proxies.json", raw);
        assert!(vault::is_envelope(&future));
        cli(root, Some(MASTER), |_, commands| {
            let result = commands.export(&[], None).expect("CLI export refusal");
            assert_eq!(result.exit, 1);
            assert!(
                result.payload["error"]
                    .as_str()
                    .expect("error")
                    .contains("cannot be read")
            );
        });
    }
);
