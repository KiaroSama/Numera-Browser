use serde_json::Value;
use std::{io, path::Path};

/// Snapshot a plain protocol fixture whose descendants stay in its worker
/// group. Successful session cleanup retires its unused browser receipt, so
/// prove that receipt contains no other group before retaining this capability.
pub fn capture(mut record: Value) -> io::Result<Value> {
    if let Some(path) = record.get("browserUnits") {
        let path = path
            .as_str()
            .ok_or_else(|| io::Error::other("Fixture browser receipt path is invalid"))?;
        let report = numera_core::ownership::unit_report::read(Path::new(path))?;
        if !report.complete || !report.trees.is_empty() {
            return Err(io::Error::other(
                "Plain protocol fixture unexpectedly owns separate browser groups",
            ));
        }
        record
            .as_object_mut()
            .ok_or_else(|| io::Error::other("Fixture ownership record is invalid"))?
            .remove("browserUnits");
    }
    Ok(record)
}
