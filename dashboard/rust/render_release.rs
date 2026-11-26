use super::*;
use crate::release_controls::{PREFERENCES, can_apply, preferences};

pub(super) fn country_badge(value: &Value) -> String {
    let code = s(value, "country_code");
    if code.len() != 2 || !code.bytes().all(|b| b.is_ascii_uppercase()) {
        return r#"<span class="ps-country-badge" title="Unknown country"><span aria-hidden="true">--</span><span class="ps-sr-only">Unknown country</span></span>"#.into();
    }
    let expected: String = code
        .bytes()
        .filter_map(|b| char::from_u32(0x1f1e6 + u32::from(b - b'A')))
        .collect();
    let flag = if s(value, "country_flag") == expected {
        expected.as_str()
    } else {
        ""
    };
    let country = s(value, "country_name");
    let name = if country.is_empty() {
        code.into()
    } else {
        format!("{country} ({code})")
    };
    format!(
        r#"<span class="ps-country-badge" title="{}"><span aria-hidden="true">{flag}</span><span class="ps-mono" aria-hidden="true">{code}</span><span class="ps-sr-only">{}</span></span>"#,
        escape(&name),
        escape(&name)
    )
}

pub(super) fn ip_readiness(value: &Value) -> String {
    let (label, class) = match value["ready"].as_bool() {
        Some(true) => ("IP check: ready", "ok"),
        Some(false) => ("IP check: review", "warn"),
        None => return String::new(),
    };
    let detail = [
        s(value, "ip"),
        s(value, "timezone"),
        s(value, "webrtc_policy"),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" · ");
    format!(
        r#"<div class="ps-readiness" role="status"><span class="ps-pill {class}">{label}</span> {} <small>{}</small></div>"#,
        country_badge(value),
        escape(&detail)
    )
}

pub(super) fn update_controls(a: &App) -> String {
    let mut out = String::from(
        r#"<section class="ps-set-card" aria-labelledby="release-title"><h2 class="ps-set-h" id="release-title">App and browser core updates</h2>"#,
    );
    for (channel, label) in [
        ("app", "Check for app updates"),
        ("browsers", "Check for browser updates"),
        ("all", "Check for App + Browsers updates"),
    ] {
        out.push_str(&button(
            "release-check",
            channel,
            label,
            "ps-btn ghost sm",
            a.busy || !a.live(),
        ));
    }
    if !a.live() {
        out.push_str(&hint(
            "Connect the engine to check for updates or change preferences.",
        ));
    }
    let channel = s(&a.release_status, "channel");
    if !channel.is_empty() {
        out.push_str(r#"<div role="status" class="ps-release-result">"#);
        if channel != "browsers" {
            out.push_str(&version_status("App", &a.release_status["app"]));
            if a.release_status["app"]["update_available"] == true
                && a.release_status["app"]["verified"] != true
            {
                out.push_str(&hint("The engine has not verified this app update."));
            }
            let notes = s(&a.release_status["app"], "notes");
            if !notes.is_empty() {
                out.push_str(&hint(notes));
            }
        }
        if channel != "app" {
            let cores = list(&a.release_status["browsers"]);
            if cores.is_empty() {
                out.push_str(&hint("The engine reported no browser cores."));
            }
            for core in cores {
                let id = s(&core, "id");
                let label = match id {
                    "chromium" => "Chromium",
                    "chromium-patchright" => "Chromium (Patchright)",
                    "camoufox" => "Camoufox",
                    _ => id,
                };
                out.push_str(&version_status(label, &core));
            }
        }
        if a.live() && can_apply(&a.release_status, channel) {
            let label = match channel {
                "all" => "Review App + Browsers update",
                "app" => "Review app update",
                _ => "Review browser updates",
            };
            if a.update_bridge.is_some() && a.mcp_bridge_command.is_some() {
                out.push_str(&button(
                    "release-apply",
                    channel,
                    label,
                    "ps-btn primary sm",
                    a.busy || !preferences(&a.release_preferences),
                ));
            } else {
                out.push_str(&hint("Open the desktop app to apply this update."));
            }
        }
        out.push_str("</div>");
    }
    if a.release_preferences_error {
        out.push_str(&hint("Could not load update preferences."));
        out.push_str(&button(
            "release-retry",
            "",
            "Retry preferences",
            "ps-btn ghost sm",
            !a.live() || a.busy,
        ));
    } else if a.live() && a.release_preferences.is_null() {
        out.push_str(&hint("Loading update preferences…"));
    }
    out.push_str(r#"<div class="ps-release-preferences">"#);
    let desktop = a.update_bridge.is_some() && a.mcp_bridge_command.is_some();
    if a.live() && !desktop {
        out.push_str(&hint("Open the desktop app to enable automatic updates."));
    }
    for (key, label) in PREFERENCES {
        let on = a.release_preferences[key] == true;
        let needs_approval = key != "clean_old_versions_on_update" && !on
            || PREFERENCES[..2]
                .iter()
                .any(|(other, _)| *other != key && a.release_preferences[other] == true);
        out.push_str(&toggle(
            "release-preference",
            key,
            label,
            on,
            !a.live()
                || a.busy
                || !preferences(&a.release_preferences)
                || (!desktop && needs_approval),
        ));
    }
    out.push_str("</div></section>");
    out
}

fn version_status(label: &str, value: &Value) -> String {
    let version = if s(value, "current").is_empty() {
        "unknown"
    } else {
        s(value, "current")
    };
    let status = if value["update_available"] == true {
        format!("{} available", s(value, "latest"))
    } else {
        "No update reported".into()
    };
    hint(&format!("{label}: {version} installed. {status}."))
}

pub(super) fn bridge_status(a: &App) -> String {
    let mut o = String::from(
        r#"<section class="ps-set-card" aria-labelledby="mcp-title"><h2 class="ps-set-h" id="mcp-title">MCP bridge</h2>"#,
    );
    if let Some(command) = a.mcp_bridge_command.as_ref().filter(|_| a.live()) {
        o.push_str(&hint("Local engine address. The bridge requires a bearer token; this command does not include it."));
        let _ = write!(o, r#"<pre class="ps-code">{}</pre>"#, escape(command));
        o.push_str(&button(
            "mcp-copy",
            "",
            "Copy bridge command",
            "ps-btn ghost sm",
            a.busy,
        ));
        o.push_str(&button(
            "mcp-check",
            "",
            "Check bridge connection",
            "ps-btn ghost sm",
            a.busy,
        ));
    } else {
        o.push_str(&hint(
            "Connect to the local engine to show the bridge command.",
        ));
    }
    o.push_str("</section>");
    o
}
