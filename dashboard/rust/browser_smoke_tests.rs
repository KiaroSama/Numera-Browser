async fn all_navigation_and_palette_use_real_dom_events() {
    let _h = mount(Engine::default(), false).await;
    for (view, _) in VIEWS {
        click(&format!(".ps-navitem[data-id={view}]")).await;
        assert_eq!(snapshot().view, view);
        assert!(exists(&format!(".ps-navitem.active[data-id={view}]")));
        assert!(!visible().contains("This view is not available"));
    }
    click("[data-action=commands]").await;
    assert!(exists("#command-query"));
    for (_, label) in VIEWS {
        assert!(visible().contains(&format!("Go to {label}")));
    }
    input("#command-query", "Go to Extensions");
    click("[data-action=command]").await;
    assert_eq!(snapshot().view, "extensions");
}
async fn profile_save_obeys_server_default_and_retains_unknown_fields() {
    let h = mount(Engine::default(), false).await;
    open().await;
    input("#editor-notes", "just a note");
    click("[data-action=save]").await;
    until(|| !exists(".ps-editor")).await;
    assert_eq!(h.engine.borrow().profiles[0]["notes"], "just a note");
    assert_eq!(snapshot().default_engine(), "camoufox");
    let sent = &writes(&h)[0].body;
    assert_eq!(sent["notes"], "just a note");
    assert!(sent.get("proxy").is_none());
    assert!(sent.get("engine").is_none());
}
async fn timezone_auto_manual_and_unknown_zone_work_in_chromium() {
    let _h = mount(Engine::default(), false).await;
    open().await;
    click("[data-action=tab][data-id=fingerprint]").await;
    click("[data-action=timezone-auto]").await;
    assert!(node("#editor-timezone").has_attribute("disabled"));
    click("[data-action=timezone-manual]").await;
    assert!(!node("#editor-timezone").has_attribute("disabled"));
    input("#editor-timezone", "Mars/Olympus");
    assert_eq!(
        node("#editor-timezone").get_attribute("aria-invalid"),
        Some("true".into())
    );
    input("#editor-timezone", "Asia/Tehran");
    assert!(!node("#editor-timezone").has_attribute("aria-invalid"));
}
async fn global_selection_survives_filter_and_bulk_edit_counts_real_server_result() {
    let h = mount(Engine::default(), false).await;
    click("[data-action=select-all]").await;
    input("#profile-search", "alpha");
    assert_eq!(snapshot().selected_profiles().len(), 2);
    click("[data-action=sync]").await;
    assert!(visible().contains("Apply to 2"));
    h.engine.borrow_mut().profiles.pop();
    input("#form-tags", "audit");
    click("[data-action=apply-sync]").await;
    assert!(visible().contains("1 no longer existed"));
    let calls = &h.engine.borrow().calls;
    let bulk = calls
        .iter()
        .find(|c| c.path == "/api/profiles/bulk")
        .unwrap();
    assert_eq!(bulk.body["ids"], json!(["alpha", "bravo"]));
}
async fn launch_is_idempotent_and_partial_failures_remain_selected() {
    let mut e = Engine::default();
    e.profiles[0]["status"] = json!("running");
    e.errors.insert(
        "/api/profiles/bravo/launch".into(),
        (423, "credential locked".into(), String::new()),
    );
    let h = mount(e, false).await;
    click("[data-action=select-all]").await;
    click("[data-action=bulk-launch]").await;
    until(|| !snapshot().busy).await;
    assert_eq!(snapshot().selected, BTreeSet::from(["bravo".into()]));
    assert!(visible().contains("credential locked"));
    assert!(
        !h.engine
            .borrow()
            .calls
            .iter()
            .any(|c| c.path.ends_with("/stop"))
    );
}
async fn destructive_profile_actions_require_confirmation_and_keep_scope() {
    let h = mount(Engine::default(), false).await;
    click("[data-action=select-all]").await;
    input("#profile-search", "alpha");
    click("[data-action=delete-selected]").await;
    assert!(visible().contains("Delete 2 profiles?"));
    assert!(
        !h.engine
            .borrow()
            .calls
            .iter()
            .any(|c| c.path == "/api/profiles/delete")
    );
    click("[data-action=confirm]").await;
    assert!(h.engine.borrow().profiles.is_empty());
}
async fn folders_create_rename_delete_await_engine_and_reassign_profiles() {
    let h = mount(Engine::default(), false).await;
    click(".ps-navitem[data-id=folders]").await;
    input("#form-newFolder", "Client A");
    click("[data-action=create-folder]").await;
    assert!(snapshot().folders().contains(&"Client A".into()));
    click("[data-action=rename-folder][data-id=Work]").await;
    input("#form-to", "Client B");
    click("[data-action=commit-folder]").await;
    assert_eq!(h.engine.borrow().profiles[0]["folder"], "Client B");
    click("[data-action=delete-folder][data-id='Client B']").await;
    click("[data-action=confirm]").await;
    assert_eq!(h.engine.borrow().profiles[0]["folder"], "Unfiled");
}
async fn first_token_is_adopted_second_preserved_and_revocation_locks_ui() {
    let h = mount(Engine::default(), false).await;
    click(".ps-navitem[data-id=settings]").await;
    input("#form-tokenName", "dashboard");
    click("[data-action=create-token]").await;
    assert_eq!(auth(), "nmr_dashboard");
    assert!(visible().contains("Copy it now"));
    input("#form-tokenName", "ci");
    click("[data-action=create-token]").await;
    assert_eq!(auth(), "nmr_dashboard");
    click("[data-action=revoke-token][data-id=t1]").await;
    click("[data-action=confirm]").await;
    assert!(visible().contains("This engine needs a token"));
    assert_eq!(h.engine.borrow().tokens.len(), 1);
}
async fn extension_icons_fetch_with_bearer_and_object_url_is_owned() {
    let mut e = Engine::default();
    e.config["default_extensions"] = json!(["C:\\ext\\ublock"]);
    let h = mount(e, false).await;
    set_auth("nmr_dashboard");
    click(".ps-navitem[data-id=extensions]").await;
    assert!(exists("img.ps-ext-logo"));
    let image = node("img.ps-ext-logo").get_attribute("src").unwrap();
    assert!(image.starts_with("blob:"));
    assert!(
        h.engine
            .borrow()
            .calls
            .iter()
            .any(|c| c.path == "/api/extensions/icon" && c.bearer == "nmr_dashboard")
    );
    click(".ps-navitem[data-id=profiles]").await;
    assert!(snapshot().icons.is_empty());
    let response = JsFuture::from(window().fetch_with_str(&image)).await;
    assert!(response.is_err(), "revoked blob still fetchable");
}
async fn missing_extension_icon_keeps_name_and_glyph() {
    let mut e = Engine::default();
    e.config["default_extensions"] = json!(["/ext"]);
    e.icons_missing = true;
    let _h = mount(e, false).await;
    click(".ps-navitem[data-id=extensions]").await;
    assert!(visible().contains("uBlock Origin"));
    assert!(!exists("img.ps-ext-logo"));
    assert!(exists(".ps-ext-ic svg"));
}
async fn extension_apply_merges_and_keeps_profile_specific_entries() {
    let mut e = Engine::default();
    e.config["default_extensions"] = json!(["/default"]);
    e.profiles[0]["extensions"] = json!("/specific");
    e.profiles[1]["extensions"] = json!("/default");
    let h = mount(e, false).await;
    click(".ps-navitem[data-id=extensions]").await;
    click("[data-action=apply-extensions]").await;
    assert_eq!(
        h.engine.borrow().profiles[0]["extensions"],
        "/specific\n/default"
    );
    assert_eq!(writes(&h).len(), 1);
    assert!(visible().contains("Updated 1, already had them 1"));
}
async fn cloak_requires_review_exact_consent_and_failed_download_consumes_it() {
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
    assert!(node("[data-action=cloak-acquire]").has_attribute("disabled"));
    click("[data-action=cloak-consent]").await;
    h.engine.borrow_mut().errors.insert(
        "/api/engines/cloak/acquisition".into(),
        (500, "Vendor operation failed".into(), String::new()),
    );
    click("[data-action=cloak-acquire]").await;
    assert!(snapshot().cloak.is_null());
    assert!(!snapshot().cloak_approved);
    h.engine.borrow_mut().errors.clear();
    click("[data-action=cloak-review]").await;
    assert!(node("[data-action=cloak-acquire]").has_attribute("disabled"));
    click("[data-action=cloak-consent]").await;
    click("[data-action=cloak-acquire]").await;
    assert!(visible().contains("Approved browser acquired"));
    let e = h.engine.borrow();
    let calls = e
        .calls
        .iter()
        .filter(|c| c.path.contains("acquisition") && c.method == "POST")
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1].body,
        json!({"offer_id":"f".repeat(64),"approve":true})
    );
}
async fn update_download_and_restart_each_require_exact_target_confirmation() {
    let h = mount(Engine::default(), true).await;
    until(|| snapshot().update.is_object()).await;
    click(".ps-navitem[data-id=settings]").await;
    click("[data-action=update-review]").await;
    assert!(visible().contains("1.0 MiB"));
    assert!(
        !h.engine
            .borrow()
            .calls
            .iter()
            .any(|c| c.path == "/download")
    );
    click("[data-action=update-later]").await;
    assert!(
        !h.engine
            .borrow()
            .calls
            .iter()
            .any(|c| c.path == "/download")
    );
    click("[data-action=update-review]").await;
    click("[data-action=update-approve]").await;
    assert_eq!(snapshot().update["state"], "ready");
    assert!(!h.engine.borrow().calls.iter().any(|c| c.path == "/install"));
    click("[data-action=update-review]").await;
    click("[data-action=update-approve]").await;
    let e = h.engine.borrow();
    for action in ["/download", "/install"] {
        let c = e.calls.iter().find(|c| c.path == action).unwrap();
        assert_eq!(c.body, json!({"offerId":"offer-11","approve":true}));
        assert_eq!(c.bearer, "a".repeat(64));
        assert_eq!(c.redirect, "error");
    }
}
async fn update_request_is_aborted_when_dashboard_unmounts() {
    let mut e = Engine::default();
    e.hold = "/status".into();
    let h = mount(e, true).await;
    until(|| !h.engine.borrow().signals.is_empty()).await;
    let signal = h.engine.borrow().signals[0].clone();
    assert!(!signal.aborted());
    shutdown();
    assert!(signal.aborted());
    delay().await;
}
async fn automation_attach_detach_and_snippets_use_same_session() {
    let h = mount(Engine::default(), false).await;
    click(".ps-navitem[data-id=automation]").await;
    click("[data-action=headless]").await;
    click("[data-action=attach][data-id=alpha]").await;
    assert!(visible().contains("attached :9222"));
    assert!(visible().contains("connect_over_cdp"));
    {
        let e = h.engine.borrow();
        let attach = e
            .calls
            .iter()
            .find(|c| c.path.ends_with("/attach"))
            .unwrap();
        assert_eq!(attach.body, json!({"headless":true}));
    }
    click("[data-action=detach][data-id=alpha]").await;
    assert!(!snapshot().connections.contains_key("alpha"));
}
async fn proxy_pool_edit_archive_restore_and_delete_use_real_routes() {
    let h = mount(Engine::default(), false).await;
    click(".ps-navitem[data-id=proxies]").await;
    click("[data-action=edit-proxy]").await;
    input("#form-pass", "");
    click("[data-action=save-proxy]").await;
    {
        let e = h.engine.borrow();
        assert_eq!(
            e.calls
                .iter()
                .find(|c| c.method == "PUT" && c.path == "/api/proxies/pool1")
                .unwrap()
                .body["pass"],
            ""
        );
    }
    click("[data-action=archive-proxy]").await;
    assert_eq!(h.engine.borrow().pool[0]["archived"], true);
    click("[data-action=proxy-tab][data-id=archive]").await;
    click("[data-action=archive-proxy]").await;
    assert_eq!(h.engine.borrow().pool[0]["archived"], false);
    click("[data-action=proxy-tab][data-id=all]").await;
    click("[data-action=delete-proxy]").await;
    assert!(!h.engine.borrow().pool.is_empty());
    click("[data-action=confirm]").await;
    assert!(h.engine.borrow().pool.is_empty());
}
async fn demo_mode_is_honest_and_live_locked_engine_never_uses_samples() {
    let mut e = Engine::default();
    e.offline = true;
    let h = mount(e, false).await;
    assert_eq!(snapshot().mode, "demo");
    assert!(visible().contains("Demo mode"));
    drop(h);
    let mut e = Engine::default();
    e.tokens = vec![json!({"id":"t1","name":"ci","token":"nmr_ci","created":1})];
    let _h = mount(e, false).await;
    assert_eq!(snapshot().mode, "locked");
    assert!(visible().contains("This engine needs a token"));
    assert!(!visible().contains("FB-Ads-01"));
    input("#form-token", "nmr_ci");
    click("[data-action=unlock]").await;
    assert_eq!(snapshot().mode, "live");
    assert!(visible().contains("Engine online"));
}
async fn stylesheet_and_event_cleanup_do_not_accumulate_on_remount() {
    let h = mount(Engine::default(), false).await;
    let root = node("#root");
    let before = document().query_selector_all("style").unwrap().length();
    for _ in 0..3 {
        draw();
    }
    assert_eq!(
        document().query_selector_all("style").unwrap().length(),
        before
    );
    drop(h);
    assert_eq!(
        document().query_selector_all("style").unwrap().length(),
        before - 1
    );
    let h = mount(Engine::default(), false).await;
    click("[data-action=theme]").await;
    assert_eq!(snapshot().theme, "light");
    assert!(root.is_connected());
    drop(h);
}
async fn desktop_startup_error_hook_sanitizes_bounds_and_escapes_log() {
    let _h = mount(Engine::default(), false).await;
    let f = Reflect::get(window().as_ref(), &JsValue::from_str("numeraError"))
        .unwrap()
        .dyn_into::<Function>()
        .unwrap();
    f.call1(
        &JsValue::NULL,
        &JsValue::from_str(&format!(
            "<script>secret</script>\u{0}{}",
            "x".repeat(20000)
        )),
    )
    .unwrap();
    assert!(visible().contains("The Numera engine did not start."));
    assert!(!exists("#root script"));
    assert!(snapshot().auth_error.chars().count() <= 16384);
    assert!(!snapshot().auth_error.contains('\0'));
}

