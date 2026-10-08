use numera_core::http::auth_policy::{AuthPolicy, read};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn only_explicit_false_opens_a_persisted_auth_policy() -> Result<(), Box<dyn std::error::Error>> {
    let parent = std::env::var_os("NUMERA_TEST_TEMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp"));
    let root = parent.join(format!(
        "auth-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(read(&root), AuthPolicy::Absent);
        let path = root.join("auth-policy.json");
        for value in [
            json!({}),
            json!([]),
            json!(null),
            json!({"protected":0}),
            json!({"protected":null}),
            json!({"protected":""}),
            json!({"protected":[]}),
            json!({"protected":{}}),
        ] {
            fs::write(&path, serde_json::to_vec(&value)?)?;
            assert_eq!(
                read(&root),
                AuthPolicy::Ambiguous,
                "Damaged policy must never be an open decision"
            );
        }
        fs::write(&path, b"{broken")?;
        assert_eq!(read(&root), AuthPolicy::Ambiguous);
        fs::write(&path, b"{\"protected\":true}")?;
        assert_eq!(read(&root), AuthPolicy::Protected);
        fs::write(&path, b"{\"protected\":false}")?;
        assert_eq!(read(&root), AuthPolicy::Open);
        Ok(())
    })();
    fs::remove_dir_all(&root)?;
    result
}
