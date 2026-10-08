use serde_json::Value;
use std::sync::OnceLock;

static CATALOG: OnceLock<Value> = OnceLock::new();

pub(crate) fn catalog() -> Result<&'static Value, serde_json::Error> {
    if let Some(value) = CATALOG.get() {
        return Ok(value);
    }
    let value: Value = serde_json::from_str(include_str!("device_catalog.json"))?;
    let _ = CATALOG.set(value);
    // Initialization is immutable and another racing initializer is equivalent.
    Ok(CATALOG.get().expect("immutable parsed catalog installed"))
}
