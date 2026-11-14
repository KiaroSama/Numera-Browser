use numera_core::{browser::options, models::Profile};
use serde_json::Value;

#[test]
fn dns_options_match_original_whitespace_and_refusals() -> Result<(), Box<dyn std::error::Error>> {
    // Frozen execution of the immutable source; native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/dns_options_match_original_whitespace_and_refusals.json"
    ))?;
    assert_eq!(rows.len(), 126, "DNS option observations missing");
    for (index, row) in rows.iter().enumerate() {
        let profile = Profile::from_record(&row["input"])?;
        let before = serde_json::to_value(&profile)?;
        for backend in ["playwright", "patchright", "cloak", "camoufox"] {
            let actual = options::build(&profile, backend, "user-data/a", true, &[], &[], None);
            let refuses = backend != "camoufox" && row["result"]["error"].is_string();
            assert_eq!(
                actual.is_err(),
                refuses,
                "DNS launch options differ: {index}/{backend}/admission"
            );
            if refuses {
                let error = actual.expect_err("DNS launch options differ: refusal missing");
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput,
                    "DNS launch options differ: {index}/{backend}/error kind"
                );
                assert_eq!(
                    error.to_string(),
                    row["result"]["error"].as_str().ok_or("Refusal missing")?,
                    "DNS launch options differ: {index}/{backend}/refusal"
                );
                continue;
            }
            let actual = actual?;
            let mut baseline = profile.clone();
            baseline.dns.clear();
            let mut expected =
                options::build(&baseline, backend, "user-data/a", true, &[], &[], None)?;
            if backend != "camoufox" {
                let dns: Vec<Value> = serde_json::from_value(row["result"]["args"].clone())?;
                let arguments = expected["args"]
                    .as_array_mut()
                    .ok_or("Launch args missing")?;
                let position = if backend == "cloak" { 4 } else { 5 };
                arguments.splice(position..position, dns);
            }
            assert_eq!(
                actual, expected,
                "DNS launch options differ: {index}/{backend}"
            );
        }
        assert_eq!(
            serde_json::to_value(&profile)?,
            before,
            "Native DNS options changed input"
        );
    }
    Ok(())
}

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
