use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    models::Profile,
    storage::repo::RepoError,
};
use serde_json::Value;
use std::io::{self, Write};

fn width(text: &str, size: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(size.saturating_sub(text.chars().count()))
    )
}
fn styled(output: &Output<impl Write, impl Write>, text: &str, tone: Tone) -> String {
    output.style(text, tone)
}
pub fn summary(
    output: &mut Output<impl Write, impl Write>,
    profile: &Profile,
    full: bool,
) -> io::Result<()> {
    let fp = &profile.fingerprint;
    let mut rows = vec![
        ("id", profile.id.clone()),
        ("engine", profile.engine.clone()),
        (
            "os",
            format!("{}{}", fp.os, if fp.is_mobile { " (mobile)" } else { "" }),
        ),
        ("chrome", fp.chrome_version.clone()),
        (
            "screen",
            format!("{}x{}", fp.screen_width, fp.screen_height),
        ),
        (
            "cpu / ram",
            format!(
                "{} cores / {} GB",
                fp.hardware_concurrency,
                value_text(&fp.device_memory)
            ),
        ),
        ("gpu", fp.webgl_renderer.clone()),
        ("locale / tz", format!("{}  {}", fp.locale, fp.timezone)),
    ];
    if full {
        rows.insert(2, ("user-agent", fp.user_agent.clone()));
        rows.extend([
            ("languages", fp.languages.join(", ")),
            (
                "proxy",
                profile
                    .proxy
                    .as_ref()
                    .map_or("-".into(), |proxy| proxy.server.clone()),
            ),
            (
                "tags",
                if profile.tags.is_empty() {
                    "-".into()
                } else {
                    profile.tags.join(", ")
                },
            ),
            (
                "notes",
                if truth(&profile.notes) {
                    value_text(&profile.notes)
                } else {
                    "-".into()
                },
            ),
            ("seed", fp.seed.to_string()),
        ]);
    }
    for (key, value) in rows {
        let key = styled(output, &format!("{key}:"), Tone::Dim);
        output.say(&format!("  {}{value}", width(&key, 24)), None)?;
    }
    Ok(())
}
pub fn engine_notice(output: &mut Output<impl Write, impl Write>, engine: &str) -> io::Result<()> {
    output.say(
        &format!("Note: engine '{engine}' isn't installed yet; install it before launching."),
        Some(Tone::Yellow),
    )
}
pub fn created(
    output: &mut Output<impl Write, impl Write>,
    payload: &Value,
) -> Result<(), RepoError> {
    let profile = Profile::from_record(&payload["profile"]).map_err(io::Error::other)?;
    let text = styled(
        output,
        &format!("Created profile '{}'  ", value_text(&profile.name)),
        Tone::Green,
    );
    let id = styled(output, &format!("[{}]", profile.id), Tone::Dim);
    output.say(&format!("{text}{id}"), None)?;
    summary(output, &profile, false)?;
    Ok(())
}
/// Render only the implemented profile commands; other command families supply
/// their own human narration rather than printing a generic JSON fallback.
pub fn render(
    output: &mut Output<impl Write, impl Write>,
    command: &str,
    payload: &Value,
) -> Result<bool, RepoError> {
    if output.json_mode() {
        return Ok(true);
    }
    if let Some(error) = payload.get("error").and_then(Value::as_str)
        && error.starts_with("no profile matching '")
    {
        let message = format!("No{}.", &error[2..]);
        output.say(&message, Some(Tone::Red))?;
        return Ok(true);
    }
    match command {
        "list" => {
            let rows = payload["profiles"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Profile list result missing".into()))?;
            if rows.is_empty() {
                output.say(
                    "No profiles yet. Create one with:  numera create <name>",
                    Some(Tone::Dim),
                )?;
            } else {
                let header = format!(
                    "{}{}{}{}PROXY",
                    width("ID", 14),
                    width("NAME", 22),
                    width("OS", 9),
                    width("LOCALE", 9)
                );
                output.say(&header, Some(Tone::Bold))?;
                for row in rows {
                    let name = row["name"].as_str().ok_or_else(|| {
                        RepoError::Message("Stored profile name is not text".into())
                    })?;
                    let proxy = row["proxy"]
                        .as_str()
                        .filter(|value| !value.is_empty())
                        .unwrap_or("-");
                    output.say(
                        &format!(
                            "{}{}{}{}{proxy}",
                            width(&styled(output, &value_text(&row["id"]), Tone::Dim), 23),
                            width(name, 22),
                            width(&value_text(&row["os"]), 9),
                            width(&value_text(&row["locale"]), 9)
                        ),
                        None,
                    )?;
                }
                output.say(&format!("\n{} profile(s).", rows.len()), Some(Tone::Dim))?;
            }
        }
        "show" => {
            let profile = Profile::from_record(payload).map_err(io::Error::other)?;
            summary(output, &profile, true)?;
        }
        "create" => {
            if payload["engineInstalled"] == false {
                engine_notice(
                    output,
                    payload["profile"]["engine"].as_str().ok_or_else(|| {
                        RepoError::Message("Created profile engine missing".into())
                    })?,
                )?;
            }
            created(output, payload)?;
        }
        "regen" => {
            let profile = Profile::from_record(&payload["profile"]).map_err(io::Error::other)?;
            output.say(
                &format!(
                    "Regenerated fingerprint for '{}'.",
                    value_text(&profile.name)
                ),
                Some(Tone::Green),
            )?;
            summary(output, &profile, false)?;
        }
        "edit" => {
            output.say(
                &format!("Updated '{}'.", value_text(&payload["profile"]["name"])),
                Some(Tone::Green),
            )?;
        }
        "import" | "restore" => {
            let profile = &payload["profile"];
            let verb = if command == "import" {
                "Imported"
            } else {
                "Restored"
            };
            let text = styled(
                output,
                &format!("{verb} '{}'  ", value_text(&profile["name"])),
                Tone::Green,
            );
            let id = styled(
                output,
                &format!("[{}]", value_text(&profile["id"])),
                Tone::Dim,
            );
            output.say(&format!("{text}{id}"), None)?;
        }
        "export" => {
            output.say(
                &format!("Exported to {}", value_text(&payload["file"])),
                Some(Tone::Green),
            )?;
        }
        "backup" => {
            output.say(
                &format!(
                    "Backed up '{}' (profile + session) to {}",
                    value_text(&payload["name"]),
                    value_text(&payload["file"])
                ),
                Some(Tone::Green),
            )?;
            output.say(
                "  keep it safe — the archive holds the proxy password in plaintext.",
                Some(Tone::Dim),
            )?;
        }
        "check" => {
            if payload["ok"] == true {
                output.say(
                    &format!(
                        "'{}' fingerprint is coherent.",
                        value_text(&payload["name"])
                    ),
                    Some(Tone::Green),
                )?;
            } else {
                let issues = payload["issues"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Profile issues missing".into()))?;
                output.say(
                    &format!(
                        "'{}' has {} issue(s):",
                        value_text(&payload["name"]),
                        issues.len()
                    ),
                    Some(Tone::Red),
                )?;
                for issue in issues {
                    output.say(&format!("  - {}", value_text(issue)), None)?;
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
