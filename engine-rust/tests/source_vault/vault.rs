use super::*;
source_test!(round_trip, "tests/test_vault.py::test_round_trip", {
    let temp = common::TempDirectory::new("source-vault").expect("temporary root");
    let _root = temp.path();
    let desc = descriptor("pw", "round-trip-salt!");
    let salt = desc["salt"].as_str().expect("salt");
    let token = vault::encrypt("pw", salt, "hello").expect("seal");
    assert!(vault::is_envelope(&token));
    assert_eq!(vault::decrypt("pw", salt, &token).expect("open"), "hello");
    temp.cleanup().expect("owned cleanup");
});
source_test!(
    wrong_password_raises,
    "tests/test_vault.py::test_wrong_password_raises",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let _root = temp.path();
        let desc = descriptor("right", "round-trip-salt!");
        let token =
            vault::encrypt("right", desc["salt"].as_str().expect("salt"), "hello").expect("seal");
        assert!(vault::decrypt("wrong", desc["salt"].as_str().expect("salt"), &token).is_err());
    }
);
source_test!(
    verifier_checks_password,
    "tests/test_vault.py::test_verifier_checks_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let _root = temp.path();
        let desc = descriptor("pw", "round-trip-salt!");
        assert!(vault::check_password(
            "pw",
            desc["salt"].as_str().expect("salt"),
            desc["verifier"].as_str().expect("verifier")
        ));
        assert!(!vault::check_password(
            "nope",
            desc["salt"].as_str().expect("salt"),
            desc["verifier"].as_str().expect("verifier")
        ));
    }
);
source_test!(
    enable_encrypts_existing_secret,
    "tests/test_vault.py::test_enable_encrypts_existing_secret",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "acct", 1, Some("s3cret"));
        enable(root, MASTER);
        let raw = raw_pair(root, &p.id).0;
        assert!(vault::is_envelope(
            raw["proxy"]["password"].as_str().expect("ciphertext")
        ));
        assert!(!raw.to_string().contains("s3cret"));
    }
);
source_test!(
    unlocked_store_reads_the_secret_back,
    "tests/test_vault.py::test_unlocked_store_reads_the_secret_back",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _p = saved(root, "acct", 1, Some("s3cret"));
        enable(root, MASTER);
        let reopened =
            ProfileRepo::open(root.to_owned(), Some(MASTER.into()), None).expect("reopen");
        assert!(reopened.unlocked().expect("unlock"));
        assert_eq!(
            secret(
                &reopened
                    .resolve("acct", 1001.0)
                    .expect("resolve")
                    .expect("profile")
            ),
            "s3cret"
        );
    }
);
source_test!(
    locked_store_hides_the_secret,
    "tests/test_vault.py::test_locked_store_hides_the_secret",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "acct", 1, Some("s3cret"));
        enable(root, MASTER);
        let locked = repo(root, None);
        assert!(locked.vault_enabled().expect("enabled"));
        assert!(!locked.unlocked().expect("locked"));
        let loaded = locked.get(&p.id).expect("get").expect("profile");
        assert_eq!(loaded.name, "acct");
        assert_eq!(secret(&loaded), Value::Null);
    }
);
source_test!(
    wrong_password_leaves_store_locked,
    "tests/test_vault.py::test_wrong_password_leaves_store_locked",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _p = saved(root, "acct", 1, Some("s3cret"));
        enable(root, MASTER);
        assert!(!repo(root, Some("nope")).unlocked().expect("wrong password"));
    }
);
source_test!(
    saving_a_secret_while_locked_is_refused,
    "tests/test_vault.py::test_saving_a_secret_while_locked_is_refused",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "acct", 1, Some("s3cret"));
        enable(root, MASTER);
        let locked = repo(root, None);
        let mut loaded = locked.get(&p.id).expect("get").expect("profile");
        loaded
            .proxy
            .as_mut()
            .expect("proxy")
            .set_password(Some("new-plaintext".into()));
        let before = snapshot(root);
        assert!(locked.save(loaded, 1001.0).is_err());
        assert_eq!(snapshot(root), before);
    }
);
source_test!(
    non_secret_profiles_save_fine_while_locked,
    "tests/test_vault.py::test_non_secret_profiles_save_fine_while_locked",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, MASTER);
        assert_eq!(
            repo(root, None)
                .save(model("noproxy", 2, None), 1001.0)
                .expect("no secret save")
                .name,
            "noproxy"
        );
    }
);
source_test!(
    enable_is_idempotent_guard,
    "tests/test_vault.py::test_enable_is_idempotent_guard",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, MASTER);
        assert!(transaction::enable(root, MASTER).is_err());
    }
);
source_test!(
    a_non_secret_edit_while_locked_keeps_the_password,
    "tests/test_vault.py::test_a_non_secret_edit_while_locked_keeps_the_password",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "acct", 1, Some("s3cret"));
        enable(root, MASTER);
        let ciphertext = raw_pair(root, &p.id).0["proxy"]["password"].clone();
        let locked = repo(root, None);
        let mut loaded = locked.get(&p.id).expect("get").expect("profile");
        assert_eq!(secret(&loaded), Value::Null);
        loaded.name = "renamed while locked".into();
        locked.save(loaded, 1001.0).expect("ordinary edit");
        let raw = raw_pair(root, &p.id).0;
        assert_eq!(raw["proxy"]["password"], ciphertext);
        assert_eq!(raw["name"], "renamed while locked");
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .resolve("renamed while locked", 1002.0)
                    .expect("resolve")
                    .expect("profile")
            ),
            "s3cret"
        );
    }
);
source_test!(
    an_interrupted_save_does_not_truncate_a_profile,
    "tests/test_vault.py::test_an_interrupted_save_does_not_truncate_a_profile",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "acct", 1, Some("s3cret"));
        let path = root.join("profiles").join(format!("{}.json", p.id));
        let good = fs::read(&path).expect("original bytes");
        let mut changed = p.clone();
        changed.notes = "this write fails".into();
        assert!(
            testing::with_observer(error_at("before-atomic-write", "", 28), || repo(root, None)
                .save(changed, 1001.0))
            .is_err()
        );
        assert_eq!(fs::read(path).expect("preserved bytes"), good);
    }
);
