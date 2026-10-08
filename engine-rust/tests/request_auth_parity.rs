use numera_core::http::{request_auth::RequestAuth, tokens::TokenStore};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn request_auth_preserves_public_paths_and_header_precedence()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/request-auth.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Auth cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("request-auth-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("tokens.json"),
                match case["table"].as_str() {
                    Some(text) => text.as_bytes().to_vec(),
                    None => serde_json::to_vec(&case["table"])?,
                },
            )?;
            if !case["policy"].is_null() {
                fs::write(
                    root.join("auth-policy.json"),
                    serde_json::to_vec(&case["policy"])?,
                )?;
            }
            let auth = RequestAuth::new(
                TokenStore::new(root.clone()),
                case["environment"].as_str().ok_or("Environment missing")?,
            );
            auth.adopt(1700000000.0)?;
            let verdict = auth.authorize(
                case["method"].as_str().ok_or("Method missing")?,
                case["path"].as_str().ok_or("Path missing")?,
                case["headers"]["authorization"].as_str(),
                case["headers"]["x-numera-token"].as_str(),
                1700000000.0,
            )?;
            assert_eq!(Value::Bool(!verdict), case["denied"], "{}", case["name"]);
            assert_eq!(
                Value::Bool(auth.required()),
                case["required"],
                "{}",
                case["name"]
            );
            if !verdict {
                assert_eq!(RequestAuth::denial_body(), case["denial_body"]);
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
