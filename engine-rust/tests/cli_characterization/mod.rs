//! Immutable-source CLI observations. Only their owned fixture root is dynamic.
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use sha2::Digest;
use std::{fs, io, path::Path};
fn materialize(text: &str, root: &str) -> String {
    const TOKEN: &str = "@CLI_ROOT@";
    let mut output = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(TOKEN) {
        let prefix = &rest[..at];
        output.push_str(prefix);
        let quoted = prefix.ends_with('\'') || prefix.ends_with('"');
        let tail = &rest[at + TOKEN.len()..];
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '\u{1b}')
            .unwrap_or(tail.len());
        let path = format!(
            "{root}{}",
            tail[..end].replace('/', std::path::MAIN_SEPARATOR_STR)
        );
        if quoted {
            output.push_str(&path.replace('\\', "\\\\"));
        } else {
            output.push_str(&path);
        }
        rest = &tail[end..];
    }
    output.push_str(rest);
    output
}
fn paths(value: &mut Value, root: &str) {
    match value {
        Value::String(text) => *text = materialize(text, root),
        Value::Array(values) => values.iter_mut().for_each(|value| paths(value, root)),
        Value::Object(values) => values.values_mut().for_each(|value| paths(value, root)),
        _ => (),
    }
}
pub fn load(label: &str, root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let text = match label {
        "cli_profile_parity" => {
            include_str!("../fixtures/cli-characterization/cli_profile_parity.json")
        }
        "cli_pool_parity" => include_str!("../fixtures/cli-characterization/cli_pool_parity.json"),
        "cli_import_parity" => {
            include_str!("../fixtures/cli-characterization/cli_import_parity.json")
        }
        "cli_settings_parity" => {
            include_str!("../fixtures/cli-characterization/cli_settings_parity.json")
        }
        "cli_cookie_parity" => {
            include_str!("../fixtures/cli-characterization/cli_cookie_parity.json")
        }
        "bulk_parity" => include_str!("../fixtures/cli-characterization/bulk_parity.json"),
        "proxy_assignment_parity" => {
            include_str!("../fixtures/cli-characterization/proxy_assignment_parity.json")
        }
        _ => return Err(io::Error::other("Unknown CLI observation").into()),
    };
    let manifest: Value = serde_json::from_str(include_str!(
        "../fixtures/cli-characterization-manifest.json"
    ))?;
    let suffix = format!("/{label}.json");
    let receipt = manifest["fixtures"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["file"]
                    .as_str()
                    .is_some_and(|file| file.ends_with(&suffix))
            })
        })
        .ok_or("CLI observation provenance missing")?;
    let hash = sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if receipt["sha256"] != hash {
        return Err(
            io::Error::other("CLI observation digest differs from reviewed manifest").into(),
        );
    }
    let mut document: Value = serde_json::from_str(text)?;
    paths(&mut document, &root.to_string_lossy());
    fs::create_dir_all(root)?;
    for (name, raw) in document["files"]
        .as_object()
        .ok_or("CLI source fixture files missing")?
    {
        let target = numera_core::storage::archive_paths::member_path(root, name)?;
        fs::create_dir_all(target.parent().ok_or("CLI fixture parent missing")?)?;
        fs::write(
            target,
            STANDARD.decode(raw.as_str().ok_or("CLI fixture bytes missing")?)?,
        )?;
    }
    Ok(document["observation"].take())
}
