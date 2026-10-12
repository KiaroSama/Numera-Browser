use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    storage::repo::RepoError,
};
use serde_json::{Map, Value};
use std::io::Write;

pub fn render(
    output: &mut Output<impl Write, impl Write>,
    args: &Map<String, Value>,
    payload: &Value,
    name: Option<&Value>,
) -> Result<bool, RepoError> {
    if output.json_mode() {
        return Ok(true);
    }
    if let Some(error) = payload.get("error").and_then(Value::as_str) {
        let text = if error.starts_with("no profile matching '")
            || error.starts_with("no usable cookies found in ")
        {
            format!("No{}.", &error[2..])
        } else {
            error.into()
        };
        output.say(&text, Some(Tone::Red))?;
        return Ok(true);
    }
    match args
        .get("cookies_command")
        .and_then(Value::as_str)
        .unwrap_or("")
    {
        "list" => {
            let domains = payload["domains"]
                .as_object()
                .ok_or_else(|| RepoError::Message("Cookie domains missing".into()))?;
            if payload["total"] == 0 {
                output.say(
                    &format!(
                        "'{}' has no cookies yet.",
                        value_text(name.ok_or_else(|| RepoError::Message(
                            "Cookie profile name missing".into()
                        ))?)
                    ),
                    Some(Tone::Dim),
                )?;
            } else {
                output.say(
                    &format!(
                        "{} cookie(s) across {} domain(s):",
                        value_text(&payload["total"]),
                        domains.len()
                    ),
                    Some(Tone::Bold),
                )?;
                let mut rows = domains.iter().collect::<Vec<_>>();
                rows.sort_by(|(_, left), (_, right)| right.as_u64().cmp(&left.as_u64()));
                for (domain, count) in rows {
                    let spaces = " ".repeat(40usize.saturating_sub(domain.chars().count()));
                    output.say(
                        &format!(
                            "  {domain}{spaces}{}",
                            output.style(&value_text(count), Tone::Dim)
                        ),
                        None,
                    )?;
                }
            }
        }
        "export" => {
            let text = output.style(
                &format!(
                    "Exported {} cookie(s) to {}  ",
                    value_text(&payload["count"]),
                    value_text(&args["file"])
                ),
                Tone::Green,
            );
            let format = output.style(&format!("({})", value_text(&payload["format"])), Tone::Dim);
            output.say(&format!("{text}{format}"), None)?;
        }
        "import" => {
            let text = output.style(
                &format!("Imported {} cookie(s).", value_text(&payload["imported"])),
                Tone::Green,
            );
            let note = if truth(&payload["cleared"]) {
                output.style("  (existing cookies were cleared first)", Tone::Dim)
            } else {
                String::new()
            };
            output.say(&format!("{text}{note}"), None)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}
