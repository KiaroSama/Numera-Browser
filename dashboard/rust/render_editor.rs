use super::*;
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
            o.push_str(&section("Network identity"));
            o.push_str(&seg(
                "editor",
                "mac_address_mode",
                "MAC address",
                if s(p, "mac_address_mode").is_empty() {
                    "off"
                } else {
                    s(p, "mac_address_mode")
                },
                &["off", "manual", "generate", "noise"],
            ));
            o.push_str(&input(
                "editor",
                "mac_address",
                "MAC address (AA:BB:CC:DD:EE:FF)",
                &p["mac_address"],
                "text",
                "02:1A:2B:3C:4D:5E",
            ));
            o.push_str(&input(
                "editor",
                "device_name",
                "Device name",
                &p["device_name"],
                "text",
                "DESKTOP-NUMERA",
            ));
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
                for (format, label) in [
                    ("json", "Export cookies (JSON)"),
                    ("netscape", "Export cookies (Netscape)"),
                ] {
                    o.push_str(&button(
                        "export-cookies",
                        format,
                        label,
                        "ps-btn ghost sm",
                        a.busy,
                    ));
                }
                o.push_str(&hint(
                    "Cookie exports can contain active sessions. Keep the downloaded file private.",
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
