use numera_core::{
    conversion::{secret_intent, to_dashboard, to_engine},
    models::Profile,
};
use serde_json::Value;
mod common;

#[test]
fn conversion_text_boundaries_match_original_profiles_and_roundtrips()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("conversion-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/conversion_text_boundaries_match_original_profiles_and_roundtrips.json"
    ))?;
    assert_eq!(cases.len(), 69, "Conversion text observations missing");
    for case in &cases {
        let profile = to_engine(&case["input"], 1700000000.0)?;
        assert_eq!(
            serde_json::to_value(&profile)?,
            case["profile"],
            "Conversion text boundaries differ: {}",
            case["label"]
        );
        assert_eq!(
            to_dashboard(&profile),
            case["dashboard"],
            "Conversion text roundtrip differs: {}",
            case["label"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn container_text_conversion_matches_original_profiles_and_roundtrips()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("conversion-container-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/container_text_conversion_matches_original_profiles_and_roundtrips.json"
    ))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Container cases missing")?;
    assert_eq!(cases.len(), 54, "Container text observations missing");
    for case in cases {
        let mut record = oracle["base"].clone();
        record
            .as_object_mut()
            .ok_or("Profile input missing")?
            .extend(
                case["changes"]
                    .as_object()
                    .ok_or("Container changes missing")?
                    .clone(),
            );
        let before = record.clone();
        let profile = to_engine(&record, 1700000000.0)?;
        assert_eq!(
            serde_json::to_value(&profile)?,
            case["profile"],
            "Container text conversion differs: {}",
            case["label"]
        );
        assert_eq!(
            to_dashboard(&profile),
            case["dashboard"],
            "Container text roundtrip differs: {}",
            case["label"]
        );
        assert_eq!(record, before, "Container conversion mutated its input");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn reverse_proxy_conversion_matches_original_without_canonicalizing_records()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("conversion-proxy")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/reverse_proxy_conversion_matches_original_without_canonicalizing_records.json"
    ))?;
    let cases = oracle["cases"].as_array().ok_or("Proxy cases missing")?;
    assert_eq!(cases.len(), 112, "Reverse proxy observations missing");
    for case in cases {
        let mut record = oracle["base"].clone();
        record["proxy"] = case["proxy"].clone();
        let profile = Profile::from_record(&record)?;
        let before = serde_json::to_value(&profile)?;
        assert_eq!(
            to_dashboard(&profile),
            case["dashboard"],
            "Reverse proxy conversion differs: {}",
            case["label"]
        );
        assert_eq!(
            serde_json::to_value(&profile)?,
            before,
            "Reverse conversion mutated its profile"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn integer_conversion_matches_original_grammar_defaults_and_seed_magnitude()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("conversion-integer")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/integer_conversion_matches_original_grammar_defaults_and_seed_magnitude.json"
    ))?;
    let cases = oracle["cases"].as_array().ok_or("Integer cases missing")?;
    assert_eq!(cases.len(), 80, "Integer conversion observations missing");
    for case in cases {
        let mut record = oracle["base"].clone();
        record
            .as_object_mut()
            .ok_or("Profile input missing")?
            .extend(
                case["changes"]
                    .as_object()
                    .ok_or("Numeric changes missing")?
                    .clone(),
            );
        let before = record.clone();
        let profile = to_engine(&record, 1700000000.0)?;
        assert_eq!(
            serde_json::to_value(&profile)?,
            case["profile"],
            "Integer conversion differs: {}",
            case["label"]
        );
        assert_eq!(
            to_dashboard(&profile),
            case["dashboard"],
            "Integer conversion roundtrip differs: {}",
            case["label"]
        );
        assert_eq!(record, before, "Integer conversion mutated its input");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fractional_conversion_matches_original_numeric_values_and_defaults()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("conversion-fractional")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/fractional_conversion_matches_original_numeric_values_and_defaults.json"
    ))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Fractional cases missing")?;
    assert_eq!(
        cases.len(),
        82,
        "Fractional conversion observations missing"
    );
    for case in cases {
        let mut record = oracle["base"].clone();
        record
            .as_object_mut()
            .ok_or("Profile input missing")?
            .extend(
                case["changes"]
                    .as_object()
                    .ok_or("Numeric changes missing")?
                    .clone(),
            );
        let before = record.clone();
        let profile = to_engine(&record, 1700000000.0)?;
        for (mut actual, mut expected, memory, ratio) in [
            (
                serde_json::to_value(&profile)?,
                case["profile"].clone(),
                "/fingerprint/device_memory",
                "/fingerprint/device_pixel_ratio",
            ),
            (
                to_dashboard(&profile),
                case["dashboard"].clone(),
                "/memory",
                "/devicePixelRatio",
            ),
        ] {
            for path in [ratio]
                .into_iter()
                .chain((case["memory_float"] == true).then_some(memory))
            {
                let bits = |record: &Value| {
                    record
                        .pointer(path)
                        .and_then(Value::as_f64)
                        .filter(|number| number.is_finite())
                        .map(f64::to_bits)
                };
                let observed = bits(&actual).ok_or("Converted numeric field is not finite")?;
                let original = bits(&expected).ok_or("Original numeric field is not finite")?;
                assert_eq!(
                    observed, original,
                    "Fractional conversion differs: {} {path}",
                    case["label"]
                );
                *actual
                    .pointer_mut(path)
                    .ok_or("Converted numeric field missing")? = Value::Null;
                *expected
                    .pointer_mut(path)
                    .ok_or("Original numeric field missing")? = Value::Null;
            }
            assert_eq!(
                actual, expected,
                "Fractional conversion differs: {}",
                case["label"]
            );
        }
        assert_eq!(record, before, "Fractional conversion mutated its input");
    }
    temporary.cleanup()?;
    Ok(())
}

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
