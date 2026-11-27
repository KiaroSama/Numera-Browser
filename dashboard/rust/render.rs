use crate::model::*;
use serde_json::{Value, json};
use std::fmt::Write;

pub fn escape(v: &str) -> String {
    v.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn e(v: &Value) -> String {
    escape(&text(v))
}
pub fn icon(name: &str) -> String {
    icon_at(name, 16)
}
fn icon_at(name: &str, size: u8) -> String {
    let path = match name {
        "profiles" => "M12 3 2 8l10 5 10-5-10-5ZM2 12l10 5 10-5M2 16l10 5 10-5",
        "health" => "M2 12h4l3-9 6 18 3-9h4",
        "engines" => "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z",
        "extensions" => {
            "M19 13h-2a2 2 0 1 1 0-4h2V5h-6V3a2 2 0 1 0-4 0v2H5v4H3a2 2 0 1 0 0 4h2v6h4v2a2 2 0 1 0 4 0v-2h6z"
        }
        "folders" => "M3 7V3h6l2 2h10v14H3V7Z",
        "folder-tree" => "M3 3v14a2 2 0 0 0 2 2h4M3 7h6m0-3h4l2 2h6v6H9zm0 11h4l2 2h6v5H9z",
        "boxes" => {
            "m12 3 4 2.3v4.5L12 12l-4-2.2V5.3zm0 9 4.1-2.3 4 2.3v4.5l-4 2.3-4.1-2.3zm0 0-4.1-2.3-4 2.3v4.5l4 2.3 4.1-2.3zM8 5.3l4 2.2 4-2.2M12 7.5V12m-8 0 4 2.2 4-2.2m-4 2.2v4.5m4-6.7 4 2.2 4-2.2m-4 2.2v4.5"
        }
        "server" => {
            "M4 3h16a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1zm0 11h16a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-5a1 1 0 0 1 1-1zM7 6.5h.01M7 17.5h.01"
        }
        "automation" => "m13 2-9 12h7l-1 8 10-12h-7l1-8Z",
        "settings" => {
            "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8ZM9 3h6l1 3 3 1 2 5-2 5-3 1-1 3H9l-1-3-3-1-2-5 2-5 3-1 1-3Z"
        }
        "plus" => "M12 5v14M5 12h14",
        "x" => "m6 6 12 12M6 18 18 6",
        "edit" => "m15 5 4 4M4 20l4-1L20 7a3 3 0 0 0-4-4L4 15v5Z",
        "clone" => "M9 9h12v12H9zM5 15H3V3h12v2",
        "delete" => "M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7",
        "run" => "m7 4 14 8-14 8V4Z",
        "stop" => "M5 5h14v14H5z",
        "check" => "m5 12 4 4L19 6",
        "shield" => "M12 3 3 6v6c0 5 9 9 9 9s9-4 9-9V6l-9-3Zm-5 9 3 3 7-7",
        "search" => "M10 3a7 7 0 1 0 0 14 7 7 0 0 0 0-14Zm5 12 6 6",
        "command" => "M8 5a3 3 0 1 0-3 3h14a3 3 0 1 0-3-3v14a3 3 0 1 0 3-3H5a3 3 0 1 0 3 3z",
        "download" => "M12 3v12m-5-5 5 5 5-5M3 15v6h18v-6",
        "filter" => "M3 4h18l-7 8v8l-4-2v-6L3 4Z",
        "theme" => {
            "M12 3v2m0 14v2M3 12h2m14 0h2M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z"
        }
        "moon" => "M20.9 13.3A9 9 0 0 1 10.7 3.1 9 9 0 1 0 20.9 13.3Z",
        "wifi" => {
            "M2 8.8a16 16 0 0 1 20 0M5 12.8a11 11 0 0 1 14 0m-11 4a6.5 6.5 0 0 1 8 0M12 20h.01"
        }
        "wifi-off" => {
            "m3 3 18 18M2 9a16 16 0 0 1 2-1.4m4-1.7A16 16 0 0 1 22 9M5 13a11 11 0 0 1 3.1-1.6m8-.4A11 11 0 0 1 19 13M8 17a6 6 0 0 1 6.5-1M12 20h.01"
        }
        "refresh" => "M20 7a8 8 0 1 0 0 10M20 3v5h-5",
        "key" => "M8 3a5 5 0 1 0 0 10 5 5 0 0 0 0-10Zm4 9 9 9m-4-4 3-3",
        "proxies" => {
            "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20ZM2 12h20M12 2c-5 6-5 14 0 20 5-6 5-14 0-20Z"
        }
        _ => "M3 5h18v14H3zM7 9h10M7 13h6",
    };
    format!(
        r#"<svg width="{size}" height="{size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="{path}"/></svg>"#
    )
}
pub fn button(action: &str, id: &str, label: &str, class: &str, disabled: bool) -> String {
    let accessible = match action {
        "commands" => "Command palette (⌘K / Ctrl-K)",
        "theme" if label.starts_with("<svg") => "Switch theme",
        "close-editor" if label.starts_with("<svg") => "Close editor",
        _ => "",
    };
    format!(
        r#"<button type="button" class="{}" data-action="{}" data-id="{}" {} {}>{}</button>"#,
        escape(class),
        escape(action),
        escape(id),
        if disabled { "disabled" } else { "" },
        if accessible.is_empty() {
            String::new()
        } else {
            format!(
                "title=\"{}\" aria-label=\"{}\"",
                escape(accessible),
                escape(accessible)
            )
        },
        label
    )
}
fn btn(action: &str, id: &str, label: &str) -> String {
    button(action, id, &escape(label), "ps-btn ghost sm", false)
}
fn primary(action: &str, label: &str, disabled: bool) -> String {
    button(action, "", &escape(label), "ps-btn primary", disabled)
}
fn action(action: &str, id: &str, title: &str, ic: &str, disabled: bool) -> String {
    let class = if title == "Launch" || action.starts_with("health-") || action == "test-proxy" {
        "run"
    } else if title == "Stop" {
        "stop"
    } else if title == "Delete" || title == "Revoke" {
        "del"
    } else {
        ""
    };
    format!(
        r#"<button type="button" class="{class}" data-action="{action}" data-id="{}" title="{}" aria-label="{}" {}>{}</button>"#,
        escape(id),
        escape(title),
        escape(title),
        if disabled { "disabled" } else { "" },
        icon_at(ic, 13)
    )
}
pub fn input(
    scope: &str,
    key: &str,
    label: &str,
    value: &Value,
    kind: &str,
    placeholder: &str,
) -> String {
    let id = format!("{scope}-{}", key.replace('.', "-"));
    format!(
        r#"<div class="ps-field"><label class="ps-flabel" for="{id}">{}</label><input id="{id}" class="ps-in {}" type="{}" data-scope="{scope}" data-field="{key}" value="{}" placeholder="{}"></div>"#,
        escape(label),
        if kind == "password" { "mono" } else { "" },
        kind,
        e(value),
        escape(placeholder)
    )
}
pub fn input_with_name(
    scope: &str,
    key: &str,
    label: &str,
    value: &Value,
    kind: &str,
    placeholder: &str,
    own_name: &str,
) -> String {
    input(scope, key, label, value, kind, placeholder).replacen(
        "<input ",
        &format!("<input aria-label=\"{}\" ", escape(own_name)),
        1,
    )
}
pub fn textarea(
    scope: &str,
    key: &str,
    label: &str,
    value: &Value,
    rows: u8,
    placeholder: &str,
) -> String {
    let id = format!("{scope}-{}", key.replace('.', "-"));
    format!(
        r#"<div class="ps-field"><label class="ps-flabel" for="{id}">{}</label><textarea id="{id}" class="ps-in" data-scope="{scope}" data-field="{key}" rows="{rows}" placeholder="{}">{}</textarea></div>"#,
        escape(label),
        escape(placeholder),
        e(value)
    )
}
pub fn select(
    scope: &str,
    key: &str,
    label: &str,
    value: &Value,
    options: Vec<(String, String)>,
    disabled: bool,
) -> String {
    let id = format!("{scope}-{}", key.replace('.', "-"));
    let mut out = format!(
        r#"<div class="ps-field"><label class="ps-flabel" for="{id}">{}</label><select id="{id}" class="ps-in" data-scope="{scope}" data-field="{key}" {}>"#,
        escape(label),
        if disabled { "disabled" } else { "" }
    );
    for (v, l) in options {
        let _ = write!(
            out,
            r#"<option value="{}" {}>{}</option>"#,
            escape(&v),
            if text(value) == v { "selected" } else { "" },
            escape(&l)
        );
    }
    out.push_str("</select></div>");
    out
}
fn opts(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|v| (v.to_string(), v.to_string()))
        .collect()
}
fn current_options(current: &Value, options: Vec<(String, String)>) -> Vec<(String, String)> {
    let value = text(current);
    let mut result = vec![(value.clone(), value)];
    for (v, l) in options {
        if !result.iter().any(|(have, _)| have == &v) {
            result.push((v, l));
        }
    }
    result
}
fn engine_opts(empty: &str) -> Vec<(String, String)> {
    std::iter::once((String::new(), empty.into()))
        .chain(ENGINES.iter().map(|(v, l)| (v.to_string(), l.to_string())))
        .collect()
}
fn section(name: &str) -> String {
    format!(
        r#"<div class="ps-seclabel"><span>{}</span><div class="ps-secline"></div></div>"#,
        escape(name)
    )
}
fn hint(text: &str) -> String {
    format!(r#"<div class="ps-hint">{}</div>"#, escape(text))
}
fn head(title: &str, sub: &str) -> String {
    format!(
        r#"<div class="ps-view-head"><div><h2 class="ps-view-title">{}</h2><p class="ps-view-sub">{}</p></div></div>"#,
        escape(title),
        escape(sub)
    )
}
fn seg(scope: &str, key: &str, label: &str, value: &str, options: &[&str]) -> String {
    let mut out = format!(
        r#"<div class="ps-field" role="group" aria-label="{}"><span class="ps-flabel">{}</span><div class="ps-seg wrap">"#,
        escape(label),
        escape(label)
    );
    for opt in options {
        out.push_str(&button(
            "pick",
            &format!("{scope}|{key}|{opt}"),
            &escape(opt),
            if value == *opt { "on" } else { "" },
            false,
        ));
    }
    out.push_str("</div></div>");
    out
}
fn toggle(action: &str, id: &str, label: &str, on: bool, disabled: bool) -> String {
    button(
        action,
        id,
        &format!(
            r#"<span class="knob" aria-hidden="true"></span><em>{}</em>"#,
            escape(label)
        ),
        if on { "ps-toggle on" } else { "ps-toggle" },
        disabled,
    )
    .replacen("<button ", &format!("<button aria-pressed=\"{on}\" "), 1)
}
fn row(content: String) -> String {
    format!(r#"<div class="ps-row">{content}</div>"#)
}
pub fn app(a: &App) -> String {
    if a.mode == "startup-error" {
        return format!(
            r#"<div class="ps-root"><div class="ps-locked"><div class="ps-locked-card"><h1>The Numera engine did not start.</h1><pre class="ps-code" role="alert">{}</pre><button type="button" id="retry-startup" class="ps-btn ghost" data-action="retry-startup" {}>{}</button></div></div></div>"#,
            escape(&a.auth_error),
            if a.busy { "disabled" } else { "" },
            if a.busy { "Connecting…" } else { "Try again" }
        );
    }
    if a.mode == "locked" {
        return locked(a);
    }
    let editor_error = a.editor.as_ref().is_some_and(|ed| !ed.draft.is_object());
    let inert = if editor_error { " inert" } else { "" };
    let mut o = format!(
        r#"<div class="ps-root"><div class="ps-shell"{inert}><aside class="ps-side"><div class="ps-brand"><div class="ps-logo"><img src="/logo-512.png" alt="" width="36" height="36"></div><div><div class="ps-brand-name">Numera</div><div class="ps-brand-sub">Browser</div></div></div><nav class="ps-nav">"#,
    );
    for (key, label) in VIEWS {
        let _ = write!(
            o,
            r#"<button class="ps-navitem{}" data-action="nav" data-id="{key}">{}<span>{label}</span></button>"#,
            if a.view == key { " active" } else { "" },
            icon(if key == "folders" { "folder-tree" } else { key })
        );
    }
    o.push_str(r#"</nav><div class="ps-folders"><div class="ps-folders-h"><span style="flex:1">Folders</span>"#);
    o.push_str(&button(
        "nav",
        "folders",
        "Manage",
        "ps-folders-manage",
        false,
    ));
    o.push_str("</div>");
    for f in std::iter::once("All profiles".into()).chain(a.folders()) {
        let count = if f == "All profiles" {
            a.profiles.len()
        } else {
            a.profiles.iter().filter(|p| s(p, "folder") == f).count()
        };
        let _ = write!(
            o,
            r#"<button class="ps-folder{}" data-action="folder" data-id="{}">{}<span style="flex:1;text-align:left">{}</span><span class="ps-count">{count}</span></button>"#,
            if a.folder == f && a.view == "profiles" {
                " active"
            } else {
                ""
            },
            escape(&f),
            icon_at("folder-tree", 13),
            escape(&f)
        );
    }
    let _ = write!(
        o,
        r#"</div><div class="ps-side-foot {}"><span class="ps-foot-dot"></span><span>{}</span><span class="ps-foot-ver">{}</span></div></aside><main class="ps-main"><div class="ps-stats">"#,
        a.mode,
        if a.live() {
            "Engine online"
        } else if a.mode == "connecting" {
            "Connecting…"
        } else {
            "Demo mode"
        },
        if s(&a.config, "version").is_empty() {
            String::new()
        } else {
            format!("v{}", escape(s(&a.config, "version")))
        }
    );
    let running = a
        .profiles
        .iter()
        .filter(|p| s(p, "status") == "running")
        .count();
    let proxies = a
        .profiles
        .iter()
        .filter_map(|p| p["proxy"]["host"].as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let coherent = a
        .profiles
        .iter()
        .filter(|p| coherence(p).is_empty())
        .count();
    for (label, value, ic, accent) in [
        ("Profiles", a.profiles.len().to_string(), "boxes", "muted"),
        ("Running", running.to_string(), "health", "mint"),
        ("Proxies", proxies.to_string(), "server", "lilac"),
        (
            "Coherent",
            format!("{coherent}/{}", a.profiles.len()),
            "shield",
            "violet",
        ),
    ] {
        let _ = write!(
            o,
            r#"<div class="ps-stat"><span class="ps-stat-ic{}" style="color:var(--{accent})">{}</span><div><div class="ps-stat-val">{value}</div><div class="ps-stat-label">{label}</div></div></div>"#,
            if ic == "health" && running > 0 {
                " pulse"
            } else {
                ""
            },
            icon(ic)
        );
    }
    o.push_str(r#"<div style="flex:1"></div>"#);
    for (action, label, symbol, size, class) in [
        ("import-profiles", "Import", "download", 14, "ps-btn ghost"),
        ("bulk", "Bulk create", "boxes", 14, "ps-btn ghost"),
        ("new", "New profile", "plus", 15, "ps-btn primary"),
    ] {
        o.push_str(&button(
            action,
            "",
            &format!("{} {label}", icon_at(symbol, size)),
            class,
            false,
        ));
    }
    o.push_str(&button(
        "commands",
        "",
        &icon_at("command", 15),
        "ps-iconbtn",
        false,
    ));
    o.push_str(&button(
        "theme",
        "",
        &icon_at(if a.theme == "light" { "moon" } else { "theme" }, 15),
        "ps-iconbtn",
        false,
    ));
    o.push_str("</div>");
    if a.update["offer"].is_object()
        && s(&a.update["offer"], "id") != a.dismissed
        && ["available", "ready", "error"].contains(&s(&a.update, "state"))
    {
        let _ = write!(
            o,
            r#"<div class="ps-update-notice" role="status"><span>Software update available · {}</span>{}{}</div>"#,
            e(&a.update["offer"]["version"]),
            btn("nav", "settings", "Review update"),
            btn("dismiss-update", "", "Later")
        );
    }
    o.push_str(&view(a));
    o.push_str("</main></div>");
    if editor_error {
        o.push_str(r#"<div class="ps-overlay center"><div class="ps-modal ps-recovery" role="dialog" aria-modal="true" aria-labelledby="editor-error-title"><h2 id="editor-error-title">This profile couldn't be opened</h2><p>The editor couldn't read the profile. Close the editor to return to your current view.</p><button type="button" id="close-editor-error" class="ps-btn ghost" data-action="close-editor">Close editor</button></div></div>"#);
    } else {
        if let Some(editor) = &a.editor {
            o.push_str(&editor_view(a, editor));
        }
        if !a.modal.is_empty() {
            o.push_str(&modal(a));
        }
        if !a.update_approval.is_null() {
            o.push_str(&update_approval(a));
        }
    }
    o.push_str(r#"<div class="ps-toaster" aria-live="polite">"#);
    for toast in &a.toasts {
        let _ = write!(
            o,
            r#"<div class="ps-toast {}">{} {}</div>"#,
            if toast.error { "error" } else { "success" },
            escape(&toast.text),
            btn("clear-message", &toast.id.to_string(), "×")
        );
    }
    o.push_str("</div>");
    o.push_str("</div>");
    o
}
fn locked(a: &App) -> String {
    format!(
        r#"<div class="ps-root"><div class="ps-locked"><form class="ps-locked-card" data-form="unlock"><div class="ps-locked-ic">{}</div><h1>This engine needs a token</h1><p>It is running and reachable — it just will not answer without one. Create a token with <code>numera token create "dashboard"</code>, or use the value of <code>NUMERA_API_TOKEN</code>.</p>{}<div class="ps-locked-err" role="alert">{}</div>{}<div class="ps-locked-note">Stored in this browser only. It is never sent anywhere but this engine.</div></form></div></div>"#,
        icon("key"),
        input(
            "form",
            "token",
            "API token",
            &a.form["token"],
            "password",
            "nmr_…"
        ),
        escape(&a.auth_error),
        primary(
            "unlock",
            if a.busy { "Checking…" } else { "Unlock" },
            a.busy || s(&a.form, "token").trim().is_empty()
        )
    )
}
pub fn view(a: &App) -> String {
    if let Some(message) = a.view_error.as_deref().or_else(|| {
        (a.view == "profiles" && a.profiles.iter().any(|profile| !profile.is_object()))
            .then_some("The profile list contains a record this dashboard cannot read.")
    }) {
        return format!(
            r#"<div class="ps-empty ps-recovery"><div role="alert"><h2>This view couldn't be displayed</h2><p>{}</p></div><button type="button" id="retry-view" class="ps-btn ghost" data-action="retry-view" {}>{}</button></div>"#,
            escape(message),
            if a.busy { "disabled" } else { "" },
            if a.busy { "Retrying…" } else { "Try again" }
        );
    }
    match a.view.as_str() {
        "profiles" => profiles(a),
        "health" => health(a),
        "engines" => engines(a),
        "extensions" => extensions(a),
        "proxies" => proxies(a),
        "folders" => folders(a),
        "automation" => automation(a),
        "settings" => settings(a),
        v => format!(
            r#"<div class="ps-empty"><h2>{}</h2><p>This view is not available.</p></div>"#,
            escape(v)
        ),
    }
}

#[path = "render_profiles.rs"]
mod profiles;
pub use profiles::{profiles, summary};

#[path = "render_editor.rs"]
mod editor;
pub use editor::{editor_view, timezone};

#[path = "render_health.rs"]
mod health;
pub use health::probe;
use health::{folders, health};

#[path = "render_resources.rs"]
mod resources;
#[cfg(target_arch = "wasm32")]
pub(crate) use resources::engine_install_command;
pub use resources::{cloak, proxies};
use resources::{engines, extensions};

#[path = "render_settings.rs"]
mod settings;
use settings::{automation, settings, update_approval};
pub use settings::{cdp_snippet, update_panel};

#[path = "render_modal.rs"]
mod modal;
pub use modal::modal;

#[path = "render_release.rs"]
mod release;
