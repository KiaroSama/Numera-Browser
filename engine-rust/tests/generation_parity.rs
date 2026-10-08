use numera_core::fingerprint::{GenerateOptions, generate};
use serde_json::Value;

#[test]
fn seeded_identities_match_original_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/generation.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Missing source generation cases")?;
    assert!(!cases.is_empty(), "Identity oracle must not be empty");
    for case in cases {
        let input = &case["input"];
        let actual = generate(GenerateOptions {
            seed: input["seed"].as_i64().ok_or("Missing exact seed")?,
            os: input["os"].as_str(),
            locale: input["locale"].as_str(),
            country: input["country"].as_str(),
            mobile: input["mobile"].as_bool().unwrap_or(false),
        })?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "Seed identity case {}",
            case["name"]
        );
    }
    Ok(())
}
