use super::*;
source_test!(
    a_repo_create_under_an_unlocked_vault_leaves_no_plaintext,
    "tests/test_repo_secrets.py::test_a_repo_create_under_an_unlocked_vault_leaves_no_plaintext",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, MASTER);
        let p = repo(root, Some(MASTER))
            .save(model("fresh", 1, Some("SECRET")), 1001.0)
            .expect("vaulted create");
        assert_no_plain(root, MASTER);
        let (engine, dashboard) = raw_pair(root, &p.id);
        assert!(vault::is_envelope(
            engine["proxy"]["password"].as_str().expect("engine secret")
        ));
        assert!(vault::is_envelope(
            dashboard["proxy"]["pass"]
                .as_str()
                .expect("dashboard secret")
        ));
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .get(&p.id)
                    .expect("read")
                    .expect("profile")
            ),
            "SECRET"
        );
    }
);
source_test!(
    a_repo_edit_under_an_unlocked_vault_leaves_no_plaintext,
    "tests/test_repo_secrets.py::test_a_repo_edit_under_an_unlocked_vault_leaves_no_plaintext",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "creds", 1, Some("SECRET"));
        enable(root, MASTER);
        repo(root, Some(MASTER))
            .edit(
                "creds",
                |p| {
                    p.notes = "edited".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("edit");
        assert_no_plain(root, MASTER);
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .get(&p.id)
                    .expect("read")
                    .expect("profile")
            ),
            "SECRET"
        );
    }
);
source_test!(
    the_cli_edit_path_leaves_no_plaintext,
    "tests/test_repo_secrets.py::test_the_cli_edit_path_leaves_no_plaintext",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "creds", 1, Some("SECRET"));
        enable(root, MASTER);
        cli(root, Some(MASTER), |profiles, _| {
            assert_eq!(
                profiles
                    .edit(
                        "creds",
                        json!({"notes":"from the cli"}).as_object().expect("args"),
                        1001.0
                    )
                    .expect("CLI edit")
                    .exit,
                0
            )
        });
        assert_eq!(raw_pair(root, &p.id).1["notes"], "from the cli");
        assert_no_plain(root, MASTER);
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .get(&p.id)
                    .expect("read")
                    .expect("profile")
            ),
            "SECRET"
        );
    }
);
source_test!(
    a_locked_notes_edit_keeps_both_ciphertexts_untouched,
    "tests/test_repo_secrets.py::test_a_locked_notes_edit_keeps_both_ciphertexts_untouched",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "creds", 1, Some("SECRET"));
        enable(root, MASTER);
        let before = raw_pair(root, &p.id);
        assert!(vault::is_envelope(
            before.0["proxy"]["password"].as_str().expect("ciphertext")
        ));
        repo(root, None)
            .edit(
                "creds",
                |p| {
                    p.notes = "while locked".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("locked edit");
        let after = raw_pair(root, &p.id);
        assert_eq!(after.0["proxy"]["password"], before.0["proxy"]["password"]);
        assert_eq!(after.1["proxy"]["pass"], before.1["proxy"]["pass"]);
        assert_eq!(after.1["notes"], "while locked");
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .get(&p.id)
                    .expect("read")
                    .expect("profile")
            ),
            "SECRET"
        );
    }
);
source_test!(
    a_locked_password_replacement_is_refused_and_changes_nothing,
    "tests/test_repo_secrets.py::test_a_locked_password_replacement_is_refused_and_changes_nothing",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "creds", 1, Some("SECRET"));
        enable(root, MASTER);
        let before = raw_pair(root, &p.id);
        assert!(
            repo(root, None)
                .edit(
                    "creds",
                    |p| {
                        p.proxy
                            .as_mut()
                            .expect("proxy")
                            .set_password(Some("NEW-PLAINTEXT".into()));
                        Ok(None)
                    },
                    1001.0
                )
                .is_err()
        );
        assert_eq!(raw_pair(root, &p.id), before);
    }
);
source_test!(
    save_many_seals_every_secret,
    "tests/test_repo_secrets.py::test_save_many_seals_every_secret",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, MASTER);
        let profiles = (0..3)
            .map(|i| model(&format!("b{i}"), i, Some("SECRET")))
            .collect();
        repo(root, Some(MASTER))
            .save_many(profiles, Default::default(), "reject", 1001.0)
            .expect("batch save");
        assert_no_plain(root, MASTER);
        for i in 0..3 {
            assert_eq!(
                secret(
                    &repo(root, Some(MASTER))
                        .resolve(&format!("b{i}"), 1002.0)
                        .expect("resolve")
                        .expect("profile")
                ),
                "SECRET"
            );
        }
    }
);
source_test!(
    one_locked_replacement_refuses_the_whole_batch,
    "tests/test_repo_secrets.py::test_one_locked_replacement_refuses_the_whole_batch",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let first = saved(root, "one", 1, Some("SECRET"));
        let second = saved(root, "two", 2, Some("SECRET"));
        enable(root, MASTER);
        let before = raw_pair(root, &first.id);
        let locked = repo(root, None);
        let mut a = locked.get(&first.id).expect("get").expect("first");
        let mut b = locked.get(&second.id).expect("get").expect("second");
        a.notes = "fine".into();
        b.proxy
            .as_mut()
            .expect("proxy")
            .set_password(Some("NEW-PLAINTEXT".into()));
        assert!(
            locked
                .save_many(vec![a, b], Default::default(), "reject", 1001.0)
                .is_err()
        );
        assert_eq!(raw_pair(root, &first.id), before);
    }
);
source_test!(
    a_password_rotation_after_repo_writes_can_read_every_credential,
    "tests/test_repo_secrets.py::test_a_password_rotation_after_repo_writes_can_read_every_credential",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        enable(root, MASTER);
        for i in 0..3 {
            repo(root, Some(MASTER))
                .save(model(&format!("p{i}"), i, Some("SECRET")), 1001.0)
                .expect("repo write");
        }
        transaction::rotate(root, MASTER, NEW).expect("rotate repo writes");
        assert!(repo(root, Some(NEW)).unlocked().expect("unlock"));
        for i in 0..3 {
            assert_eq!(
                secret(
                    &repo(root, Some(NEW))
                        .resolve(&format!("p{i}"), 1002.0)
                        .expect("resolve")
                        .expect("profile")
                ),
                "SECRET"
            );
        }
        assert_no_plain(root, NEW);
    }
);
source_test!(
    a_new_twin_is_born_with_created_and_an_edit_advances_updated_at,
    "tests/test_repo_secrets.py::test_a_new_twin_is_born_with_created_and_an_edit_advances_updated_at",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let p = saved(root, "meta", 1, None);
        let before = raw_pair(root, &p.id);
        assert!(before.1["_created"].as_f64().is_some_and(|v| v > 0.0));
        assert_eq!(before.1["_rev"], 1);
        repo(root, None)
            .edit(
                "meta",
                |p| {
                    p.notes = "n".into();
                    Ok(None)
                },
                1001.0,
            )
            .expect("edit");
        let after = raw_pair(root, &p.id);
        assert!(after.0["updated_at"].as_f64() > before.0["updated_at"].as_f64());
        assert_eq!(after.1["_created"], before.1["_created"]);
        assert_eq!(after.1["_rev"], 2);
    }
);
