use super::*;
source_test!(round_trip, "tests/test_vault.py::test_round_trip", {
    iv_randomness_failure_is_fallible();
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
        salt_randomness_failure_preserves_cause_and_store();
        iv_randomness_failure_preserves_store();
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
fn iv_randomness_failure_is_fallible() {
    use std::error::Error;

    let reached = Arc::new(Mutex::new(0));
    let observed = reached.clone();
    let result = testing::with_observer(
        move |event| {
            if event.operation == "before-vault-iv-randomness" {
                *observed.lock().expect("IV observations") += 1;
                Err(io::Error::other("controlled IV entropy failure"))
            } else {
                Ok(Action::Continue)
            }
        },
        || {
            std::panic::catch_unwind(|| {
                vault::encrypt("pw", "cm91bmQtdHJpcC1zYWx0IQ==", "never published")
            })
        },
    )
    .expect("Unavailable IV randomness must return an error instead of panicking")
    .expect_err("Encryption must consume the fallible IV randomness boundary");
    assert_eq!(*reached.lock().expect("IV observations"), 1);
    let cause = result
        .source()
        .and_then(|cause| cause.downcast_ref::<io::Error>())
        .expect("Encryption must retain the underlying randomness failure");
    assert_eq!(cause.to_string(), "controlled IV entropy failure");
    assert!(
        result
            .to_string()
            .contains("Vault IV randomness unavailable")
    );
}

fn iv_randomness_failure_preserves_store() {
    use std::error::Error;

    let temp = common::TempDirectory::new("vault-iv-randomness").expect("temporary root");
    let root = temp.path();
    saved(root, "iv-refusal", 20, Some("uncommitted-secret"));
    write(root, "config.json", json!({"ordinary-setting": "preserve"}));
    let before = snapshot(root);
    let mut repository = repo(root, None);
    let error = testing::with_observer(
        |event| {
            if event.operation == "before-vault-iv-randomness" {
                Err(io::Error::other("controlled verifier IV failure"))
            } else {
                Ok(Action::Continue)
            }
        },
        || repository.enable_vault(MASTER),
    )
    .expect_err("A failed verifier must refuse before migration");
    let cause = error
        .source()
        .and_then(|cause| cause.source())
        .and_then(|cause| cause.source())
        .and_then(|cause| cause.downcast_ref::<io::Error>())
        .expect("Verifier refusal must retain the exact underlying IV error");
    assert_eq!(cause.to_string(), "controlled verifier IV failure");
    assert_eq!(snapshot(root), before);
    assert!(!root.join("vault-migration.json").exists());
    assert!(!repository.vault_enabled().expect("descriptor unchanged"));
    assert!(repository.unlocked().expect("password state unchanged"));
}

fn salt_randomness_failure_preserves_cause_and_store() {
    use std::error::Error;

    let temp = common::TempDirectory::new("vault-salt-randomness").expect("temporary root");
    let root = temp.path();
    saved(root, "entropy-refusal", 19, Some("uncommitted-secret"));
    write(root, "config.json", json!({"ordinary-setting": "preserve"}));
    let before = snapshot(root);
    let observed = Arc::new(Mutex::new((0, 0)));
    let reached = observed.clone();
    let mut repository = repo(root, None);
    let error = testing::with_observer(
        move |event| {
            let mut observed = reached.lock().expect("entropy observations");
            match event.operation {
                "before-vault-salt-randomness" => {
                    observed.0 += 1;
                    Err(io::Error::other("controlled salt entropy failure"))
                }
                "before-key-derivation" => {
                    observed.1 += 1;
                    Ok(Action::Continue)
                }
                _ => Ok(Action::Continue),
            }
        },
        || repository.enable_vault(MASTER),
    )
    .expect_err("Unavailable salt randomness must refuse before migration");
    assert_eq!(*observed.lock().expect("entropy observations"), (1, 0));
    let recovery = error
        .source()
        .and_then(|cause| cause.downcast_ref::<recovery::RecoveryError>())
        .expect("Repository errors must retain the structured recovery cause");
    assert_eq!(
        recovery.kind,
        recovery::RecoveryErrorKind::VaultRecoveryRequired
    );
    let entropy = recovery
        .source()
        .and_then(|cause| cause.downcast_ref::<io::Error>())
        .expect("Salt refusal must retain the original entropy error");
    assert_eq!(entropy.kind(), io::ErrorKind::Other);
    assert_eq!(entropy.to_string(), "controlled salt entropy failure");
    assert!(
        error
            .to_string()
            .contains("Vault salt randomness unavailable")
    );
    assert_eq!(snapshot(root), before);
    assert!(!root.join("vault-migration.json").exists());
    assert!(!repository.vault_enabled().expect("descriptor unchanged"));
    assert!(repository.unlocked().expect("password state unchanged"));
    assert_eq!(repository.enable_vault(MASTER).expect("retry enable"), 2);
    assert!(repository.unlocked().expect("retry unlock"));
    let descriptor = repository.vault().expect("new descriptor").expect("vault");
    use base64::{Engine, engine::general_purpose::STANDARD};
    assert_eq!(
        STANDARD
            .decode(descriptor["salt"].as_str().expect("salt"))
            .expect("salt base64")
            .len(),
        16
    );
}
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
        atomic_replacement_retry_contract(root);
    }
);

// The original interrupted-write case also owns the replacement boundary:
// transient sharing failures retry, terminal errors preserve the previous file.
fn atomic_replacement_retry_contract(root: &Path) {
    use numera_core::storage::atomic::write_bytes;
    use std::sync::{Arc, Mutex};
    for (refusals, kind) in [
        (0, io::ErrorKind::PermissionDenied),
        (1, io::ErrorKind::PermissionDenied),
        (4, io::ErrorKind::PermissionDenied),
        (5, io::ErrorKind::PermissionDenied),
        (5, io::ErrorKind::InvalidInput),
    ] {
        let directory = root.join(format!("retry-{refusals}-{kind:?}"));
        fs::create_dir(&directory).expect("owned retry fixture");
        let destination = directory.join("document.json");
        fs::write(&destination, b"original\0\xff").expect("original retry bytes");
        let observations = Arc::new(Mutex::new((0_usize, Vec::new())));
        let observed = observations.clone();
        let checked = destination.clone();
        let result = testing::with_observer(
            move |event| {
                if event.path != checked {
                    return Ok(testing::Action::Continue);
                }
                let mut state = observed.lock().expect("retry observations");
                match event.operation {
                    "before-atomic-replace-attempt" => {
                        state.0 += 1;
                        if state.0 <= refusals {
                            return Err(io::Error::new(kind, "controlled replacement refusal"));
                        }
                    }
                    "before-atomic-retry-delay" => {
                        state.1.push(event.delay.expect("actual retry delay"));
                        return Ok(testing::Action::Skip);
                    }
                    _ => {}
                }
                Ok(testing::Action::Continue)
            },
            || write_bytes(&destination, b"replacement\xff\0"),
        );
        let expected_attempts = if kind == io::ErrorKind::PermissionDenied {
            (refusals + 1).min(5)
        } else {
            1
        };
        let expected_delays = if kind == io::ErrorKind::PermissionDenied {
            (1..=refusals.min(4))
                .map(|attempt| Duration::from_millis(50 * attempt as u64))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let actual = observations.lock().expect("recorded retries");
        assert_eq!(
            actual.0, expected_attempts,
            "replacement attempt budget changed"
        );
        assert_eq!(
            actual.1, expected_delays,
            "source replacement backoff changed"
        );
        let terminal = refusals == 5;
        assert_eq!(result.is_err(), terminal);
        if let Err(error) = result {
            assert_eq!(error.kind(), kind);
            assert_eq!(error.to_string(), "controlled replacement refusal");
        }
        assert_eq!(
            fs::read(&destination).expect("retry destination"),
            if terminal {
                &b"original\0\xff"[..]
            } else {
                &b"replacement\xff\0"[..]
            }
        );
        let entries = fs::read_dir(&directory)
            .expect("retry cleanup")
            .map(|entry| entry.expect("retry entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(
            entries,
            vec![destination.file_name().expect("destination name")],
            "owned temporary survived replacement"
        );
    }
}
