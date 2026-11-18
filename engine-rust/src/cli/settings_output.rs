use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    storage::repo::RepoError,
};
use serde_json::{Map, Value};
use std::io::Write;

const ENGINES: &[&str] = &["camoufox", "cloak", "patchright", "playwright"];
fn pad(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(text.chars().count()))
    )
}
pub fn render(
    output: &mut Output<impl Write, impl Write>,
    args: &Map<String, Value>,
    payload: &Value,
) -> Result<bool, RepoError> {
    if output.json_mode() {
        return Ok(true);
    }
    match args.get("command").and_then(Value::as_str).unwrap_or("") {
        "engines" => {
            output.say("Launch engines:", Some(Tone::Bold))?;
            for engine in ENGINES {
                let installed = payload["engines"][*engine] == true;
                let mark = output.style(
                    if installed {
                        "installed"
                    } else {
                        "not installed"
                    },
                    if installed { Tone::Green } else { Tone::Dim },
                );
                let star = if payload["default"] == *engine {
                    output.style("  <- default (new profiles)", Tone::Cyan)
                } else {
                    String::new()
                };
                output.say(&format!("  {}{mark}{star}", pad(engine, 14)), None)?;
            }
        }
        "default-engine" => {
            let requested = args.get("name").filter(|value| truth(value));
            if let Some(name) = requested {
                let name = value_text(name);
                if payload["ok"] == false {
                    output.say(
                        &format!(
                            "Unknown engine '{name}'. Choose one of: {}.",
                            ENGINES.join(", ")
                        ),
                        Some(Tone::Red),
                    )?;
                } else {
                    let note = if payload["installed"] == false {
                        output.style(
                            "  (not installed yet — install it before launching)",
                            Tone::Yellow,
                        )
                    } else {
                        String::new()
                    };
                    let text = output.style(
                        &format!("Default engine for new profiles is now '{name}'."),
                        Tone::Green,
                    );
                    output.say(&format!("{text}{note}"), None)?;
                }
            } else {
                let label = output.style("Default engine for new profiles: ", Tone::Bold);
                let name = output.style(&value_text(&payload["default_engine"]), Tone::Cyan);
                output.say(&format!("{label}{name}"), None)?;
                output.say(
                    &format!(
                        "Set it with:  numera default-engine <{}>",
                        ENGINES.join("|")
                    ),
                    Some(Tone::Dim),
                )?;
            }
        }
        "config" => {
            if let Some(error) = payload.get("error").filter(|_| payload["ok"] == false) {
                let message = if args
                    .get("default_engine")
                    .is_some_and(|value| !value.is_null())
                    && error
                        .as_str()
                        .is_some_and(|message| message.starts_with("unknown engine '"))
                {
                    format!("Unknown engine '{}'.", value_text(&args["default_engine"]))
                } else {
                    value_text(error)
                };
                output.say(&message, Some(Tone::Red))?;
                return Ok(true);
            }
            let changed = [
                ("default_engine", "default_engine"),
                ("headless_launch", "headless_launch"),
                ("default_extension", "default_extensions"),
            ]
            .into_iter()
            .filter(|(argument, _)| args.get(*argument).is_some_and(|value| !value.is_null()))
            .map(|(_, field)| field)
            .collect::<Vec<_>>();
            if !changed.is_empty() {
                output.say(
                    &format!("Updated: {}", changed.join(", ")),
                    Some(Tone::Green),
                )?;
            }
            output.say("Settings:", Some(Tone::Bold))?;
            for key in ["default_engine", "headless_launch", "root", "version"] {
                let label = output.style(&format!("{key}:"), Tone::Dim);
                output.say(
                    &format!("  {}{}", pad(&label, 28), value_text(&payload[key])),
                    None,
                )?;
            }
            let label = output.style("default_extensions:", Tone::Dim);
            let extensions = payload["default_extensions"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Settings extensions missing".into()))?;
            output.say(
                &format!(
                    "  {}{}",
                    pad(&label, 28),
                    if extensions.is_empty() {
                        "-".into()
                    } else {
                        extensions
                            .iter()
                            .map(value_text)
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                ),
                None,
            )?;
        }
        "folder" if payload.get("error").is_some() => {
            output.say(&value_text(&payload["error"]), Some(Tone::Red))?;
        }
        "folder" => match args
            .get("folder_command")
            .and_then(Value::as_str)
            .unwrap_or("")
        {
            "list" => {
                let folders = payload["folders"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Folder result missing".into()))?;
                for folder in folders {
                    output.say(&format!("  {}", value_text(folder)), None)?;
                }
                if folders.is_empty() {
                    output.say("No folders yet.", None)?;
                }
            }
            "create" => output.say(
                &format!("Created folder '{}'.", value_text(&args["name"])),
                Some(Tone::Green),
            )?,
            "rename" => output.say(
                &format!(
                    "Renamed '{}' to '{}'; its profiles moved with it.",
                    value_text(&args["old"]),
                    value_text(&args["new"])
                ),
                Some(Tone::Green),
            )?,
            "delete" => output.say(
                &format!(
                    "Deleted '{}'. Its profiles moved to '{}'.",
                    value_text(&args["name"]),
                    value_text(&payload["reassignedTo"])
                ),
                Some(Tone::Green),
            )?,
            _ => return Ok(false),
        },
        "vault" => match args
            .get("vault_command")
            .and_then(Value::as_str)
            .unwrap_or("")
        {
            "status" => {
                let state = if !truth(&payload["enabled"]) {
                    format!(
                        "{} — proxy passwords are stored in plaintext",
                        output.style("off", Tone::Dim)
                    )
                } else if truth(&payload["unlocked"]) {
                    output.style("unlocked", Tone::Green)
                } else {
                    format!(
                        "{} — set NUMERA_PASSWORD or pass --password",
                        output.style("locked", Tone::Yellow)
                    )
                };
                let label = output.style("Vault: ", Tone::Bold);
                output.say(&format!("{label}{state}"), None)?;
                if !truth(&payload["backend"]) {
                    output.say(
                        "  (install the backend with: pip install -e \".[secure]\")",
                        Some(Tone::Dim),
                    )?;
                }
            }
            "enable" => {
                output.say(
                    &format!(
                        "Vault enabled — encrypted {} secret(s) across the data directory.",
                        value_text(&payload["encrypted"])
                    ),
                    Some(Tone::Green),
                )?;
                output.say("Keep this password safe: without it, encrypted proxy passwords can't be recovered.",Some(Tone::Yellow))?;
                output.say("Set NUMERA_PASSWORD (or pass --password) when launching so profiles can use their proxy.",Some(Tone::Dim))?;
            }
            "change" => {
                if payload["ok"] == false {
                    output.say(&value_text(&payload["error"]), Some(Tone::Red))?;
                } else {
                    output.say(
                        &format!(
                            "Master password changed - re-encrypted {} secret(s).",
                            value_text(&payload["rotated"])
                        ),
                        Some(Tone::Green),
                    )?;
                    output.say(
                        "The old password no longer opens anything. Update NUMERA_PASSWORD.",
                        Some(Tone::Yellow),
                    )?;
                }
            }
            "resume" => {}
            _ => return Ok(false),
        },
        _ => return Ok(false),
    }
    Ok(true)
}
