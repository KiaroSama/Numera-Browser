use numera_core::{browser::options, models::Profile};
use serde_json::Value;

#[test]
fn effective_sdk_options_match_retained_launch_boundary() -> Result<(), Box<dyn std::error::Error>>
{
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-options.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Browser option cases missing")?;
    assert_eq!(cases.len(), 40);
    for case in cases {
        let profile = Profile::from_record(&case["input"])?;
        let extras: Vec<String> = serde_json::from_value(case["extra_args"].clone())?;
        let actual = options::build(
            &profile,
            case["backend"].as_str().ok_or("Backend missing")?,
            "user-data/a",
            case["headless"].as_bool().ok_or("Headless missing")?,
            &extras,
            &[],
            None,
        )?;
        assert_eq!(actual, case["expected"], "SDK options {}", case["name"]);
    }
    Ok(())
}
