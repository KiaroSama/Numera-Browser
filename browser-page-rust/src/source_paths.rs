//! Stable source identities for the locked browser dependency graph.
use serde_json::Value;
use std::{collections::BTreeMap, io, path::Path};
fn refuse() -> io::Error {
    io::Error::other("Locked dependency source metadata is malformed or ambiguous")
}
/// Registry cache identifiers and mirror names are machine-local. Map actual
/// locked package roots, including aliases, to their stable crate identities.
pub fn dependency_remaps(metadata: &Value) -> io::Result<Vec<String>> {
    if metadata["version"] != 1 {
        return Err(refuse());
    }
    let packages = metadata["packages"]
        .as_array()
        .filter(|p| !p.is_empty() && p.len() <= 512)
        .ok_or_else(refuse)?;
    let mut roots = BTreeMap::new();
    let mut identities = BTreeMap::new();
    for package in packages {
        let source = package.get("source").ok_or_else(refuse)?;
        if source.is_null() {
            continue;
        }
        if !source
            .as_str()
            .is_some_and(|s| s.starts_with("registry+") || s.starts_with("git+"))
        {
            return Err(refuse());
        }
        let name = package["name"].as_str().ok_or_else(refuse)?;
        let version = package["version"].as_str().ok_or_else(refuse)?;
        for value in [name, version] {
            if value.is_empty()
                || value.len() > 128
                || value == "."
                || value.contains("..")
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.+".contains(&b))
            {
                return Err(refuse());
            }
        }
        let manifest = Path::new(package["manifest_path"].as_str().ok_or_else(refuse)?);
        if !manifest.is_absolute() || manifest.file_name().is_none_or(|n| n != "Cargo.toml") {
            return Err(refuse());
        }
        let canonical = manifest.canonicalize()?;
        let directory = canonical
            .parent()
            .filter(|p| p.parent().is_some())
            .ok_or_else(refuse)?;
        let target = format!("crates/{name}-{version}/");
        if identities
            .insert(target.clone(), directory.to_owned())
            .is_some_and(|previous| previous != directory)
        {
            return Err(refuse());
        }
        for parent in [manifest.parent().ok_or_else(refuse)?, directory] {
            let parent = parent.to_str().ok_or_else(refuse)?;
            if parent.contains(['=', '\n', '\r', '\u{1f}']) {
                return Err(refuse());
            }
            let prefix = format!("{parent}{}", std::path::MAIN_SEPARATOR);
            if roots
                .insert(prefix, target.clone())
                .is_some_and(|previous| previous != target)
            {
                return Err(refuse());
            }
        }
    }
    if roots.is_empty() {
        return Err(refuse());
    }
    Ok(roots
        .into_iter()
        .map(|(source, target)| format!("--remap-path-prefix={source}={target}"))
        .collect())
}
