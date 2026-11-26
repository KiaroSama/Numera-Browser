use super::*;

#[wasm_bindgen_test]
async fn cookie_exports_download_both_formats_without_following_redirects() {
    let h = mount(Engine::default(), false).await;
    open().await;
    click("[data-action=tab][data-id=advanced]").await;
    for (format, mime, content) in [
        ("json", "application/json", r#"{"cookies":[]}"#),
        ("netscape", "text/plain", "# Netscape HTTP Cookie File\n"),
    ] {
        let seen = h.blobs.borrow().len();
        click(&format!("[data-action=export-cookies][data-id={format}]")).await;
        until(|| h.blobs.borrow().len() == seen + 1).await;
        let blob = h.blobs.borrow()[seen].clone();
        assert_eq!(blob.type_(), mime);
        assert_eq!(
            JsFuture::from(blob.text())
                .await
                .unwrap()
                .as_string()
                .as_deref(),
            Some(content)
        );
        let path = format!("/api/profiles/alpha/cookies/export?format={format}");
        assert!(
            h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.path == path && c.method == "GET" && c.redirect == "error")
        );
    }
}

#[wasm_bindgen_test]
async fn readiness_and_stop_are_scoped_to_the_clicked_or_selected_profile() {
    let mut engine = Engine::default();
    for p in &mut engine.profiles {
        p["status"] = json!("running");
    }
    let h = mount(engine, false).await;
    click("[data-action=profile-ip-check][data-id=alpha]").await;
    until(|| visible().contains("IP check: ready")).await;
    assert!(visible().contains("192.0.2.1"));
    assert_eq!(
        document()
            .query_selector_all(".ps-readiness")
            .unwrap()
            .length(),
        1
    );
    click("[data-action=select][data-id=alpha]").await;
    click("[data-action=fleet-stop]").await;
    until(|| {
        snapshot()
            .profiles
            .iter()
            .any(|p| p["id"] == "alpha" && p["status"] == "stopped")
    })
    .await;
    let engine = h.engine.borrow();
    assert_eq!(engine.profiles[1]["status"], "running");
    assert!(engine.calls.iter().any(|c| c.path == "/api/fleet/sync"
        && c.body == json!({"profile_ids":["alpha"],"url":null,"action":"stop"})));
}

#[wasm_bindgen_test]
async fn release_mutations_send_the_shell_approval_and_apply_requires_review() {
    let h = mount(Engine::default(), true).await;
    change(|a| {
        a.mcp_bridge_command = Some("numera-engine mcp --bridge http://127.0.0.1:17654".into())
    });
    click("[data-action=nav][data-id=settings]").await;
    until(|| snapshot().release_preferences.is_object()).await;
    click("[data-action=release-preference][data-id=app_auto_update]").await;
    until(|| snapshot().release_preferences["app_auto_update"] == true).await;
    click("[data-action=release-check][data-id=browsers]").await;
    until(|| visible().contains("Camoufox: 1 installed")).await;
    click("[data-action=release-check][data-id=app]").await;
    until(|| exists("[data-action=release-apply][data-id=app]")).await;
    click("[data-action=release-apply][data-id=app]").await;
    until(|| exists("[data-action=confirm]")).await;
    assert!(
        !h.engine
            .borrow()
            .calls
            .iter()
            .any(|c| c.path == "/api/updates/apply")
    );
    click("[data-action=confirm]").await;
    until(|| visible().contains("Applied fixture update")).await;
    let engine = h.engine.borrow();
    let apply = engine
        .calls
        .iter()
        .find(|c| c.path == "/api/updates/apply")
        .expect("update apply call");
    assert_eq!(apply.body, json!({"channel":"app", "clean_old":true}));
    assert_eq!(apply.update_approval, "a".repeat(64));
    assert_eq!(apply.redirect, "error");
    let saved = engine
        .calls
        .iter()
        .find(|c| c.path == "/api/updates/preferences" && c.method == "PUT")
        .expect("preferences write");
    assert_eq!(saved.update_approval, "a".repeat(64));
    assert_eq!(saved.redirect, "error");
    assert!(
        engine
            .calls
            .iter()
            .filter(|c| c.path != "/api/updates/apply"
                && (c.path != "/api/updates/preferences" || c.method != "PUT"))
            .all(|c| c.update_approval.is_empty())
    );
}

#[wasm_bindgen_test]
async fn combined_update_shows_both_versions_and_sends_approved_all_channel() {
    let h = mount(Engine::default(), true).await;
    h.engine.borrow_mut().preferences["clean_old_versions_on_update"] = json!(false);
    change(|a| {
        a.mcp_bridge_command = Some("numera-engine mcp --bridge http://127.0.0.1:17654".into());
    });
    click("[data-action=nav][data-id=settings]").await;
    until(|| snapshot().release_preferences.is_object()).await;
    click("[data-action=release-check][data-id=all]").await;
    until(|| visible().contains("App: 1.0.0 installed. 1.1.0 available.")).await;
    assert!(visible().contains("Chromium: 158 installed. 159 available."));
    click("[data-action=release-apply][data-id=all]").await;
    assert!(visible().contains("Apply the App + Browsers update?"));
    assert!(
        !h.engine
            .borrow()
            .calls
            .iter()
            .any(|call| call.path == "/api/updates/apply")
    );
    click("[data-action=confirm]").await;
    until(|| visible().contains("Applied fixture update")).await;
    let engine = h.engine.borrow();
    assert!(engine.calls.iter().any(|call| call.method == "GET"
        && call.path == "/api/updates/check?channel=all"
        && call.update_approval.is_empty()));
    let apply = engine
        .calls
        .iter()
        .find(|call| call.method == "POST" && call.path == "/api/updates/apply")
        .expect("combined update apply request");
    assert_eq!(apply.body, json!({"channel":"all","clean_old":false}));
    assert_eq!(apply.update_approval, "a".repeat(64));
    assert_eq!(apply.redirect, "error");
}

#[wasm_bindgen_test]
async fn mcp_connection_check_sends_only_a_loopback_ping() {
    let h = mount(Engine::default(), false).await;
    set_auth("fixture-token");
    change(|a| {
        a.mcp_bridge_command = Some("numera-engine mcp --bridge http://127.0.0.1:17654".into())
    });
    click("[data-action=nav][data-id=settings]").await;
    assert!(visible().contains("numera-engine mcp --bridge http://127.0.0.1:17654"));
    assert!(!visible().contains("fixture-token"));
    click("[data-action=mcp-check]").await;
    until(|| visible().contains("MCP bridge responded")).await;
    assert!(h.engine.borrow().calls.iter().any(|c| c.path == "/api/mcp"
        && c.body == json!({"jsonrpc":"2.0","id":"dashboard-ping","method":"ping"})
        && c.bearer == "fixture-token"
        && c.redirect == "error"));
}
