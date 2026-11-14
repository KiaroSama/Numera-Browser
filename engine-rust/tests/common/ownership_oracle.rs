//! Frozen execution of immutable historical ownership observers, kept separate
//! from the exact source-ID test map and live kernel/process tests.
use serde_json::Value;
use sha2::Digest;
use std::io;
pub fn read(name: &str) -> io::Result<Value> {
    let data = match name {
        "receipt-tokens" => {
            include_bytes!("../fixtures/ownership-characterization/receipt-tokens.json").as_slice()
        }
        "procfs-observation" => {
            include_bytes!("../fixtures/ownership-characterization/procfs-observation.json")
                .as_slice()
        }
        _ => return Err(io::Error::other("Unknown ownership observation")),
    };
    let manifest: Value = serde_json::from_str(include_str!(
        "../fixtures/ownership-characterization/manifest.json"
    ))?;
    let filename = format!("{name}.json");
    let row = manifest["files"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["file"] == filename))
        .ok_or_else(|| io::Error::other("Ownership observer provenance absent"))?;
    if row["source_commit"] != "13b6d9c6973281fe123353eaec05b29d65781a2d"
        || row["bytes"].as_u64() != Some(data.len() as u64)
        || row["sha256"]
            != sha2::Sha256::digest(data)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
    {
        return Err(io::Error::other("Ownership observer checksum changed"));
    }
    Ok(serde_json::from_slice(data)?)
}
