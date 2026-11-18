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
pub fn checked(
    output: &mut Output<impl Write, impl Write>,
    proxy: &Value,
    result: &Value,
) -> Result<(), RepoError> {
    let status = result["status"]
        .as_str()
        .ok_or_else(|| RepoError::Message("Proxy result status missing".into()))?;
    let tone = match status {
        "ok" => Tone::Green,
        "skipped" => Tone::Dim,
        _ => Tone::Red,
    };
    let detail = result
        .get("ip")
        .filter(|value| truth(value))
        .or_else(|| result.get("error").filter(|value| truth(value)))
        .map(value_text)
        .unwrap_or_default();
    output.say(
        &format!(
            "  {}:{}{}  {}",
            value_text(&proxy["host"]),
            pad(&value_text(&proxy["port"]), 8),
            output.style(status, tone),
            output.style(&detail, Tone::Dim)
        ),
        None,
    )?;
    Ok(())
}
pub fn rotated(
    output: &mut Output<impl Write, impl Write>,
    proxy: &Value,
    result: &Value,
) -> Result<(), RepoError> {
    let host = format!(
        "{}:{}",
        value_text(&proxy["host"]),
        value_text(&proxy["port"])
    );
    if truth(&result["rotated"]) {
        let previous = result
            .get("previousIp")
            .filter(|value| truth(value))
            .map(value_text)
            .unwrap_or("?".into());
        output.say(
            &format!("{host}  {previous} -> {}", value_text(&result["ip"])),
            Some(Tone::Green),
        )?;
    } else {
        let error = result
            .get("error")
            .filter(|value| truth(value))
            .map(value_text)
            .unwrap_or("did not move".into());
        output.say(&format!("{host}  {error}"), Some(Tone::Red))?;
    }
    Ok(())
}
pub fn assigned(
    output: &mut Output<impl Write, impl Write>,
    row: &Value,
    locale: Option<&str>,
) -> Result<(), RepoError> {
    let proxy = output.style(&value_text(&row["proxy"]), Tone::Dim);
    let note = locale
        .map(|locale| output.style(&format!("  -> {locale}"), Tone::Dim))
        .unwrap_or_default();
    output.say(
        &format!("  {}{proxy}{note}", pad(&value_text(&row["name"]), 24)),
        None,
    )?;
    Ok(())
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
        .get("pool_command")
        .and_then(Value::as_str)
        .unwrap_or("");
    if payload["ok"] == false {
        let error = value_text(&payload["error"]);
        let message = match error.as_str() {
            "nothing to change" => {
                "Nothing to change. Pass --host, --port, --user, --pass, --type, --country, --label or --rotate-url."
            }
            "no proxy with that id" => "No proxy with that id.",
            "no proxies could be parsed from that input" => {
                "No proxies could be parsed from that input."
            }
            "the proxy pool is empty" => "The proxy pool is empty.",
            "no profiles matched" => {
                "Nothing matched. Pass profile names/ids, --tag TAG, or --all."
            }
            "nothing to test" if truth(&args["all"]) => "The proxy pool is empty.",
            "nothing to test" => "No proxy with that id.",
            _ => &error,
        };
        output.say(message, Some(Tone::Red))?;
        return Ok(true);
    }
    match sub {
        "list" => {
            let rows = payload["proxies"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Proxy list missing".into()))?;
            if rows.is_empty() {
                output.say(
                    "The proxy pool is empty. Add some with:  numera pool add <file>",
                    Some(Tone::Dim),
                )?;
            } else {
                output.say(
                    &format!(
                        "{}{}{}{}STATUS",
                        pad("ID", 14),
                        pad("TYPE", 9),
                        pad("HOST", 26),
                        pad("CC", 5)
                    ),
                    Some(Tone::Bold),
                )?;
                for row in rows {
                    let kind = row.get("type").map(value_text).unwrap_or("HTTP".into());
                    let host = format!("{}:{}", value_text(&row["host"]), value_text(&row["port"]));
                    let country = if truth(&row["country"]) {
                        value_text(&row["country"])
                    } else {
                        "-".into()
                    };
                    let status = row
                        .get("status")
                        .map(value_text)
                        .unwrap_or("untested".into());
                    output.say(
                        &format!(
                            "{}{}{}{}{status}",
                            pad(&value_text(&row["id"]), 14),
                            pad(&kind, 9),
                            pad(&host, 26),
                            pad(&country, 5)
                        ),
                        None,
                    )?;
                }
                output.say(&format!("\n{} proxy/proxies.", rows.len()), Some(Tone::Dim))?;
            }
        }
        "add" => {
            let added = payload["added"]
                .as_u64()
                .ok_or_else(|| RepoError::Message("Proxy added count missing".into()))?;
            let parsed = payload["parsed"]
                .as_u64()
                .ok_or_else(|| RepoError::Message("Proxy parsed count missing".into()))?;
            let duplicate = parsed.checked_sub(added).ok_or_else(|| {
                RepoError::Message("Proxy added count exceeds parsed count".into())
            })?;
            let text = output.style(&format!("Added {added} proxy/proxies"), Tone::Green);
            let note = output.style(&format!(" ({duplicate} already in the pool)"), Tone::Dim);
            output.say(&format!("{text}{note}"), None)?;
        }
        "remove" => {
            let removed = payload["removed"]
                .as_u64()
                .ok_or_else(|| RepoError::Message("Removed proxy count missing".into()))?;
            if removed > 0 {
                output.say(
                    &format!("Removed {removed} proxy/proxies."),
                    Some(Tone::Green),
                )?;
            }
            for id in payload["missing"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Missing proxy IDs absent".into()))?
            {
                output.say(
                    &format!("No proxy with id {}.", value_text(id)),
                    Some(Tone::Red),
                )?;
            }
        }
        "archive" => {
            let restore = truth(&args["restore"]);
            let count = payload[if restore { "restored" } else { "archived" }]
                .as_u64()
                .ok_or_else(|| RepoError::Message("Archived proxy count missing".into()))?;
            if count == 0 {
                output.say("No proxy with that id.", Some(Tone::Red))?;
            } else {
                output.say(
                    &format!(
                        "{} {count} proxy/proxies.",
                        if restore { "Restored" } else { "Archived" }
                    ),
                    Some(Tone::Green),
                )?;
            }
        }
        "edit" => output.say(
            &format!(
                "Updated {}:{}.",
                value_text(&payload["proxy"]["host"]),
                value_text(&payload["proxy"]["port"])
            ),
            Some(Tone::Green),
        )?,
        "export" => {
            if truth(&args["file"]) {
                output.say(
                    &format!(
                        "Wrote {} proxy/proxies to {}.",
                        value_text(&payload["exported"]),
                        value_text(&args["file"])
                    ),
                    Some(Tone::Green),
                )?;
            } else {
                output.say(
                    payload["text"]
                        .as_str()
                        .ok_or_else(|| RepoError::Message("Proxy export text missing".into()))?
                        .trim_end_matches('\n'),
                    None,
                )?;
            }
        }
        "test" | "rotate" => {}
        "assign" => {
            output.say(
                &format!(
                    "Assigned {} proxy/proxies across {} profile(s).",
                    value_text(&payload["pool"]),
                    value_text(&payload["assigned"])
                ),
                Some(Tone::Green),
            )?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
