use super::common;
use numera_core::{import_format, importers};
use serde_json::{Value, json};

#[test]
fn importer_lexical_profiles_match_original_without_changing_inputs()
-> Result<(), Box<dyn std::error::Error>> {
    extended_json_lexical_contracts();
    let temporary = common::TempDirectory::new("importer-lexical")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/importer_lexical_profiles_match_original_without_changing_inputs.json"
    ))?;
    assert_eq!(cases.len(), 89, "Importer observations missing");
    for (index, case) in cases.iter().enumerate() {
        let raw = case["input"].clone();
        let (dashboard, issues) = importers::profile(&raw, 42, 1000.0)?;
        assert_eq!(
            dashboard, case["dashboard"],
            "Imported lexical dashboard differs: {index}"
        );
        assert_eq!(
            json!(issues),
            case["issues"],
            "Imported lexical issues differ: {index}"
        );
        let outcome = match importers::model(&raw, 42, 1000.0) {
            Ok((profile, issues)) => {
                assert_eq!(profile.id.len(), 12);
                assert!(
                    profile
                        .id
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                );
                let mut record = serde_json::to_value(profile)?;
                record["id"] = "generated".into();
                assert_eq!(json!(issues), case["issues"]);
                json!({"ok":record})
            }
            Err(error) => json!({"error":error}),
        };
        assert_eq!(
            outcome, case["model"],
            "Imported lexical model differs: {index}"
        );
        assert_eq!(raw, case["input"], "Native importer changed input");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn native_envelope_versions_match_original_exact_numeric_guidance()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("import-envelope-versions")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/native_envelope_versions_match_original_exact_numeric_guidance.json"
    ))?;
    assert_eq!(cases.len(), 17, "Native version observations missing");
    for (index, case) in cases.iter().enumerate() {
        let raw = case["input"].clone();
        let actual = match import_format::native(&raw) {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":error}),
        };
        assert_eq!(
            actual, case["outcome"],
            "Native export version guidance differs: {index}"
        );
        assert_eq!(raw, case["input"], "Native envelope parser changed input");
    }
    temporary.cleanup()?;
    Ok(())
}

// Fold the HTTP JSON grammar controls into the existing lexical-import case,
// preserving its ID and the complete original native case inventory.
fn extended_json_lexical_contracts() {
    use numera_core::http::json_extended::parse;
    let value=parse(r#"{"a":[NaN,Infinity,-Infinity],"__numera_nonfinite_0__":"__numera_nonfinite_1__","quoted":"NaN","escaped":"\u005f_numera_nonfinite_2__","literal":"-Infinity"}"#).expect("source JSON constants");
    assert_eq!(value["a"][0].as_number().expect("NaN").as_str(), "NaN");
    assert_eq!(
        value["a"][1].as_number().expect("infinity").as_str(),
        "Infinity"
    );
    assert_eq!(
        value["a"][2]
            .as_number()
            .expect("negative infinity")
            .as_str(),
        "-Infinity"
    );
    assert_eq!(value["quoted"], "NaN");
    assert_eq!(value["literal"], "-Infinity");
    assert_eq!(value["escaped"], "__numera_nonfinite_2__");
    assert_eq!(value["__numera_nonfinite_0__"], "__numera_nonfinite_1__");
    for text in [
        "[NaNx]",
        "[Infinity0]",
        "[-Infinity.0]",
        "[+Infinity]",
        "[nan]",
        "[NaN,]",
        "{NaN:1}",
        "{\"x\":NaN //comment\n}",
        "{\"x\":Infinity} trailing",
        "[NaN Infinity]",
    ] {
        assert!(parse(text).is_err(), "Accepted invalid JSON: {text}");
    }
    for text in ["null", "{\"x\":1.25}", "[true,false,\"NaN\"]"] {
        assert_eq!(
            parse(text).expect("ordinary JSON"),
            serde_json::from_str::<Value>(text).expect("ordinary JSON")
        );
    }
}
