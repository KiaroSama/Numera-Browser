use numera_core::validation::profile_problems;
use serde_json::Value;
#[path = "validation_parity/contract.rs"]
mod contract;

#[test]
fn packed_screen_integer_grammar_matches_original() -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::http::profile_fields;

    // Frozen execution of the immutable source; native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/packed_screen_integer_grammar_matches_original.json"
    ))?;
    assert_eq!(cases.len(), 46, "Packed screen observations missing");
    for case in cases {
        assert_eq!(
            serde_json::to_value(profile_problems(&case["input"], "dashboard patch")?)?,
            case["core"],
            "Packed screen integer grammar differs: {}",
            case["input"]
        );
        assert_eq!(
            serde_json::to_value(profile_fields::check(&case["input"], "profile", &[])?)?,
            case["http"],
            "HTTP packed screen integer grammar differs: {}",
            case["input"]
        );
    }
    Ok(())
}

#[test]
fn ascii_refusal_representation_matches_original_without_exposing_secret_values()
-> Result<(), Box<dyn std::error::Error>> {
    // Frozen execution of the immutable source; native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/ascii_refusal_representation_matches_original_without_exposing_secret_values.json"
    ))?;
    assert_eq!(cases.len(), 268, "ASCII refusal observations missing");
    for (index, case) in cases.iter().enumerate() {
        let input = case["input"].clone();
        let actual = profile_problems(&input, "dashboard patch")?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "ASCII refusal representation differs: case={index}"
        );
        assert_eq!(input, case["input"], "Validation changed its input");
    }
    Ok(())
}

#[test]
fn unicode_representation_matches_original_public_validation_and_argv()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{cli, http::profile_fields};

    // Frozen execution of the immutable source; native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/unicode_representation_matches_original_public_validation_and_argv.json"
    ))?;
    assert_eq!(
        cases.len(),
        315,
        "Unicode representation observations missing"
    );
    for (index, case) in cases.iter().enumerate() {
        let input = case["input"].clone();
        assert_eq!(
            serde_json::to_value(profile_problems(&input, "dashboard patch")?)?,
            case["core"],
            "Unicode representation differs: core case={index}"
        );
        assert_eq!(
            serde_json::to_value(profile_fields::check(&case["http_input"], "profile", &[])?)?,
            case["http"],
            "Unicode representation differs: HTTP case={index}"
        );
        assert_eq!(
            serde_json::to_value(cli::reflection::argv(
                "show",
                case["arguments"]
                    .as_object()
                    .ok_or("Source arguments missing")?
            )?)?,
            case["argv"],
            "Unicode representation differs: argv case={index}"
        );
        assert_eq!(input, case["input"], "Validation changed its input");
    }
    Ok(())
}

#[test]
fn finite_numbers_match_original_validation_and_mcp_representation()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{cli, http::profile_fields};

    // Frozen execution of the immutable source; native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/finite_numbers_match_original_validation_and_mcp_representation.json"
    ))?;
    assert_eq!(
        rows.len(),
        651,
        "Numeric representation observations missing"
    );
    for (index, row) in rows.iter().enumerate() {
        let value: Value =
            serde_json::from_str(row["text"].as_str().ok_or("Numeric input missing")?)?;
        let input = serde_json::json!({
            "name":value,"notes":{"nested":[value,true,null]},"cores":value,
            "proxy":{"password":[value],"pass":{"private-control":value}}
        });
        let before = input.clone();
        assert_eq!(
            serde_json::to_value(profile_problems(&input, "dashboard patch")?)?,
            row["core"],
            "Numeric representation differs: core case={index}"
        );
        assert_eq!(
            serde_json::to_value(profile_fields::check(
                &serde_json::json!({"cores":value}),
                "profile",
                &[]
            )?)?,
            row["http"],
            "Numeric representation differs: HTTP case={index}"
        );
        let arguments = serde_json::json!({"ref":[value,{"nested":[value,true,null]},row["text"]]});
        assert_eq!(
            serde_json::to_value(cli::reflection::argv(
                "show",
                arguments.as_object().ok_or("Arguments missing")?
            )?)?,
            row["argv"],
            "Numeric representation differs: argv case={index}"
        );
        assert_eq!(input, before, "Validation changed its numeric input");
    }
    Ok(())
}

#[test]
fn permitted_fraction_spellings_match_original_numeric_admission()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{http::profile_fields, validation};

    // Frozen execution of the immutable source; native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/permitted_fraction_spellings_match_original_numeric_admission.json"
    ))?;
    assert_eq!(rows.len(), 47, "Fractional admission observations missing");
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo-write.json"))?;
    let base = &fixture["cases"][0]["initial"]["profiles"];
    for (index, row) in rows.iter().enumerate() {
        let value: Value = serde_json::from_str(row["text"].as_str().ok_or("Input missing")?)?;
        let mut record = base.clone();
        record["fingerprint"]["device_memory"] = value.clone();
        let patch = serde_json::json!({"memory": value});
        let before = (record.clone(), patch.clone());
        for (label, actual) in [
            ("engine", profile_problems(&record, "engine record")?),
            ("patch", profile_problems(&patch, "dashboard patch")?),
            ("flat", validation::flat_number_problems(&patch)?),
            ("http", profile_fields::check(&patch, "profile", &[])?),
        ] {
            assert_eq!(
                serde_json::to_value(actual)?,
                row[label],
                "Fraction numeric admission differs: {label} case={index}"
            );
        }
        assert_eq!(
            (record, patch),
            before,
            "Numeric validation changed its input"
        );
    }
    Ok(())
}

#[test]
fn uncoerced_write_validation_matches_source() -> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/validation.json"))?;
    let cases = oracle["cases"]
        .as_array()
        .ok_or("Missing write validation cases")?;
    assert!(
        !cases.is_empty(),
        "Write validation oracle must not be empty"
    );
    for case in cases {
        let actual = profile_problems(
            &case["input"],
            case["kind"]
                .as_str()
                .ok_or("Missing declared profile shape")?,
        )?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "Write validation {}",
            case["name"]
        );
    }
    Ok(())
}
