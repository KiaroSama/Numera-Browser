use numera_core::{storage::pool::ProxyPool, vault::decrypt};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn locked_pool_edits_preserve_ciphertext_and_refuse_secret_loss()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/pool-vault.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!("pool-vault-{}-{index}", std::process::id()));
        fs::create_dir_all(&root)?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            let path = root.join("proxies.json");
            let before = serde_json::to_vec(&case["initial"])?;
            fs::write(&path, &before)?;
            let pool = ProxyPool::new(root.clone(), case["password"].as_str().map(str::to_owned));
            match pool.update(
                "fixture-a",
                case["changes"]
                    .as_object()
                    .ok_or("Changes missing")?
                    .clone(),
            ) {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(value)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            assert_eq!(serde_json::to_value(pool.list()?)?, case["listed"]);
            let after = fs::read(path)?;
            assert_eq!(
                before == after,
                case["unchanged"].as_bool().ok_or("Byte verdict missing")?
            );
            let mut observed: Value = serde_json::from_slice(&after)?;
            for row in observed.as_array_mut().ok_or("Pool missing")? {
                if let Some(token) = row["pass"]
                    .as_str()
                    .filter(|value| value.starts_with("enc:v2:"))
                {
                    row["pass"] =
                        decrypt("fixture-pool-master", "AAECAwQFBgcICQoLDA0ODw==", token)?.into();
                }
            }
            assert_eq!(observed, case["observed"]);
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}
