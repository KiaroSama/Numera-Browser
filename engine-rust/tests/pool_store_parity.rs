use numera_core::storage::pool::ProxyPool;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn pool_edits_reread_and_keep_unrelated_fields_and_secret_intent()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/pool-store.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Pool cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("pool-store-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join("proxies.json");
            let before = serde_json::to_vec(&case["initial"])?;
            fs::write(&path, &before)?;
            let pool = ProxyPool::new(root.clone(), None);
            let actual = match case["operation"].as_str() {
                Some("update") => serde_json::to_value(
                    pool.update(
                        case["id"].as_str().ok_or("ID missing")?,
                        case["changes"]
                            .as_object()
                            .ok_or("Changes missing")?
                            .clone(),
                    )?,
                )?,
                Some("update-many") => serde_json::to_value(
                    pool.update_many(
                        case["changes"]
                            .as_object()
                            .ok_or("Changes missing")?
                            .clone(),
                    )?,
                )?,
                Some("delete") => {
                    Value::Bool(pool.delete(case["id"].as_str().ok_or("ID missing")?)?)
                }
                _ => return Err("Unknown pool operation".into()),
            };
            assert_eq!(actual, case["result"]);
            assert_eq!(serde_json::to_value(pool.list()?)?, case["listed"]);
            let after = fs::read(path)?;
            assert_eq!(
                before == after,
                case["unchanged"].as_bool().ok_or("Byte verdict missing")?
            );
            assert_eq!(serde_json::from_slice::<Value>(&after)?, case["written"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
