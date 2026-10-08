use numera_core::{
    identity::{headers, metadata, validate},
    models::Fingerprint,
};
use serde_json::{Value, json};

#[test]
fn identity_diagnostics_and_hints_match_source() -> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/identity.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Missing identity contract fixtures")?;
    assert!(
        !cases.is_empty(),
        "Identity contract oracle must not be empty"
    );
    for case in cases {
        let fingerprint = Fingerprint::from_record(&case["input"])?;
        let actual = json!({"metadata": metadata(&fingerprint)?, "headers": headers(&fingerprint)?, "issues": validate(&fingerprint)?});
        assert_eq!(
            actual, case["expected"],
            "Identity contract {}",
            case["name"]
        );
    }
    Ok(())
}
