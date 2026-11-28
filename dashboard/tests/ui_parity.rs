#![cfg(not(target_arch = "wasm32"))]

use numera_dashboard::{
    model::{App, Batch, Editor, new_profile},
    render,
};
use serde_json::json;

#[test]
fn engine_cards_show_copyable_sdk_commands_only_for_optional_engines() {
    let app = App {
        view: "engines".into(),
        ..App::default()
    };
    let html = render::view(&app);
    let command = "cargo run --manifest-path tools/xtask/Cargo.toml --locked -- provision-resources build/vendor-resources";
    for engine in ["cloak", "camoufox", "patchright"] {
        assert!(
            html.contains(&format!("<code>{command}</code>")),
            "{engine}"
        );
        assert!(html.contains(&format!(
            r#"data-action="copy-engine-install" data-id="{engine}""#
        )));
    }
    assert!(!html.contains(r#"data-action="copy-engine-install" data-id="playwright""#));
    assert!(!html.contains("pip install"));
    assert!(html.contains("docs/setup.md#explicit-browser-resources"));
    assert!(html.contains("Portable packages include the matched SDK and Chromium."));
}

#[test]
fn failed_view_can_retry_without_losing_the_dashboard_shell() {
    let app = App {
        view: "profiles".into(),
        profiles: vec![json!("unreadable profile")],
        ..App::default()
    };
    let html = render::app(&app);
    assert!(html.contains(r#"data-action="retry-view""#));
    assert!(html.contains("Try again"));
    assert!(html.contains(r#"class="ps-nav""#));
    assert!(!html.contains(r#"class="ps-table""#));
}

#[test]
fn failed_editor_can_close_without_losing_the_view() {
    let app = App {
        editor: Some(Editor::new(json!(null))),
        ..App::default()
    };
    let html = render::app(&app);
    assert!(html.contains("This profile couldn't be opened"));
    assert!(html.contains(r#"aria-modal="true""#));
    assert!(html.contains(r#"data-action="close-editor""#));
    assert!(html.contains(r#"class="ps-table""#));
    assert!(!html.contains(r#"id="editor-name""#));
}

#[test]
fn startup_error_escapes_details_and_provides_retry() {
    let mut app = App::default();
    app.startup_failed("<script>failure</script>", "http://127.0.0.1:8787/");
    let html = render::app(&app);
    assert!(html.contains("The Numera engine did not start."));
    assert!(html.contains("&lt;script&gt;failure&lt;/script&gt;"));
    assert!(html.contains(r#"data-action="retry-startup""#));
    assert!(!html.contains(r#"data-action="close-editor""#));
    for embedded in [
        "tauri://localhost/",
        "http://tauri.localhost/",
        "https://tauri.localhost/",
    ] {
        app.startup_failed("<script>failure</script>\0", embedded);
        let html = render::app(&app);
        assert!(html.contains("close Numera Browser and open it again"));
        assert!(html.contains("&lt;script&gt;failure&lt;/script&gt;"));
        assert!(!html.contains(r#"data-action="retry-startup""#));
        assert!(!app.auth_error.contains('\0'));
    }
}

#[test]
fn no_match_and_empty_library_have_different_next_actions() {
    let mut app = App {
        profiles: vec![new_profile("Example", "playwright")],
        query: "no-match".into(),
        ..App::default()
    };
    let search = render::profiles(&app);
    assert!(search.contains("No profiles match your search."));
    assert!(search.contains("Clear search"));
    assert!(!search.contains("No profiles have been created yet."));

    app.query.clear();
    app.filters.insert("engine".into(), "camoufox".into());
    let filter = render::profiles(&app);
    assert!(filter.contains("No profiles match your filters."));
    assert!(filter.contains("Clear filters"));

    app.profiles.clear();
    let empty = render::profiles(&app);
    assert!(empty.contains("No profiles have been created yet."));
    assert!(empty.contains("Create profile"));
    assert!(!empty.contains(r#"data-action="clear-search""#));
}

#[test]
fn batch_preview_matches_generated_names_and_selected_settings() {
    let mut app = App {
        modal: "bulk".into(),
        batch: Batch {
            prefix: "<Team>".into(),
            count: 3,
            start: 98,
            os: vec!["Windows".into(), "Linux".into()],
            countries: vec!["US".into(), "DE".into()],
            ..Batch::default()
        },
        ..App::default()
    };
    let names = app.batch.build("Example", |_| 0);
    let first = render::escape(names[0]["name"].as_str().unwrap_or_default());
    let last = render::escape(names[2]["name"].as_str().unwrap_or_default());
    let html = render::modal(&app);
    assert!(html.contains(&format!("{first} … {last}")));
    assert!(html.contains("Windows / Linux") && html.contains("US / DE"));
    assert!(!html.contains("<Team>"));

    app.batch.count = 1;
    app.batch.countries = vec!["DE".into()];
    let html = render::modal(&app);
    assert!(html.contains("&lt;Team&gt;-98</strong>"));
    assert!(!html.contains("US / DE"));
}
