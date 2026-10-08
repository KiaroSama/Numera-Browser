use numera_core::{
    conversion::{secret_intent, to_dashboard, to_engine},
    models::Profile,
};
use serde_json::Value;

#[test]
fn profile_shapes_and_secret_intent_match_source() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/conversion.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing conversion source cases")?;
    assert!(!cases.is_empty(), "Conversion oracle must not be empty");
    for case in cases {
        let actual = match case["operation"]
            .as_str()
            .ok_or("Missing conversion operation")?
        {
            "to-engine" => serde_json::to_value(to_engine(&case["input"], 1700000000.0)?)?,
            "to-dashboard" => to_dashboard(&Profile::from_record(&case["input"])?),
            "secret-intent" => Value::String(secret_intent(&case["input"]).into()),
            _ => return Err("Unknown conversion operation".into()),
        };
        assert_eq!(
            actual, case["expected"],
            "Conversion source case {}",
            case["name"]
        );
    }
    Ok(())
}
