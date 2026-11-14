use numera_core::ownership::unit_report;
use serde_json::json;
use std::fs;
mod common;
#[path = "common/ownership_oracle.rs"]
mod ownership_oracle;

#[test]
fn receipt_tokens_match_original_without_resolving_invalid_transfers()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("receipt-token-parity")?;
    let document = ownership_oracle::read("receipt-tokens")?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Receipt cases missing")?;
    assert_eq!(cases.len(), 33, "Receipt token observations missing");
    let prefix = document["prefix"]
        .as_str()
        .ok_or("Receipt prefix missing")?;
    let jobs = temporary.path().join("jobs");
    fs::create_dir(&jobs)?;
    let path = jobs.join("fixture.units");
    for case in cases {
        let raw = case["raw"].as_str().ok_or("Receipt bytes missing")?;
        fs::write(&path, raw)?;
        let report = unit_report::read(&path)?;
        assert_eq!(
            report.complete,
            case["complete"] == true,
            "Receipt completeness differs: {}",
            case["label"]
        );
        assert_eq!(report.problem.is_some(), case["problem"] == true);
        assert_eq!(json!(report.trees), case["trees"]);
        assert_eq!(
            fs::read(&path)?,
            raw.as_bytes(),
            "Receipt read changed bytes"
        );
        let scoped = unit_report::read_scoped(&path, Some("unrelated"))?;
        assert_eq!(scoped.complete, case["complete"] == true);
        assert_eq!(scoped.problem.is_some(), case["problem"] == true);
        assert!(
            scoped.trees.is_empty(),
            "Unrelated receipt scope leaked a unit"
        );

        fs::write(&path, prefix)?;
        let input = &case["input"];
        let result = unit_report::record_at(
            &path,
            &json!({"pid":input["pid"],"token":input["pidToken"],"tree":input["tree"]}),
            "fixture",
        );
        assert_eq!(
            result.is_ok(),
            case["complete"] == true,
            "Receipt write admission differs: {}",
            case["label"]
        );
        if result.is_ok() {
            let report = unit_report::read(&path)?;
            assert!(report.complete && report.problem.is_none());
            assert_eq!(json!(report.trees), case["trees"]);
        } else {
            assert_eq!(
                fs::read(&path)?,
                prefix.as_bytes(),
                "Refused receipt write changed bytes"
            );
            assert!(
                !unit_report::read(&path)?.complete,
                "Refusal resolved its pending transfer"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}
