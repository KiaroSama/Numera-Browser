use numera_core::{
    models::{Fingerprint, Profile},
    proxy::Proxy,
};
use serde_json::{Value, json};

#[test]
fn models_and_proxy_intent_match_source_oracle() -> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/models.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Fixture cases must be an array")?;
    assert!(!cases.is_empty(), "Source oracle must not be empty");
    for case in cases {
        let actual = match case["operation"]
            .as_str()
            .ok_or("Missing fixture operation")?
        {
            "fingerprint" => serde_json::to_value(Fingerprint::from_record(&case["input"])?)?,
            "profile" => serde_json::to_value(Profile::from_record(&case["input"])?)?,
            "proxy" => {
                let mut proxy = Proxy::from_record(
                    &case["input"],
                    case["stored"].as_bool().ok_or("Missing storage origin")?,
                )?;
                if case["assignment"] != "untouched" {
                    proxy.set_password(case["assignment"].as_str().map(str::to_owned));
                }
                json!({"record": proxy.to_record(), "intent": proxy.password_intent(), "playwright": proxy.to_playwright()})
            }
            other => return Err(format!("Unknown oracle operation: {other}").into()),
        };
        assert_eq!(
            actual, case["expected"],
            "Source parity case {}",
            case["name"]
        );
    }
    Ok(())
}
