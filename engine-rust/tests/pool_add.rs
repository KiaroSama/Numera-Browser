use numera_core::{pool::normalize, storage::pool::ProxyPool};
use serde_json::json;
use std::{fs, path::PathBuf};

#[test]
fn pool_import_distinguishes_protocol_and_credentials_without_losing_duplicates()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("pool-add-{}", std::process::id()));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let pool = ProxyPool::new(root.clone(), None);
        let http = normalize(
            &json!({"host":"host.test","port":8080,"user":"fixture-user","pass":"fixture-a"}),
        )?;
        let socks = normalize(
            &json!({"type":"SOCKS5","host":"host.test","port":8080,"user":"fixture-user","pass":"fixture-a"}),
        )?;
        let other = normalize(
            &json!({"host":"host.test","port":8080,"user":"fixture-user","pass":"fixture-b"}),
        )?;
        let added = pool.add_many(vec![http.clone(), http.clone(), socks, other])?;
        assert_eq!(added.len(), 3);
        assert!(pool.add_many(vec![http])?.is_empty());
        let listed = pool.list()?;
        assert_eq!(listed.len(), 3);
        let mut ids = std::collections::HashSet::new();
        for row in &listed {
            let id = row["id"].as_str().ok_or("Pool ID missing")?;
            assert_eq!(id.len(), 12);
            assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert!(ids.insert(id));
            assert_eq!(row["status"], "untested");
        }
        assert_eq!(listed, added);
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
