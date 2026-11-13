use numera_core::storage::pool::ProxyPool;
use serde_json::Value;
use std::{fs, path::PathBuf};

mod common;

#[test]
fn malformed_pool_text_refuses_without_replacing_original_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::repo::RepoError;
    use serde_json::{Map, json};
    use std::io;

    let temporary = common::TempDirectory::new("pool-text-storage")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/malformed_pool_text_refuses_without_replacing_original_bytes.json"
    ))?;
    assert_eq!(cases.len(), 100, "Pool storage text cases missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir(&root)?;
        let path = root.join("proxies.json");
        let before = case["raw"]
            .as_str()
            .map(|raw| {
                (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        if let Some(bytes) = &before {
            fs::write(&path, bytes)?;
        }
        let pool = ProxyPool::new(root, None);
        let operation = case["operation"].as_str().ok_or("Pool operation missing")?;
        let result: Result<Value, RepoError> = match operation {
            "list" => pool.list().map(Value::Array),
            "get" => pool
                .get("fixture")
                .map(|value| value.unwrap_or(Value::Null)),
            "withheld" => pool.secret_withheld("fixture").map(Value::Bool),
            "update" | "empty-update" => pool
                .update(
                    "fixture",
                    if operation == "update" {
                        Map::from_iter([("label".into(), "α".into())])
                    } else {
                        Map::new()
                    },
                )
                .map(|value| value.unwrap_or(Value::Null)),
            "update-many" => pool
                .update_many(Map::from_iter([("fixture".into(), json!({"label":"α"}))]))
                .map(Value::Array),
            "empty-many" => pool.update_many(Map::new()).map(Value::Array),
            "delete" => pool.delete("fixture").map(Value::Bool),
            "missing-delete" => pool.delete("missing").map(Value::Bool),
            "add-empty" => pool.add_many(Vec::new()).map(Value::Array),
            _ => return Err("Unknown pool text operation".into()),
        };
        let outcome = match result {
            Ok(value) => json!({"result":value,"error":null}),
            Err(RepoError::Io(error)) if error.kind() == io::ErrorKind::InvalidData => {
                json!({"result":null,"error":"UnicodeDecodeError"})
            }
            Err(error) => return Err(error.into()),
        };
        assert_eq!(
            outcome["error"], case["outcome"]["error"],
            "Pool storage text admission differs"
        );
        assert_eq!(
            outcome, case["outcome"],
            "Pool storage operation result differs"
        );
        let after = fs::read(&path).ok();
        let disk = after
            .as_ref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        assert_eq!(disk, case["disk"], "Pool storage disk differs");
        assert_eq!(
            before == after,
            case["unchanged"]
                .as_bool()
                .ok_or("Pool byte verdict missing")?,
            "Pool storage preserved-byte verdict differs"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

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
