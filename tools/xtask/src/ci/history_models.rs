//! Reviewed caller-only ABI bridge for historical `engine-rust/src/models.rs`
//! snapshots that predate optional MAC address and device name fields.
use super::{
    Result,
    history::{Case, Source},
    history_compat::{Replacement, caller_replacement},
};
use std::{collections::BTreeMap, fs, path::Path};

const HISTORICAL_MODELS_SHA: &str =
    "be512b37753170fc18a479cadd50e308f6088f76a040977a3f932bffd7fda0f4";

pub(super) fn additional(
    root: &Path,
    case: &Case,
    sources: &BTreeMap<String, Source>,
) -> Result<Vec<(String, Replacement)>> {
    let Some(source) = sources
        .get("engine-rust/src/models.rs")
        .filter(|source| source.sha256 == HISTORICAL_MODELS_SHA)
    else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for (path, patches) in [
        (
            "engine-rust/src/conversion.rs",
            vec![
                (
                    r#"    if let Some(raw_mode) = d.get("mac_address_mode").and_then(Value::as_str) {
        fp.mac_address_mode = match trim(raw_mode).to_ascii_lowercase().as_str() {
            "" | "off" => crate::models::MacAddressMode::Off,
            "manual" => crate::models::MacAddressMode::Manual,
            "generate" => crate::models::MacAddressMode::Generate,
            "noise" => crate::models::MacAddressMode::Noise,
            other => {
                return Err(ConversionError(format!(
                    "unsupported mac_address_mode '{other}'"
                )));
            }
        };
    }
    if let Some(raw_mac) = d.get("mac_address").and_then(Value::as_str) {
        let mac = trim(raw_mac).to_ascii_uppercase();
        if !mac.is_empty() {
            fp.mac_address = Some(mac);
        }
    }
    if let Some(raw_dev) = d.get("device_name").and_then(Value::as_str) {
        let dev = trim(raw_dev);
        if !dev.is_empty() {
            fp.device_name = Some(dev.to_owned());
        }
    }"#,
                    "    let _ = &mut fp;",
                ),
                (
                    r#"        if !fp.mac_address_mode.is_off() {
            object.insert(
                "mac_address_mode".into(),
                serde_json::to_value(fp.mac_address_mode).unwrap_or(Value::Null),
            );
        }
        if let Some(mac) = &fp.mac_address {
            object.insert("mac_address".into(), mac.clone().into());
        }
        if let Some(device_name) = &fp.device_name {
            object.insert("device_name".into(), device_name.clone().into());
        }"#,
                    "",
                ),
            ],
        ),
        (
            "engine-rust/src/identity.rs",
            vec![
                (
                    r#"pub(crate) fn profile_mac_address(fp: &Fingerprint) -> Option<String> {
    use crate::models::MacAddressMode;
    match fp.mac_address_mode {
        MacAddressMode::Off => None,
        MacAddressMode::Manual => fp
            .mac_address
            .as_deref()
            .filter(|value| crate::validation::valid_mac_address(value))
            .map(str::to_owned),
        MacAddressMode::Generate => Some(crate::fingerprint::local_mac_address(
            &fp.seed.to_string(),
            "mac-generate",
        )),
        MacAddressMode::Noise => Some(crate::fingerprint::local_mac_address(
            &fp.seed.to_string(),
            "mac-noise",
        )),
    }
}"#,
                    "pub(crate) fn profile_mac_address(_fp: &Fingerprint) -> Option<String> {\n    None\n}",
                ),
                (
                    r#"    if fp
        .mac_address
        .as_deref()
        .is_some_and(|value| !crate::validation::valid_mac_address(value))
        || (fp.mac_address_mode == crate::models::MacAddressMode::Manual
            && fp.mac_address.is_none())
    {
        issues.push("invalid MAC address".into());
    }
    if fp
        .device_name
        .as_deref()
        .is_some_and(|value| !crate::validation::valid_device_name(value))
    {
        issues.push("invalid device name".into());
    }"#,
                    "",
                ),
            ],
        ),
        (
            "engine-rust/src/identity_script.rs",
            vec![(
                r#"    if let Some(mac) = identity::profile_mac_address(fp) {
        config.insert("macAddress".into(), mac.into());
        config.insert(
            "macAddressMode".into(),
            serde_json::to_value(fp.mac_address_mode)?,
        );
    }
    if let Some(name) = fp
        .device_name
        .as_deref()
        .filter(|value| crate::validation::valid_device_name(value))
    {
        config.insert("deviceName".into(), name.into());
    }"#,
                "    let _ = identity::profile_mac_address(fp);",
            )],
        ),
    ] {
        if sources.contains_key(path) {
            continue;
        }
        let bytes = fs::read(root.join(path))?;
        let normalized = std::str::from_utf8(&bytes)?.replace("\r\n", "\n");
        let mut replacement =
            caller_replacement(case, path, normalized.as_bytes(), source, &patches)?;
        replacement.receipt["historical_source_path"] = "engine-rust/src/models.rs".into();
        replacement.receipt["reason"] = "Adapt later MAC/device-name callers to the frozen pre-0.37 models.rs ABI; selected historical models.rs bytes remain exact Git bytes.".into();
        result.push((path.into(), replacement));
    }
    Ok(result)
}
