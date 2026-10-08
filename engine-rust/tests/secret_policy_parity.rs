use numera_core::vault::policy::{VaultState, write_secret};
use serde_json::Value;

#[test]
fn withheld_clear_replace_and_future_envelopes_match_source()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/secrets.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing secret policy cases")?;
    assert!(!cases.is_empty());
    for case in cases {
        let state = match case["mode"].as_str() {
            Some("absent" | "disabled") => VaultState::Disabled,
            Some("locked") => VaultState::Locked,
            Some("unlocked") => VaultState::Unlocked,
            _ => return Err("Unknown source vault state".into()),
        };
        let result = write_secret(
            state,
            case["value"].as_str(),
            case["stored"].as_str(),
            |_| Ok("fixture-randomized-envelope".into()),
        );
        match result {
            Ok(value) => {
                assert!(case["error"].is_null());
                assert_eq!(serde_json::to_value(value)?, case["result"]);
            }
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"].as_str().ok_or("Unexpected vault refusal")?
            ),
        }
    }
    Ok(())
}
