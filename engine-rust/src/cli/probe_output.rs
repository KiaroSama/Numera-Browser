use super::output::{Output, Tone};
use crate::{
    conversion::{truth, value_text},
    storage::repo::RepoError,
};
use serde_json::Value;
use std::io::{self, Write};

/// Called after profile resolution and before the browser operation.
pub fn start(
    output: &mut Output<impl Write, impl Write>,
    operation: &str,
    name: &Value,
    proxy: Option<&str>,
) -> io::Result<bool> {
    let name = value_text(name);
    let text = match operation {
        "tls" => format!("Reading '{name}' TLS/HTTP2 handshake..."),
        "dns" => format!("Checking '{name}' for DNS leaks..."),
        "proxy" => format!(
            "Testing '{name}' via {}...",
            proxy.unwrap_or("no proxy (direct connection)")
        ),
        "trust" => format!("Inspecting '{name}' the way a fingerprinting script would..."),
        "align" => format!("Aligning '{name}' to what its browser actually presents..."),
        _ => return Ok(false),
    };
    output.say(&text, Some(Tone::Cyan))?;
    Ok(true)
}

pub fn checks(output: &mut Output<impl Write, impl Write>, rows: &Value) -> io::Result<()> {
    let rows = rows
        .as_array()
        .ok_or_else(|| io::Error::other("Probe checks missing"))?;
    for row in rows {
        let ok = truth(&row["ok"]);
        let mark = output.style(
            if ok { "PASS" } else { "FAIL" },
            if ok { Tone::Green } else { Tone::Red },
        );
        output.say(&format!("  [{mark}] {}", value_text(&row["name"])), None)?;
        if !ok && row.get("detail").is_some_and(truth) {
            let detail = output.style(&value_text(&row["detail"]), Tone::Dim);
            output.say(&format!("         {detail}"), None)?;
        }
    }
    Ok(())
}

fn present(value: &Value) -> String {
    if truth(value) {
        value_text(value)
    } else {
        "-".into()
    }
}
fn grade(report: &Value) -> Result<Tone, RepoError> {
    let score = report["score"]
        .as_f64()
        .ok_or_else(|| RepoError::Message("Trust score missing".into()))?;
    Ok(if score >= 95.0 {
        Tone::Green
    } else if score >= 70.0 {
        Tone::Yellow
    } else {
        Tone::Red
    })
}

/// Result narration is separate from startup so failures cannot invent a
/// successful browser operation or move its progress after completion.
pub fn render(
    output: &mut Output<impl Write, impl Write>,
    operation: &str,
    reference: &str,
    payload: &Value,
) -> Result<bool, RepoError> {
    if !matches!(operation, "tls" | "dns" | "proxy" | "trust" | "align") {
        return Ok(false);
    }
    if output.json_mode() {
        return Ok(true);
    }
    if payload["error"] == format!("no profile matching '{reference}'") {
        output.say(
            &format!("No profile matching '{reference}'."),
            Some(Tone::Red),
        )?;
        return Ok(true);
    }
    if matches!(operation, "tls" | "dns" | "proxy") && truth(&payload["error"]) {
        output.say(
            &format!("Failed: {}", value_text(&payload["error"])),
            Some(Tone::Red),
        )?;
        return Ok(true);
    }
    match operation {
        "tls" => {
            for (label, key) in [("JA3 hash", "ja3Hash"), ("JA4", "ja4"), ("HTTP/2", "http2")] {
                output.say(&format!("  {label:<13}{}", present(&payload[key])), None)?;
            }
            checks(output, &payload["checks"])?;
            output.say(
                "\n  Tip: run this on two engines and compare — TLS is the one layer JS can't touch.",
                Some(Tone::Dim),
            )?;
        }
        "dns" => {
            if truth(&payload["countries"]) {
                let countries = payload["countries"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("DNS countries missing".into()))?;
                let resolvers = payload["resolvers"]
                    .as_array()
                    .ok_or_else(|| RepoError::Message("DNS resolvers missing".into()))?;
                output.say(
                    &format!(
                        "  resolvers    {} in {}",
                        resolvers.len(),
                        countries
                            .iter()
                            .map(value_text)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    None,
                )?;
            }
            checks(output, &payload["checks"])?;
        }
        "proxy" => {
            let location = if truth(&payload["city"]) {
                format!(
                    "  ({}, {})",
                    value_text(&payload["city"]),
                    value_text(&payload["country"])
                )
            } else {
                String::new()
            };
            output.say(
                &format!("  exit IP      {}{location}", value_text(&payload["ip"])),
                None,
            )?;
            output.say(
                &format!("  round trip   {} ms", value_text(&payload["latencyMs"])),
                None,
            )?;
            checks(output, &payload["checks"])?;
        }
        "trust" => {
            output.say(
                &format!(
                    "\n  Trust score: {}%  (grade {}, {}/{} checks)\n",
                    value_text(&payload["score"]),
                    value_text(&payload["grade"]),
                    value_text(&payload["passed"]),
                    value_text(&payload["total"])
                ),
                Some(grade(payload)?),
            )?;
            checks(output, &payload["checks"])?;
        }
        "align" => {
            let before = &payload["before"];
            let after = &payload["after"];
            output.say(
                &format!(
                    "\n  grade {} ({}%)  ->  grade {} ({}%)   {}/{} checks\n",
                    value_text(&before["grade"]),
                    value_text(&before["score"]),
                    value_text(&after["grade"]),
                    value_text(&after["score"]),
                    value_text(&after["passed"]),
                    value_text(&after["total"])
                ),
                Some(grade(after)?),
            )?;
            let changed = payload["changed"]
                .as_array()
                .ok_or_else(|| RepoError::Message("Alignment fields missing".into()))?;
            if changed.is_empty() {
                output.say(
                    "  Nothing to change — the profile already matches its browser.",
                    Some(Tone::Dim),
                )?;
            } else {
                let label = output.style("  Adjusted: ", Tone::Bold);
                let fields = output.style(
                    &changed
                        .iter()
                        .map(value_text)
                        .collect::<Vec<_>>()
                        .join(", "),
                    Tone::Dim,
                );
                output.say(&format!("{label}{fields}"), None)?;
                output.say(
                    &format!("  Identity now reads as: {}", value_text(&payload["os"])),
                    Some(Tone::Dim),
                )?;
            }
            output.say("", None)?;
            checks(output, &payload["checks"])?;
        }
        _ => unreachable!(),
    }
    Ok(true)
}
