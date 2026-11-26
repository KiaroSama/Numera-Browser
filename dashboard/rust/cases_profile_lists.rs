mod auth_lockout {
    use super::*;
    case!(first_token_adopted, {
        let h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=settings]").await;
        input("#form-tokenName", "dashboard");
        click("[data-action=create-token]").await;
        assert_eq!(auth(), "nmr_dashboard");
        assert!(visible().contains("nmr_dashboard"));
        assert!(visible().contains("The API now requires one of these"));
        assert!(!visible().contains("This engine needs a token"));
        assert!(
            h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.path == "/api/tokens"
                    && c.method == "GET"
                    && c.bearer == "nmr_dashboard")
        );
    });
    case!(second_token_preserves_session_credential, {
        let h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=settings]").await;
        input("#form-tokenName", "dashboard");
        click("[data-action=create-token]").await;
        input("#form-tokenName", "ci");
        click("[data-action=create-token]").await;
        assert_eq!(auth(), "nmr_dashboard");
        assert!(visible().contains("nmr_ci"));
        assert_eq!(h.engine.borrow().tokens.len(), 2);
    });
    case!(revoked_token_shows_lock_screen, {
        first_token_is_adopted_second_preserved_and_revocation_locks_ui().await;
    });
}
mod bulk_actions {
    use super::*;
    case!(launch_ignores_running, {
        let mut e = Engine::default();
        e.profiles[1]["status"] = json!("running");
        e.profiles.push(profile("charlie"));
        let h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click("[data-action=bulk-launch]").await;
        until(|| !snapshot().busy).await;
        let e = h.engine.borrow();
        let calls = e
            .calls
            .iter()
            .filter(|c| c.path.ends_with("/launch"))
            .map(|c| c.path.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            calls,
            ["/api/profiles/alpha/launch", "/api/profiles/charlie/launch"]
        );
        assert!(!e.calls.iter().any(|c| c.path.ends_with("/stop")));
        assert!(visible().contains("Launched 2"));
        assert!(visible().contains("already running"));
    });
    case!(all_running_launch_has_no_targets, {
        let mut e = Engine::default();
        for p in &mut e.profiles {
            p["status"] = json!("running");
        }
        let h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click("[data-action=bulk-launch]").await;
        assert!(
            !h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.path.ends_with("/launch") || c.path.ends_with("/stop"))
        );
        assert!(visible().contains("already running"));
    });
    case!(failed_launch_remains_selected, {
        launch_is_idempotent_and_partial_failures_remain_selected().await;
    });
    case!(demo_launch_is_launch_only, {
        let mut e = Engine::default();
        e.offline = true;
        let _h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click("[data-action=bulk-launch]").await;
        until(|| !snapshot().busy).await;
        assert_eq!(count("tbody .ps-status.running"), 6);
        assert_eq!(count("tbody .ps-status.stopped"), 0);
    });
    case!(warmup_reports_partial_failure, {
        let mut e = Engine::default();
        e.profiles.push(profile("charlie"));
        for id in ["bravo", "charlie"] {
            e.errors.insert(
                format!("/api/profiles/{id}/warmup"),
                (423, "Session is busy".into(), String::new()),
            );
        }
        let h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click("[data-action=bulk-warmup]").await;
        until(|| !snapshot().busy).await;
        assert_eq!(
            snapshot().selected,
            BTreeSet::from(["bravo".into(), "charlie".into()])
        );
        assert!(visible().contains("Warmed up 1 of 3. 2 failed: Session is busy"));
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .filter(|c| c.path.ends_with("/warmup"))
                .count(),
            3
        );
    });
    case!(warmup_failure_is_never_success, {
        let mut e = Engine::default();
        e.profiles.truncate(1);
        e.errors.insert(
            "/api/profiles/alpha/warmup".into(),
            (423, "No engine installed".into(), String::new()),
        );
        let _h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click("[data-action=bulk-warmup]").await;
        until(|| !snapshot().busy).await;
        assert!(visible().contains("No engine installed"));
        assert_eq!(count(".ps-toast.success"), 0);
        assert_eq!(snapshot().selected.len(), 1);
    });
    case!(warmup_busy_disables_repeat, {
        let h = mount(Engine::default(), false).await;
        h.engine.borrow_mut().hold = "/api/profiles/alpha/warmup".into();
        click("[data-action=select][data-id=alpha]").await;
        click("[data-action=bulk-warmup]").await;
        assert!(node("[data-action=bulk-warmup]").has_attribute("disabled"));
        click("[data-action=bulk-warmup]").await;
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .filter(|c| c.path.ends_with("/warmup"))
                .count(),
            1
        );
        finish_held(&h);
        until(|| !snapshot().busy).await;
    });
    case!(filter_does_not_shrink_delete, {
        destructive_profile_actions_require_confirmation_and_keep_scope().await;
    });
    case!(export_includes_hidden_selection, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        input("#profile-search", "alpha");
        assert_eq!(count("tbody tr"), 1);
        click("[data-action=export-profiles]").await;
        let f = exported(&h).await;
        assert_eq!(
            list(&f["profiles"])
                .iter()
                .map(|p| s(p, "id"))
                .collect::<Vec<_>>(),
            ["alpha", "bravo"]
        );
        assert_eq!(snapshot().selected_profiles().len(), 2);
    });
    case!(launch_export_same_scope, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        input("#profile-search", "alpha");
        click("[data-action=export-profiles]").await;
        let file = exported(&h).await;
        click("[data-action=bulk-launch]").await;
        until(|| !snapshot().busy).await;
        let actual = h
            .engine
            .borrow()
            .calls
            .iter()
            .filter(|c| c.path.ends_with("/launch"))
            .map(|c| c.path.split('/').nth(3).unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            list(&file["profiles"])
                .iter()
                .map(|p| s(p, "id").into())
                .collect::<Vec<String>>()
        );
    });
}
mod bulk_edit_scope {
    use super::*;
    case!(disappeared_profile_not_in_modal_count, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        h.engine.borrow_mut().profiles.pop();
        until(|| count("tbody tr") == 1).await;
        assert!(visible().contains("1 selected"));
        click("[data-action=sync]").await;
        assert!(visible().contains("Apply to 1"));
        input("#form-tags", "audit");
        click("[data-action=apply-sync]").await;
        let e = h.engine.borrow();
        assert_eq!(
            e.calls
                .iter()
                .find(|c| c.path == "/api/profiles/bulk")
                .unwrap()
                .body["ids"],
            json!(["alpha"])
        );
    });
    case!(filtered_selection_stays_whole, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        input("#profile-search", "alpha");
        click("[data-action=sync]").await;
        assert!(visible().contains("Apply to 2"));
        input("#form-tags", "audit");
        click("[data-action=apply-sync]").await;
        let e = h.engine.borrow();
        assert_eq!(
            e.calls
                .iter()
                .find(|c| c.path == "/api/profiles/bulk")
                .unwrap()
                .body["ids"],
            json!(["alpha", "bravo"])
        );
    });
    case!(folder_change_preserves_selection, {
        let mut e = Engine::default();
        e.profiles[1]["folder"] = json!("Ops");
        let h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click(".ps-folder[data-id=Ops]").await;
        assert_eq!(texts(".ps-pname"), ["bravo"]);
        click("[data-action=sync]").await;
        assert!(visible().contains("Apply to 2"));
        input("#form-tags", "audit");
        click("[data-action=apply-sync]").await;
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .find(|c| c.path == "/api/profiles/bulk")
                .unwrap()
                .body["ids"],
            json!(["alpha", "bravo"])
        );
    });
    case!(selection_spans_pages, {
        let mut e = Engine::default();
        e.profiles = (0..101).map(|i| profile(&format!("p-{i:03}"))).collect();
        let h = mount(e, false).await;
        click("[data-action=select-all]").await;
        click("[data-action=page][data-id='1']").await;
        assert_eq!(texts(".ps-pname"), ["p-100"]);
        click("[data-action=sync]").await;
        assert!(visible().contains("Apply to 101"));
        input("#form-tags", "audit");
        click("[data-action=apply-sync]").await;
        assert_eq!(
            list(
                &h.engine
                    .borrow()
                    .calls
                    .iter()
                    .find(|c| c.path == "/api/profiles/bulk")
                    .unwrap()
                    .body["ids"]
            )
            .len(),
            101
        );
    });
    case!(partial_bulk_edit_reports_missing, {
        global_selection_survives_filter_and_bulk_edit_counts_real_server_result().await;
    });
}
mod bulk_generate {
    use super::*;
    case!(os_country_cartesian_spread, {
        let (_h, p) = generate("4", "1", &["macOS"], &["GB"], "").await;
        assert_eq!(
            p.iter()
                .map(|p| (s(p, "os"), s(p, "locale"), s(p, "timezone")))
                .collect::<Vec<_>>(),
            [
                ("Windows", "en-US", "America/New_York"),
                ("macOS", "en-US", "America/New_York"),
                ("Windows", "en-GB", "Europe/London"),
                ("macOS", "en-GB", "Europe/London")
            ]
        );
    });
    case!(names_padded_from_last_number, {
        let (h, p) = generate("5", "98", &[], &[], "").await;
        assert_eq!(
            p.iter().map(|p| s(p, "name")).collect::<Vec<_>>(),
            [
                "Batch-098",
                "Batch-099",
                "Batch-100",
                "Batch-101",
                "Batch-102"
            ]
        );
        drop(h);
        let (_h, p) = generate("3", "1", &[], &[], "").await;
        assert_eq!(
            p.iter().map(|p| s(p, "name")).collect::<Vec<_>>(),
            ["Batch-01", "Batch-02", "Batch-03"]
        );
    });
    case!(unset_engine_is_null, {
        let (h, p) = generate("2", "1", &[], &[], "").await;
        assert!(p.iter().all(|p| p["engine"].is_null()));
        drop(h);
        let (_h, p) = generate("2", "1", &[], &[], "camoufox").await;
        assert!(p.iter().all(|p| p["engine"] == "camoufox"));
    });
    case!(generated_devices_are_coherent, {
        let (_h, p) = generate(
            "24",
            "1",
            &["macOS", "Linux", "Android"],
            &["DE", "TR", "IN"],
            "",
        )
        .await;
        assert_eq!(p.len(), 24);
        for p in p {
            assert!(coherence(&p).is_empty());
            assert!(s(&p, "userAgent").contains(&format!("Chrome/{}", s(&p, "browserVersion"))));
            assert!(s(&p, "userAgent").contains(match s(&p, "os") {
                "Windows" => "Windows NT",
                "macOS" => "Mac OS X",
                "Linux" => "X11; Linux",
                _ => "Android 13",
            }));
            let family = s(&p, "webglVendor")
                .strip_prefix("Google Inc. (")
                .and_then(|v| v.strip_suffix(')'))
                .expect("GPU vendor names its family");
            let token = match family {
                "AMD" => "radeon",
                "Qualcomm" => "adreno",
                "ARM" => "mali",
                family => family,
            };
            assert!(
                s(&p, "webglRenderer")
                    .to_lowercase()
                    .contains(&token.to_lowercase())
            );
        }
    });
    case!(sync_omits_untouched_fields, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        click("[data-action=sync]").await;
        input("#form-locale", "en-GB");
        click("[data-action=apply-sync]").await;
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .find(|c| c.path == "/api/profiles/bulk")
                .unwrap()
                .body["patch"],
            json!({"locale":"en-GB"})
        );
    });
    case!(empty_and_whitespace_sync_cannot_apply, {
        let h = mount(Engine::default(), false).await;
        click("[data-action=select-all]").await;
        click("[data-action=sync]").await;
        assert!(node("[data-action=apply-sync]").has_attribute("disabled"));
        input("#form-tags", "   ");
        assert!(node("[data-action=apply-sync]").has_attribute("disabled"));
        click("[data-action=apply-sync]").await;
        assert!(
            !h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.path == "/api/profiles/bulk")
        );
        input("#form-tags", " q3-campaign , , ads ");
        click("[data-action=apply-sync]").await;
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .find(|c| c.path == "/api/profiles/bulk")
                .unwrap()
                .body["patch"],
            json!({"addTags":["q3-campaign","ads"]})
        );
    });
}
mod cloak_consent {
    use super::*;
    case!(exact_offer_needs_explicit_consent, {
        let h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=engines]").await;
        assert!(
            !h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.path.contains("acquisition"))
        );
        click("[data-action=cloak-review]").await;
        assert!(visible().contains("146.0.7680.177.5"));
        assert!(node("[data-action=cloak-acquire]").has_attribute("disabled"));
        click("[data-action=cloak-consent]").await;
        click("[data-action=cloak-acquire]").await;
        assert!(visible().contains("Approved browser acquired"));
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .find(|c| c.path.contains("acquisition") && c.method == "POST")
                .unwrap()
                .body,
            json!({"offer_id":"f".repeat(64),"approve":true})
        );
    });
    case!(failed_acquisition_consumes_consent, {
        cloak_requires_review_exact_consent_and_failed_download_consumes_it().await;
    });
    case!(unavailable_offer_cannot_be_approved, {
        let mut e = Engine::default();
        e.offer["available"] = json!(false);
        e.offer["reason"] = json!("Exact vendor generation unavailable");
        let _h = mount(e, false).await;
        click(".ps-navitem[data-id=engines]").await;
        click("[data-action=cloak-review]").await;
        assert!(visible().contains("Exact vendor generation unavailable"));
        assert!(!exists("[data-action=cloak-acquire]"));
    });
}
mod country_picker_offers_each_place_once {
    use super::*;
    case!(proxy_country_select_unique, {
        let _h = mount(Engine::default(), false).await;
        open().await;
        click("[data-action=tab][data-id=proxy]").await;
        let offered = texts("#editor-proxy-country option");
        assert!(offered.contains(&"GB".into()));
        assert!(!offered.contains(&"UK".into()));
        assert_eq!(offered.iter().collect::<BTreeSet<_>>().len(), offered.len());
    });
    case!(stored_alias_is_selected, {
        let mut e = Engine::default();
        e.profiles[0]["proxy"]["country"] = json!("UK");
        let _h = mount(e, false).await;
        open().await;
        click("[data-action=tab][data-id=proxy]").await;
        assert_eq!(value("#editor-proxy-country"), "UK");
    });
    case!(bulk_country_chips_unique, {
        let _h = mount(Engine::default(), false).await;
        click("[data-action=bulk]").await;
        let chips = texts("[data-id^='countries|']");
        assert!(chips.contains(&"GB".into()));
        assert!(!chips.contains(&"UK".into()));
        assert_eq!(chips.iter().collect::<BTreeSet<_>>().len(), COUNTRIES.len());
    });
}
mod default_engine_authority {
    use super::*;
    case!(notes_save_does_not_move_default, {
        profile_save_obeys_server_default_and_retains_unknown_fields().await;
    });
    case!(server_engine_change_moves_default, {
        let mut e = Engine::default();
        e.config["default_engine"] = json!("playwright");
        let h = mount(e, false).await;
        open().await;
        input("#editor-engine", "camoufox");
        click("[data-action=save]").await;
        click(".ps-navitem[data-id=settings]").await;
        assert_eq!(snapshot().default_engine(), "camoufox");
        assert_eq!(h.engine.borrow().config["default_engine"], "camoufox");
        assert!(
            node("[data-action=default-engine][data-id=camoufox]")
                .get_attribute("class")
                .unwrap()
                .contains("on")
        );
    });
}
mod dom_cleanup {
    use super::*;
    case!(render_does_not_create_nested_stylesheets, {
        stylesheet_and_event_cleanup_do_not_accumulate_on_remount().await;
    });
    case!(connected_css_remains_external, {
        let h = mount(Engine::default(), false).await;
        let style = document().create_element("style").unwrap();
        style.set_text_content(Some(".live-probe{color:blue}"));
        document().body().unwrap().append_child(&style).unwrap();
        drop(h);
        assert!(style.is_connected());
        assert_eq!(style.text_content().unwrap(), ".live-probe{color:blue}");
        style.remove();
    });
}
