use crate::{model::*, updates};
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
    let path = match name {
        "profiles" => "M12 3 2 8l10 5 10-5-10-5ZM2 12l10 5 10-5M2 16l10 5 10-5",
        "health" => "M2 12h4l3-9 6 18 3-9h4",
        "engines" => "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z",
        "extensions" => {
            "M19 13h-2a2 2 0 1 1 0-4h2V5h-6V3a2 2 0 1 0-4 0v2H5v4H3a2 2 0 1 0 0 4h2v6h4v2a2 2 0 1 0 4 0v-2h6z"
        }
        "folders" => "M3 7V3h6l2 2h10v14H3V7Z",
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
        "download" => "M12 3v12m-5-5 5 5 5-5M3 15v6h18v-6",
        "filter" => "M3 4h18l-7 8v8l-4-2v-6L3 4Z",
        "theme" => {
            "M12 3v2m0 14v2M3 12h2m14 0h2M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z"
        }
        "refresh" => "M20 7a8 8 0 1 0 0 10M20 3v5h-5",
        "key" => "M8 3a5 5 0 1 0 0 10 5 5 0 0 0 0-10Zm4 9 9 9m-4-4 3-3",
        "proxies" => {
            "M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20ZM2 12h20M12 2c-5 6-5 14 0 20 5-6 5-14 0-20Z"
        }
        _ => "M3 5h18v14H3zM7 9h10M7 13h6",
    };
    format!(
        r#"<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="{path}"/></svg>"#
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
        icon(ic)
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
        &format!(r#"<span class="knob"></span><em>{}</em>"#, escape(label)),
        if on { "ps-toggle on" } else { "ps-toggle" },
        disabled,
    )
}
fn row(content: String) -> String {
    format!(r#"<div class="ps-row">{content}</div>"#)
}
pub fn app(a: &App) -> String {
    if a.mode == "startup-error" {
        return format!(
            r#"<div class="ps-root"><div class="ps-locked"><div class="ps-locked-card"><h1>The Numera engine did not start.</h1><pre class="ps-code" role="alert">{}</pre></div></div></div>"#,
            escape(&a.auth_error)
        );
    }
    if a.mode == "locked" {
        return locked(a);
    }
    let mut o = String::from(
        r#"<div class="ps-root"><div class="ps-shell"><aside class="ps-side"><div class="ps-brand"><div class="ps-logo"><img src="/logo-512.png" alt="" width="36" height="36"></div><div><div class="ps-brand-name">Numera</div><div class="ps-brand-sub">Browser</div></div></div><nav class="ps-nav">"#,
    );
    for (key, label) in VIEWS {
        let _ = write!(
            o,
            r#"<button class="ps-navitem{}" data-action="nav" data-id="{key}">{}<span>{label}</span></button>"#,
            if a.view == key { " active" } else { "" },
            icon(key)
        );
    }
    o.push_str(r#"</nav><div class="ps-folders"><div class="ps-folders-h"><span style="flex:1">Folders</span>"#);
    o.push_str(&btn("nav", "folders", "Manage"));
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
            icon("folders"),
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
    for (label, value, ic) in [
        ("Profiles", a.profiles.len().to_string(), "profiles"),
        ("Running", running.to_string(), "health"),
        ("Proxies", proxies.to_string(), "proxies"),
        (
            "Coherent",
            format!("{coherent}/{}", a.profiles.len()),
            "shield",
        ),
    ] {
        let _ = write!(
            o,
            r#"<div class="ps-stat"><span class="ps-stat-ic">{}</span><div><div class="ps-stat-val">{value}</div><div class="ps-stat-label">{label}</div></div></div>"#,
            icon(ic)
        );
    }
    o.push_str(r#"<div style="flex:1"></div>"#);
    o.push_str(&btn("import-profiles", "", "Import"));
    o.push_str(&btn("bulk", "", "Bulk create"));
    o.push_str(&primary("new", "New profile", false));
    o.push_str(&button(
        "commands",
        "",
        &icon("search"),
        "ps-iconbtn",
        false,
    ));
    o.push_str(&button("theme", "", &icon("theme"), "ps-iconbtn", false));
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
    if let Some(editor) = &a.editor {
        o.push_str(&editor_view(a, editor));
    }
    if !a.modal.is_empty() {
        o.push_str(&modal(a));
    }
    if !a.update_approval.is_null() {
        o.push_str(&update_approval(a));
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
pub fn profiles(a: &App) -> String {
    let filtered = a.filtered();
    let selected = a.selected_profiles();
    let mut o = String::new();
    if !selected.is_empty() {
        let _ = write!(
            o,
            r#"<div class="ps-bulkbar"><span>{} selected</span>"#,
            selected.len()
        );
        for (key, label) in [
            ("bulk-launch", "Launch"),
            ("sync", "Edit all"),
            ("bulk-warmup", "Warm up"),
            ("export-profiles", "Export"),
            ("delete-selected", "Delete"),
            ("clear-selection", "Clear"),
        ] {
            o.push_str(&button(
                key,
                "",
                label,
                if key == "delete-selected" {
                    "danger"
                } else {
                    ""
                },
                a.busy,
            ));
        }
        o.push_str("</div>");
    }
    let _ = write!(
        o,
        r#"<div class="ps-toolbar"><div class="ps-search">{}<input id="profile-search" placeholder="Search profiles or tags…" data-scope="app" data-field="query" value="{}"></div><span class="ps-mono dim">{} profiles</span><div style="flex:1"></div>{}</div>"#,
        icon("search"),
        escape(&a.query),
        filtered.len(),
        btn("filters", "", "Filters")
    );
    if a.filter_open {
        o.push_str(r#"<div class="ps-filter-pop">"#);
        o.push_str(&btn("clear-filters", "", "Clear all"));
        for (key, label, options) in [
            ("status", "Status", opts(&["running", "stopped"])),
            (
                "engine",
                "Engine",
                ENGINES.map(|(v, l)| (v.into(), l.into())).to_vec(),
            ),
            ("os", "OS", opts(&OS)),
            (
                "proxy",
                "Proxy",
                vec![
                    ("with".into(), "With proxy".into()),
                    ("without".into(), "Direct".into()),
                ],
            ),
            (
                "tag",
                "Tag",
                a.profiles
                    .iter()
                    .flat_map(|p| strings(&p["tags"]))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .map(|t| (t.clone(), t))
                    .collect(),
            ),
        ] {
            let _ = write!(
                o,
                r#"<div class="ps-filter-row"><span class="ps-filter-label">{label}</span><div class="ps-filter-opts">"#
            );
            for (v, l) in options {
                o.push_str(&button(
                    "filter",
                    &format!("{key}|{v}"),
                    &escape(&l),
                    if a.filters.get(key) == Some(&v) {
                        "ps-chip on"
                    } else {
                        "ps-chip"
                    },
                    false,
                ));
            }
            o.push_str("</div></div>");
        }
        o.push_str("</div>");
    }
    o.push_str(r#"<div class="ps-scroll">"#);
    if filtered.is_empty() {
        o.push_str(r#"<div class="ps-empty">No profiles here yet."#);
        o.push_str(&primary("new", "Create one", false));
        o.push_str(&btn("clear-search", "", "Clear search and filters"));
        o.push_str("</div>");
    } else {
        let all = filtered.iter().all(|p| a.selected.contains(s(p, "id")));
        let _ = write!(
            o,
            r#"<table class="ps-table"><thead><tr><th><input type="checkbox" data-action="select-all" aria-label="Select every profile" {}></th><th>Profile</th><th>Environment</th><th>Engine</th><th>Proxy</th><th>Coherence</th><th>Last active</th><th>Actions</th></tr></thead><tbody>"#,
            if all { "checked" } else { "" }
        );
        let page = a.page.min(filtered.len().saturating_sub(1) / 100);
        for p in filtered.iter().skip(page * 100).take(100) {
            let id = s(p, "id");
            let issues = coherence(p);
            let _ = write!(
                o,
                r#"<tr class="{}"><td><input type="checkbox" data-action="select" data-id="{}" aria-label="Select {}" {}></td><td><div class="ps-name"><span class="ps-status {}"></span>{}<div>"#,
                if a.selected.contains(id) { "sel" } else { "" },
                escape(id),
                e(&p["name"]),
                if a.selected.contains(id) {
                    "checked"
                } else {
                    ""
                },
                escape(s(p, "status")),
                identicon(id)
            );
            if a.rename == id {
                let _ = write!(
                    o,
                    r#"<input class="ps-pname-in" id="rename-profile" data-scope="form" data-field="rename" value="{}" aria-label="Profile name">"#,
                    e(&a.form["rename"])
                );
            } else {
                let _ = write!(
                    o,
                    r#"<div class="ps-pname editable" title="Click to rename" data-action="rename-profile" data-id="{}">{}</div>"#,
                    escape(id),
                    e(&p["name"])
                );
            }
            o.push_str(r#"<div class="ps-tags">"#);
            for tag in list(&p["tags"]) {
                o.push_str(&button(
                    "filter",
                    &format!("tag|{}", text(&tag)),
                    &e(&tag),
                    "ps-tag clickable",
                    false,
                ));
            }
            let _ = write!(
                o,
                r#"</div></div></div></td><td><div class="ps-env"><span class="ps-os">{}</span><span class="ps-mono">{}</span>{}</div></td><td><span class="ps-engine-chip">{}</span></td><td><div class="ps-proxy">{}</div></td><td><span class="ps-pill {}" title="{}">{}</span></td><td class="ps-mono dim">{}</td><td><div class="ps-actions">"#,
                e(&p["os"]),
                if owns_identity(s(p, "engine")) {
                    "Firefox · generated".into()
                } else {
                    format!(
                        "{} {}",
                        e(&p["browser"]),
                        escape(s(p, "browserVersion").split('.').next().unwrap_or(""))
                    )
                },
                if owns_identity(s(p, "engine")) {
                    String::new()
                } else {
                    format!(r#"<span class="ps-mono dim">{}</span>"#, e(&p["screen"]))
                },
                escape(engine_label(s(p, "engine"))),
                if p["proxy"].is_object() {
                    format!("{} {}", e(&p["proxy"]["country"]), e(&p["proxy"]["type"]))
                } else {
                    "Direct".into()
                },
                if issues.is_empty() { "ok" } else { "warn" },
                escape(&issues.join("\n")),
                if issues.is_empty() {
                    "Coherent".into()
                } else {
                    format!("{} issue(s)", issues.len())
                },
                e(&p["lastActive"])
            );
            for (act, title, ic) in [
                (
                    "run",
                    if s(p, "status") == "running" {
                        "Stop"
                    } else {
                        "Launch"
                    },
                    if s(p, "status") == "running" {
                        "stop"
                    } else {
                        "run"
                    },
                ),
                ("edit", "Edit", "edit"),
                ("clone", "Clone", "clone"),
                ("delete-profile", "Delete", "delete"),
            ] {
                o.push_str(&action(act, id, title, ic, a.busy));
            }
            o.push_str("</div></td></tr>");
        }
        o.push_str("</tbody></table>");
        if filtered.len() > 100 {
            let pages = filtered.len().div_ceil(100);
            let _ = write!(
                o,
                r#"<div class="ps-pager">{}{}<span>Page {} of {pages} · {} profiles</span>{}{}</div>"#,
                button("page", "0", "« First", "ps-btn ghost sm", page == 0),
                button(
                    "page",
                    &page.saturating_sub(1).to_string(),
                    "‹ Prev",
                    "ps-btn ghost sm",
                    page == 0
                ),
                page + 1,
                filtered.len(),
                button(
                    "page",
                    &(page + 1).to_string(),
                    "Next ›",
                    "ps-btn ghost sm",
                    page + 1 >= pages
                ),
                button(
                    "page",
                    &(pages - 1).to_string(),
                    "Last »",
                    "ps-btn ghost sm",
                    page + 1 >= pages
                )
            );
        }
    }
    o.push_str("</div>");
    o
}
fn identicon(id: &str) -> String {
    let hash = id.bytes().fold(2166136261u32, |v, c| {
        (v ^ u32::from(c)).wrapping_mul(16777619)
    });
    let mut out = format!(
        r#"<span class="ps-identicon"><svg width="30" height="30" viewBox="0 0 5 5" fill="hsl({} 46% 60%)" aria-hidden="true">"#,
        hash % 360
    );
    for y in 0..5 {
        for x in 0..3 {
            if hash.rotate_left(y * 3 + x) & 1 != 0 {
                let _ = write!(out, r#"<rect x="{x}" y="{y}" width="1" height="1"/>"#);
                if x < 2 {
                    let _ = write!(out, r#"<rect x="{}" y="{y}" width="1" height="1"/>"#, 4 - x);
                }
            }
        }
    }
    out.push_str("</svg></span>");
    out
}
pub fn summary(p: &Value) -> String {
    let issues = coherence(p);
    let score = 100usize.saturating_sub(issues.len() * 22);
    let built = owns_identity(s(p, "engine"));
    let mut o = format!(
        r#"<aside class="ps-summary"><div class="ps-summary-h"><span>Profile summary</span><span>{score}%</span></div>"#
    );
    if built {
        o.push_str(r#"<div class="ps-summary-note">Camoufox builds its own Firefox fingerprint. It is given this profile's OS, language and proxy; the marked rows below are generated by the browser and take effect only on a Chromium engine.</div>"#);
    }
    o.push_str(r#"<dl class="ps-summary-list">"#);
    let rows = vec![
        ("Profile name", text(&p["name"]), false),
        ("Folder", text(&p["folder"]), false),
        ("Engine", engine_label(s(p, "engine")).into(), false),
        (
            "Proxy",
            if p["proxy"].is_object() {
                format!(
                    "{} {}:{}",
                    s(&p["proxy"], "type"),
                    s(&p["proxy"], "host"),
                    text(&p["proxy"]["port"])
                )
            } else {
                "Direct".into()
            },
            false,
        ),
        ("OS", text(&p["os"]), false),
        (
            "Browser",
            if built {
                "Firefox".into()
            } else {
                format!("Chrome {}", s(p, "browserVersion"))
            },
            true,
        ),
        ("User-Agent", text(&p["userAgent"]), true),
        ("Resolution", text(&p["screen"]), true),
        ("Languages", text(&p["locale"]), false),
        ("Timezone", text(&p["timezone"]), false),
        ("Geolocation", text(&p["geo"]), false),
        ("WebRTC", text(&p["webrtc"]), true),
        ("Canvas", text(&p["canvas"]), true),
        (
            "Client Rects",
            p["clientRects"].as_str().unwrap_or("noise").into(),
            true,
        ),
        (
            "WebGL Image",
            p["webglImage"].as_str().unwrap_or("noise").into(),
            true,
        ),
        ("WebGL Vendor", text(&p["webglVendor"]), true),
        ("Audio Context", text(&p["audio"]), true),
        (
            "Fonts",
            if s(p, "fonts").eq_ignore_ascii_case("real") {
                "real".into()
            } else {
                let n = s(p, "fontList").lines().filter(|l| !l.is_empty()).count();
                format!(
                    "masked ({})",
                    if n == 0 {
                        "default".into()
                    } else {
                        n.to_string()
                    }
                )
            },
            true,
        ),
        (
            "Media devices",
            format!(
                "{}/{}/{}",
                number(p, "mediaVideo"),
                number(p, "mediaAudio"),
                p["mediaSpeakers"].as_u64().unwrap_or(1)
            ),
            true,
        ),
        (
            "CPU / RAM",
            format!(
                "{} threads · {} GB",
                number(p, "cores"),
                number(p, "memory")
            ),
            true,
        ),
        (
            "Do Not Track",
            if p["dnt"] == true { "on" } else { "off" }.into(),
            false,
        ),
    ];
    for (label, value, generated) in rows {
        let _ = write!(
            o,
            r#"<div class="ps-summary-row"><dt>{label}</dt><dd title="{}">{}{}</dd></div>"#,
            escape(&value),
            if value.is_empty() {
                "—".into()
            } else {
                escape(&value)
            },
            if built && generated {
                " · generated"
            } else {
                ""
            }
        );
    }
    for (label, key) in [
        ("Extensions", "extensions"),
        ("Startup URLs", "startupUrls"),
        ("Bookmarks", "bookmarks"),
        ("Launch args", "launchArgs"),
    ] {
        let n = s(p, key).lines().filter(|l| !l.trim().is_empty()).count();
        let _ = write!(
            o,
            r#"<div class="ps-summary-row"><dt>{label}</dt><dd>{}</dd></div>"#,
            if n == 0 { "none".into() } else { n.to_string() }
        );
    }
    for (label, key) in [
        ("DNS", "dns"),
        ("Follows exit IP", "ipBased"),
        ("Cleared each launch", "clearOnStart"),
        ("Switched off", "switchedOff"),
    ] {
        let v = if p[key].is_array() {
            strings(&p[key]).join(", ")
        } else {
            text(&p[key])
        };
        let _ = write!(
            o,
            r#"<div class="ps-summary-row"><dt>{label}</dt><dd>{}</dd></div>"#,
            if v.is_empty() {
                "nothing".into()
            } else {
                escape(&v)
            }
        );
    }
    o.push_str("</dl>");
    if !issues.is_empty() {
        let _ = write!(
            o,
            r#"<div class="ps-summary-iss">{} contradiction(s): {}</div>"#,
            issues.len(),
            escape(&issues.join(" · "))
        );
    }
    o.push_str("</aside>");
    o
}
pub fn timezone(p: &Value, zones: &[String], valid: bool) -> String {
    let auto = strings(&p["ipBased"]).contains(&"timezone".into());
    let proxy = p["proxy"].is_object();
    let mut o = format!(
        r#"<div class="ps-field"><label class="ps-flabel" for="editor-timezone">Timezone</label><div class="ps-seg">{}{}</div><input id="editor-timezone" class="ps-in mono" data-scope="editor" data-field="timezone" value="{}" list="numera-timezones" placeholder="Region/City" spellcheck="false" {} {}><datalist id="numera-timezones">"#,
        button(
            "timezone-auto",
            "",
            "Auto",
            if auto { "on" } else { "" },
            !proxy
        ),
        button(
            "timezone-manual",
            "",
            "Manual",
            if !auto { "on" } else { "" },
            false
        ),
        e(&p["timezone"]),
        if valid { "" } else { "aria-invalid=\"true\"" },
        if auto && proxy { "disabled" } else { "" }
    );
    for z in zones {
        let _ = write!(o, r#"<option value="{}"></option>"#, escape(z));
    }
    o.push_str("</datalist>");
    if !auto && !valid && !s(p, "timezone").is_empty() {
        o.push_str(&hint("Not a zone this browser knows."));
    }
    o.push_str("</div>");
    o
}
pub fn editor_view(a: &App, ed: &Editor) -> String {
    let p = &ed.draft;
    let issues = coherence(p);
    let score = 100usize.saturating_sub(issues.len() * 22);
    let coherence_class = if issues.is_empty() {
        "ps-coh-ok"
    } else {
        "ps-coh-iss"
    };
    let coherence_color = if issues.is_empty() {
        "var(--mint)"
    } else {
        "var(--amber)"
    };
    let inert = if ed.editable() { "" } else { "inert" };
    let mut folders = vec![(String::new(), "No folder".into())];
    for f in std::iter::once(s(p, "folder").into()).chain(a.folders()) {
        if !f.is_empty() && f != "All profiles" && !folders.iter().any(|(v, _)| v == &f) {
            folders.push((f.clone(), f));
        }
    }
    let mut o = format!(
        r#"<div class="ps-overlay ps-page"><div class="ps-editor" aria-busy="{}"><div class="ps-editor-h" {inert}><div class="ps-editor-id"><div class="ps-eyebrow">{} profile</div>{}</div>{}<div style="flex:1"></div>{}</div><div class="ps-coh"><div class="ps-coh-top"><span class="ps-coh-h">Fingerprint coherence</span><span class="ps-coh-score" style="color:{coherence_color}">{score}%</span></div><div class="ps-coh-bar"><div style="width:{score}%;background:{coherence_color}"></div></div><div class="{coherence_class}">{}</div></div><div class="ps-tabs" {inert}>"#,
        ed.saving,
        if p["_new"] == true { "New" } else { "Edit" },
        input_with_name(
            "editor",
            "name",
            "Profile name",
            &p["name"],
            "text",
            "Untitled profile",
            "Profile name"
        )
        .replace("class=\"ps-in \"", "class=\"ps-in ps-editor-name\""),
        select("editor", "folder", "Folder", &p["folder"], folders, false)
            .replace("class=\"ps-in\"", "class=\"ps-in ps-editor-folder\""),
        button("close-editor", "", &icon("x"), "ps-x", ed.saving),
        if issues.is_empty() {
            "All signals agree — this identity reads like one real device.".into()
        } else {
            escape(&issues.join(" · "))
        }
    );
    for tab in ["general", "fingerprint", "proxy", "bookmarks", "advanced"] {
        o.push_str(&button(
            "tab",
            tab,
            tab,
            if ed.tab == tab {
                "ps-tab active"
            } else {
                "ps-tab"
            },
            !ed.editable(),
        ));
    }
    let _ = write!(
        o,
        r#"</div><div class="ps-editor-main" {inert}><div class="ps-editor-body">"#
    );
    match ed.tab.as_str() {
        "general" => {
            o.push_str(&input(
                "editor",
                "tags",
                "Tags (comma separated)",
                &json!(
                    list(&p["tags"])
                        .iter()
                        .map(text)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                "text",
                "client-a, priority",
            ));
            o.push_str(&textarea(
                "editor",
                "notes",
                "Notes",
                &p["notes"],
                3,
                "Anything worth remembering about this account…",
            ));
            o.push_str(&select(
                "editor",
                "engine",
                "Launch engine",
                &p["engine"],
                engine_opts("— engine default —"),
                false,
            ));
            o.push_str(&hint("Numera manages this profile; the engine renders it. Manage engines in the Engines tab."));
        }
        "fingerprint" => {
            if owns_identity(s(p, "engine")) {
                o.push_str(&hint("Camoufox generates its own Firefox fingerprint and is given only the OS, the language and the proxy below. The rest of this tab is stored and takes effect on a Chromium engine."));
            }
            o.push_str(&section("Operating system & browser"));
            o.push_str(&row(seg(
                "editor",
                "os",
                "Operating system",
                s(p, "os"),
                &OS,
            ) + &input(
                "editor",
                "browserVersion",
                "Browser version",
                &p["browserVersion"],
                "text",
                "",
            )));
            o.push_str(&textarea(
                "editor",
                "userAgent",
                "User-Agent",
                &p["userAgent"],
                2,
                "",
            ));
            o.push_str(&section("Hardware"));
            let mut resolutions = vec![s(p, "screen")];
            resolutions.extend(screens(s(p, "os")));
            o.push_str(&row(select(
                "editor",
                "screen",
                "Screen resolution",
                &p["screen"],
                current_options(&p["screen"], opts(&resolutions)),
                false,
            ) + &select(
                "editor",
                "cores",
                "CPU cores",
                &p["cores"],
                current_options(
                    &p["cores"],
                    cores(s(p, "os"))
                        .iter()
                        .map(|v| (v.to_string(), v.to_string()))
                        .collect(),
                ),
                false,
            ) + &select(
                "editor",
                "memory",
                "Memory (GB)",
                &p["memory"],
                current_options(
                    &p["memory"],
                    ram(s(p, "os"))
                        .iter()
                        .map(|v| (v.to_string(), v.to_string()))
                        .collect(),
                ),
                false,
            )));
            let mut gpu = vec![s(p, "webglRenderer")];
            gpu.extend(gpus(s(p, "os")));
            o.push_str(&select(
                "editor",
                "webglRenderer",
                "WebGL renderer (GPU)",
                &p["webglRenderer"],
                current_options(&p["webglRenderer"], opts(&gpu)),
                false,
            ));
            o.push_str(&input(
                "editor",
                "webglVendor",
                "WebGL vendor",
                &p["webglVendor"],
                "text",
                "Google Inc. (NVIDIA)",
            ));
            o.push_str(&section("Spoofing modes"));
            for (k, l, options, default) in [
                ("canvas", "Canvas", vec!["Noise", "Block", "Real"], "Noise"),
                (
                    "webrtc",
                    "WebRTC",
                    vec!["Altered", "Disabled", "Real"],
                    "Altered",
                ),
                ("audio", "Audio context", vec!["Noise", "Real"], "Noise"),
                ("fonts", "Fonts", vec!["Masked", "Real"], "Masked"),
                (
                    "clientRects",
                    "Client rects",
                    vec!["Noise", "Real"],
                    "Noise",
                ),
                ("webglImage", "WebGL image", vec!["Noise", "Real"], "Noise"),
                (
                    "mediaDevices",
                    "Media devices",
                    vec!["Masked", "Real"],
                    "Masked",
                ),
            ] {
                o.push_str(&seg(
                    "editor",
                    k,
                    l,
                    if s(p, k).is_empty() { default } else { s(p, k) },
                    &options,
                ));
            }
            if !s(p, "fonts").eq_ignore_ascii_case("real") {
                o.push_str(&textarea(
                    "editor",
                    "fontList",
                    "Font list (one per line)",
                    &p["fontList"],
                    5,
                    "Leave empty to use the OS default set",
                ));
            }
            o.push_str(&section("Geo & language"));
            o.push_str(&ip_switch(p, "languages"));
            o.push_str(&select(
                "editor",
                "locale",
                "Locale & languages",
                &p["locale"],
                LOCALES.map(|(v, _)| (v.into(), v.into())).to_vec(),
                strings(&p["ipBased"]).contains(&"languages".into()),
            ));
            let (zones, valid) = browser_zones(s(p, "timezone"));
            o.push_str(&timezone(p, &zones, valid));
            o.push_str(&ip_switch(p, "geolocation"));
            o.push_str(&seg(
                "editor",
                "geo",
                "Geolocation",
                s(p, "geo"),
                &["Prompt", "Allow", "Block"],
            ));
            for (k, l) in [
                ("mediaVideo", "Cameras"),
                ("mediaAudio", "Microphones"),
                ("mediaSpeakers", "Speakers"),
            ] {
                o.push_str(&input("editor", k, l, &p[k], "number", ""));
            }
            o.push_str(r#"<div class="ps-field" role="group" aria-label="Do Not Track"><span class="ps-flabel">Do Not Track</span>"#);
            o.push_str(&toggle(
                "dnt",
                "",
                if p["dnt"] == true { "On" } else { "Off" },
                p["dnt"] == true,
                false,
            ));
            o.push_str("</div>");
            o.push_str(&section("Trust check"));
            for (k, l) in [
                ("trust", "Run trust check"),
                ("tls", "Check TLS / JA3"),
                ("align", "Auto-adjust"),
            ] {
                o.push_str(&button(
                    "probe",
                    k,
                    l,
                    "ps-btn ghost sm",
                    !a.live() || p["_new"] == true || a.busy,
                ));
            }
            o.push_str(&probe(&ed.probe));
        }
        "proxy" => {
            o.push_str(r#"<div class="ps-proxy-toggle"><span>Use proxy for this profile</span>"#);
            o.push_str(&toggle(
                "toggle-proxy",
                "",
                if p["proxy"].is_object() { "On" } else { "Off" },
                p["proxy"].is_object(),
                false,
            ));
            o.push_str("</div>");
            if p["proxy"].is_object() {
                if !a.pool.is_empty() {
                    let pick = a.pool.iter().find(|x| {
                        x["host"] == p["proxy"]["host"]
                            && text(&x["port"]) == text(&p["proxy"]["port"])
                    });
                    let mut options = vec![(String::new(), "— manual entry —".into())];
                    options.extend(a.pool.iter().map(|x| {
                        (
                            s(x, "id").into(),
                            format!(
                                "{}:{} · {}",
                                s(x, "host"),
                                text(&x["port"]),
                                s(x, "country")
                            ),
                        )
                    }));
                    o.push_str(&select(
                        "form",
                        "poolPick",
                        "Saved proxy",
                        &pick.map(|p| p["id"].clone()).unwrap_or(Value::Null),
                        options,
                        false,
                    ));
                }
                let mut countries = COUNTRIES.to_vec();
                let country = s(&p["proxy"], "country");
                if !country.is_empty() && !countries.contains(&country) {
                    countries.insert(0, country);
                }
                o.push_str(&row(select(
                    "editor",
                    "proxy.type",
                    "Type",
                    &p["proxy"]["type"],
                    opts(&["HTTP", "HTTPS", "SOCKS5"]),
                    false,
                ) + &select(
                    "editor",
                    "proxy.country",
                    "Country",
                    &p["proxy"]["country"],
                    opts(&countries),
                    false,
                )));
                for (k, l, ph, kind) in [
                    ("host", "Host", "host.proxy.io", "text"),
                    ("port", "Port", "8080", "text"),
                    ("user", "Username", "", "text"),
                    ("pass", "Password", "", "password"),
                ] {
                    o.push_str(&input(
                        "editor",
                        &format!("proxy.{k}"),
                        l,
                        &p["proxy"][k],
                        kind,
                        ph,
                    ));
                }
                for (k, l) in [("proxy-test", "Test connection"), ("dns", "DNS leak test")] {
                    o.push_str(&button(
                        "probe",
                        k,
                        l,
                        "ps-btn ghost sm",
                        !a.live() || p["_new"] == true || a.busy,
                    ));
                }
                o.push_str(&probe(&ed.probe));
            } else {
                o.push_str(&hint("No proxy — this profile connects directly. Turn it on to route through a residential or datacenter proxy."));
            }
        }
        "bookmarks" => {
            o.push_str(&section("Bookmarks bar"));
            o.push_str(&textarea(
                "editor",
                "bookmarks",
                "One per line — “Name | https://url”, or just the URL",
                &p["bookmarks"],
                10,
                "Gmail | https://mail.google.com\nhttps://github.com",
            ));
            o.push_str(&hint("Written into the profile's own Chromium bookmark file just before it launches, and merged — anything bookmarked from inside the browser stays. Chromium engines only; Camoufox keeps its bookmarks in a database this doesn't write."));
        }
        "advanced" => {
            for (k, l, ph) in [
                (
                    "startupUrls",
                    "Startup URLs (one per line)",
                    "https://facebook.com\nhttps://business.facebook.com",
                ),
                (
                    "launchArgs",
                    "Extra Chromium arguments (one per line)",
                    "--mute-audio\n--disable-background-timer-throttling",
                ),
            ] {
                o.push_str(&textarea("editor", k, l, &p[k], 3, ph));
            }
            o.push_str(&section("Launch"));
            o.push_str(&keep(
                p,
                "switchedOff",
                &[
                    ("googleServices", "Google services"),
                    ("systemExtensions", "Built-in Chromium extensions"),
                ],
            ));
            o.push_str(&select(
                "editor",
                "dns",
                "DNS resolver",
                &if ["", "1.1.1.1", "8.8.8.8", "9.9.9.9", "94.140.14.14"].contains(&s(p, "dns")) {
                    p["dns"].clone()
                } else {
                    json!("custom")
                },
                vec![
                    ("".into(), "System default".into()),
                    ("1.1.1.1".into(), "Cloudflare — 1.1.1.1".into()),
                    ("8.8.8.8".into(), "Google — 8.8.8.8".into()),
                    ("9.9.9.9".into(), "Quad9 — 9.9.9.9".into()),
                    ("94.140.14.14".into(), "AdGuard — 94.140.14.14".into()),
                    ("custom".into(), "Custom DoH endpoint…".into()),
                ],
                false,
            ));
            if !["", "1.1.1.1", "8.8.8.8", "9.9.9.9", "94.140.14.14"].contains(&s(p, "dns")) {
                o.push_str(&input(
                    "editor",
                    "dns",
                    "DoH endpoint URL",
                    &p["dns"],
                    "text",
                    "https://doh.example.net/dns-query",
                ));
            }
            o.push_str(&input(
                "editor",
                "webglVendor",
                "WebGL vendor",
                &p["webglVendor"],
                "text",
                "",
            ));
            o.push_str(&section("Extensions"));
            o.push_str(&textarea(
                "editor",
                "extensions",
                "Unpacked extension folders (one per line)",
                &p["extensions"],
                2,
                "C:\\tools\\metamask\n/home/me/ext/ublock",
            ));
            o.push_str(&section("Warm-up"));
            o.push_str(&select(
                "form",
                "preset",
                "Vertical (which sites to browse)",
                &a.form["preset"],
                PRESETS.map(|(v, l)| (v.into(), l.into())).to_vec(),
                false,
            ));
            o.push_str(&button(
                "warm-now",
                "",
                "Warm up 5 min",
                "ps-btn ghost sm",
                !a.live() || p["_new"] == true || a.busy,
            ));
            o.push_str(&toggle(
                "schedule",
                "",
                "Schedule recurring warm-up",
                a.schedule.is_object(),
                !a.live() || p["_new"] == true || a.busy,
            ));
            if a.schedule.is_object() {
                o.push_str(&input(
                    "schedule",
                    "everyHours",
                    "Every (hours)",
                    &a.schedule["everyHours"],
                    "number",
                    "",
                ));
                o.push_str(&select(
                    "schedule",
                    "preset",
                    "Vertical",
                    &a.schedule["preset"],
                    PRESETS.map(|(v, l)| (v.into(), l.into())).to_vec(),
                    a.busy,
                ));
            }
            o.push_str(&section("What carries over between launches"));
            o.push_str(&keep(
                p,
                "clearOnStart",
                &[
                    ("cookies", "Cookies"),
                    ("localStorage", "Local storage"),
                    ("indexedDb", "IndexedDB"),
                    ("sessions", "Open tabs"),
                    ("history", "History"),
                    ("passwords", "Saved passwords"),
                    ("extensionData", "Extension data"),
                    ("bookmarks", "Bookmarks"),
                ],
            ));
            o.push_str(&section("Session cookies"));
            if a.live() && p["_new"] != true {
                o.push_str(&button(
                    "export-cookies",
                    "",
                    "Export cookies",
                    "ps-btn ghost sm",
                    a.busy,
                ));
                o.push_str(&button(
                    "import-cookies",
                    "",
                    "Import cookies",
                    "ps-btn ghost sm",
                    a.busy,
                ));
                o.push_str(&toggle(
                    "replace-cookies",
                    "",
                    "Replace existing",
                    a.form["replaceCookies"] == true,
                    false,
                ));
                o.push_str(&hint("Reads and writes the profile's real cookie jar. Accepts Cookie-Editor / EditThisCookie JSON, Playwright storage state and Netscape cookies.txt. Stop the profile first."));
            } else {
                o.push_str(&hint(
                    "Save this profile first and connect the engine to move a session in or out.",
                ));
            }
        }
        _ => {}
    }
    o.push_str("</div>");
    o.push_str(&summary(p));
    o.push_str("</div>");
    if !ed.message.is_empty() {
        let _ = write!(
            o,
            r#"<div role="status" class="ps-save-status">{}</div>"#,
            escape(&ed.message)
        );
    }
    for (index, path) in ed.conflicts.iter().enumerate() {
        let hidden = path[0] == "proxy" && (path.len() == 1 || path[1] == "pass");
        let _ = write!(
            o,
            r#"<div role="group" aria-label="Resolve {} conflict" class="ps-conflict"><strong>{}</strong><span>My edit: {} · Latest: {}</span>{}{}</div>"#,
            escape(&path.join(".")),
            escape(&path.join(".")),
            if hidden {
                "(hidden)".into()
            } else {
                e(field(p, path))
            },
            if hidden {
                "(hidden)".into()
            } else {
                e(field(&ed.base, path))
            },
            btn("resolve-mine", &index.to_string(), "Keep my edit"),
            btn("resolve-latest", &index.to_string(), "Use latest")
        );
    }
    let _ = write!(
        o,
        r#"<div class="ps-drawer-foot">{}{}</div></div></div>"#,
        button("close-editor", "", "Cancel", "ps-btn ghost", ed.saving),
        primary(
            "save",
            if ed.saving {
                "Saving…"
            } else if p["_new"] == true {
                "Create profile"
            } else {
                "Save changes"
            },
            !ed.editable() || text(&p["name"]).is_empty()
        )
    );
    o
}
fn ip_switch(p: &Value, signal: &str) -> String {
    let on = strings(&p["ipBased"]).contains(&signal.into());
    button(
        "ip",
        signal,
        if on { "IP" } else { "Manual" },
        if on { "ps-ipsw on" } else { "ps-ipsw" },
        !p["proxy"].is_object(),
    )
}
fn keep(p: &Value, key: &str, items: &[(&str, &str)]) -> String {
    let mut o = String::from(r#"<div class="ps-keep">"#);
    let off = strings(&p[key]);
    for (k, l) in items {
        let on = !off.contains(&k.to_string());
        o.push_str(&button(
            "keep",
            &format!("{key}|{k}"),
            &format!(
                r#"<span class="ps-keep-box">{}</span><span>{l}</span>"#,
                if on { "✓" } else { "" }
            ),
            if on {
                "ps-keep-item on"
            } else {
                "ps-keep-item"
            },
            false,
        ));
    }
    o.push_str("</div>");
    o
}
#[cfg(target_arch = "wasm32")]
fn browser_zones(zone: &str) -> (Vec<String>, bool) {
    crate::browser::timezones(zone)
}
#[cfg(not(target_arch = "wasm32"))]
fn browser_zones(zone: &str) -> (Vec<String>, bool) {
    let mut zones = LOCALES.map(|(_, z)| z.to_string()).to_vec();
    zones.extend(["Asia/Tehran".into(), "America/Los_Angeles".into()]);
    let valid = zones.contains(&zone.to_string());
    (zones, valid)
}
pub fn probe(v: &Value) -> String {
    if v.is_null() {
        return String::new();
    }
    if s(v, "state") == "running" {
        return hint("Opening the profile in the background — this takes a few seconds.");
    }
    if s(v, "state") == "error" {
        return hint(s(v, "error"));
    }
    let mut o = String::from(r#"<div class="ps-probe">"#);
    for k in ["before", "after"] {
        if v[k].is_object() {
            let _ = write!(
                o,
                r#"<div class="ps-probe-head"><span class="ps-probe-score">{} {}%</span></div>"#,
                e(&v[k]["grade"]),
                e(&v[k]["score"])
            );
        }
    }
    if v["score"].is_number() {
        let _ = write!(
            o,
            r#"<div class="ps-probe-head"><span class="ps-probe-score">{}%</span><span>grade {} · {}/{} checks passed</span></div>"#,
            e(&v["score"]),
            e(&v["grade"]),
            e(&v["passed"]),
            e(&v["total"])
        );
    }
    for k in ["ip", "city", "country", "ja3Hash", "ja4", "error"] {
        if !v[k].is_null() {
            let _ = write!(o, "<div>{k}: {}</div>", e(&v[k]));
        }
    }
    if v["changed"].is_array() {
        o.push_str(&hint(&format!(
            "Aligned {} to the real browser.",
            strings(&v["changed"]).join(", ")
        )));
    }
    if v["countries"].is_array() {
        o.push_str(&hint(&format!(
            "{} resolver(s): {}",
            list(&v["resolvers"]).len(),
            strings(&v["countries"]).join(", ")
        )));
    }
    for c in list(&v["checks"]) {
        let _ = write!(
            o,
            r#"<div class="ps-check {}">{} {} {}</div>"#,
            if c["ok"] == true { "ok" } else { "" },
            if c["ok"] == true { "✓" } else { "×" },
            e(&c["name"]),
            e(&c["detail"])
        );
    }
    o.push_str("</div>");
    o
}
fn health(a: &App) -> String {
    let collisions = collision_ids(&a.profiles);
    let rows = if a.live() {
        list(&a.health["profiles"])
    } else {
        a.profiles
            .iter()
            .map(|p| {
                let mut p = p.clone();
                let issues = coherence(&p);
                p["coherent"] = json!(issues.is_empty());
                p["issues"] = json!(issues);
                p["collides"] = json!(collisions.contains(s(&p, "id")));
                p
            })
            .collect()
    };
    let mut o = String::from(r#"<div class="ps-scroll"><div class="ps-health-summary">"#);
    let total = rows.len();
    for (label, value) in [
        ("Profiles", total.to_string()),
        (
            "Coherent",
            format!(
                "{}/{}",
                rows.iter().filter(|p| p["coherent"] == true).count(),
                total
            ),
        ),
        (
            "With proxy",
            if a.live() {
                text(&a.health["summary"]["withProxy"])
            } else {
                rows.iter()
                    .filter(|p| p["proxy"].is_object())
                    .count()
                    .to_string()
            },
        ),
        ("Scheduled", text(&a.health["summary"]["scheduled"])),
        (
            "Shared FP",
            rows.iter()
                .filter(|p| p["collides"] == true)
                .count()
                .to_string(),
        ),
        (
            "Needs attention",
            text(&a.health["summary"]["needsAttention"]),
        ),
    ] {
        let _ = write!(
            o,
            r#"<div class="ps-hchip"><span class="ps-hchip-val">{value}</span><span class="ps-hchip-label">{label}</span></div>"#
        );
    }
    o.push_str(&btn("refresh-view", "", "Refresh"));
    o.push_str("</div>");
    if !a.live() {
        o.push_str(&hint("Start the engine (numera serve) for live trust grades, DNS/proxy status and warm-up schedules. Showing coherence and fingerprint collisions derived from the sample profiles."));
    }
    o.push_str(r#"<table class="ps-table"><thead><tr><th>Profile</th><th>Engine</th><th>Coherence</th><th>Proxy</th><th>Trust</th><th>Warm-up</th><th>Last active</th><th>Actions</th></tr></thead><tbody>"#);
    for p in rows {
        let id = s(&p, "id");
        let _ = write!(
            o,
            r#"<tr class="{}"><td>{} {}</td><td class="ps-mono dim">{}</td><td title="{}">{}</td><td>{}</td><td><span class="ps-grade">{}</span></td><td>{}</td><td>{}</td><td><div class="ps-actions">{}{}</div></td></tr>"#,
            if p["needsAttention"] == true {
                "ps-attention"
            } else {
                ""
            },
            button("edit", id, &e(&p["name"]), "ps-linkish", false),
            if p["collides"] == true {
                r#"<span class="ps-shared">shared FP</span>"#
            } else {
                ""
            },
            e(&p["engine"]),
            escape(&strings(&p["issues"]).join("; ")),
            if p["coherent"] == true {
                "✓ coherent".into()
            } else {
                format!("{} issue(s)", list(&p["issues"]).len())
            },
            if a.live() {
                if p["proxy"]["set"] == true {
                    e(&p["proxy"]["country"])
                } else {
                    "none".into()
                }
            } else if p["proxy"].is_object() {
                e(&p["proxy"]["country"])
            } else {
                "—".into()
            },
            if s(&p["trust"], "grade").is_empty() {
                "—".into()
            } else {
                e(&p["trust"]["grade"])
            },
            if p["schedule"].is_object() {
                format!("{}h", e(&p["schedule"]["everyHours"]))
            } else {
                "—".into()
            },
            e(&p["lastActive"]),
            action(
                "health-trust",
                id,
                "Run trust check",
                "shield",
                !a.live() || a.busy || s(&p, "status") == "running"
            ),
            action(
                "health-align",
                id,
                "Auto-adjust to grade A",
                "refresh",
                !a.live() || a.busy || s(&p, "status") == "running"
            )
        );
    }
    o.push_str("</tbody></table></div>");
    o
}
fn folders(a: &App) -> String {
    let mut o = String::from(r#"<div class="ps-scroll">"#);
    o.push_str(&head("Folders","Group profiles by client, platform or project. Renaming or deleting a folder moves the profiles inside it — nothing is deleted."));
    o.push_str(&input(
        "form",
        "newFolder",
        "New folder name",
        &a.form["newFolder"],
        "text",
        "New folder name",
    ));
    o.push_str(&primary("create-folder", "Add", a.busy));
    o.push_str(r#"<div class="ps-fold-grid">"#);
    for f in a.folders() {
        let ps = a
            .profiles
            .iter()
            .filter(|p| s(p, "folder") == f)
            .collect::<Vec<_>>();
        let _ = write!(
            o,
            r#"<div class="ps-fold-card"><div class="ps-fold-top"><div class="ps-fold-ic">{}</div>{}</div><div class="ps-fold-stats"><span><b>{}</b> profiles</span><span>{} running</span><span>{}/{} coherent</span></div><div class="ps-fold-actions">{}{}{} </div></div>"#,
            icon("folders"),
            button("folder", &f, &escape(&f), "ps-fold-name", false),
            ps.len(),
            ps.iter().filter(|p| s(p, "status") == "running").count(),
            ps.iter().filter(|p| coherence(p).is_empty()).count(),
            ps.len(),
            btn("folder", &f, "Open"),
            action("rename-folder", &f, "Rename", "edit", a.busy),
            action("delete-folder", &f, "Delete folder", "delete", a.busy)
        );
    }
    o.push_str("</div></div>");
    o
}
fn engines(a: &App) -> String {
    let mut o = String::from(
        r#"<div class="ps-scroll"><div class="ps-eng-hero"><div class="ps-eng-hero-badge">Pluggable engines</div><h1 class="ps-eng-title">Numera manages the identity.<br><span class="grad">The engine renders it.</span></h1><p class="ps-eng-lead">Every profile carries a coherent fingerprint, a proxy and a session — that part is Numera's job. Plug in the stealth backend that matches your target. Install it, pick it per profile, launch.</p><div class="ps-eng-flow"><span class="ps-flow-node">Identity</span> → <span class="ps-flow-node">Engine</span> → <span class="ps-flow-node">Real browser</span></div></div>"#,
    );
    o.push_str(&update_panel(a));
    o.push_str(&cloak(a));
    for (recommended, title) in [(true, "Recommended"), (false, "Also available")] {
        o.push_str(&section(title));
        o.push_str(r#"<div class="ps-eng-grid">"#);
        for (id, label) in ENGINES {
            let rec = ["cloak", "camoufox"].contains(&id);
            if rec != recommended {
                continue;
            }
            let (kind, desc, docs) = match id {
                "cloak" => (
                    "Patched Chromium",
                    "A real Chromium binary. Fingerprints patched at the C++ source level.",
                    "https://github.com/CloakHQ/CloakBrowser",
                ),
                "camoufox" => (
                    "Patched Firefox",
                    "A patched Firefox that spoofs at the C++ level and generates its own consistent identity.",
                    "https://github.com/daijro/camoufox",
                ),
                "patchright" => (
                    "Patched Playwright",
                    "Closes navigator.webdriver and CDP leaks the stock engine leaves behind.",
                    "https://github.com/Kaliiiiiiiiii-Vinyzu/patchright",
                ),
                _ => (
                    "Stock Chromium",
                    "Stock Chromium plus Numera's injected stealth script. Clears basic bot tests; not Cloudflare-grade.",
                    "",
                ),
            };
            let installed = if id == "playwright" {
                "Built-in"
            } else if a.engines[id] == true {
                "Installed"
            } else if a.engines[id] == false {
                "Not installed"
            } else {
                "Available"
            };
            let _ = write!(
                o,
                r#"<div class="ps-eng-card{}"><div class="ps-eng-card-top"><div><div class="ps-eng-name">{label} {}</div><div class="ps-eng-kind">{kind}</div></div><span class="ps-eng-status">{installed}</span></div><div class="ps-strength">Stealth <div class="ps-strength-bars">{}</div></div><p class="ps-eng-desc">{desc}</p><div class="ps-eng-builtin">{}</div>{}</div>"#,
                if rec { " featured" } else { "" },
                if rec {
                    r#"<span class="ps-rec">★ Recommended</span>"#
                } else {
                    ""
                },
                (1..=4)
                    .map(|n| format!(
                        r#"<span class="ps-sb{}"></span>"#,
                        if n <= if rec {
                            4
                        } else if id == "patchright" {
                            3
                        } else {
                            2
                        } {
                            " on"
                        } else {
                            ""
                        }
                    ))
                    .collect::<String>(),
                if id == "cloak" {
                    "Review the exact vendor download above."
                } else if id == "playwright" {
                    "Ships with Numera — nothing to install."
                } else {
                    "Managed by the native Rust engine. Configure its browser path through the CLI."
                },
                if docs.is_empty() {
                    String::new()
                } else {
                    format!(
                        r#"<a class="ps-eng-docs" href="{docs}" target="_blank" rel="noreferrer">Setup guide →</a>"#
                    )
                }
            );
        }
        o.push_str("</div>");
    }
    o.push_str("</div>");
    o
}
pub fn cloak(a: &App) -> String {
    let mut o = String::from(
        r#"<section class="ps-set-card ps-update-panel" aria-label="Cloak browser acquisition"><h3 class="ps-set-h">Cloak browser download</h3><p class="ps-set-note">The proprietary browser is not bundled. Review the vendor target before approving its download; opening a profile does not grant download consent.</p>"#,
    );
    if !a.live() {
        o.push_str("<p>Connect the engine to review locally installed SDK metadata.</p>");
    } else {
        o.push_str(&button(
            "cloak-review",
            "",
            "Review Cloak download",
            "ps-btn ghost sm",
            a.busy,
        ));
        let v = &a.cloak;
        if v.is_object() {
            let _ = write!(
                o,
                r#"<div class="ps-update-offer"><p>SDK {} · browser {} · {}</p><p class="ps-set-note">{}</p>"#,
                e(&v["sdk"]),
                e(&v["version"]),
                e(&v["platform"]),
                if !s(v, "reason").is_empty() {
                    e(&v["reason"])
                } else {
                    e(&v["notice"])
                }
            );
            if v["installed"] == true {
                o.push_str("<p>A browser is already available locally. Vendor background updates are disabled.</p>");
            } else if v["available"] == true && !s(v, "offer_id").is_empty() {
                let _ = write!(
                    o,
                    r#"<label class="ps-set-note"><input type="checkbox" data-action="cloak-consent" {} {}> I approve downloading this exact target directly from the vendor under its license.</label>{}"#,
                    if a.cloak_approved { "checked" } else { "" },
                    if a.busy { "disabled" } else { "" },
                    button(
                        "cloak-acquire",
                        "",
                        "Approve vendor download",
                        "ps-btn primary sm",
                        !a.cloak_approved || a.busy
                    )
                );
            }
            o.push_str("</div>");
        }
    }
    if a.busy {
        o.push_str(
            r#"<p class="ps-set-note" role="status">Waiting for the owned vendor operation…</p>"#,
        );
    }
    o.push_str("</section>");
    o
}
fn extensions(a: &App) -> String {
    let mut o = String::from(
        r#"<div class="ps-scroll"><div class="ps-eng-hero"><div class="ps-eng-hero-badge">Extensions</div><h1 class="ps-eng-title">One set of extensions,<br><span class="grad">on every new profile.</span></h1><p class="ps-eng-lead">Numera loads unpacked Chromium extensions from folders on this machine. Anything listed here is added to each profile you create from now on.</p></div>"#,
    );
    o.push_str(&section("Default set"));
    o.push_str(&input(
        "form",
        "extension",
        "Chrome Web Store link or unpacked extension folder",
        &a.form["extension"],
        "text",
        "Chrome Web Store link, or C:\\path\\to\\unpacked-extension",
    ));
    o.push_str(&primary(
        "add-extension",
        if s(&a.form, "extension").to_lowercase().starts_with("http") {
            "Install"
        } else {
            "Add"
        },
        a.busy || s(&a.form, "extension").trim().is_empty(),
    ));
    let paths = strings(&a.config["default_extensions"]);
    if paths.is_empty() {
        o.push_str(r#"<div class="ps-empty">No default extensions yet.</div>"#);
    } else {
        o.push_str(r#"<div class="ps-ext-list"><div class="ps-ext-head">Name</div>"#);
        for path in &paths {
            let meta = a.extensions.iter().find(|m| s(m, "path") == path);
            let name = meta.and_then(|m| m["name"].as_str()).unwrap_or(path);
            let _ = write!(
                o,
                r#"<div class="ps-ext-row"><span class="ps-ext-ic">{}</span><span class="ps-ext-named"><b>{}</b><code class="ps-mono ps-ext-sub">{}</code></span>{}</div>"#,
                a.icons
                    .get(path)
                    .map(|url| format!(r#"<img class="ps-ext-logo" src="{}" alt="">"#, escape(url)))
                    .unwrap_or_else(|| icon("extensions")),
                escape(name),
                escape(path),
                action(
                    "remove-extension",
                    path,
                    &format!("Remove {name}"),
                    "delete",
                    a.busy
                )
            );
        }
        o.push_str("</div>");
        o.push_str(&head("Apply to existing profiles","Adds anything missing. Extensions a profile already has are kept — nothing is removed."));
        o.push_str(&button(
            "apply-extensions",
            "",
            "Apply to all",
            "ps-btn ghost",
            a.busy || !a.live(),
        ));
    }
    o.push_str(&hint(
        "Chromium extensions do not load in Firefox-family engines; camoufox ignores this list.",
    ));
    if !a.live() {
        o.push_str(&hint(
            "Demo mode — the engine is not running, so changes here are not saved.",
        ));
    }
    o.push_str("</div>");
    o
}
fn proxy_keep(p: &Value, tab: &str) -> bool {
    match tab {
        "errors" => p["archived"] != true && s(p, "status") == "failed",
        "archive" => p["archived"] == true,
        _ => p["archived"] != true,
    }
}
pub fn proxies(a: &App) -> String {
    let mut o = String::from(r#"<div class="ps-scroll">"#);
    o.push_str(&head(
        "Proxy pool",
        "Import a list, test which are live, then assign proxies round-robin across profiles.",
    ));
    if !a.live() {
        o.push_str(&hint("Start the engine (numera serve) to import, test and assign a real proxy pool. Showing proxies in use by current profiles."));
        o.push_str(r#"<table class="ps-table"><thead><tr><th>Host</th><th>Type</th><th>Country</th><th>Used by</th></tr></thead><tbody>"#);
        let mut seen = std::collections::BTreeSet::new();
        for p in &a.profiles {
            let x = &p["proxy"];
            if x.is_object() && seen.insert(s(x, "host")) {
                let _ = write!(
                    o,
                    r#"<tr><td>{}:{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                    e(&x["host"]),
                    e(&x["port"]),
                    e(&x["type"]),
                    e(&x["country"]),
                    a.profiles
                        .iter()
                        .filter(|p| p["proxy"]["host"] == x["host"])
                        .count()
                );
            }
        }
        o.push_str("</tbody></table></div>");
        return o;
    }
    for (act, label) in [
        ("test-all-proxies", "Test all"),
        ("export-all-proxies", "Export"),
        ("assign-proxies", "Assign to profiles"),
        ("import-proxies", "Import proxies"),
    ] {
        o.push_str(&button(
            act,
            "",
            label,
            "ps-btn ghost sm",
            a.busy || (a.pool.is_empty() && act != "import-proxies"),
        ));
    }
    o.push_str(r#"<div class="ps-tabs">"#);
    for (key, label) in [("all", "All"), ("errors", "Errors"), ("archive", "Archive")] {
        let n = a.pool.iter().filter(|p| proxy_keep(p, key)).count();
        o.push_str(&button(
            "proxy-tab",
            key,
            &format!("{label} {n}"),
            if a.pool_tab == key {
                "ps-tab active"
            } else {
                "ps-tab"
            },
            false,
        ));
    }
    o.push_str("</div>");
    if !a.pool_selected.is_empty() {
        let _ = write!(
            o,
            r#"<div class="ps-bulkbar"><span>{} selected</span>"#,
            a.pool_selected.len()
        );
        for (act, label) in [
            ("test-proxies", "Test"),
            ("rotate-proxies", "Rotate"),
            ("export-proxies", "Export"),
            (
                "archive-proxies",
                if a.pool_tab == "archive" {
                    "Restore"
                } else {
                    "Archive"
                },
            ),
            ("delete-proxies", "Delete"),
            ("clear-proxies", "Clear"),
        ] {
            o.push_str(&button(act, "", label, "", a.busy));
        }
        o.push_str("</div>");
    }
    let rows = a
        .pool
        .iter()
        .filter(|p| proxy_keep(p, &a.pool_tab))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        o.push_str(r#"<div class="ps-empty">No proxies yet.</div>"#);
    } else {
        let all = rows.iter().all(|p| a.pool_selected.contains(s(p, "id")));
        let _ = write!(
            o,
            r#"<table class="ps-table"><thead><tr><th><input type="checkbox" aria-label="Select every proxy" data-action="select-all-proxies" {}></th><th>Name</th><th>Status</th><th>Profiles</th><th>Type</th><th>Host</th><th>Port</th><th>Login</th><th>Password</th><th>Actions</th></tr></thead><tbody>"#,
            if all { "checked" } else { "" }
        );
        for p in rows {
            let id = s(p, "id");
            let label = if !s(p, "label").is_empty() {
                s(p, "label")
            } else if !s(p, "country").is_empty() {
                s(p, "country")
            } else {
                s(p, "host")
            };
            let pass = text(&p["pass"]);
            let secret = if a.reveal.contains(id) {
                escape(&pass)
            } else {
                "•".repeat(pass.chars().count().min(10))
            };
            let _ = write!(
                o,
                r#"<tr><td><input type="checkbox" data-action="select-proxy" data-id="{}" aria-label="Select {}" {}></td><td>{}</td><td><span class="ps-pill {}" title="{}">{}</span></td><td>{}</td><td>{}</td><td class="ps-mono">{}</td><td>{}</td><td>{}</td><td><span class="ps-secret"><code>{secret}</code>{}{}</span></td><td><div class="ps-actions">{}{}{}{}{}</div></td></tr>"#,
                escape(id),
                escape(label),
                if a.pool_selected.contains(id) {
                    "checked"
                } else {
                    ""
                },
                escape(label),
                if s(p, "status") == "ok" {
                    "ok"
                } else if s(p, "status") == "failed" {
                    "warn"
                } else {
                    ""
                },
                e(&p["lastCheck"]["error"]),
                match s(p, "status") {
                    "ok" => format!(
                        "{} · {}ms",
                        e(&p["lastCheck"]["country"]),
                        e(&p["lastCheck"]["latencyMs"])
                    ),
                    "failed" => "Failed".into(),
                    "skipped" => "N/A".into(),
                    _ => "Untested".into(),
                },
                number(p, "profiles"),
                e(&p["type"]),
                e(&p["host"]),
                e(&p["port"]),
                e(&p["user"]),
                action(
                    "reveal-proxy",
                    id,
                    if a.reveal.contains(id) {
                        "Hide"
                    } else {
                        "Show"
                    },
                    "search",
                    false
                ),
                action("copy-proxy", id, "Copy", "clone", false),
                action("test-proxy", id, "Test", "refresh", a.busy),
                if !s(p, "rotateUrl").is_empty() {
                    action("rotate-proxy", id, "New exit IP", "refresh", a.busy)
                } else {
                    String::new()
                },
                action(
                    "archive-proxy",
                    id,
                    if p["archived"] == true {
                        "Restore"
                    } else {
                        "Archive"
                    },
                    "folders",
                    a.busy
                ),
                action("edit-proxy", id, "Edit", "edit", a.busy),
                action("delete-proxy", id, "Delete", "delete", a.busy)
            );
        }
        o.push_str("</tbody></table>");
    }
    o.push_str("</div>");
    o
}
pub fn cdp_snippet(lang: &str, port: u64) -> String {
    let base = format!("http://127.0.0.1:{port}");
    match lang {
        "Puppeteer" => format!(
            "const browser = await puppeteer.connect({{ browserURL: \"{base}\" }});\nconst [page] = await browser.pages();"
        ),
        "Selenium" => format!(
            "from selenium import webdriver\nopts = webdriver.ChromeOptions()\nopts.add_experimental_option(\"debuggerAddress\", \"127.0.0.1:{port}\")\ndriver = webdriver.Chrome(options=opts)"
        ),
        _ => format!(
            "from playwright.sync_api import sync_playwright\npw = sync_playwright().start()\nbrowser = pw.chromium.connect_over_cdp(\"{base}\")\npage = browser.contexts[0].pages[0]"
        ),
    }
}
fn automation(a: &App) -> String {
    let mut o = String::from(r#"<div class="ps-scroll">"#);
    o.push_str(&hint("Numera opens the browser wearing the identity; your script attaches to that same window over the Chrome DevTools Protocol — session, cookies, proxy and fingerprint stay exactly as set. Chromium engines only: cloak, patchright, playwright."));
    let lang = if s(&a.form, "lang").is_empty() {
        "Playwright"
    } else {
        s(&a.form, "lang")
    };
    if !a.live() {
        o.push_str(r#"<div class="ps-cdp"><div class="ps-cdp-h">Start the engine, then attach</div><pre class="ps-code">numera attach acct-01 --port 9222</pre>"#);
        o.push_str(&seg(
            "form",
            "lang",
            "Automation library",
            lang,
            &["Playwright", "Puppeteer", "Selenium"],
        ));
        let _ = write!(
            o,
            r#"<pre class="ps-code">{}</pre></div>"#,
            escape(&cdp_snippet(lang, 9222))
        );
    } else {
        let _ = write!(
            o,
            r#"<label class="ps-hcheck"><input type="checkbox" data-action="headless" {}> Headless — attach without a visible window (for servers / CI)</label><table class="ps-table"><thead><tr><th>Profile</th><th>Engine</th><th>Status</th><th>Automation</th></tr></thead><tbody>"#,
            if a.form["headless"] == true {
                "checked"
            } else {
                ""
            }
        );
        for p in &a.profiles {
            let id = s(p, "id");
            let conn = a.connections.get(id);
            let _ = write!(
                o,
                r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                e(&p["name"]),
                escape(engine_label(s(p, "engine"))),
                conn.map(|c| format!("attached :{}", number(c, "port")))
                    .unwrap_or_else(|| e(&p["status"])),
                button(
                    if conn.is_some() { "detach" } else { "attach" },
                    id,
                    if conn.is_some() { "Detach" } else { "Attach" },
                    "ps-btn ghost sm",
                    a.busy
                        || (conn.is_none()
                            && (owns_identity(s(p, "engine")) || s(p, "status") == "running"))
                )
            );
            if let Some(c) = conn {
                let _ = write!(
                    o,
                    r#"<tr class="ps-cdp-row"><td colspan="4"><div class="ps-cdp"><div class="ps-cdp-h">Connect your automation — 127.0.0.1:{}</div>{}<pre class="ps-code">{}</pre>{}</div></td></tr>"#,
                    number(c, "port"),
                    seg(
                        "form",
                        "lang",
                        "Automation library",
                        lang,
                        &["Playwright", "Puppeteer", "Selenium"]
                    ),
                    escape(&cdp_snippet(lang, number(c, "port"))),
                    btn("copy-snippet", id, "Copy")
                );
            }
        }
        o.push_str("</tbody></table>");
    }
    o.push_str("</div>");
    o
}
fn settings(a: &App) -> String {
    let mut o = String::from(r#"<div class="ps-scroll">"#);
    o.push_str(&head(
        "Settings",
        "Defaults for new profiles, appearance, and where Numera keeps your data.",
    ));
    let _ = write!(
        o,
        r#"<div class="ps-set-grid"><div class="ps-set-card"><div class="ps-set-h">Appearance</div><div class="ps-set-row"><div><div class="ps-set-label">Theme</div><div class="ps-set-note">Switch the whole console between light and dark.</div></div>{}</div></div><div class="ps-set-card"><div class="ps-set-h">Default launch engine</div><p class="ps-set-note">Every new profile starts on this engine — you can still change it per profile.</p><div class="ps-seg wrap">"#,
        btn(
            "theme",
            "",
            if a.theme == "light" { "Dark" } else { "Light" }
        )
    );
    for (id, l) in ENGINES {
        o.push_str(&button(
            "default-engine",
            id,
            &format!(
                "{l}{}",
                if a.engines[id] == false {
                    " · not installed"
                } else {
                    ""
                }
            ),
            if a.default_engine() == id { "on" } else { "" },
            a.busy,
        ));
    }
    let _ = write!(
        o,
        r#"</div></div><div class="ps-set-card"><div class="ps-set-h">Storage</div><div class="ps-set-label">Data location</div><pre class="ps-code">{}</pre><p class="ps-set-note">Profiles, sessions, proxies and settings all live here on your machine. Back up this folder to move everything to another computer.</p><div class="ps-set-counts"><span><b>{}</b> profiles</span><span><b>{}</b> folders</span><span><b>{}</b> proxies</span></div></div>"#,
        if a.live() {
            e(&a.config["root"])
        } else {
            "Connect the engine (numera serve) to see where your data is stored. In demo mode nothing is written to disk.".into()
        },
        a.profiles.len(),
        a.folders().len(),
        a.config["proxyCount"]
            .as_u64()
            .unwrap_or(a.pool.len() as u64)
    );
    o.push_str(&update_panel(a));
    o.push_str(r#"<div class="ps-set-card"><div class="ps-set-h">API tokens</div>"#);
    if !a.live() {
        o.push_str(&hint(
            "Connect the engine (numera serve) to mint and revoke tokens.",
        ));
    } else {
        if a.minted.is_object() {
            let _ = write!(
                o,
                r#"<div class="ps-token-new"><div class="ps-set-label">Copy it now — this is the only time it is shown</div><div class="ps-token-value"><code>{}</code>{}</div>{}</div>"#,
                e(&a.minted["token"]),
                btn("copy-token", "", "Copy"),
                btn("dismiss-token", "", "Done")
            );
        }
        o.push_str(r#"<table class="ps-table ps-token-table"><thead><tr><th>Name</th><th>Created</th><th>Last used</th><th></th></tr></thead><tbody>"#);
        for t in list(&a.tokens["tokens"]) {
            let _ = write!(
                o,
                r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                e(&t["name"]),
                date(&t["created"]),
                if t["lastUsed"].is_null() {
                    "never".into()
                } else {
                    date(&t["lastUsed"])
                },
                action("revoke-token", s(&t, "id"), "Revoke", "delete", a.busy)
            );
        }
        o.push_str("</tbody></table>");
        o.push_str(&input(
            "form",
            "tokenName",
            "Token name",
            &a.form["tokenName"],
            "text",
            "What is it for? e.g. CI runner",
        ));
        o.push_str(&primary(
            "create-token",
            "Create",
            a.busy || s(&a.form, "tokenName").trim().is_empty(),
        ));
        o.push_str(&hint(if a.tokens["envToken"]==true{"NUMERA_API_TOKEN is also set, and stays valid. Revoking every token here will not reopen the API while it is."}else if list(&a.tokens["tokens"]).is_empty(){"No tokens: the API is open to anything that can reach this machine. That is fine on localhost and not fine behind a tunnel."}else{"The API now requires one of these on every request."}));
    }
    let _ = write!(
        o,
        r#"</div><div class="ps-set-card"><div class="ps-set-h">About</div><div class="ps-set-row"><div class="ps-set-label">Numera Browser</div><span class="ps-pill">{}</span></div><p class="ps-set-note">Local-first browser-profile manager for authorized testing and automation. {}</p></div></div></div>"#,
        if s(&a.config, "version").is_empty() {
            "version unknown".into()
        } else {
            format!("v{}", e(&a.config["version"]))
        },
        if a.live() {
            "Engine connected."
        } else {
            "Running in demo mode — start numera serve to go live."
        }
    );
    o
}
#[cfg(target_arch = "wasm32")]
fn date(v: &Value) -> String {
    js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(
        v.as_f64().unwrap_or(0.) * 1000.,
    ))
    .to_locale_date_string("en", &js_sys::Object::new())
    .into()
}
#[cfg(not(target_arch = "wasm32"))]
fn date(v: &Value) -> String {
    text(v)
}
pub fn update_panel(a: &App) -> String {
    let mut o = String::from(
        r#"<section class="ps-set-card ps-update-panel" aria-label="Software updates"><h3 class="ps-set-h">Software updates</h3>"#,
    );
    if a.update_bridge.is_none() {
        o.push_str(r#"<p class="ps-set-note">Source and server installations use their package or deployment workflow. Open a signed portable desktop build for managed updates.</p></section>"#);
        return o;
    }
    o.push_str(r#"<p class="ps-set-note">The desktop checks GitHub after startup and once a day while open. Nothing downloads or installs without your approval.</p>"#);
    let v = &a.update;
    let _ = write!(
        o,
        r#"<p class="ps-update-current">Installed: {} · generation {}<br><code>{}</code> · {}</p><p class="ps-set-note" role="{}">{}</p>"#,
        e(&v["current"]["version"]),
        e(&v["current"]["generation"]),
        escape(
            &s(&v["current"], "sourceSha")
                .chars()
                .take(12)
                .collect::<String>()
        ),
        e(&v["current"]["platform"]),
        if !a.update_error.is_empty() || s(v, "state") == "error" {
            "alert"
        } else {
            "status"
        },
        if a.update_error.is_empty() {
            if s(v, "message").is_empty() {
                "Connecting to the desktop update service…".into()
            } else {
                e(&v["message"])
            }
        } else {
            escape(&a.update_error)
        }
    );
    let offer = &v["offer"];
    if offer.is_object() {
        let _ = write!(
            o,
            r#"<div class="ps-update-offer"><div class="ps-set-label">Available: {} · generation {}</div><p class="ps-set-note">{} · {} download · restart required</p><details><summary>Browser and SDK versions</summary><pre class="ps-code">{}</pre></details></div>"#,
            e(&offer["version"]),
            e(&offer["generation"]),
            escape(&s(offer, "sourceSha").chars().take(12).collect::<String>()),
            updates::bytes(number(offer, "downloadBytes")),
            escape(&serde_json::to_string_pretty(&offer["components"]).unwrap_or_default())
        );
    }
    if s(v, "state") == "downloading" {
        let total = number(&v["progress"], "totalBytes");
        let received = number(&v["progress"], "receivedBytes");
        let percent = received
            .saturating_mul(100)
            .checked_div(total)
            .unwrap_or(0)
            .min(100);
        let _ = write!(
            o,
            r#"<div class="ps-update-progress"><progress max="100" value="{percent}" aria-label="Update download progress"></progress><span>{} / {} ({percent}%)</span></div>"#,
            updates::bytes(received),
            updates::bytes(total)
        );
    }
    o.push_str(r#"<div class="ps-update-actions">"#);
    o.push_str(&button(
        "update-check",
        "",
        "Check for updates",
        "ps-btn ghost sm",
        a.update_busy || updates::active(v),
    ));
    if offer.is_object()
        && !a.update_busy
        && !updates::active(v)
        && a.update_error.is_empty()
        && v["current"]["portable"] == true
        && ["available", "ready", "deferred", "error"].contains(&s(v, "state"))
    {
        o.push_str(&button(
            "update-review",
            "",
            if ["ready", "deferred"].contains(&s(v, "state")) {
                "Install and restart"
            } else {
                "Download update"
            },
            "ps-btn primary sm",
            false,
        ));
    }
    if v["canCancel"] == true {
        o.push_str(&button(
            "update-cancel",
            "",
            "Cancel update",
            "ps-btn ghost sm",
            a.update_busy,
        ));
    }
    o.push_str("</div></section>");
    o
}
fn update_approval(a: &App) -> String {
    let offer = &a.update_approval["offer"];
    let install = a.update_approval["install"] == true;
    let valid = updates::approval_valid(
        &a.update,
        &a.update_approval,
        a.update_busy,
        &a.update_error,
    );
    format!(
        r#"<div class="ps-overlay center"><div class="ps-modal ps-update-dialog" role="dialog" aria-modal="true" aria-label="Confirm update"><div class="ps-drawer-h"><h3 class="ps-drawer-title">{}</h3></div><div class="ps-update-body"><p>Numera Browser {} · generation {}</p><p>{}</p><p>Managed browser SDKs and binaries update together. Profiles, vault data and external browser overrides are not replaced.</p>{}</div><div class="ps-drawer-foot">{}{}</div></div></div>"#,
        if install {
            "Install this update and restart?"
        } else {
            "Download this update?"
        },
        e(&offer["version"]),
        e(&offer["generation"]),
        if install {
            "The app will restart after active sessions and jobs are closed. Busy or uncertain work defers installation; it is not force-stopped.".into()
        } else {
            format!(
                "Download {}. The extracted update needs {}, plus space for recovery.",
                updates::bytes(number(offer, "downloadBytes")),
                updates::bytes(number(offer, "expandedBytes"))
            )
        },
        if valid {
            ""
        } else {
            r#"<p role="alert">This offer changed. Close this dialog and review the current update.</p>"#
        },
        btn("update-later", "", "Later"),
        primary(
            "update-approve",
            if install {
                "Approve restart"
            } else {
                "Approve download"
            },
            !valid
        )
    )
}
pub fn modal(a: &App) -> String {
    if a.modal == "commands" {
        let mut o = format!(
            r#"<div class="ps-cmd-scrim"><div class="ps-cmd" role="dialog" aria-label="Command palette"><div class="ps-cmd-in"><input id="command-query" data-scope="app" data-field="command" placeholder="Search commands and profiles…" value="{}"><kbd class="ps-kbd">esc</kbd>{}</div><div class="ps-cmd-list">"#,
            escape(&a.command),
            btn("close-modal", "", "×")
        );
        for (i, (label, _, _)) in a.commands().iter().enumerate() {
            o.push_str(&button(
                "command",
                &i.to_string(),
                &escape(label),
                if a.command_index == i {
                    "ps-cmd-item on"
                } else {
                    "ps-cmd-item"
                },
                false,
            ));
        }
        o.push_str("</div></div></div>");
        return o;
    }
    let (title, body, action, label, disabled) = match a.modal.as_str() {
        "bulk" => {
            let b = &a.batch;
            let mut body = row(input(
                "batch",
                "count",
                "How many (up to 10,000)",
                &json!(b.count),
                "number",
                "",
            ) + &select(
                "batch",
                "engine",
                "Launch engine",
                &json!(b.engine),
                engine_opts("— engine default —"),
                false,
            ));
            for (key, label, options, selected) in [
                (
                    "os",
                    "Operating systems (spread across)",
                    OS.to_vec(),
                    b.os.clone(),
                ),
                (
                    "countries",
                    "Countries / locales (spread across)",
                    COUNTRIES.to_vec(),
                    b.countries.clone(),
                ),
                (
                    "versions",
                    "Browser versions (Chrome)",
                    VERSIONS.to_vec(),
                    b.versions.clone(),
                ),
            ] {
                let _ = write!(
                    body,
                    r#"<div class="ps-field" role="group" aria-label="{label}"><span class="ps-flabel">{label}</span><div class="ps-chiprow">"#
                );
                for v in options {
                    body.push_str(&button(
                        "batch-chip",
                        &format!("{key}|{v}"),
                        v,
                        if selected.contains(&v.to_string()) {
                            "ps-chip-btn on"
                        } else {
                            "ps-chip-btn"
                        },
                        false,
                    ));
                }
                body.push_str("</div></div>");
            }
            for (key, label, value, kind) in [
                ("prefix", "Name prefix", json!(b.prefix), "text"),
                ("start", "Start number", json!(b.start), "number"),
                ("tags", "Tags", json!(b.tags), "text"),
            ] {
                body.push_str(&input("batch", key, label, &value, kind, ""));
            }
            body.push_str(&toggle(
                "batch-randomize",
                "",
                if b.randomize {
                    "Randomized per profile"
                } else {
                    "Fixed per OS"
                },
                b.randomize,
                false,
            ));
            body.push_str(&hint("These are created without a proxy. Attach real proxies in bulk from Proxies → Assign to profiles."));
            (
                "Generate profiles at scale".into(),
                body,
                "create-batch",
                if a.busy {
                    format!("Creating {}/{}…", a.batch_progress, b.count)
                } else {
                    format!("Create {} profiles", b.count)
                },
                a.busy,
            )
        }
        "sync" => {
            let mut body = select(
                "form",
                "engine",
                "Launch engine",
                &a.form["engine"],
                engine_opts("— leave unchanged —"),
                false,
            );
            let mut locale = vec![(String::new(), "— leave unchanged —".into())];
            locale.extend(LOCALES.map(|(v, _)| (v.into(), v.into())));
            body.push_str(&select(
                "form",
                "locale",
                "Locale (moves timezone too)",
                &a.form["locale"],
                locale,
                false,
            ));
            let mut folders = vec![(String::new(), "— leave unchanged —".into())];
            folders.extend(a.folders().into_iter().map(|f| (f.clone(), f)));
            body.push_str(&select(
                "form",
                "folder",
                "Move to folder",
                &a.form["folder"],
                folders,
                false,
            ));
            body.push_str(&input(
                "form",
                "tags",
                "Add tags (comma separated)",
                &a.form["tags"],
                "text",
                "q3-campaign",
            ));
            body.push_str(&hint("Blank fields are left alone, so you can run this again later to change one more thing."));
            (
                format!(
                    "Edit {} profiles · Change once, apply to all",
                    a.selected_profiles().len()
                ),
                body,
                "apply-sync",
                format!("Apply to {}", a.selected_profiles().len()),
                a.busy || sync_patch(&a.form).as_object().is_none_or(|m| m.is_empty()),
            )
        }
        "edit-proxy" => {
            let mut body = String::new();
            for (key, label) in [
                ("label", "Name"),
                ("country", "Country"),
                ("host", "Host"),
                ("port", "Port"),
                ("user", "Login"),
                ("pass", "Password"),
                ("rotateUrl", "Change-IP URL (rotating proxies)"),
            ] {
                body.push_str(&input(
                    "form",
                    key,
                    label,
                    &a.form[key],
                    if key == "pass" { "password" } else { "text" },
                    "",
                ));
            }
            body.push_str(&select(
                "form",
                "type",
                "Type",
                &a.form["type"],
                opts(&["HTTP", "HTTPS", "SOCKS5", "SOCKS4"]),
                false,
            ));
            (
                "Edit proxy".into(),
                body,
                "save-proxy",
                "Save".into(),
                a.busy,
            )
        }
        "import-proxies" => (
            "Import proxies".into(),
            textarea(
                "form",
                "text",
                "Paste one proxy per line",
                &a.form["text"],
                9,
                "host:port\nhost:port:user:pass\nsocks5://host:port",
            ) + &btn("load-proxy-file", "", "Load from file")
                + &hint(
                    "Understands host:port, host:port:user:pass, user:pass@host:port and scheme://…, with an optional trailing country code.",
                ),
            "add-proxies",
            "Add proxies".into(),
            a.busy || s(&a.form, "text").trim().is_empty(),
        ),
        "rename-folder" => (
            "Rename folder".into(),
            input("form", "to", "Folder name", &a.form["to"], "text", ""),
            "commit-folder",
            "Rename".into(),
            a.busy || s(&a.form, "to").trim().is_empty(),
        ),
        _ => (
            s(&a.form, "title").into(),
            format!("<p>{}</p>", e(&a.form["body"])),
            "confirm",
            text(&a.form["label"]),
            a.busy,
        ),
    };
    format!(
        r#"<div class="ps-overlay center"><div class="ps-modal ps-pop" role="dialog" aria-modal="true" aria-label="{}"><div class="ps-drawer-h"><div class="ps-drawer-title">{}</div>{}</div><div style="padding:20px;max-height:65vh;overflow:auto">{body}</div><div class="ps-drawer-foot">{}{}</div></div></div>"#,
        escape(&title),
        escape(&title),
        button("close-modal", "", &icon("x"), "ps-x", a.busy),
        button("close-modal", "", "Cancel", "ps-btn ghost", a.busy),
        button(
            action,
            "",
            &escape(&label),
            if a.modal == "confirm" {
                "ps-btn danger"
            } else {
                "ps-btn primary"
            },
            disabled
        )
    )
}
