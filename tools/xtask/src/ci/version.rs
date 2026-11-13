//! One release number across every native product and build manifest.
use super::{Result, fail};
use std::{collections::BTreeMap, fs, path::Path};
pub fn declared(root: &Path) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    let pattern = regex::Regex::new(r#"(?m)^version\s*=\s*"([^"]+)""#)?;
    for name in [
        "engine-rust/Cargo.toml",
        "dashboard/Cargo.toml",
        "desktop/src-tauri/Cargo.toml",
        "tools/xtask/Cargo.toml",
    ] {
        let text = fs::read_to_string(root.join(name))?;
        let version = pattern
            .captures(&text)
            .ok_or("Native manifest version missing")?[1]
            .to_owned();
        values.insert(name.into(), version);
    }
    let configuration: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("desktop/src-tauri/tauri.conf.json"))?)?;
    values.insert(
        "desktop/src-tauri/tauri.conf.json".into(),
        configuration["version"]
            .as_str()
            .ok_or("Desktop version missing")?
            .into(),
    );
    Ok(values)
}
pub fn release_number(value: &str) -> bool {
    regex::Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$").is_ok_and(|pattern| pattern.is_match(value))
}
pub fn verify(root: &Path) -> Result<()> {
    let values = declared(root)?;
    let version = values
        .get("engine-rust/Cargo.toml")
        .ok_or("Native version missing")?;
    if !release_number(version)
        || values.values().any(|value| value != version)
        || version != env!("CARGO_PKG_VERSION")
        || !fs::read_to_string(root.join("CHANGELOG.md"))?.contains(&format!("## [{version}]"))
    {
        return fail("Native product version or changelog differs");
    }
    Ok(())
}
