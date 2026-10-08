use numera_core::http::{bind_policy::refusal, tokens::TokenStore};
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn bind_policy_refuses_nonloopback_without_an_explicit_credential_or_override()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/bind-policy.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Bind cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let temporary = common::TempDirectory::new("bind-policy")?;
        let root = temporary.path().to_path_buf();
        if !case["tokens"].is_null() {
            let bytes = match case["tokens"].as_str() {
                Some(text) => text.as_bytes().to_vec(),
                None => serde_json::to_vec(&case["tokens"])?,
            };
            fs::write(root.join("tokens.json"), bytes)?;
        }
        assert_eq!(
            serde_json::to_value(refusal(
                case["host"].as_str().ok_or("Host missing")?,
                &TokenStore::new(root),
                case["environment"].as_str().ok_or("Environment missing")?,
                case["override"].as_str().ok_or("Override missing")?,
            ))?,
            case["refusal"],
            "{}",
            case["name"]
        );
        temporary.cleanup()?;
    }
    Ok(())
}
