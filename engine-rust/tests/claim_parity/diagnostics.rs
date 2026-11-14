use numera_core::ownership::claims::{self, Claim};
use serde_json::Value;

#[test]
fn claim_diagnostics_match_original_truth_and_display_without_changing_records()
-> Result<(), Box<dyn std::error::Error>> {
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/claim_diagnostics_match_original_truth_and_display_without_changing_records.json"
    ))?;
    assert_eq!(cases.len(), 1011, "Claim diagnostic observations missing");
    for (index, case) in cases.iter().enumerate() {
        let claim = match case["kind"].as_str() {
            Some("absent") => Claim::Absent,
            Some("unreadable") => Claim::Unreadable,
            Some("record") => Claim::Record(case["input"].clone()),
            _ => return Err("Unknown claim observation".into()),
        };
        assert_eq!(
            claims::describe(&claim, "fixture-local-host"),
            case["description"]
                .as_str()
                .ok_or("Original diagnostic missing")?,
            "Claim diagnostic display differs: {index}",
        );
        if let Claim::Record(record) = claim {
            assert_eq!(record, case["input"], "Native diagnostic changed claim");
        }
    }
    Ok(())
}
