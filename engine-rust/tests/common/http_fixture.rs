// Historical native characterization, separate from the original source-ID suites.
use serde_json::Value;
use sha2::Digest;
use std::{io, path::Path};

pub fn oracle(name: &str) -> io::Result<Value> {
    let text = match name {
        "launch-http" => include_str!("../fixtures/http-characterization/launch-http.json"),
        "cookie-http" => include_str!("../fixtures/http-characterization/cookie-http.json"),
        "stop-http" => include_str!("../fixtures/http-characterization/stop-http.json"),
        "mcp" => include_str!("../fixtures/http-characterization/mcp.json"),
        "job" => include_str!("../fixtures/http-characterization/job.json"),
        "import-tags" => include_str!("../fixtures/http-characterization/import-tags.json"),
        "import-refusals" => include_str!("../fixtures/http-characterization/import-refusals.json"),
        "log-verbosity" => include_str!("../fixtures/http-characterization/log-verbosity.json"),
        "log-retention" => include_str!("../fixtures/http-characterization/log-retention.json"),
        _ => return Err(io::Error::other("Unknown historical HTTP observation")),
    };
    let manifest: Value =
        serde_json::from_str(include_str!("../fixtures/http-oracle-manifest.json"))?;
    let suffix = format!("/{name}.json");
    let row = manifest["fixtures"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["file"]
                    .as_str()
                    .is_some_and(|file| file.ends_with(&suffix))
            })
        })
        .ok_or_else(|| io::Error::other("Historical HTTP observation provenance missing"))?;
    if row["bytes"].as_u64() != Some(text.len() as u64)
        || row["sha256"]
            != sha2::Sha256::digest(text.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
    {
        return Err(io::Error::other(
            "Historical HTTP observation checksum differs",
        ));
    }
    Ok(serde_json::from_str(text)?)
}

/// Rebase only paths explicitly recorded beneath the observer's temporary root.
/// Fixture directories and records are created by each calling test.
pub fn rebase(value: &mut Value, observed: &str, root: &Path) -> io::Result<()> {
    match value {
        Value::String(text) => {
            if let Some(relative) = text.strip_prefix(observed) {
                *text = root
                    .join(relative.trim_start_matches(['/', '\\']))
                    .to_str()
                    .ok_or_else(|| io::Error::other("Fixture root is not Unicode"))?
                    .to_owned();
            }
        }
        Value::Array(rows) => {
            for row in rows {
                rebase(row, observed, root)?;
            }
        }
        Value::Object(rows) => {
            for row in rows.values_mut() {
                rebase(row, observed, root)?;
            }
        }
        _ => {}
    }
    Ok(())
}
