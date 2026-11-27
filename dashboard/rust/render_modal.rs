use super::*;
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
            let mut names = b.names();
            let first = escape(&names.next().unwrap_or_default());
            let range = names
                .next_back()
                .map_or(first.clone(), |last| format!("{first} … {}", escape(&last)));
            let _ = write!(
                body,
                r#"<p class="ps-hint ps-batch-preview" role="status">Preview: <strong>{range}</strong> · OS: {} · Countries: {}</p>"#,
                escape(&b.os.join(" / ")),
                escape(&b.countries.join(" / "))
            );
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
