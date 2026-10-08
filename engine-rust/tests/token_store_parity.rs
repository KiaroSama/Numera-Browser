use numera_core::http::{token_errors::TokenError, tokens::TokenStore};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn token_loss_never_reopens_protection_and_verification_preserves_revocation()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/token-store.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Token cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("token-store-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join("tokens.json");
            if !case["table"].is_null() {
                fs::write(
                    &path,
                    match case["table"].as_str() {
                        Some(text) => text.as_bytes().to_vec(),
                        None => serde_json::to_vec(&case["table"])?,
                    },
                )?;
            }
            if !case["policy"].is_null() {
                fs::write(
                    root.join("auth-policy.json"),
                    serde_json::to_vec(&case["policy"])?,
                )?;
            }
            let before = fs::read(&path).ok();
            let store = TokenStore::new(root.clone());
            let operation = case["operation"].as_str().ok_or("Operation missing")?;
            let outcome = match operation {
                "adopt" => store
                    .adopt(1700000000.0)
                    .map(Value::Bool)
                    .map_err(TokenError::from),
                "verify" => store
                    .verify(
                        case["argument"].as_str().ok_or("Argument missing")?,
                        1700000000.0,
                    )
                    .map(|value| value.unwrap_or(Value::Null))
                    .map_err(TokenError::from),
                "revoke" | "revoke-missing" => store
                    .revoke(
                        if operation == "revoke" {
                            "fixture-token"
                        } else {
                            "missing"
                        },
                        case["argument"].as_bool().ok_or("Lockout flag missing")?,
                    )
                    .map(Value::Bool),
                "set-open" => store
                    .set_protected(false, 1700000000.0)
                    .map(|()| Value::Null)
                    .map_err(TokenError::from),
                _ => Ok(Value::Null),
            };
            match outcome {
                Ok(value) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(value, case["result"], "{}", case["name"]);
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
            assert_eq!(store.state().as_str(), case["state"], "{}", case["name"]);
            assert_eq!(
                Value::Bool(store.protected()),
                case["protected"],
                "{}",
                case["name"]
            );
            assert_eq!(
                Value::Array(store.list()),
                case["listed"],
                "{}",
                case["name"]
            );
            assert_eq!(
                Value::Bool(before == fs::read(&path).ok()),
                case["table_unchanged"]
            );
            let policy = fs::read(root.join("auth-policy.json"))
                .ok()
                .map(|bytes| serde_json::from_slice::<Value>(&bytes))
                .transpose()?
                .unwrap_or(Value::Null);
            assert_eq!(policy, case["persisted_policy"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
