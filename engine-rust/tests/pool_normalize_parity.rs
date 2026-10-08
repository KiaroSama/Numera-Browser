use numera_core::pool::normalize;
use serde_json::Value;

#[test]
fn structured_proxy_fields_are_normalized_or_refused_like_the_source()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/pool-normalize.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Normalize cases missing")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        match normalize(&case["input"]) {
            Ok(value) => {
                assert!(case["error"].is_null(), "case {index}");
                assert_eq!(value, case["result"], "case {index}");
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "case {index}"
            ),
        }
    }
    Ok(())
}
