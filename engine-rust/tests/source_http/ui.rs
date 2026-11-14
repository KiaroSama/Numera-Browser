use super::*;

use numera_dashboard::{
    contracts,
    model::{self, App, Editor},
    render, updates,
};
fn application() -> App {
    let mut app = App {
        mode: "live".into(),
        ..App::default()
    };
    let mut p = model::new_profile("QA", "playwright");
    p["id"] = json!("ui-profile");
    p["name"] = json!("Visible Fixture");
    p["_new"] = json!(false);
    app.profiles = vec![p];
    app.config = json!({"folders":["QA"],"default_engine":"playwright","default_extensions":[],"root":"isolated-data"});
    app
}
fn component(file: &str) -> (String, &'static str) {
    let mut app = application();
    match file {
        "App.tsx" => (render::app(&app), "Numera"),
        "main.tsx" => {
            app.mode = "locked".into();
            (render::app(&app), "token")
        }
        "toast.tsx" => {
            app.say("Saved fixture", false);
            (render::app(&app), "Saved fixture")
        }
        "ui.tsx" => (
            render::input(
                "editor",
                "notes",
                "Fixture field",
                &json!("one & two"),
                "text",
                "",
            ),
            "one &amp; two",
        ),
        "AutomationView.tsx" => {
            app.view = "automation".into();
            (render::view(&app), "Automation")
        }
        "CloakAcquisition.tsx" => {
            app.cloak = json!({"engine":"cloak","available":true,"installed":false,"offer_id":"1111111111111111111111111111111111111111111111111111111111111111","version":"146.0.0.1","reason":null});
            (render::cloak(&app), "146.0.0.1")
        }
        "CommandPalette.tsx" => {
            app.modal = "commands".into();
            (render::modal(&app), "Command palette")
        }
        "Editor.tsx" => {
            let editor = Editor::new(app.profiles[0].clone());
            (render::editor_view(&app, &editor), "Profile name")
        }
        "EnginesView.tsx" => {
            app.view = "engines".into();
            (render::view(&app), "Camoufox")
        }
        "ExtensionsView.tsx" => {
            app.view = "extensions".into();
            (render::view(&app), "Extensions")
        }
        "FoldersView.tsx" => {
            app.view = "folders".into();
            (render::view(&app), "QA")
        }
        "HealthView.tsx" => {
            app.view = "health".into();
            (render::view(&app), "ps-health-summary")
        }
        "IpSwitch.tsx" => {
            let mut editor = Editor::new(app.profiles[0].clone());
            editor.tab = "fingerprint".into();
            (render::editor_view(&app, &editor), "data-action=\"ip\"")
        }
        "ProbePanel.tsx" => (
            render::probe(
                &json!({"score":62,"grade":"C","passed":1,"total":2,"checks":[{"name":"canvas","ok":false,"detail":"noisy"}]}),
            ),
            "canvas",
        ),
        "ProfileSummary.tsx" => (render::summary(&app.profiles[0]), "1920"),
        "ProfilesView.tsx" => (render::profiles(&app), "Visible Fixture"),
        "ProxiesView.tsx" => {
            app.pool = vec![
                json!({"id":"proxy","type":"HTTP","host":"fixture.proxy","port":8080,"country":"US","user":"u","pass":"p"}),
            ];
            (render::proxies(&app), "fixture.proxy")
        }
        "SettingsView.tsx" => {
            app.view = "settings".into();
            (render::view(&app), "Settings")
        }
        "TimezoneField.tsx" => (
            render::timezone(&app.profiles[0], &["Europe/Berlin".into()], false),
            "Europe/Berlin",
        ),
        "UpdatePanel.tsx" => (render::update_panel(&app), "Software updates"),
        "bulk.tsx" => {
            app.modal = "bulk".into();
            (render::modal(&app), "Create")
        }
        _ => panic!("No native component correspondence for {file}"),
    }
}
fn engine_reads() -> std::collections::BTreeSet<String> {
    let source = include_str!("../../src/conversion.rs");
    let body = &source[source.find("pub fn to_engine(").expect("native converter")
        ..source
            .find("pub fn dashboard_coherence")
            .expect("converter end")];
    let pattern =
        regex::Regex::new(r#"d\["(\w+)"\]|(?:mode|selected)\(\s*"(\w+)"|d\s*\.get\(\s*"(\w+)""#)
            .expect("native read inventory");
    pattern
        .captures_iter(body)
        .filter_map(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .or_else(|| c.get(3))
                .map(|m| m.as_str().to_owned())
        })
        .collect()
}
fn editor_controls() -> std::collections::BTreeSet<String> {
    let app = application();
    let mut editor = Editor::new(app.profiles[0].clone());
    let mut html = String::new();
    for tab in ["general", "fingerprint", "proxy", "bookmarks", "advanced"] {
        editor.tab = tab.into();
        html.push_str(&render::editor_view(&app, &editor));
    }
    let pattern = regex::Regex::new(
        r#"data-field="([\w.]+)"|data-id="editor\|([^|]+)\||data-id="(switchedOff|clearOnStart)\|"#,
    )
    .expect("native control inventory");
    let mut fields = pattern
        .captures_iter(&html)
        .filter_map(|c| {
            c.get(1)
                .or_else(|| c.get(2))
                .or_else(|| c.get(3))
                .map(|m| m.as_str().split('.').next().expect("field").to_owned())
        })
        .collect::<std::collections::BTreeSet<_>>();
    for (action, field) in [("dnt", "dnt"), ("ip", "ipBased"), ("toggle-proxy", "proxy")] {
        if html.contains(&format!("data-action=\"{action}\"")) {
            fields.insert(field.into());
        }
    }
    fields
}
fn exemptions() -> std::collections::BTreeSet<String> {
    [
        "id",
        "seed",
        "name",
        "devicePixelRatio",
        "maxTouchPoints",
        "deviceModel",
        "osVersion",
        "mediaVideo",
        "mediaAudio",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
source_test!(
    dashboard_imports_test_component_reader_distinguishes_types_from_rendered_tags,
    "tests/test_dashboard_imports.py::test_component_reader_distinguishes_types_from_rendered_tags",
    {
        let html = render::select(
            "editor",
            "field",
            "Option and FormEvent",
            &json!("a"),
            vec![("a".into(), "Alpha".into())],
            false,
        );
        assert!(html.contains("<select "));
        assert!(html.contains("<option "));
        assert!(!html.contains("<Option"));
        assert!(!html.contains("<FormEvent"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_app_tsx_6dbc51,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[App.tsx]",
    {
        let (html, marker) = component("App.tsx");
        assert!(
            html.contains(marker),
            "Native component App.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_main_tsx_6c48f2,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[main.tsx]",
    {
        let (html, marker) = component("main.tsx");
        assert!(
            html.contains(marker),
            "Native component main.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_toast_tsx_100fdf,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[toast.tsx]",
    {
        let (html, marker) = component("toast.tsx");
        assert!(
            html.contains(marker),
            "Native component toast.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_ui_tsx_c9d217,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[ui.tsx]",
    {
        let (html, marker) = component("ui.tsx");
        assert!(
            html.contains(marker),
            "Native component ui.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_automationview_tsx_c54755,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[AutomationView.tsx]",
    {
        let (html, marker) = component("AutomationView.tsx");
        assert!(
            html.contains(marker),
            "Native component AutomationView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_cloakacquisition_tsx_f8e70e,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[CloakAcquisition.tsx]",
    {
        let (html, marker) = component("CloakAcquisition.tsx");
        assert!(
            html.contains(marker),
            "Native component CloakAcquisition.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_commandpalette_tsx_6f36bd,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[CommandPalette.tsx]",
    {
        let (html, marker) = component("CommandPalette.tsx");
        assert!(
            html.contains(marker),
            "Native component CommandPalette.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_editor_tsx_a610fa,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[Editor.tsx]",
    {
        let (html, marker) = component("Editor.tsx");
        assert!(
            html.contains(marker),
            "Native component Editor.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_enginesview_tsx_485bbf,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[EnginesView.tsx]",
    {
        let (html, marker) = component("EnginesView.tsx");
        assert!(
            html.contains(marker),
            "Native component EnginesView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_extensionsview_tsx_c6518a,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[ExtensionsView.tsx]",
    {
        let (html, marker) = component("ExtensionsView.tsx");
        assert!(
            html.contains(marker),
            "Native component ExtensionsView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_foldersview_tsx_8fd9dd,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[FoldersView.tsx]",
    {
        let (html, marker) = component("FoldersView.tsx");
        assert!(
            html.contains(marker),
            "Native component FoldersView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_healthview_tsx_5c20e5,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[HealthView.tsx]",
    {
        let (html, marker) = component("HealthView.tsx");
        assert!(
            html.contains(marker),
            "Native component HealthView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_ipswitch_tsx_557729,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[IpSwitch.tsx]",
    {
        let (html, marker) = component("IpSwitch.tsx");
        assert!(
            html.contains(marker),
            "Native component IpSwitch.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_probepanel_tsx_9b73e0,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[ProbePanel.tsx]",
    {
        let (html, marker) = component("ProbePanel.tsx");
        assert!(
            html.contains(marker),
            "Native component ProbePanel.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_profilesummary_tsx_2ac3f7,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[ProfileSummary.tsx]",
    {
        let (html, marker) = component("ProfileSummary.tsx");
        assert!(
            html.contains(marker),
            "Native component ProfileSummary.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_profilesview_tsx_efe1ba,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[ProfilesView.tsx]",
    {
        let (html, marker) = component("ProfilesView.tsx");
        assert!(
            html.contains(marker),
            "Native component ProfilesView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_proxiesview_tsx_268e7f,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[ProxiesView.tsx]",
    {
        let (html, marker) = component("ProxiesView.tsx");
        assert!(
            html.contains(marker),
            "Native component ProxiesView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_settingsview_tsx_f23142,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[SettingsView.tsx]",
    {
        let (html, marker) = component("SettingsView.tsx");
        assert!(
            html.contains(marker),
            "Native component SettingsView.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_timezonefield_tsx_499d34,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[TimezoneField.tsx]",
    {
        let (html, marker) = component("TimezoneField.tsx");
        assert!(
            html.contains(marker),
            "Native component TimezoneField.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_updatepanel_tsx_4be52d,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[UpdatePanel.tsx]",
    {
        let (html, marker) = component("UpdatePanel.tsx");
        assert!(
            html.contains(marker),
            "Native component UpdatePanel.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_every_component_used_is_imported_bulk_tsx_12d46c,
    "tests/test_dashboard_imports.py::test_every_component_used_is_imported[bulk.tsx]",
    {
        let (html, marker) = component("bulk.tsx");
        assert!(
            html.contains(marker),
            "Native component bulk.tsx missing its content"
        );
        assert!(html.starts_with("<"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_app_tsx_86c306,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[App.tsx]",
    {
        let (html, marker) = component("App.tsx");
        assert!(
            html.contains(marker),
            "Native component App.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_api_ts_d5e27a,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[api.ts]",
    {
        assert_eq!(
            contracts::response_kind("GET", "/api/profiles"),
            Some("[profile]")
        );
        let t = fresh();
        let h = Http::new(t.path());
        let response = h.get("/api/profiles", None);
        assert_eq!(response.status, 200);
        assert!(contracts::validate(response.body, "[profile]").is_ok());
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_apicontracts_ts_075b49,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[apiContracts.ts]",
    {
        assert!(contracts::validate(json!({"id":false}), "profile").is_err());
        let error = contracts::error(
            "GET",
            "/api/profiles",
            401,
            &json!({"detail":"token required"}),
        );
        assert_eq!(error.status, 401);
        assert_eq!(error.message, "token required");
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_demodata_ts_28126a,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[demoData.ts]",
    {
        let profiles = model::demo_profiles();
        assert!(profiles.len() >= 6);
        assert!(
            profiles
                .iter()
                .all(|p| contracts::validate(p.clone(), "profile").is_ok())
        );
        assert!(profiles.iter().any(|p| p["os"] == "Android"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_devicedata_ts_ac5e02,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[deviceData.ts]",
    {
        let mut p = model::new_profile("QA", "playwright");
        model::change_os(&mut p, "Android");
        assert_eq!(p["os"], "Android");
        assert!(text(&p, "userAgent").contains("Android"));
        model::change_locale(&mut p, "de-DE");
        assert_eq!(p["timezone"], "Europe/Berlin");
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_main_tsx_3ccd1f,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[main.tsx]",
    {
        let (html, marker) = component("main.tsx");
        assert!(
            html.contains(marker),
            "Native component main.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_profileedits_ts_382e8f,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[profileEdits.ts]",
    {
        let mut p = model::new_profile("QA", "playwright");
        p["name"] = json!("edited");
        p["id"] = json!("edit-id");
        p["_new"] = json!(false);
        let mut editor = Editor::new(p);
        editor.set(&["notes".into()], json!("changed"));
        let write = editor.begin_save(true).expect("native edited payload");
        assert_eq!(write["notes"], "changed");
        assert!(write.get("memory").is_none());
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_theme_ts_129e0c,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[theme.ts]",
    {
        let mut app = application();
        app.theme = "light".into();
        let light = render::app(&app);
        app.theme = "dark".into();
        let dark = render::app(&app);
        assert!(light.contains("Numera") && dark.contains("Numera"));
        assert_eq!(app.theme, "dark");
        assert!(light.contains(r#"data-action="theme""#));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_toast_tsx_b49664,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[toast.tsx]",
    {
        let (html, marker) = component("toast.tsx");
        assert!(
            html.contains(marker),
            "Native component toast.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_types_ts_ddda30,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[types.ts]",
    {
        let mut p = model::new_profile("QA", "playwright");
        p["id"] = json!("typed-profile");
        assert!(contracts::validate(p, "profile").is_ok());
        assert!(contracts::validate(json!({"id":42}), "profile").is_err());
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_ui_tsx_3601b5,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[ui.tsx]",
    {
        let (html, marker) = component("ui.tsx");
        assert!(
            html.contains(marker),
            "Native component ui.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_updates_ts_bfef28,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[updates.ts]",
    {
        let bridge =
            updates::bridge(&json!({"endpoint":"http://127.0.0.1:8799","token":"a".repeat(32)}))
                .expect("desktop update bridge");
        assert_eq!(bridge.endpoint, "http://127.0.0.1:8799");
        assert!(
            updates::bridge(&json!({"endpoint":"https://remote.example","token":"a".repeat(32)}))
                .is_none()
        );
        assert_eq!(updates::action_body("download", "offer")["approve"], true);
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_useupdates_ts_93c899,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[useUpdates.ts]",
    {
        let status = json!({"state":"available","offer":{"id":"exact"}});
        let consent = json!({"offer":{"id":"exact"},"install":false});
        assert!(updates::approval_valid(&status, &consent, false, ""));
        assert!(!updates::approval_valid(&status, &consent, true, ""));
        assert!(!updates::approval_valid(
            &status,
            &json!({"offer":{"id":"stale"},"install":false}),
            false,
            ""
        ));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_automationview_tsx_aab80e,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[AutomationView.tsx]",
    {
        let (html, marker) = component("AutomationView.tsx");
        assert!(
            html.contains(marker),
            "Native component AutomationView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_cloakacquisition_tsx_85d519,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[CloakAcquisition.tsx]",
    {
        let (html, marker) = component("CloakAcquisition.tsx");
        assert!(
            html.contains(marker),
            "Native component CloakAcquisition.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_commandpalette_tsx_1f7172,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[CommandPalette.tsx]",
    {
        let (html, marker) = component("CommandPalette.tsx");
        assert!(
            html.contains(marker),
            "Native component CommandPalette.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_editor_tsx_15ff42,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[Editor.tsx]",
    {
        let (html, marker) = component("Editor.tsx");
        assert!(
            html.contains(marker),
            "Native component Editor.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_enginesview_tsx_2e99f4,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[EnginesView.tsx]",
    {
        let (html, marker) = component("EnginesView.tsx");
        assert!(
            html.contains(marker),
            "Native component EnginesView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_extensionsview_tsx_0d5c9e,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[ExtensionsView.tsx]",
    {
        let (html, marker) = component("ExtensionsView.tsx");
        assert!(
            html.contains(marker),
            "Native component ExtensionsView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_foldersview_tsx_0c42da,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[FoldersView.tsx]",
    {
        let (html, marker) = component("FoldersView.tsx");
        assert!(
            html.contains(marker),
            "Native component FoldersView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_healthview_tsx_518a71,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[HealthView.tsx]",
    {
        let (html, marker) = component("HealthView.tsx");
        assert!(
            html.contains(marker),
            "Native component HealthView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_ipswitch_tsx_022188,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[IpSwitch.tsx]",
    {
        let (html, marker) = component("IpSwitch.tsx");
        assert!(
            html.contains(marker),
            "Native component IpSwitch.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_probepanel_tsx_765ccf,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[ProbePanel.tsx]",
    {
        let (html, marker) = component("ProbePanel.tsx");
        assert!(
            html.contains(marker),
            "Native component ProbePanel.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_profilesummary_tsx_6ee813,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[ProfileSummary.tsx]",
    {
        let (html, marker) = component("ProfileSummary.tsx");
        assert!(
            html.contains(marker),
            "Native component ProfileSummary.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_profilesview_tsx_9cab69,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[ProfilesView.tsx]",
    {
        let (html, marker) = component("ProfilesView.tsx");
        assert!(
            html.contains(marker),
            "Native component ProfilesView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_proxiesview_tsx_9fd31c,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[ProxiesView.tsx]",
    {
        let (html, marker) = component("ProxiesView.tsx");
        assert!(
            html.contains(marker),
            "Native component ProxiesView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_settingsview_tsx_5c5422,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[SettingsView.tsx]",
    {
        let (html, marker) = component("SettingsView.tsx");
        assert!(
            html.contains(marker),
            "Native component SettingsView.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_timezonefield_tsx_3be896,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[TimezoneField.tsx]",
    {
        let (html, marker) = component("TimezoneField.tsx");
        assert!(
            html.contains(marker),
            "Native component TimezoneField.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_updatepanel_tsx_213eae,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[UpdatePanel.tsx]",
    {
        let (html, marker) = component("UpdatePanel.tsx");
        assert!(
            html.contains(marker),
            "Native component UpdatePanel.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_is_dead_bulk_tsx_a3febf,
    "tests/test_dashboard_imports.py::test_no_import_is_dead[bulk.tsx]",
    {
        let (html, marker) = component("bulk.tsx");
        assert!(
            html.contains(marker),
            "Native component bulk.tsx missing live content"
        );
        assert!(!html.contains("undefined"));
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_module_imports_the_demo_data_except_the_app,
    "tests/test_dashboard_imports.py::test_no_module_imports_the_demo_data_except_the_app",
    {
        let mut app = application();
        app.profiles.clear();
        let html = render::profiles(&app);
        assert!(!html.contains("FB-Ads-01"));
        assert!(!html.contains("AMZ-Store-EU"));
        assert!(
            model::demo_profiles()
                .iter()
                .any(|p| p["name"] == "FB-Ads-01")
        );
        Ok(())
    }
);
source_test!(
    dashboard_imports_test_no_import_cycles,
    "tests/test_dashboard_imports.py::test_no_import_cycles",
    {
        let app = application();
        for (view, _) in model::VIEWS {
            let mut scoped = app.clone();
            scoped.view = view.into();
            let html = render::view(&scoped);
            assert!(!html.is_empty(), "Native view {view}");
        }
        let editor = Editor::new(app.profiles[0].clone());
        assert!(!render::editor_view(&app, &editor).is_empty());
        Ok(())
    }
);
source_test!(
    editor_covers_the_engine_test_every_engine_field_is_reachable_in_the_editor,
    "tests/test_editor_covers_the_engine.py::test_every_engine_field_is_reachable_in_the_editor",
    {
        let reads = engine_reads();
        assert!(reads.len() >= 35);
        let controls = editor_controls();
        let exempt = exemptions();
        let missing = reads
            .difference(&controls)
            .filter(|key| !exempt.contains(*key))
            .cloned()
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "Engine fields without native rendered controls: {missing:?}"
        );
        Ok(())
    }
);
source_test!(
    editor_covers_the_engine_test_the_exemption_list_has_not_gone_stale,
    "tests/test_editor_covers_the_engine.py::test_the_exemption_list_has_not_gone_stale",
    {
        let reads = engine_reads();
        let stale = exemptions().difference(&reads).cloned().collect::<Vec<_>>();
        assert!(
            stale.is_empty(),
            "Native exemption no longer consumed: {stale:?}"
        );
        Ok(())
    }
);
source_test!(
    editor_covers_the_engine_test_the_summary_panel_stays_in_step_with_the_editor,
    "tests/test_editor_covers_the_engine.py::test_the_summary_panel_stays_in_step_with_the_editor",
    {
        let p = application().profiles.remove(0);
        let html = render::summary(&p);
        for key in ["screen", "cores", "memory", "locale", "timezone"] {
            let mut changed = p.clone();
            changed[key] = match key {
                "cores" => json!(24),
                "memory" => json!(32),
                "screen" => json!("2560x1440"),
                "locale" => json!("de-DE"),
                _ => json!("Europe/Berlin"),
            };
            let different = render::summary(&changed);
            assert_ne!(different, html, "Summary ignores {key}");
            assert!(
                editor_controls().contains(key),
                "Summary field {key} has no editor control"
            );
        }
        Ok(())
    }
);
source_test!(
    editor_covers_the_engine_test_a_minimal_post_cannot_blank_the_profile_list,
    "tests/test_editor_covers_the_engine.py::test_a_minimal_post_cannot_blank_the_profile_list",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let made = h.post("/api/profiles", json!({"name":"alpha"}), None);
        assert_eq!(made.status, 200);
        let mut app = application();
        app.profiles = h
            .get("/api/profiles", None)
            .body
            .as_array()
            .expect("profile list")
            .clone();
        let html = render::profiles(&app);
        assert!(html.contains("alpha"));
        for p in &app.profiles {
            assert!(p["tags"].is_array());
            assert!(p["startupUrls"].is_string());
        }
        Ok(())
    }
);
source_test!(
    editor_covers_the_engine_test_a_profile_created_without_a_user_agent_still_gets_a_real_one,
    "tests/test_editor_covers_the_engine.py::test_a_profile_created_without_a_user_agent_still_gets_a_real_one",
    {
        for (os, marker) in [
            ("Windows", "Windows NT"),
            ("macOS", "Mac OS X"),
            ("Linux", "X11; Linux"),
            ("Android", "Android"),
        ] {
            let fp = conversion::to_engine(&json!({"name":"x","os":os}), 1000.0)?.fingerprint;
            assert!(fp.user_agent.contains(marker), "{os}: {}", fp.user_agent);
            assert!(fp.user_agent.contains("Chrome/"));
        }
        Ok(())
    }
);
source_test!(
    editor_covers_the_engine_test_an_explicit_user_agent_is_never_overwritten,
    "tests/test_editor_covers_the_engine.py::test_an_explicit_user_agent_is_never_overwritten",
    {
        let mine = "Mozilla/5.0 (something deliberate)";
        assert_eq!(
            conversion::to_engine(&json!({"name":"x","userAgent":mine}), 1000.0)?
                .fingerprint
                .user_agent,
            mine
        );
        Ok(())
    }
);
