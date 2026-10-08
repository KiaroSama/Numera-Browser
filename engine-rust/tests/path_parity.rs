use numera_core::storage::paths::safe_id;
use serde_json::Value;

#[test]
fn profile_filenames_follow_the_retained_portable_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/paths.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing filename cases")?;
    assert!(!cases.is_empty(), "An empty oracle cannot establish parity");
    for case in cases {
        let value = case["input"].as_str().ok_or("Missing ID input")?;
        match safe_id(value) {
            Ok(id) => {
                assert_eq!(case["error"], Value::Null, "{value:?}");
                assert_eq!(id, case["expected"].as_str().ok_or("Missing source ID")?);
            }
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"].as_str().ok_or("Unexpected refusal")?
            ),
        }
    }
    Ok(())
}
