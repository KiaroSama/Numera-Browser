use super::*;
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
pub(super) fn health(a: &App) -> String {
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
pub(super) fn folders(a: &App) -> String {
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
