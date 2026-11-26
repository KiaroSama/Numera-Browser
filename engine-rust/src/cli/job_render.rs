use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    storage::repo::RepoError,
};
use serde_json::{Map, Value};
use std::io::Write;
fn width(text: &str, size: usize) -> String {
    format!(
        "{text}{}",
        " ".repeat(size.saturating_sub(text.chars().count()))
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
    let sub = args
        .get("jobs_command")
        .and_then(Value::as_str)
        .unwrap_or("");
    if payload["ok"] == false
        && payload["error"]
            .as_str()
            .is_some_and(|error| error.starts_with("no job with id"))
    {
        output.say(
            &format!("No job with id '{}'.", value_text(&args["id"])),
            Some(Tone::Red),
        )?;
        return Ok(true);
    }
    match sub {
        "list" => {
            let rows = payload["jobs"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Job list result missing".into()))?;
            if rows.is_empty() {
                output.say(
                    "No jobs. Start one by adding --detach to any command.",
                    Some(Tone::Dim),
                )?;
            } else {
                output.say(
                    &format!(
                        "{}{}{}PROFILE",
                        width("ID", 14),
                        width("STATE", 10),
                        width("COMMAND", 14)
                    ),
                    Some(Tone::Bold),
                )?;
                for row in rows {
                    let state = if truth(&row["orphaned"]) {
                        "orphaned".into()
                    } else {
                        value_text(&row["state"])
                    };
                    let tone = if truth(&row["running"]) {
                        Tone::Cyan
                    } else if row["exit"] == 0 {
                        Tone::Green
                    } else {
                        Tone::Yellow
                    };
                    let state = output.style(&state, tone);
                    output.say(
                        &format!(
                            "{}{}{}{}",
                            width(&value_text(&row["id"]), 14),
                            width(&state, 19),
                            width(
                                row.get("command").map(value_text).as_deref().unwrap_or(""),
                                14
                            ),
                            row.get("profile")
                                .filter(|value| truth(value))
                                .map(value_text)
                                .unwrap_or("-".into())
                        ),
                        None,
                    )?;
                }
            }
        }
        "clear" => output.say(
            &format!(
                "Cleared {} finished job(s).",
                value_text(&payload["cleared"])
            ),
            Some(Tone::Green),
        )?,
        "stop" => output.say(
            &format!(
                "Job {} is now {}.",
                value_text(&args["id"]),
                value_text(&payload["state"])
            ),
            Some(Tone::Green),
        )?,
        "show" => {
            let title = output.style(&format!("job {}  ", value_text(&payload["id"])), Tone::Bold);
            let state = output.style(&value_text(&payload["state"]), Tone::Cyan);
            output.say(&format!("{title}{state}"), None)?;
            let argv = payload
                .get("argv")
                .and_then(Value::as_array)
                .map(|values| values.iter().map(value_text).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            output.say(&format!("  command      numera {argv}"), None)?;
            output.say(
                &format!(
                    "  profile      {}",
                    payload
                        .get("profile")
                        .filter(|value| truth(value))
                        .map(value_text)
                        .unwrap_or("-".into())
                ),
                None,
            )?;
            output.say(
                &format!("  pid          {}", value_text(&payload["pid"])),
                None,
            )?;
            if !payload["exit"].is_null() {
                output.say(
                    &format!("  exit code    {}", value_text(&payload["exit"])),
                    None,
                )?;
            }
            if truth(&payload["orphaned"]) {
                output.say("  the process this job started has exited, but something it launched is still running",Some(Tone::Yellow))?;
            }
            if payload["verified"] == false {
                output.say("  Numera never captured a verified identity for this pid, so `jobs stop` will refuse it: end it by hand",Some(Tone::Yellow))?;
            }
            if let Some(lines) = payload
                .get("tail")
                .and_then(Value::as_array)
                .filter(|values| !values.is_empty())
            {
                output.say(
                    &format!(
                        "\n  last {} line(s) of {}:",
                        lines.len(),
                        value_text(&payload["log"])
                    ),
                    Some(Tone::Dim),
                )?;
                for line in lines {
                    output.say(&format!("    {}", value_text(line)), None)?;
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
