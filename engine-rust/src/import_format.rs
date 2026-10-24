//! Native envelope detection and vendor collection shape shared by CLI and HTTP.
use crate::validation;
use serde_json::{Value, json};

pub fn native(raw: &Value) -> Result<Option<Vec<Value>>, String> {
    let Some(version) = raw
        .as_object()
        .and_then(|record| record.get("numeraProfiles"))
    else {
        return Ok(None);
    };
    if version != &json!(1) && version != &json!(1.0) && version != &json!(true) {
        let spelling = version.to_string();
        let next = if validation::kind(version) == "int"
            && !spelling.starts_with('-')
            && (spelling.len() > 1 || spelling.as_str() > "1")
        {
            "It was written by a newer Numera - update this install, or re-export from that one."
        } else {
            "Re-export it from the install that wrote it."
        };
        return Err(format!(
            "this file says it is Numera export format {}; this build reads format 1. {next}",
            validation::repr(version)
        ));
    }
    let entries = raw["profiles"]
        .as_array()
        .filter(|entries| entries.iter().all(Value::is_object))
        .ok_or("a Numera export must carry 'profiles' as a list of objects")?;
    Ok(Some(entries.clone()))
}
pub fn entries(raw: &Value) -> Vec<Value> {
    let objects = |rows: &[Value]| rows.iter().filter(|row| row.is_object()).cloned().collect();
    if let Some(rows) = raw.as_array() {
        return objects(rows);
    }
    if raw.is_object() {
        for key in ["data", "profiles", "list", "items", "rows"] {
            if let Some(rows) = raw[key].as_array() {
                return objects(rows);
            }
        }
        return vec![raw.clone()];
    }
    Vec::new()
}
pub fn looks_native(record: &Value) -> bool {
    [
        "webglVendor",
        "webglRenderer",
        "clientRects",
        "webglImage",
        "mediaDevices",
        "mediaVideo",
        "mediaAudio",
        "mediaSpeakers",
        "fontList",
        "startupUrls",
        "launchArgs",
        "clearOnStart",
        "switchedOff",
        "ipBased",
        "devicePixelRatio",
        "maxTouchPoints",
        "osVersion",
        "deviceModel",
        "lastActive",
    ]
    .iter()
    .filter(|key| record.get(**key).is_some())
    .count()
        >= 6
}
pub fn vendor(raw: &Value) -> Result<Vec<Value>, String> {
    let entries = entries(raw);
    if native(raw)?.is_some() || entries.iter().any(looks_native) {
        return Err("this is a Numera profile export, not a vendor one, and reading it with the vendor importer would drop its fingerprint seed, signal modes, startup URLs, extensions and folder. Import it from the dashboard, which reads the native format field for field.".into());
    }
    Ok(entries)
}
