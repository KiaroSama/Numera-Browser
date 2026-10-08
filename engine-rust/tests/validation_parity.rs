use numera_core::validation::profile_problems;
use serde_json::Value;

#[test]
fn uncoerced_write_validation_matches_source() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/validation.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing write validation cases")?;
    assert!(
        !cases.is_empty(),
        "Write validation oracle must not be empty"
    );
    for case in cases {
        let actual = profile_problems(
            &case["input"],
            case["kind"]
                .as_str()
                .ok_or("Missing declared profile shape")?,
        )?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "Write validation {}",
            case["name"]
        );
    }
    Ok(())
}
