use numera_core::{identity_script, models::Fingerprint};
use serde_json::Value;

#[test]
fn native_script_and_config_match_original_generator() -> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Init script cases missing")?;
    assert_eq!(cases.len(), 10);
    for case in cases {
        let fingerprint = Fingerprint::from_record(&case["input"])?;
        assert_eq!(
            identity_script::config(&fingerprint)?,
            case["config"],
            "Config {}",
            case["name"]
        );
        assert_eq!(
            identity_script::build(&fingerprint)?,
            case["script"],
            "Script {}",
            case["name"]
        );
    }
    Ok(())
}
