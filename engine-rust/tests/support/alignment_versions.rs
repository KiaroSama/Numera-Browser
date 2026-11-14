use numera_core::{browser::alignment, models::Profile};
use serde_json::{Value, json};

#[test]
fn alignment_versions_match_original_decimal_captures() -> Result<(), Box<dyn std::error::Error>> {
    // Frozen execution of the immutable source; native assertions below remain live.
    let document: Value = serde_json::from_str(include_str!(
        "../fixtures/characterization/alignment_versions_match_original_decimal_captures.json"
    ))?;
    let rows = document["rows"]
        .as_array()
        .ok_or("Alignment rows missing")?;
    assert_eq!(document["ranges"], 64, "Original decimal ranges changed");
    assert_eq!(rows.len(), 92, "Alignment observations missing");
    let profile = Profile::from_record(&document["profile"])?;
    for (index, row) in rows.iter().enumerate() {
        let mut seen = document["seen"].clone();
        seen["userAgent"] = row["ua"].clone();
        let before_profile = serde_json::to_value(&profile)?;
        let before_seen = seen.clone();
        let (aligned, changed) = alignment::rewrite(&profile.fingerprint, &seen)?;
        assert_eq!(
            serde_json::to_value(&aligned)?,
            row["fingerprint"],
            "Alignment version fingerprint differs: {index}"
        );
        assert_eq!(
            json!(changed),
            row["changed"],
            "Alignment change labels differ: {index}"
        );
        assert_eq!(serde_json::to_value(&profile)?, before_profile);
        assert_eq!(seen, before_seen);
    }
    Ok(())
}
