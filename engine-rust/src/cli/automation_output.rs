use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    storage::repo::RepoError,
};
use serde_json::{Map, Value};
use std::io::Write;

fn pad(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(text.chars().count()))
    )
}
fn local_stamp(value: &Value) -> Result<String, RepoError> {
    let seconds = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| RepoError::Message("Stored timestamp is not finite".into()))?;
    if seconds < i64::MIN as f64 || seconds >= i64::MAX as f64 {
        return Err(RepoError::Message(
            "Stored timestamp is outside the supported range".into(),
        ));
    }
    let instant = chrono::DateTime::from_timestamp(seconds.floor() as i64, 0).ok_or_else(|| {
        RepoError::Message("Stored timestamp is outside the supported range".into())
    })?;
    Ok(instant
        .with_timezone(&chrono::Local)
        .format("%Y-%m-%d %H:%M")
        .to_string())
}

pub fn render(
    output: &mut Output<impl Write, impl Write>,
    args: &Map<String, Value>,
    payload: &Value,
    now: f64,
    resolved_name: Option<&Value>,
) -> Result<bool, RepoError> {
    if output.json_mode() {
        return Ok(true);
    }
    match args.get("command").and_then(Value::as_str).unwrap_or("") {
        "token" => match args
            .get("token_command")
            .and_then(Value::as_str)
            .unwrap_or("")
        {
            "list" => {
                let rows = payload["tokens"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Token list missing".into()))?;
                if rows.is_empty() {
                    output.say(if truth(&payload["envToken"]) { "No named tokens. NUMERA_API_TOKEN is set, so the API still requires it." } else { "No named tokens. The API is open on this machine." }, Some(Tone::Dim))?;
                } else {
                    output.say(
                        &format!(
                            "{}{}{}LAST USED",
                            pad("ID", 14),
                            pad("NAME", 24),
                            pad("CREATED", 22)
                        ),
                        Some(Tone::Bold),
                    )?;
                    for row in rows {
                        let name = value_text(&row["name"])
                            .chars()
                            .take(23)
                            .collect::<String>();
                        let created = local_stamp(row.get("created").unwrap_or(&Value::from(0)))?;
                        let used = if truth(&row["lastUsed"]) {
                            local_stamp(&row["lastUsed"])?
                        } else {
                            "never".into()
                        };
                        output.say(
                            &format!(
                                "{}{}{}{used}",
                                pad(&value_text(&row["id"]), 14),
                                pad(&name, 24),
                                pad(&created, 22)
                            ),
                            None,
                        )?;
                    }
                }
            }
            "create" => {
                if payload["ok"] == false {
                    let error = value_text(&payload["error"]);
                    let mut chars = error.chars();
                    let first = chars
                        .next()
                        .map(|c| c.to_uppercase().collect::<String>())
                        .unwrap_or_default();
                    output.say(&format!("{first}{}.", chars.as_str()), Some(Tone::Red))?;
                } else {
                    output.say(&value_text(&payload["token"]), Some(Tone::Green))?;
                    output.say("Copy it now - only its hash is stored, so this is the only time it can be shown.", Some(Tone::Dim))?;
                }
            }
            "revoke" => output.say(
                if payload["ok"] == false {
                    "No token with that id."
                } else {
                    "Revoked."
                },
                Some(if payload["ok"] == false {
                    Tone::Red
                } else {
                    Tone::Green
                }),
            )?,
            _ => return Ok(false),
        },
        "schedule" => match args
            .get("schedule_command")
            .and_then(Value::as_str)
            .unwrap_or("")
        {
            "list" => {
                let rows = payload["schedules"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Schedule list missing".into()))?;
                if rows.is_empty() {
                    output.say("No warm-up schedules.", Some(Tone::Dim))?;
                }
                for row in rows {
                    let enabled = row.get("enabled").is_none_or(truth);
                    let state = output.style(
                        if enabled { "on" } else { "off" },
                        if enabled { Tone::Green } else { Tone::Dim },
                    );
                    let next = row["nextRun"]
                        .as_f64()
                        .ok_or_else(|| RepoError::Message("Schedule next run missing".into()))?;
                    let when = if next <= now {
                        "now".into()
                    } else {
                        local_stamp(&row["nextRun"])?
                    };
                    output.say(
                        &format!(
                            "  {}  {} every {}h  {}min  {} [{state}]  next: {when}",
                            value_text(&row["id"]),
                            pad(&value_text(&row["profileName"]), 20),
                            value_text(&row["everyHours"]),
                            value_text(&row["minutes"]),
                            pad(&value_text(&row["preset"]), 10)
                        ),
                        None,
                    )?;
                }
            }
            "add" if payload["ok"] == false => {
                output.say(
                    &format!("No profile matching '{}'.", value_text(&args["ref"])),
                    Some(Tone::Red),
                )?;
            }
            "add" => {
                output.say(
                    &format!(
                        "Scheduled '{}' — warm up every {}h for {}min ({}).",
                        value_text(resolved_name.ok_or_else(|| RepoError::Message(
                            "Scheduled profile name missing".into()
                        ))?),
                        value_text(&payload["everyHours"]),
                        value_text(&payload["minutes"]),
                        value_text(&payload["preset"])
                    ),
                    Some(Tone::Green),
                )?;
                output.say(
                    &format!("  id {}", value_text(&payload["id"])),
                    Some(Tone::Dim),
                )?;
            }
            "update" => {
                if payload["ok"] == false {
                    output.say(
                        if payload["error"] == "no changes given" {
                            "Nothing to change. Try --every, --minutes, --preset or --enabled."
                        } else {
                            "No schedule with that id."
                        },
                        Some(Tone::Red),
                    )?;
                } else {
                    output.say(
                        &format!(
                            "Updated schedule {}: every {}h, {}min, {}, {}.",
                            value_text(&payload["id"]),
                            value_text(&payload["everyHours"]),
                            value_text(&payload["minutes"]),
                            value_text(&payload["preset"]),
                            if payload.get("enabled").is_none_or(truth) {
                                "on"
                            } else {
                                "off"
                            }
                        ),
                        Some(Tone::Green),
                    )?;
                }
            }
            "remove" => output.say(
                if truth(&payload["ok"]) {
                    "Removed."
                } else {
                    "No schedule with that id."
                },
                Some(if truth(&payload["ok"]) {
                    Tone::Green
                } else {
                    Tone::Red
                }),
            )?,
            _ => return Ok(false),
        },
        _ => return Ok(false),
    }
    Ok(true)
}
