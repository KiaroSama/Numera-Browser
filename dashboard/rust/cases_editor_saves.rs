mod editor_save_lifecycle {
    use super::*;
    async fn failure_retry(status: u16) {
        let h = mount(Engine::default(), false).await;
        open().await;
        h.engine.borrow_mut().errors.insert(
            "/api/profiles/alpha".into(),
            (status, format!("Rejected {status}"), String::new()),
        );
        input("#editor-notes", "keep this exact draft");
        click("[data-action=save]").await;
        assert!(exists(".ps-editor"));
        assert_eq!(value("#editor-notes"), "keep this exact draft");
        assert!(!node("[data-action=save]").has_attribute("disabled"));
        assert!(visible().contains("Save failed:"));
        h.engine.borrow_mut().errors.clear();
        click("[data-action=save]").await;
        assert!(!exists(".ps-editor"));
        assert_eq!(writes(&h).len(), 2);
        assert_eq!(
            h.engine.borrow().profiles[0]["notes"],
            "keep this exact draft"
        );
    }
    case!(failed_409_preserves_draft, {
        failure_retry(409).await;
    });
    case!(failed_423_preserves_draft, {
        failure_retry(423).await;
    });
    case!(failed_422_preserves_draft, {
        failure_retry(422).await;
    });
    case!(failed_500_preserves_draft, {
        failure_retry(500).await;
    });
    case!(failed_network_preserves_draft, {
        failure_retry(0).await;
    });
    case!(stale_notes_retain_remote_tags, {
        let h = mount(Engine::default(), false).await;
        open().await;
        input("#editor-notes", "local notes");
        {
            let mut e = h.engine.borrow_mut();
            e.profiles[0]["tags"] = json!(["remote"]);
            e.profiles[0]["_rev"] = json!(6);
        }
        click("[data-action=save]").await;
        assert!(visible().contains("rebased onto the latest profile"));
        assert_eq!(value("#editor-notes"), "local notes");
        click("[data-action=save]").await;
        assert!(!exists(".ps-editor"));
        assert_eq!(h.engine.borrow().profiles[0]["tags"], json!(["remote"]));
        assert_eq!(
            writes(&h)
                .iter()
                .map(|c| number(&c.body, "_rev"))
                .collect::<Vec<_>>(),
            [5, 6]
        );
        assert!(writes(&h)[1].body.get("tags").is_none());
    });
    async fn overlap(latest: bool) {
        let h = mount(Engine::default(), false).await;
        open().await;
        input("#editor-notes", "local notes");
        {
            let mut e = h.engine.borrow_mut();
            e.profiles[0]["notes"] = json!("remote notes");
            e.profiles[0]["_rev"] = json!(6);
        }
        click("[data-action=save]").await;
        assert!(visible().contains("both changed"));
        assert!(node("[data-action=save]").has_attribute("disabled"));
        click("[data-action=save]").await;
        assert_eq!(writes(&h).len(), 1);
        click(if latest {
            "[data-action=resolve-latest]"
        } else {
            "[data-action=resolve-mine]"
        })
        .await;
        click("[data-action=save]").await;
        assert!(!exists(".ps-editor"));
        assert_eq!(
            h.engine.borrow().profiles[0]["notes"],
            if latest {
                "remote notes"
            } else {
                "local notes"
            }
        );
        assert_eq!(writes(&h)[1].body["_rev"], 6);
    }
    case!(overlapping_notes_keep_mine, {
        overlap(false).await;
    });
    case!(overlapping_notes_use_latest, {
        overlap(true).await;
    });
    async fn stale_intent(intent: &str) {
        let h = mount(Engine::default(), false).await;
        open().await;
        {
            let mut e = h.engine.borrow_mut();
            e.profiles[0]["tags"] = json!(["remote"]);
            e.profiles[0]["_rev"] = json!(6);
        }
        if intent == "preserve" {
            h.engine.borrow_mut().profiles[0]["proxy"]["pass"] = json!("remote secret");
            input("#editor-notes", "local notes");
        } else {
            click("[data-action=tab][data-id=proxy]").await;
            if intent == "remove" {
                click("[data-action=toggle-proxy]").await;
            } else {
                input(
                    "#editor-proxy-pass",
                    if intent == "clear" { "" } else { "replacement" },
                );
            }
        }
        click("[data-action=save]").await;
        assert!(visible().contains("rebased onto the latest profile"));
        click("[data-action=save]").await;
        assert!(!exists(".ps-editor"));
        assert_eq!(h.engine.borrow().profiles[0]["tags"], json!(["remote"]));
        let sent = &writes(&h)[1].body;
        assert_eq!(sent["_rev"], 6);
        assert!(sent.get("_passwordSet").is_none());
        match intent {
            "remove" => assert!(h.engine.borrow().profiles[0]["proxy"].is_null()),
            "preserve" => {
                assert!(sent.get("proxy").is_none());
                assert_eq!(
                    h.engine.borrow().profiles[0]["proxy"]["pass"],
                    "remote secret"
                );
            }
            _ => assert_eq!(
                sent["proxy"]["pass"],
                if intent == "clear" { "" } else { "replacement" }
            ),
        }
    }
    case!(stale_proxy_clear, {
        stale_intent("clear").await;
    });
    case!(stale_proxy_replace, {
        stale_intent("replace").await;
    });
    case!(stale_proxy_remove, {
        stale_intent("remove").await;
    });
    case!(stale_proxy_preserve, {
        stale_intent("preserve").await;
    });
    case!(conflict_locks_draft_and_masks_secrets, {
        let h = mount(Engine::default(), false).await;
        open().await;
        click("[data-action=tab][data-id=proxy]").await;
        input("#editor-proxy-pass", "local secret");
        {
            let mut e = h.engine.borrow_mut();
            e.profiles[0]["proxy"]["pass"] = json!("remote secret");
            e.profiles[0]["_rev"] = json!(6);
        }
        click("[data-action=save]").await;
        assert!(node(".ps-editor-main").has_attribute("inert"));
        assert!(!visible().contains("local secret"));
        assert!(!visible().contains("remote secret"));
        click("[data-action=toggle-proxy]").await;
        assert_eq!(value("#editor-proxy-pass"), "local secret");
        click("[data-action=resolve-latest]").await;
        assert_eq!(value("#editor-proxy-pass"), "remote secret");
        assert!(!node(".ps-editor-main").has_attribute("inert"));
        click("[data-action=save]").await;
        assert!(writes(&h)[1].body.get("proxy").is_none());
    });
    async fn removed(field: &str, value_: &str, secret: Value, latest: bool, refuse: bool) {
        let mut e = Engine::default();
        e.profiles[0]["proxy"]["pass"] = secret.clone();
        let h = mount(e, false).await;
        open().await;
        click("[data-action=tab][data-id=proxy]").await;
        input(&format!("#editor-proxy-{field}"), value_);
        {
            let mut e = h.engine.borrow_mut();
            e.profiles[0]["proxy"] = Value::Null;
            e.profiles[0]["_rev"] = json!(6);
        }
        click("[data-action=save]").await;
        assert!(visible().contains("both changed"));
        assert!(!visible().contains("original"));
        click(if latest {
            "[data-action=resolve-latest]"
        } else {
            "[data-action=resolve-mine]"
        })
        .await;
        if refuse {
            assert!(visible().contains("password is unavailable"));
            assert!(node("[data-action=save]").has_attribute("disabled"));
            assert_eq!(value(&format!("#editor-proxy-{field}")), value_);
            assert_eq!(writes(&h).len(), 1);
            click("[data-action=resolve-latest]").await;
            assert!(!node("[data-action=save]").has_attribute("disabled"));
            return;
        }
        click("[data-action=save]").await;
        assert!(!exists(".ps-editor"));
        let sent = &writes(&h)[1].body;
        assert_eq!(sent["_rev"], 6);
        if latest {
            assert!(sent.get("proxy").is_none());
            assert!(h.engine.borrow().profiles[0]["proxy"].is_null());
        } else {
            assert_eq!(sent["proxy"][field], value_);
            assert_eq!(sent["proxy"]["pass"], secret);
        }
    }
    case!(removed_proxy_restore_host, {
        removed("host", "127.0.0.2", json!("original"), false, false).await;
    });
    case!(removed_proxy_restore_port, {
        removed("port", "8082", json!("original"), false, false).await;
    });
    case!(removed_proxy_restore_user, {
        removed("user", "other-user", json!("original"), false, false).await;
    });
    case!(removed_proxy_restore_empty_password, {
        removed("host", "127.0.0.2", json!(""), false, false).await;
    });
    case!(latest_proxy_removal_with_secret, {
        removed("host", "127.0.0.2", json!("original"), true, false).await;
    });
    case!(latest_proxy_removal_without_secret, {
        removed("host", "127.0.0.2", Value::Null, true, false).await;
    });
    case!(unavailable_restoration_is_refused, {
        removed("host", "127.0.0.2", Value::Null, false, true).await;
    });
    case!(diverged_error_is_not_stale, {
        let h = mount(Engine::default(), false).await;
        open().await;
        let reads = h
            .engine
            .borrow()
            .calls
            .iter()
            .filter(|c| c.path == "/api/profiles")
            .count();
        h.engine.borrow_mut().errors.insert(
            "/api/profiles/alpha".into(),
            (409, "Profile diverged".into(), "ProfileDiverged".into()),
        );
        input("#editor-notes", "preserved draft");
        click("[data-action=save]").await;
        assert!(visible().contains("Save failed:"));
        assert_eq!(value("#editor-notes"), "preserved draft");
        assert!(!visible().contains("rebased onto the latest profile"));
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .filter(|c| c.path == "/api/profiles")
                .count(),
            reads
        );
    });
    case!(failed_stale_read_retains_draft_for_retry, {
        let h = mount(Engine::default(), false).await;
        open().await;
        {
            let mut e = h.engine.borrow_mut();
            e.profiles[0]["_rev"] = json!(6);
            e.profiles[0]["tags"] = json!(["remote"]);
            e.errors.insert(
                "/api/profiles".into(),
                (503, "Refresh unavailable".into(), String::new()),
            );
        }
        input("#editor-notes", "preserved draft");
        click("[data-action=save]").await;
        assert!(visible().contains("Could not load the latest profile:"));
        assert_eq!(value("#editor-notes"), "preserved draft");
        h.engine.borrow_mut().errors.clear();
        click("[data-action=save]").await;
        assert!(visible().contains("rebased onto the latest profile"));
        click("[data-action=save]").await;
        assert_eq!(
            writes(&h)
                .iter()
                .map(|c| number(&c.body, "_rev"))
                .collect::<Vec<_>>(),
            [5, 5, 6]
        );
        assert_eq!(h.engine.borrow().profiles[0]["notes"], "preserved draft");
    });
    async fn locked_intent(remove: bool) {
        let h = mount(Engine::default(), false).await;
        open().await;
        click("[data-action=tab][data-id=proxy]").await;
        if remove {
            click("[data-action=toggle-proxy]").await;
        } else {
            input("#editor-proxy-pass", "");
        }
        h.engine.borrow_mut().errors.insert(
            "/api/profiles/alpha".into(),
            (423, "vault locked".into(), String::new()),
        );
        click("[data-action=save]").await;
        assert!(exists(".ps-editor"));
        h.engine.borrow_mut().errors.clear();
        click("[data-action=save]").await;
        assert_eq!(writes(&h)[0].body["proxy"], writes(&h)[1].body["proxy"]);
        if remove {
            assert!(writes(&h)[1].body["proxy"].is_null())
        } else {
            assert_eq!(writes(&h)[1].body["proxy"]["pass"], "")
        }
    }
    case!(locked_retry_retains_clear, {
        locked_intent(false).await;
    });
    case!(locked_retry_retains_remove, {
        locked_intent(true).await;
    });
    case!(pending_save_cannot_repeat, {
        let h = mount(Engine::default(), false).await;
        open().await;
        h.engine.borrow_mut().hold = "/api/profiles/alpha".into();
        click("[data-action=save]").await;
        assert!(node("[data-action=save]").has_attribute("disabled"));
        click("[data-action=save]").await;
        assert_eq!(writes(&h).len(), 1);
        assert!(exists("#editor-notes"));
        finish_held(&h);
        until(|| !exists(".ps-editor")).await;
    });
    case!(committed_save_survives_refresh_failure, {
        let h = mount(Engine::default(), false).await;
        open().await;
        h.engine.borrow_mut().errors.insert(
            "/api/profiles".into(),
            (503, "Refresh unavailable".into(), String::new()),
        );
        input("#editor-notes", "committed draft");
        click("[data-action=save]").await;
        assert!(!exists(".ps-editor"));
        assert_eq!(writes(&h).len(), 1);
        open().await;
        assert_eq!(value("#editor-notes"), "committed draft");
    });
    async fn password_action(action: &str) {
        let h = mount(Engine::default(), false).await;
        open().await;
        if action == "notes" {
            input("#editor-notes", "notes only");
        } else {
            click("[data-action=tab][data-id=proxy]").await;
            match action {
                "retype" => {
                    input("#editor-proxy-pass", "");
                    input("#editor-proxy-pass", "original");
                }
                "pool" => input("#form-poolPick", "pool1"),
                _ => {
                    click("[data-action=toggle-proxy]").await;
                    click("[data-action=toggle-proxy]").await;
                    input("#editor-proxy-host", "localhost");
                    input("#editor-proxy-port", "8080");
                }
            }
        }
        click("[data-action=save]").await;
        let sent = &writes(&h)[0].body;
        assert!(sent.get("_passwordSet").is_none());
        if action == "notes" {
            assert!(sent.get("proxy").is_none())
        } else {
            assert_eq!(
                sent["proxy"]["pass"],
                match action {
                    "retype" => "original",
                    "pool" => "pooled",
                    _ => "",
                }
            )
        }
        open().await;
        input("#editor-notes", "next draft");
        click("[data-action=save]").await;
        assert!(writes(&h)[1].body.get("proxy").is_none());
    }
    case!(editor_notes_preserves_password, {
        password_action("notes").await;
    });
    case!(editor_retype_replaces_password, {
        password_action("retype").await;
    });
    case!(editor_pool_replaces_password, {
        password_action("pool").await;
    });
    case!(editor_off_on_clears_password, {
        password_action("off-on").await;
    });
}
