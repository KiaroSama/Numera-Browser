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
        let tail = &rest[at + TOKEN.len()..];
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '\u{1b}')
            .unwrap_or(tail.len());
        let path = format!(
            "{root}{}",
            tail[..end].replace('/', std::path::MAIN_SEPARATOR_STR)
        );
        // setup.py's two removal diagnostics interpolate str(path) inside
        // literal quotes. store.py's unloadable-folder error uses path!r.
        // Quotation alone cannot select the latter's backslash escaping.
        let suffix = &tail[end..];
        let literal_removal = prefix.ends_with('\'')
            && suffix
                .strip_prefix("' isn't on this profile")
                .or_else(|| suffix.strip_prefix("' isn't in the default set"))
                .is_some_and(|rest| matches!(rest, "" | "." | ".\n" | ".\u{1b}[0m\n"));
        let quoted = prefix.ends_with('\'') || prefix.ends_with('"');
        if quoted && !literal_removal {
            output.push_str(&path.replace('\\', "\\\\"));
        } else {
            output.push_str(&path);
        }
        rest = &tail[end..];
    }
    output.push_str(rest);
    output
}

// This shared fixture module is compiled by several original targets. Only
// the existing settings case calls these platform-independent fault controls.
#[allow(dead_code)]
pub fn check_extension_path_materialization() {
    let root = r"C:\fixture root\extensions\missing";
    for (template, expected) in [
        (
            "'@CLI_ROOT@' isn't on this profile",
            r"'C:\fixture root\extensions\missing' isn't on this profile",
        ),
        (
            "'@CLI_ROOT@' isn't in the default set.\n",
            "'C:\\fixture root\\extensions\\missing' isn't in the default set.\n",
        ),
        (
            "\u{1b}[31m'@CLI_ROOT@' isn't on this profile.\u{1b}[0m\n",
            "\u{1b}[31m'C:\\fixture root\\extensions\\missing' isn't on this profile.\u{1b}[0m\n",
        ),
        (
            "'@CLI_ROOT@' is not a folder on this machine.",
            r"'C:\\fixture root\\extensions\\missing' is not a folder on this machine.",
        ),
        (
            "'@CLI_ROOT@' isn't on this profiles",
            r"'C:\\fixture root\\extensions\\missing' isn't on this profiles",
        ),
        (
            "other '@CLI_ROOT@' diagnostic",
            r"other 'C:\\fixture root\\extensions\\missing' diagnostic",
        ),
        ("@CLI_ROOT@", root),
    ] {
        assert_eq!(
            materialize(template, root),
            expected,
            "CLI path observation materialization differs"
        );
    }
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
