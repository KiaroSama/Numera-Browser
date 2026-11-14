use super::*;
source_test!(
    rotation_moves_every_secret_and_retires_the_old_password,
    "tests/test_vault_rotation.py::test_rotation_moves_every_secret_and_retires_the_old_password",
    {
        missing_or_nontext_salt_refuses_before_rotation_journal();
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let before = active(root);
        assert!(transaction::rotate(root, MASTER, NEW).expect("rotate") >= 2);
        assert_ne!(active(root)["salt"], before["salt"]);
        assert!(repo(root, Some(NEW)).unlocked().expect("new key"));
        assert_eq!(
            secret(
                &repo(root, Some(NEW))
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            "PROXY-PASSWORD"
        );
        assert_eq!(
            pool(root, Some(NEW)).list().expect("pool")[0]["pass"],
            "PROXY-PASSWORD"
        );
        assert!(
            !repo(root, Some(MASTER))
                .unlocked()
                .expect("old key retired")
        );
    }
);
fn missing_or_nontext_salt_refuses_before_rotation_journal() {
    let verifier = vault::encrypt(MASTER, "", "numera-vault-ok").expect("empty-salt verifier");
    let ciphertext = vault::encrypt(MASTER, "", "original-secret").expect("empty-salt secret");
    let mut accepted = Vec::new();
    for salt in [
        None,
        Some(Value::Null),
        Some(json!(17)),
        Some(json!(false)),
        Some(json!([])),
        Some(json!({})),
    ] {
        let temp = common::TempDirectory::new("malformed-rotation-salt").expect("owned root");
        let root = temp.path();
        let profile = saved(root, "rotation-refusal", 22, Some("original-secret"));
        let (mut engine, mut dashboard) = raw_pair(root, &profile.id);
        engine["proxy"]["password"] = ciphertext.clone().into();
        dashboard["proxy"]["pass"] = ciphertext.clone().into();
        write(root, &format!("profiles/{}.json", profile.id), engine);
        write(root, &format!("dashboard/{}.json", profile.id), dashboard);
        let mut descriptor = json!({"verifier":verifier});
        if let Some(value) = &salt {
            descriptor["salt"] = value.clone();
        }
        write(root, "config.json", json!({"vault":descriptor}));
        let before = snapshot(root);
        let reached = Arc::new(Mutex::new(Vec::new()));
        let observed = reached.clone();
        let result = testing::with_observer(
            move |event| {
                if matches!(
                    event.operation,
                    "before-vault-salt-randomness" | "before-migration"
                ) {
                    observed
                        .lock()
                        .expect("rotation observations")
                        .push(event.operation);
                }
                Ok(Action::Continue)
            },
            || transaction::rotate(root, MASTER, NEW),
        );
        match result {
            Ok(_) => accepted.push(format!("salt={salt:?}")),
            Err(error) => {
                assert_eq!(
                    error.kind,
                    recovery::RecoveryErrorKind::VaultRecoveryRequired
                );
                assert!(error.to_string().contains("descriptor"));
                assert!(
                    reached.lock().expect("rotation observations").is_empty(),
                    "Refusal reached new randomness or journal publication"
                );
                assert_eq!(snapshot(root), before);
                assert!(!root.join("vault-migration.json").exists());
            }
        }
    }
    assert!(
        accepted.is_empty(),
        "Malformed descriptors entered actual rotation: {accepted:?}"
    );
}
source_test!(
    the_cli_rotates,
    "tests/test_vault_rotation.py::test_the_cli_rotates",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        assert_eq!(
            numera_core::cli::vault_commands::run(root, "change", Some(MASTER), Some(NEW))
                .expect("CLI rotate")
                .exit,
            0
        );
        assert_eq!(
            secret(
                &repo(root, Some(NEW))
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            "PROXY-PASSWORD"
        );
    }
);
source_test!(
    the_wrong_current_password_changes_nothing,
    "tests/test_vault_rotation.py::test_the_wrong_current_password_changes_nothing",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let before = snapshot(root);
        assert!(transaction::rotate(root, "not-the-password", NEW).is_err());
        assert_eq!(snapshot(root), before);
        assert!(repo(root, Some(MASTER)).unlocked().expect("old key"));
    }
);
source_test!(
    a_failure_part_way_leaves_the_old_password_working,
    "tests/test_vault_rotation.py::test_a_failure_part_way_leaves_the_old_password_working",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let before = snapshot(root);
        let mut calls = 0;
        assert!(
            testing::with_observer(
                move |e| {
                    if e.operation == "encrypt" {
                        calls += 1;
                        if calls == 2 {
                            return Err(io::Error::from_raw_os_error(28));
                        }
                    }
                    Ok(Action::Continue)
                },
                || transaction::rotate(root, MASTER, NEW)
            )
            .is_err()
        );
        for (path, bytes) in before {
            assert_eq!(fs::read(root.join(path)).expect("restored bytes"), bytes);
        }
        assert!(repo(root, Some(MASTER)).unlocked().expect("old key"));
        assert_eq!(
            secret(
                &repo(root, Some(MASTER))
                    .get(&prof.id)
                    .expect("get")
                    .expect("profile")
            ),
            "PROXY-PASSWORD"
        );
    }
);
source_test!(
    a_secret_this_build_cannot_open_stops_the_rotation,
    "tests/test_vault_rotation.py::test_a_secret_this_build_cannot_open_stops_the_rotation",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        let mut rows = read(root, "proxies.json");
        rows[0]["pass"] = format!("enc:v9:{}", "Z".repeat(40)).into();
        write(root, "proxies.json", rows);
        let before = active(root);
        assert!(transaction::rotate(root, MASTER, NEW).is_err());
        assert_eq!(active(root), before);
        assert!(repo(root, Some(MASTER)).unlocked().expect("old key"));
    }
);
source_test!(
    rotating_without_a_vault_is_refused,
    "tests/test_vault_rotation.py::test_rotating_without_a_vault_is_refused",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        saved(root, "p", 3, None);
        assert!(transaction::rotate(root, MASTER, NEW).is_err());
    }
);
source_test!(
    an_empty_new_password_is_refused,
    "tests/test_vault_rotation.py::test_an_empty_new_password_is_refused",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let _prof = saved(root, "p1", 1, Some("PROXY-PASSWORD"));
        pool(root, None)
            .add_many(vec![pool_entry("5.6.7.8", json!("PROXY-PASSWORD"))])
            .expect("pool");
        enable(root, MASTER);
        assert!(transaction::rotate(root, MASTER, "").is_err());
        assert!(repo(root, Some(MASTER)).unlocked().expect("old key"));
    }
);
