use super::*;
source_test!(
    source_checkout_does_not_claim_an_installation,
    "tests/test_installation_lease.py::test_source_checkout_does_not_claim_an_installation",
    {
        let temp = temp();
        let root = temp.path();
        let exe = root.join("checkout/target/debug/numera-engine");
        assert!(numera_core::installation_gate::installation_root(&exe).is_none());
        let _lease = numera_core::installation_gate::RuntimeLease::for_executable(&exe, None)
            .expect("development runtime");
        assert!(!root.join(".updates").exists());
    }
);
source_test!(
    entry_checks_activation_before_importing_cli,
    "tests/test_installation_lease.py::test_entry_checks_activation_before_importing_cli",
    {
        let temp = temp();
        let root = temp.path();
        let installation = root.join("portable");
        let engine = installation.join("engine");
        fs::create_dir_all(&engine).expect("engine directory");
        fs::create_dir_all(installation.join(".updates")).expect("update state");
        write(
            &installation,
            ".updates/activation.json",
            json!({"schema":1,"nonce_sha256":"mismatch"}),
        );
        let executable = engine.join(if cfg!(windows) {
            "numera-engine.exe"
        } else {
            "numera-engine"
        });
        fs::copy(env!("CARGO_BIN_EXE_numera-engine"), &executable).expect("owned installed engine");
        let mut command = Command::new(&executable);
        command
            .arg("--argument-that-the-cli-would-refuse")
            .env_remove("NUMERA_UPDATE_ACTIVATION");
        let output = capture::run(
            &mut command,
            capture::Limits {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(5),
                bytes: 65536,
                cleanup: Duration::from_secs(2),
            },
        )
        .expect("owned entry process");
        assert_eq!(output.exit, 1);
        let stderr = String::from_utf8(output.stderr).expect("stderr");
        assert!(
            stderr
                .to_lowercase()
                .contains("recovery owns this installation"),
            "{stderr}"
        );
        assert!(
            !stderr.contains("unexpected argument"),
            "activation preceded CLI parsing"
        );
    }
);
source_test!(
    shared_lease_refuses_exclusive_writer_and_activation_requires_nonce,
    "tests/test_installation_lease.py::test_shared_lease_refuses_exclusive_writer_and_activation_requires_nonce",
    {
        let temp = temp();
        let root = temp.path();
        use sha2::{Digest, Sha256};
        let installation = root.join("portable with spaces");
        fs::create_dir_all(installation.join("engine")).expect("installation");
        {
            let _lease =
                numera_core::installation_gate::RuntimeLease::acquire(Some(&installation), None)
                    .expect("shared runtime lease");
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(installation.join(".updates/runtime.lock"))
                .expect("lock file");
            assert!(matches!(file.try_lock(), Err(fs::TryLockError::WouldBlock)));
        }
        let nonce = "fixture-activation-nonce";
        let digest = Sha256::digest(nonce.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        write(
            &installation,
            ".updates/activation.json",
            json!({"schema":1,"nonce_sha256":digest}),
        );
        assert!(
            numera_core::installation_gate::RuntimeLease::acquire(Some(&installation), None)
                .is_err()
        );
        {
            let _lease = numera_core::installation_gate::RuntimeLease::acquire(
                Some(&installation),
                Some(nonce),
            )
            .expect("matching activation");
        }
        fs::write(installation.join(".updates/activation.json"), b"{").expect("corrupt activation");
        assert!(
            numera_core::installation_gate::RuntimeLease::acquire(Some(&installation), Some(nonce))
                .is_err()
        );
    }
);
