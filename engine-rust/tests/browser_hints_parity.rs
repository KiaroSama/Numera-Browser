use numera_core::{browser::hints, models::Fingerprint};
use serde_json::Value;

#[test]
fn cdp_hint_commands_and_new_page_policy_match_original() -> Result<(), Box<dyn std::error::Error>>
{
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-hints.json"))?;
    let cases = document["cases"].as_array().ok_or("Hint cases missing")?;
    assert_eq!(cases.len(), 10);
    for case in cases {
        let fp = Fingerprint::from_record(&case["input"])?;
        let policy = hints::policy(&fp)?;
        assert_eq!(
            policy["commands"], case["initial"],
            "Initial hints {}",
            case["name"]
        );
        assert_eq!(
            policy["commands"], case["later"],
            "New page hints {}",
            case["name"]
        );
        assert_eq!(
            policy["strip_headers_prefixes"],
            if fp.os == "ios" {
                serde_json::json!(["sec-ch-ua"])
            } else {
                serde_json::json!([])
            }
        );
    }
    Ok(())
}
