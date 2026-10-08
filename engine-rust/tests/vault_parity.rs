use numera_core::vault::{decrypt, encrypt, is_envelope};
use serde_json::Value;

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
