use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    storage::repo::RepoError,
};
use serde_json::{Map, Value};
use std::{io::Write, path::Path};

pub fn render(
    output: &mut Output<impl Write, impl Write>,
    args: &Map<String, Value>,
    payload: &Value,
    resolved_name: Option<&Value>,
    home: &Path,
    added_default: bool,
) -> Result<bool, RepoError> {
    if output.json_mode() {
        return Ok(true);
    }
    let sub = args
        .get("ext_command")
        .and_then(Value::as_str)
        .unwrap_or("");
    if let Some(error) = payload.get("error") {
        let error = value_text(error);
        let message = if error.starts_with("no profile matching '") {
            format!("No{}.", &error[2..])
        } else if matches!(sub, "remove" | "default-remove") {
            format!("{error}.")
        } else {
            error
        };
        output.say(&message, Some(Tone::Red))?;
        return Ok(true);
    }
    match sub {
        "list" | "default-list" => {
            let rows = payload["extensions"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Extension list missing".into()))?;
            if sub == "list" {
                let name =
                    value_text(resolved_name.ok_or_else(|| {
                        RepoError::Message("Extension profile name missing".into())
                    })?);
                if rows.is_empty() {
                    output.say(&format!("'{name}' loads no extensions."), Some(Tone::Dim))?;
                } else {
                    output.say(&format!("Extensions for '{name}':"), Some(Tone::Bold))?;
                }
            } else if rows.is_empty() {
                output.say(
                    "No default extensions. New profiles start with none.",
                    Some(Tone::Dim),
                )?;
            } else {
                output.say("Default extensions for new profiles:", Some(Tone::Bold))?;
            }
            for path in rows {
                let text = value_text(path);
                let expanded = super::settings_commands::expand(&text, home)?;
                let note = if Path::new(&expanded).is_dir() {
                    String::new()
                } else {
                    output.style("  (folder not found)", Tone::Red)
                };
                output.say(&format!("  {text}{note}"), None)?;
            }
        }
        "add" | "remove" => {
            if payload["changed"] == false {
                output.say("Already loaded.", Some(Tone::Dim))?;
            } else {
                let name =
                    value_text(resolved_name.ok_or_else(|| {
                        RepoError::Message("Extension profile name missing".into())
                    })?);
                let count = payload["extensions"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Extension result missing".into()))?
                    .len();
                output.say(
                    &format!(
                        "{} — '{name}' now loads {count} extension(s).",
                        if sub == "add" { "Added" } else { "Removed" }
                    ),
                    Some(Tone::Green),
                )?;
            }
        }
        "default-add" | "default-remove" => {
            if payload["changed"] == false {
                output.say("Already in the default set.", Some(Tone::Dim))?;
            } else {
                let count = payload["extensions"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Default extension result missing".into()))?
                    .len();
                output.say(
                    &format!(
                        "Default set now has {count} extension(s). New profiles will load them."
                    ),
                    Some(Tone::Green),
                )?;
            }
        }
        "installed" => {
            let rows = payload["extensions"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Installed extensions missing".into()))?;
            for row in rows {
                output.say(
                    &format!("  {}  {}", value_text(&row["id"]), value_text(&row["name"])),
                    None,
                )?;
            }
            if rows.is_empty() {
                output.say(
                    "No extensions installed from the Web Store yet.",
                    Some(Tone::Dim),
                )?;
            }
        }
        "uninstall" => output.say(
            if truth(&payload["removed"]) {
                "Removed."
            } else {
                "Not installed."
            },
            Some(if truth(&payload["removed"]) {
                Tone::Green
            } else {
                Tone::Dim
            }),
        )?,
        "install" => {
            output.say(
                &format!(
                    "{}: {}  ({})",
                    if truth(&payload["reused"]) {
                        "Already installed"
                    } else {
                        "Installed"
                    },
                    value_text(&payload["name"]),
                    value_text(&payload["id"])
                ),
                Some(Tone::Green),
            )?;
            output.say(
                &format!("  {}", value_text(&payload["path"])),
                Some(Tone::Dim),
            )?;
            if added_default {
                output.say(
                    "  Added to the set every new profile starts with.",
                    Some(Tone::Dim),
                )?;
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
