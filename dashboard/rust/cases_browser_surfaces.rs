mod engine_built_identity_is_not_claimed {
    use super::*;
    case!(summary_marks_generated_rows, {
        let mut e = Engine::default();
        e.profiles[0]["engine"] = json!("camoufox");
        let _h = mount(e, false).await;
        assert!(visible().contains("Firefox · generated"));
        open().await;
        let summary = node(".ps-summary").text_content().unwrap();
        assert!(summary.contains("Firefox"));
        assert!(summary.contains("builds its own"));
        assert!(
            texts(".ps-summary dd")
                .iter()
                .filter(|t| t.contains("· generated"))
                .count()
                > 5
        );
        assert!(!texts(".ps-summary dd").contains(&"Chrome 128.0.0.0".into()));
    });
    case!(chromium_summary_remains_concrete, {
        let _h = mount(Engine::default(), false).await;
        open().await;
        let summary = node(".ps-summary").text_content().unwrap();
        assert!(summary.contains("Chrome 128.0.0.0"));
        assert!(!summary.contains("· generated"));
    });
}
mod extension_icon_auth {
    use super::*;
    case!(extension_icon_uses_blob_not_bare_protected_url, {
        let mut e = Engine::default();
        e.config["default_extensions"] = json!(["C:\\ext\\ublock"]);
        let h = mount(e, false).await;
        set_auth("nmr_dashboard");
        click(".ps-navitem[data-id=extensions]").await;
        let src = node("img.ps-ext-logo").get_attribute("src").unwrap();
        assert!(src.starts_with("blob:"));
        assert!(!src.contains("/api/extensions/icon"));
        assert!(!src.contains("nmr_dashboard"));
        let e = h.engine.borrow();
        let c = e
            .calls
            .iter()
            .find(|c| c.path == "/api/extensions/icon")
            .unwrap();
        assert_eq!(c.bearer, "nmr_dashboard");
    });
    case!(removed_icon_has_no_dom_reference, {
        extension_icons_fetch_with_bearer_and_object_url_is_owned().await;
    });
    case!(missing_icon_has_svg_fallback, {
        missing_extension_icon_keeps_name_and_glyph().await;
    });
}
mod field_names_each_control {
    use super::*;
    fn fixture(html: String) {
        node("#root").set_inner_html(&html);
    }
    fn named(id: &str, label: &str) {
        let labels = Reflect::get(
            node(&format!("#{id}")).as_ref(),
            &JsValue::from_str("labels"),
        )
        .unwrap();
        let labels = Array::from(&labels);
        assert_eq!(labels.length(), 1);
        assert_eq!(
            labels
                .get(0)
                .dyn_into::<Element>()
                .unwrap()
                .text_content()
                .unwrap(),
            label
        );
    }
    case!(buttons_keep_their_names, {
        let _h = mount(Engine::default(), false).await;
        fixture(render::timezone(
            &profile("p"),
            &["Asia/Tehran".into()],
            true,
        ));
        assert_eq!(texts("button"), ["Auto", "Manual"]);
        named("editor-timezone", "Timezone");
        for selector in [
            "[data-action=timezone-auto]",
            "[data-action=timezone-manual]",
        ] {
            assert_eq!(
                Array::from(
                    &Reflect::get(node(selector).as_ref(), &JsValue::from_str("labels")).unwrap()
                )
                .length(),
                0
            );
        }
    });
    case!(input_named_by_field, {
        let _h = mount(Engine::default(), false).await;
        fixture(render::input(
            "editor",
            "name",
            "Profile name",
            &json!("x"),
            "text",
            "",
        ));
        named("editor-name", "Profile name");
    });
    case!(select_and_textarea_named, {
        let _h = mount(Engine::default(), false).await;
        fixture(
            render::select(
                "editor",
                "os",
                "Operating system",
                &json!("Windows"),
                vec![("Windows".into(), "Windows".into())],
                false,
            ) + &render::textarea("editor", "notes", "Notes", &json!(""), 3, ""),
        );
        named("editor-os", "Operating system");
        named("editor-notes", "Notes");
    });
    case!(explicit_control_name_preserved, {
        let _h = mount(Engine::default(), false).await;
        fixture(render::input_with_name(
            "editor",
            "zone",
            "Timezone",
            &json!(""),
            "text",
            "",
            "Timezone search",
        ));
        assert_eq!(
            node("#editor-zone").get_attribute("aria-label"),
            Some("Timezone search".into())
        );
        named("editor-zone", "Timezone");
        assert_eq!(count("[aria-label=Timezone]"), 0);
    });
    case!(buttons_group_has_context, {
        let _h = mount(Engine::default(), false).await;
        open().await;
        click("[data-action=tab][data-id=fingerprint]").await;
        assert_eq!(count("[role=group][aria-label='Operating system']"), 1);
        assert!(
            texts("[role=group][aria-label='Operating system'] button").contains(&"Windows".into())
        );
    });
    case!(label_announced_once, {
        let _h = mount(Engine::default(), false).await;
        fixture(render::input(
            "batch",
            "count",
            "How many profiles",
            &json!(5),
            "number",
            "",
        ));
        named("batch-count", "How many profiles");
        assert_eq!(count("[role=group]"), 0);
        assert_eq!(count("label"), 1);
    });
}
mod fingerprint_coherence {
    use super::*;
    case!(macos_rewrites_whole_device, {
        let h = mount(Engine::default(), false).await;
        open().await;
        click("[data-action=tab][data-id=fingerprint]").await;
        click("[data-id='editor|os|macOS']").await;
        click("[data-action=save]").await;
        let p = &h.engine.borrow().profiles[0];
        assert_eq!(p["os"], "macOS");
        assert!(s(p, "userAgent").contains("Macintosh; Intel Mac OS X 10_15_7"));
        assert!(!s(p, "userAgent").contains("Windows NT"));
        assert_eq!(p["screen"], "2560x1600");
        assert_eq!(p["cores"], 10);
        assert_eq!(p["webglRenderer"], "ANGLE (Apple M1, OpenGL 4.1)");
        assert_eq!(p["webglVendor"], "Google Inc. (Apple)");
        assert!(coherence(p).is_empty());
    });
    case!(android_rewrites_mobile_and_memory, {
        let h = mount(Engine::default(), false).await;
        open().await;
        click("[data-action=tab][data-id=fingerprint]").await;
        click("[data-id='editor|os|Android']").await;
        click("[data-action=save]").await;
        let p = &h.engine.borrow().profiles[0];
        assert_eq!(p["memory"], 6);
        assert_eq!(p["cores"], 8);
        assert_eq!(p["screen"], "360x800");
        assert!(s(p, "userAgent").contains("Android 13; Pixel 7"));
        assert!(s(p, "userAgent").contains(" Mobile Safari"));
        assert_eq!(p["webglVendor"], "Google Inc. (Qualcomm)");
        assert!(coherence(p).is_empty());
    });
    case!(locale_moves_zone, {
        let mut e = Engine::default();
        e.profiles[0]["proxy"] = Value::Null;
        let h = mount(e, false).await;
        open().await;
        click("[data-action=tab][data-id=fingerprint]").await;
        assert_eq!(value("#editor-timezone"), "America/New_York");
        input("#editor-locale", "de-DE");
        assert_eq!(value("#editor-timezone"), "Europe/Berlin");
        click("[data-action=save]").await;
        assert_eq!(h.engine.borrow().profiles[0]["locale"], "de-DE");
        assert_eq!(h.engine.borrow().profiles[0]["timezone"], "Europe/Berlin");
        assert!(coherence(&h.engine.borrow().profiles[0]).is_empty());
    });
    case!(stored_contradiction_visible, {
        let mut e = Engine::default();
        e.profiles[0]["locale"] = json!("de-DE");
        e.profiles[0]["proxy"] = Value::Null;
        let _h = mount(e, false).await;
        open().await;
        assert!(
            node(".ps-coh-iss")
                .text_content()
                .unwrap()
                .contains("Timezone doesn't match locale")
        );
        assert_eq!(texts(".ps-coh-score"), ["78%"]);
    });
    case!(unnamed_cannot_save, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=new]").await;
        assert!(node("[data-action=save]").has_attribute("disabled"));
        click("[data-action=save]").await;
        assert!(
            !h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.method == "POST" && c.path == "/api/profiles")
        );
        input("#editor-name", "Ad account 12");
        assert!(!node("[data-action=save]").has_attribute("disabled"));
        click("[data-action=save]").await;
        assert!(
            h.engine
                .borrow()
                .profiles
                .iter()
                .any(|p| p["name"] == "Ad account 12")
        );
    });
    case!(new_flag_cleared_for_committed_write, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=new]").await;
        input("#editor-name", "Ad account 12");
        click("[data-action=save]").await;
        let id = h.engine.borrow().profiles.last().unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        click(&format!("[data-action=edit][data-id='{id}']")).await;
        input("#editor-notes", "second save");
        click("[data-action=save]").await;
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .filter(|c| c.method == "POST" && c.path == "/api/profiles")
                .count(),
            1
        );
        assert_eq!(writes(&h).len(), 1);
        assert_eq!(h.engine.borrow().profiles.len(), 3);
    });
}
mod health_actions_say_what_they_did {
    use super::*;
    case!(trust_grade_reported, {
        let _h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=health]").await;
        click("[data-action=health-trust][data-id=alpha]").await;
        assert!(visible().contains("trust C (81/100), 13 of 16 checks passed"));
    });
    case!(align_names_fields_and_grade_change, {
        let _h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=health]").await;
        click("[data-action=health-align][data-id=alpha]").await;
        assert!(visible().contains("C → B"));
        assert!(visible().contains("os, screen, webglRenderer"));
    });
    case!(unchanged_alignment_explained, {
        let mut e = Engine::default();
        e.align_result = json!({"before":{"grade":"A","score":100},"after":{"grade":"A","score":100},"changed":[]});
        let _h = mount(e, false).await;
        click(".ps-navitem[data-id=health]").await;
        click("[data-action=health-align][data-id=alpha]").await;
        assert!(visible().contains("nothing needed changing"));
    });
    case!(failure_has_no_success_grade, {
        let mut e = Engine::default();
        e.errors.insert(
            "/api/profiles/alpha/trust".into(),
            (500, "the engine is not installed".into(), String::new()),
        );
        let _h = mount(e, false).await;
        click(".ps-navitem[data-id=health]").await;
        click("[data-action=health-trust][data-id=alpha]").await;
        assert!(visible().contains("Trust check failed: the engine is not installed"));
        assert_eq!(count(".ps-toast.success"), 0);
    });
}
mod legacy_name_render {
    use super::*;
    case!(repaired_names_all_render, {
        let mut e = Engine::default();
        e.profiles = ["123", "", "", "", "healthy"]
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                let mut p = profile(&i.to_string());
                p["name"] = json!(n);
                p
            })
            .collect();
        let _h = mount(e, false).await;
        assert_eq!(texts(".ps-pname"), ["123", "", "", "", "healthy"]);
    });
    case!(repaired_names_are_searchable, {
        let mut e = Engine::default();
        e.profiles[0]["name"] = json!("123");
        e.profiles[1]["name"] = json!("healthy");
        let _h = mount(e, false).await;
        input("#profile-search", "healthy");
        assert_eq!(texts(".ps-pname"), ["healthy"]);
        input("#profile-search", "123");
        assert_eq!(texts(".ps-pname"), ["123"]);
    });
    case!(raw_legacy_names_do_not_blank_view, {
        let mut e = Engine::default();
        e.profiles[0]["name"] = json!(123);
        e.profiles[1]["name"] = Value::Null;
        e.profiles.push(profile("healthy"));
        let _h = mount(e, false).await;
        assert!(texts(".ps-pname").contains(&"healthy".into()));
        input("#profile-search", "healthy");
        assert_eq!(texts(".ps-pname"), ["healthy"]);
    });
}
mod native_export_round_trip {
    use super::*;
    case!(file_is_versioned_numera_envelope, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        click("[data-action=export-profiles]").await;
        let f = exported(&h).await;
        assert_eq!(f["numeraProfiles"], 1);
        assert_eq!(
            list(&f["profiles"])
                .iter()
                .map(|p| s(p, "id"))
                .collect::<Vec<_>>(),
            ["alpha", "bravo"]
        );
    });
    case!(every_editable_unknown_field_preserved, {
        let mut e = Engine::default();
        let p = &mut e.profiles[0];
        for (k, v) in [
            ("seed", json!(424242)),
            ("fontList", json!("Sentinel Sans")),
            ("clientRects", json!("Off")),
            ("webglImage", json!("Off")),
            ("mediaDevices", json!("Real")),
            ("mediaSpeakers", json!(4)),
            (
                "startupUrls",
                json!("https://example.test/search?q=red,blue"),
            ),
            ("extensions", json!("C:\\ext\\sentinel")),
            ("launchArgs", json!("--sentinel")),
            ("ipBased", json!(["timezone"])),
            ("clearOnStart", json!(["cookies"])),
            ("switchedOff", json!(["googleServices"])),
        ] {
            p[k] = v;
        }
        let source = p.clone();
        let h = mount(e, false).await;
        click("[data-action=select][data-id=alpha]").await;
        click("[data-action=export-profiles]").await;
        let f = exported(&h).await;
        assert_eq!(f["profiles"][0], source);
    });
    case!(session_exclusion_is_explained, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        click("[data-action=export-profiles]").await;
        assert!(!h.blobs.borrow().is_empty());
        assert!(visible().contains("cookies and logins — are not included"));
        assert!(visible().contains("numera backup"));
    });
    case!(withheld_vault_password_is_named, {
        let mut e = Engine::default();
        e.profiles[0]["proxy"]["pass"] = Value::Null;
        let h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click("[data-action=export-profiles]").await;
        assert!(visible().contains("1 proxy password is locked in the vault"));
        let f = exported(&h).await;
        assert_eq!(list(&f["profiles"]).len(), 2);
        assert_eq!(f["profiles"][0]["proxy"]["host"], "localhost");
    });
    case!(exact_file_text_is_opaque_to_import, {
        let h = mount(Engine::default(), false).await;
        let text = "{ \"numeraProfiles\":1, \"profiles\":[] }\n";
        import_file(text).await;
        let e = h.engine.borrow();
        assert_eq!(
            e.calls
                .iter()
                .find(|c| c.path == "/api/profiles/import")
                .unwrap()
                .body["text"],
            text
        );
    });
    case!(vendor_import_explains_regeneration, {
        let mut e = Engine::default();
        e.import_answer = json!({"imported":1,"format":"vendor","results":[{"issues":[]}]});
        let _h = mount(e, false).await;
        import_file("[{\"name\":\"from gologin\"}]").await;
        assert!(visible().contains("vendor export"));
        assert!(visible().contains("fingerprint was regenerated"));
    });
    case!(native_import_reports_plainly, {
        let _h = mount(Engine::default(), false).await;
        import_file("{\"numeraProfiles\":1,\"profiles\":[]}").await;
        assert!(visible().contains("Imported 2 profiles"));
        assert!(!visible().contains("vendor export"));
    });
    case!(import_password_warning_count_reported, {
        let mut e = Engine::default();
        e.import_answer = json!({"imported":1,"format":"numera","results":[{"issues":["the proxy password was not in this file"]}]});
        let _h = mount(e, false).await;
        import_file("{\"numeraProfiles\":1,\"profiles\":[]}").await;
        assert!(visible().contains("1 flagged for review"));
    });
    case!(engine_import_refusal_reason_preserved, {
        let mut e = Engine::default();
        e.errors.insert("/api/profiles/import".into(),(400,"this looks like a Numera profile export written before the format carried a version".into(),String::new()));
        let _h = mount(e, false).await;
        import_file("[]").await;
        assert!(visible().contains("written before the format carried a version"));
        assert_eq!(count(".ps-toast.success"), 0);
    });
    case!(canceled_file_has_no_import_request, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=import-profiles]").await;
        assert!(
            !h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.path == "/api/profiles/import")
        );
        node("#numera-file-picker").remove();
    });
}
mod nav_reaches_a_screen {
    use super::*;
    case!(every_nav_destination_renders, {
        all_navigation_and_palette_use_real_dom_events().await;
        automation_attach_detach_and_snippets_use_same_session().await;
        folders_create_rename_delete_await_engine_and_reassign_profiles().await;
        extension_apply_merges_and_keeps_profile_specific_entries().await;
        demo_mode_is_honest_and_live_locked_engine_never_uses_samples().await;
        desktop_startup_error_hook_sanitizes_bounds_and_escapes_log().await;
    });
    case!(palette_reaches_all_nav_sections, {
        let _h = mount(Engine::default(), false).await;
        click("[data-action=commands]").await;
        for (view, label) in VIEWS {
            assert!(visible().contains(&format!("Go to {label}")));
            assert!(exists(&format!(".ps-navitem[data-id={view}]")));
        }
        input("#command-query", "Extensions");
        click("[data-action=command]").await;
        assert_eq!(snapshot().view, "extensions");
    });
    case!(unknown_view_has_honest_fallback, {
        let _h = mount(Engine::default(), false).await;
        change(|a| a.view = "team".into());
        draw();
        assert!(visible().contains("team"));
        assert!(visible().contains("not available"));
        assert!(!visible().contains("roadmap"));
    });
}
