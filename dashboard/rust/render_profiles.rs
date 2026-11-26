use super::*;
use ferrav::{Avatar, ColorOptions};
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
        if a.live() {
            o.push_str(&button(
                "fleet-stop",
                "",
                "Stop selected windows",
                "ps-btn ghost sm",
                a.busy || !selected.iter().any(|p| p["status"] == "running"),
            ));
        }
        o.push_str("</div>");
    }
    let active_filters = a.filters.values().filter(|value| !value.is_empty()).count();
    let filter_label = if active_filters == 0 {
        "Filter".into()
    } else {
        format!("Filter · {active_filters}")
    };
    let _ = write!(
        o,
        r#"<div class="ps-toolbar"><div class="ps-search">{}<input id="profile-search" placeholder="Search profiles, tags…" data-scope="app" data-field="query" value="{}"></div><div class="ps-crumb">{} {}</div><div style="flex:1"></div><div class="ps-filter-wrap">{}"#,
        icon_at("search", 15),
        escape(&a.query),
        icon_at("folder-tree", 13),
        escape(&a.folder),
        button(
            "filters",
            "",
            &format!("{} {filter_label}", icon_at("filter", 13)),
            if active_filters > 0 {
                "ps-btn ghost sm active"
            } else {
                "ps-btn ghost sm"
            },
            false
        )
    );
    if a.filter_open {
        o.push_str(r#"<div class="ps-filter-pop"><div class="ps-filter-head"><span>Filter profiles</span>"#);
        if active_filters > 0 {
            o.push_str(&button(
                "clear-filters",
                "",
                "Clear all",
                "ps-filter-clear",
                false,
            ));
        }
        o.push_str("</div>");
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
    o.push_str(r#"</div></div><div class="ps-scroll">"#);
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
                r#"</div></div></div></td><td><div class="ps-env"><span class="ps-os">{}</span><span class="ps-mono{}">{}</span>{}</div></td><td><span class="ps-engine-chip">{}</span></td><td><div class="ps-proxy{}">{}</div></td><td><span class="ps-pill {}" title="{}">{}{}</span></td><td class="ps-mono dim">{}</td><td><div class="ps-actions">"#,
                e(&p["os"]),
                if owns_identity(s(p, "engine")) {
                    " dim"
                } else {
                    ""
                },
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
                if p["proxy"].is_object() { "" } else { " off" },
                if p["proxy"].is_object() {
                    format!(
                        r#"<span style="color:var(--mint)">{}</span><span class="ps-mono">{}</span><span class="ps-mono dim">{}</span>"#,
                        icon_at("wifi", 13),
                        e(&p["proxy"]["country"]),
                        e(&p["proxy"]["type"])
                    )
                } else {
                    format!("{} Direct", icon_at("wifi-off", 13))
                },
                if issues.is_empty() { "ok" } else { "warn" },
                escape(&issues.join("\n")),
                icon_at("shield", 12),
                if issues.is_empty() {
                    "Coherent".into()
                } else {
                    format!(
                        "{} issue{}",
                        issues.len(),
                        if issues.len() == 1 { "" } else { "s" }
                    )
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
            if a.live() {
                o.push_str(&action(
                    "profile-ip-check",
                    id,
                    "Check IP readiness",
                    "search",
                    a.busy,
                ));
            }
            o.push_str("</div>");
            if let Some(result) = a.ip_checks.get(id) {
                o.push_str(&release::ip_readiness(result));
            }
            o.push_str("</td></tr>");
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
    let icon = Avatar::builder()
        .text(if id.is_empty() { "?" } else { id })
        .size(30)
        .padding(0.06)
        .colors(ColorOptions {
            hues: Vec::new(),
            saturation: 0.46,
            grayscale_saturation: 0.12,
            lightness: (0.44, 0.74),
            grayscale_lightness: (0.32, 0.62),
        })
        .build()
        .map(|avatar| avatar.to_svg())
        .unwrap_or_default();
    format!(
        r#"<span class="ps-identicon" style="width:30px;height:30px" aria-hidden="true">{icon}</span>"#
    )
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
