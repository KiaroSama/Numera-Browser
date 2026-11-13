use super::common;
use numera_core::{import_format, importers};
use serde_json::{Value, json};

#[test]
fn importer_lexical_profiles_match_original_without_changing_inputs()
-> Result<(), Box<dyn std::error::Error>> {
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
