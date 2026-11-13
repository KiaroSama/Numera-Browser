use numera_core::vault::{decrypt, encrypt, is_envelope};
use serde_json::Value;
mod common;

#[test]
fn envelope_classification_matches_original_unicode_and_prefix_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::vault::policy::{self, VaultState};

    let temporary = common::TempDirectory::new("envelope-classification")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/envelope_classification_matches_original_unicode_and_prefix_contracts.json"
    ))?;
    assert_eq!(cases.len(), 832, "Unicode envelope observations missing");
    for case in cases {
        let value = case["value"].as_str().ok_or("Envelope input missing")?;
        assert_eq!(
            is_envelope(value),
            case["envelope"]
                .as_bool()
                .ok_or("Envelope verdict missing")?,
            "Unicode envelope classification differs: {value:?}"
        );
        assert_eq!(
            policy::is_supported_envelope(value),
            case["supported"]
                .as_bool()
                .ok_or("Supported-prefix verdict missing")?,
            "Supported envelope prefix differs: {value:?}"
        );
        if case["envelope"] == true {
            let preserved = policy::write_secret(VaultState::Locked, Some(value), None, |_| {
                panic!("An existing envelope was encrypted again")
            })?;
            assert_eq!(
                preserved.as_deref(),
                Some(value),
                "Envelope was not preserved"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn crypto_text_admission_matches_original_and_new_tokens_reopen_there()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, vault::check_password};
    use std::{fs, path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("crypto-text-admission")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/crypto_text_admission_matches_original_and_new_tokens_reopen_there.json"
    ))?;
    assert_eq!(cases.len(), 185, "Crypto text observations missing");
    let mut order: Vec<_> = cases.iter().enumerate().collect();
    order.sort_by(|(_, a), (_, b)| {
        (a["password"].as_str(), a["salt"].as_str())
            .cmp(&(b["password"].as_str(), b["salt"].as_str()))
    });
    for (index, case) in order {
        let password = case["password"].as_str().ok_or("Password missing")?;
        let salt = case["salt"].as_str().ok_or("Salt missing")?;
        let value = case["value"].as_str().ok_or("Ciphertext missing")?;
        let (result, error) = match decrypt(password, salt, value) {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        assert_eq!(
            serde_json::json!({"result":result,"error":error,
                "check":check_password(password,salt,value)}),
            serde_json::json!({"result":case["result"],"error":case["error"],"check":case["check"]}),
            "Crypto text admission differs for case {index}"
        );
    }
    let fixture: Value = serde_json::from_str(include_str!("fixtures/vault.json"))?;
    let password = fixture["cases"][0]["password"]
        .as_str()
        .ok_or("Password missing")?;
    let salt = fixture["cases"][0]["salt"].as_str().ok_or("Salt missing")?;
    let rows = ["", "fixture-value", "Unicode α فارسی", "numera-vault-ok"]
        .into_iter()
        .map(|plaintext| {
            let token = encrypt(password, salt, plaintext)?;
            Ok(serde_json::json!({"password":password,"salt":salt,"token":token,"expected":plaintext}))
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    let input = temporary.path().join("native-tokens.json");
    fs::write(&input, serde_json::to_vec(&rows)?)?;
    // Compile an isolated standard Fernet reader; it has no product crate
    // dependency and authenticates the actual fresh ciphertext in another
    // owned native process, preserving the original interoperability check.
    let dependencies = std::env::current_exe()?
        .parent()
        .ok_or("Test dependency directory missing")?
        .to_owned();
    let mut compiler = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    let reader = temporary.path().join(if cfg!(windows) {
        "native-token-reader.exe"
    } else {
        "native-token-reader"
    });
    compiler
        .arg("--edition=2024")
        .arg("--crate-name=native_token_reader")
        .arg("-Cdebuginfo=0")
        .arg("-Copt-level=2")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vault_parity/token_reader.rs"))
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()))
        .arg("-o")
        .arg(&reader);
    for name in ["serde_json", "base64", "pbkdf2", "aes"] {
        let mut libraries = fs::read_dir(&dependencies)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        libraries.retain(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|file| {
                    file.starts_with(&format!("lib{name}-")) && file.ends_with(".rlib")
                })
        });
        libraries.sort_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .ok()
        });
        compiler.arg("--extern").arg(format!(
            "{name}={}",
            libraries
                .last()
                .ok_or("Native reader dependency missing")?
                .display()
        ));
    }
    let compiled = capture::run(
        &mut compiler,
        capture::Limits {
            wall: Duration::from_secs(45),
            idle: Duration::from_secs(30),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        compiled.exit,
        0,
        "Native authenticated reader compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let mut command = Command::new(reader);
    command.arg(&input);
    let reopened = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 8192,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        reopened.exit, 0,
        "Original refused authenticated native tokens"
    );
    assert_eq!(String::from_utf8(reopened.stdout)?.trim(), "4");
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn original_ciphertext_and_errors_are_interoperable() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/vault.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing vault oracle")?;
    assert!(!cases.is_empty(), "Vault oracle must not be empty");
    for case in cases {
        let password = case["password"]
            .as_str()
            .ok_or("Missing fixture password")?;
        let salt = case["salt"].as_str().ok_or("Missing fixture salt")?;
        let token = case["token"].as_str().ok_or("Missing fixture ciphertext")?;
        assert!(is_envelope(token), "Future envelopes must remain envelopes");
        let actual = decrypt(password, salt, token);
        if let Some(error) = case["error"].as_str() {
            assert_eq!(
                actual
                    .err()
                    .ok_or("Expected refused vault opening")?
                    .to_string(),
                error,
                "Vault case {}",
                case["name"]
            );
        } else {
            let expected = case["expected"]
                .as_str()
                .ok_or("Missing source plaintext expectation")?;
            assert_eq!(actual?, expected, "Vault case {}", case["name"]);
        }
    }
    let first = &cases[0];
    let password = first["password"]
        .as_str()
        .ok_or("Missing fixture password")?;
    let salt = first["salt"].as_str().ok_or("Missing fixture salt")?;
    let a = encrypt(password, salt, "fixture-value")?;
    let b = encrypt(password, salt, "fixture-value")?;
    assert_ne!(a, b, "Independent encryption must not reuse IV/token");
    assert_eq!(decrypt(password, salt, &a)?, "fixture-value");
    Ok(())
}
