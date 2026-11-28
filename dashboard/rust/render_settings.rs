use super::*;
use crate::updates;
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
pub(super) fn automation(a: &App) -> String {
    let mut o = String::from(r#"<div class="ps-scroll">"#);
    o.push_str(&hint("Numera opens the browser wearing the identity; your script attaches to that same window over the Chrome DevTools Protocol — session, cookies, proxy and fingerprint stay exactly as set. Chromium engines only: cloak, patchright, playwright."));
    let lang = if s(&a.form, "lang").is_empty() {
        "Playwright"
    } else {
        s(&a.form, "lang")
    };
    if !a.live() {
        o.push_str(r#"<div class="ps-cdp"><div class="ps-cdp-h">Start the engine, then attach</div><pre class="ps-code">numera-engine attach acct-01 --port 9222</pre>"#);
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
pub(super) fn settings(a: &App) -> String {
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
            "Connect the engine (numera-engine serve) to see where your data is stored. In demo mode nothing is written to disk.".into()
        },
        a.profiles.len(),
        a.folders().len(),
        a.config["proxyCount"]
            .as_u64()
            .unwrap_or(a.pool.len() as u64)
    );
    o.push_str(&update_panel(a));
    o.push_str(&release::update_controls(a));
    o.push_str(&release::bridge_status(a));
    o.push_str(r#"<div class="ps-set-card"><div class="ps-set-h">API tokens</div>"#);
    if !a.live() {
        o.push_str(&hint(
            "Connect the engine (numera-engine serve) to mint and revoke tokens.",
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
            "Manager connected."
        } else {
            "Running in demo mode — start numera-engine serve to go live."
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
pub(super) fn update_approval(a: &App) -> String {
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
