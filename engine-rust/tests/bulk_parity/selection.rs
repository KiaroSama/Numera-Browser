use super::common;
use numera_core::{bulk, models::Profile};
use serde_json::{Value, json};

#[test]
fn selection_matches_original_complete_profile_membership() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = common::TempDirectory::new("bulk-selection")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/selection_matches_original_complete_profile_membership.json"
    ))?;
    assert_eq!(rows.len(), 83, "Bulk selection observations missing");
    for (index, row) in rows.iter().enumerate() {
        let profiles = row["input"]
            .as_array()
            .ok_or("Selection profiles missing")?
            .iter()
            .map(Profile::from_record)
            .collect::<Result<Vec<_>, _>>()?;
        let before = serde_json::to_value(&profiles)?;
        let references: Vec<String> =
            serde_json::from_value(row["options"].get("refs").cloned().unwrap_or(json!([])))?;
        let selected = bulk::select(
            &profiles,
            &references,
            row["options"]["tag"].as_str(),
            row["options"]["everything"] == true,
        );
        assert_eq!(
            serde_json::to_value(selected)?,
            row["selected"],
            "Bulk profile selection differs: {index}"
        );
        assert_eq!(
            serde_json::to_value(&profiles)?,
            before,
            "Selection changed input"
        );
    }
    temporary.cleanup()?;
    Ok(())
}
