#![cfg(not(target_arch = "wasm32"))]

use numera_dashboard::{
    model::{App, Editor, new_profile},
    render,
};
use serde_json::json;

#[test]
fn proxy_country_badge_names_the_country_and_marks_unknown() {
    let mut a = App {
        view: "proxies".into(),
        mode: "live".into(),
        ..App::default()
    };
    a.pool = vec![
        json!({"id":"recorded","lastCheck":{"ok":true,"country_code":"US","country_name":"United States","country_flag":"\u{1f1fa}\u{1f1f8}"}}),
        json!({"id":"unknown","lastCheck":{"ok":true,"country_code":null,"country_name":null,"country_flag":"--"}}),
    ];
    let html = render::view(&a);
    assert!(html.contains("United States (US)"));
    assert!(html.contains("Unknown country"));
}

#[test]
fn saved_profile_exposes_both_cookie_formats_and_ip_check() {
    let mut a = App {
        mode: "live".into(),
        ..App::default()
    };
    let mut p = new_profile("", "playwright");
    p["id"] = json!("p1");
    p["_new"] = json!(false);
    a.profiles = vec![p.clone()];
    let mut ed = Editor::new(p);
    ed.tab = "advanced".into();
    let html = render::editor_view(&a, &ed);
    assert!(html.contains("Export cookies (JSON)"));
    assert!(html.contains("Export cookies (Netscape)"));
    assert!(render::profiles(&a).contains("Check IP readiness"));
}

#[test]
fn selected_profiles_can_be_stopped_from_the_bulk_bar() {
    let mut a = App {
        mode: "live".into(),
        ..App::default()
    };
    let mut p = new_profile("", "playwright");
    p["id"] = json!("p1");
    a.profiles = vec![p];
    a.selected.insert("p1".into());
    assert!(render::profiles(&a).contains("Stop selected windows"));
}

#[test]
fn settings_distinguish_app_and_browser_updates_without_a_precheck_apply() {
    let a = App {
        view: "settings".into(),
        ..App::default()
    };
    let html = render::view(&a);
    for label in [
        "Check for app updates",
        "Check for browser updates",
        "Check for App + Browsers updates",
        "Automatically update the app",
        "Automatically update browser cores",
        "Clean old versions after updating",
        "MCP bridge",
    ] {
        assert!(html.contains(label), "Missing settings control: {label}");
    }
    assert!(!html.contains("data-action=\"release-apply\""));
}

#[test]
fn reduced_motion_never_shows_the_silver_canvas() {
    let css = include_str!("../assets/app.css");
    let (_, reduced) = css
        .split_once("@media (prefers-reduced-motion: reduce)")
        .expect("Reduced-motion rules");
    assert!(reduced.contains(".ps-theme-sweep { display: none; }"));
}
