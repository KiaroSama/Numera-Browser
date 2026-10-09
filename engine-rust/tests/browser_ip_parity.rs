use numera_core::{browser::ip_identity, models::Profile};
use serde_json::Value;

#[test]
fn only_marked_identity_signals_follow_observed_exit_country()
-> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-ip.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("IP identity cases missing")?;
    assert_eq!(cases.len(), 33);
    for case in cases {
        let profile = Profile::from_record(&case["input"])?;
        assert_eq!(
            u64::from(ip_identity::needs_lookup(&profile)),
            case["lookups"].as_u64().ok_or("Lookup count missing")?
        );
        let actual = ip_identity::resolve(&profile, case["country"].as_str())?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "IP identity {}",
            case["name"]
        );
    }
    Ok(())
}
