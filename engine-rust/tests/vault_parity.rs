use numera_core::vault::{decrypt, encrypt, is_envelope};
use serde_json::Value;
mod common;

#[test]
fn envelope_classification_matches_original_unicode_and_prefix_contracts()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        ownership::capture,
        vault::policy::{self, VaultState},
    };
    use std::{path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("envelope-classification")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.crypto import is_envelope,is_supported_envelope
from run_logging import run_log
with run_log('envelope-classification-oracle',root/'logs') as log:
    catalog=json.loads((repository/'engine-rust/src/pool_digit_ranges.json').read_text(encoding='utf-8'))
    ranges=catalog['decimal_ranges']
    values=['enc:v١:opaque','enc:v1:','enc:v2:invalid','enc:v9:',
            'enc:v01:','enc:v0:','enc:v12:\n','enc:v:', 'enc:v1',
            'enc:v1x:', 'enc:v-1:', 'enc:v+1:', 'enc:v1.0:',
            'ENC:v1:', ' enc:v1:', '\nenc:v1:', 'enc:v1\n:',
            'enc:v²:', 'enc:v½:', 'enc:vⅠ:',
            'enc:v1٢:', 'enc:v1:\u0000', 'plain','']
    scalars=set()
    for first,last in ranges:
        scalars.update(range(first,last+1))
        scalars.update(c for c in (first-1,last+1) if 0<=c<=0x10ffff)
    values+=['enc:v'+chr(c)+':opaque' for c in sorted(scalars)]
    rows=[{'value':value,'envelope':is_envelope(value),
           'supported':is_supported_envelope(value)} for value in values]
    log.info('Observed envelope classification cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path()).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original envelope observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
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
