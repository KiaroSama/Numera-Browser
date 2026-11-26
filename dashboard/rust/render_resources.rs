use super::*;
pub(super) fn engines(a: &App) -> String {
    let mut o = String::from(
        r#"<div class="ps-scroll"><div class="ps-eng-hero"><div class="ps-eng-hero-badge">Browser backends</div><h1 class="ps-eng-title">Manage profiles in Numera.<br><span class="grad">Launch them in a browser.</span></h1><p class="ps-eng-lead">Numera stores your profile settings and manages each session. The selected backend connects the manager to a compatible browser that opens your pages.</p><div class="ps-eng-flow"><span class="ps-flow-node">Profile manager</span> → <span class="ps-flow-node">Runtime adapter</span> → <span class="ps-flow-node">Browser</span></div></div>"#,
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
                    "Chromium browser",
                    "Uses the separately licensed CloakBrowser executable.",
                    "https://github.com/CloakHQ/CloakBrowser",
                ),
                "camoufox" => (
                    "Firefox browser",
                    "Opens a Camoufox session through its compatible driver.",
                    "https://github.com/daijro/camoufox",
                ),
                "patchright" => (
                    "Chromium SDK",
                    "Opens a compatible Chromium session through the Patchright SDK.",
                    "https://github.com/Kaliiiiiiiiii-Vinyzu/patchright",
                ),
                _ => (
                    "Chromium SDK",
                    "Opens the matched Chromium build through the Playwright SDK.",
                    "",
                ),
            };
            let installed = if a.engines[id] == true {
                "SDK installed"
            } else if a.engines[id] == false {
                "SDK missing"
            } else {
                "SDK status unknown"
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
                    "Portable packages include the matched SDK and Chromium."
                } else {
                    "The manager uses bundled browser resources or a compatible path configured through the CLI."
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
        o.push_str("<p>Connect the manager to review locally installed SDK metadata.</p>");
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
pub(super) fn extensions(a: &App) -> String {
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
            r#"<table class="ps-table"><thead><tr><th><input type="checkbox" aria-label="Select every proxy" data-action="select-all-proxies" {}></th><th>Name</th><th>Status</th><th>Country</th><th>Profiles</th><th>Type</th><th>Host</th><th>Port</th><th>Login</th><th>Password</th><th>Actions</th></tr></thead><tbody>"#,
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
                r#"<tr><td><input type="checkbox" data-action="select-proxy" data-id="{}" aria-label="Select {}" {}></td><td>{}</td><td><span class="ps-pill {}" title="{}">{}</span></td><td>{}</td><td>{}</td><td>{}</td><td class="ps-mono">{}</td><td>{}</td><td>{}</td><td><span class="ps-secret"><code>{secret}</code>{}{}</span></td><td><div class="ps-actions">{}{}{}{}{}</div></td></tr>"#,
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
                    "ok" => match p["lastCheck"]["latencyMs"].as_u64() {
                        Some(ms) => format!("Live · {ms} ms"),
                        None => "Live".into(),
                    },
                    "failed" => "Failed".into(),
                    "skipped" => "N/A".into(),
                    _ => "Untested".into(),
                },
                release::country_badge(&p["lastCheck"]),
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
