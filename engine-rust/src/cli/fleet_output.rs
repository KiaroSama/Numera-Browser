use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    storage::repo::RepoError,
};
use serde_json::Value;
use std::io::Write;

fn pad(text: &str, width: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(width.saturating_sub(text.chars().count()))
    )
}
pub fn render(
    output: &mut Output<impl Write, impl Write>,
    command: &str,
    payload: &Value,
) -> Result<bool, RepoError> {
    if output.json_mode() {
        return Ok(true);
    }
    match command {
        "health" => {
            let rows = payload["profiles"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Health profiles missing".into()))?;
            if rows.is_empty() {
                output.say("No profiles yet.", Some(Tone::Dim))?;
            } else {
                output.say(
                    &format!(
                        "{}{}{}{}{}NOTES",
                        pad("NAME", 22),
                        pad("STATUS", 10),
                        pad("COHERENT", 10),
                        pad("PROXY", 8),
                        pad("SCHED", 8)
                    ),
                    Some(Tone::Bold),
                )?;
                for row in rows {
                    let coherent = truth(&row["coherent"]);
                    let flag = output.style(
                        if coherent { "yes" } else { "no" },
                        if coherent { Tone::Green } else { Tone::Red },
                    );
                    let issues = row["issues"]
                        .as_array()
                        .ok_or_else(|| RepoError::Message("Health issues missing".into()))?;
                    let notes = if issues.is_empty() {
                        if truth(&row["collides"]) {
                            "shares a fingerprint".into()
                        } else {
                            String::new()
                        }
                    } else {
                        issues.iter().map(value_text).collect::<Vec<_>>().join("; ")
                    };
                    output.say(
                        &format!(
                            "{}{}{}{}{}{}",
                            pad(&value_text(&row["name"]), 22),
                            pad(&value_text(&row["status"]), 10),
                            pad(&flag, 19),
                            pad(if truth(&row["hasProxy"]) { "yes" } else { "-" }, 8),
                            pad(if truth(&row["schedule"]) { "yes" } else { "-" }, 8),
                            output.style(&notes, Tone::Dim)
                        ),
                        None,
                    )?;
                }
                output.say(
                    &format!(
                        "\n{} of {} need attention.",
                        value_text(&payload["summary"]["needsAttention"]),
                        value_text(&payload["summary"]["total"])
                    ),
                    Some(Tone::Dim),
                )?;
            }
        }
        "collisions" => {
            let groups = payload["groups"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Collision groups missing".into()))?;
            if groups.is_empty() {
                output.say(
                    &format!(
                        "No collisions — all {} profile(s) look unique.",
                        value_text(&payload["total"])
                    ),
                    Some(Tone::Green),
                )?;
            } else {
                let count = groups
                    .iter()
                    .map(|group| {
                        group["members"]
                            .as_array()
                            .map(Vec::len)
                            .ok_or_else(|| RepoError::Message("Collision members missing".into()))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .iter()
                    .sum::<usize>();
                output.say(
                    &format!(
                        "{count} of {} profiles share a fingerprint ({} group(s)):\n",
                        value_text(&payload["total"]),
                        groups.len()
                    ),
                    Some(Tone::Yellow),
                )?;
                for group in groups {
                    let names = group["members"]
                        .as_array()
                        .ok_or_else(|| RepoError::Message("Collision members missing".into()))?
                        .iter()
                        .map(|row| value_text(&row["name"]))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let tag = if truth(&group["identical"]) {
                        output.style(" IDENTICAL (same seed too)", Tone::Red)
                    } else {
                        String::new()
                    };
                    output.say(&format!("  • {names}{tag}"), Some(Tone::Bold))?;
                    let sig = &group["signals"];
                    output.say(
                        &format!(
                            "    {} · {} · {}x{} · {} cores / {}GB",
                            value_text(&sig["os"]),
                            value_text(&sig["webgl_renderer"]),
                            value_text(&sig["screen_width"]),
                            value_text(&sig["screen_height"]),
                            value_text(&sig["hardware_concurrency"]),
                            value_text(&sig["device_memory"])
                        ),
                        Some(Tone::Dim),
                    )?;
                }
                output.say(
                    "\nRegenerate one side of each pair:  numera regen <name>",
                    Some(Tone::Dim),
                )?;
            }
        }
        "bulk-create" => {
            let count = value_text(&payload["created"]);
            let skipped = &payload["skipped"];
            let text = output.style(&format!("Created {count} profile(s)"), Tone::Green);
            let note = if truth(skipped) {
                output.style(
                    &format!(", skipped {} name clash(es)", value_text(skipped)),
                    Tone::Yellow,
                )
            } else {
                String::new()
            };
            let engine = output.style(
                &format!("  (engine {})", value_text(&payload["engine"])),
                Tone::Dim,
            );
            output.say(&format!("{text}{note}{engine}"), None)?;
        }
        "import-from" => {
            if let Some(error) = payload.get("error").and_then(Value::as_str) {
                let message = if error == "no profiles found in that file" {
                    "No profiles found in that file.".into()
                } else if let Some(rest) = error.strip_prefix("couldn't read ") {
                    format!("Couldn't read {rest}")
                } else {
                    error.into()
                };
                output.say(&message, Some(Tone::Red))?;
            } else {
                let text = output.style(
                    &format!("Imported {} profile(s)  ", value_text(&payload["imported"])),
                    Tone::Green,
                );
                let note = output.style(
                    &format!(
                        "({} coherent out of the box)",
                        value_text(&payload["coherent"])
                    ),
                    Tone::Dim,
                );
                output.say(&format!("{text}{note}"), None)?;
                for row in payload["results"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Import results missing".into()))?
                {
                    let issues = row["issues"]
                        .as_array()
                        .ok_or_else(|| RepoError::Message("Import issues missing".into()))?;
                    if !issues.is_empty() {
                        let issues = output.style(
                            &format!(
                                "review: {}",
                                issues.iter().map(value_text).collect::<Vec<_>>().join("; ")
                            ),
                            Tone::Yellow,
                        );
                        output.say(
                            &format!("  {}{issues}", pad(&value_text(&row["name"]), 24)),
                            None,
                        )?;
                    }
                }
                let skipped = payload["skipped"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("Import skipped names missing".into()))?;
                if !skipped.is_empty() {
                    let mut names = skipped.iter().map(value_text).collect::<Vec<_>>();
                    names.sort();
                    names.dedup();
                    names.truncate(10);
                    output.say(
                        &format!(
                            "Skipped {} - those names are already in use: {}",
                            skipped.len(),
                            names.join(", ")
                        ),
                        Some(Tone::Yellow),
                    )?;
                }
                output.say(
                    "Run `numera check <name>` on any flagged profile to see details.",
                    Some(Tone::Dim),
                )?;
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
