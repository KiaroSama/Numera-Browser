use numera_core::http::tokens::TokenStore;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn named_tokens_return_once_store_only_digest_and_refuse_duplicate_names()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/token-create.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Token create cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("token-create-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join("tokens.json");
            fs::write(&path, serde_json::to_vec(&case["initial"])?)?;
            let before = fs::read(&path)?;
            let store = TokenStore::new(root.clone());
            match store.create(case["input"].as_str().ok_or("Name missing")?, 1700000000.0) {
                Ok(created) => {
                    assert!(case["error"].is_null());
                    let token = created["token"].as_str().ok_or("Token missing")?;
                    assert!(token.starts_with("nmr_") && token.len() == 47);
                    let id = created["id"].as_str().ok_or("ID missing")?;
                    assert!(id.len() == 12 && id.bytes().all(|byte| byte.is_ascii_hexdigit()));
                    let listed = store.list();
                    assert_eq!(listed.len(), 1);
                    assert!(listed[0].get("token").is_none());
                    assert!(listed[0]["hash"].as_str() == Some(TokenStore::hash(token).as_str()));
                    // Only OS randomness differs; compare every other source field.
                    let mut normalized = created.clone();
                    for key in ["token", "id", "hash"] {
                        normalized[key] = case["result"][key].clone();
                    }
                    assert_eq!(normalized, case["result"]);
                    let mut stored = listed[0].clone();
                    for key in ["id", "hash"] {
                        stored[key] = case["listed"][0][key].clone();
                    }
                    assert_eq!(stored, case["listed"][0]);
                    let synthetic = case["result"]["token"]
                        .as_str()
                        .ok_or("Source synthetic token missing")?;
                    assert_eq!(TokenStore::hash(synthetic), case["result"]["hash"]);
                    assert!(store.verify(token, 1700000000.0)?.is_some());
                    assert!(!fs::read_to_string(&path)?.contains(token));
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            assert_eq!(
                Value::Bool(before == fs::read(&path)?),
                case["table_unchanged"]
            );
            assert_eq!(Value::Bool(store.protected()), case["protected"]);
            let policy = fs::read(root.join("auth-policy.json"))
                .ok()
                .map(|bytes| serde_json::from_slice::<Value>(&bytes))
                .transpose()?
                .unwrap_or(Value::Null);
            assert_eq!(policy, case["policy"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
