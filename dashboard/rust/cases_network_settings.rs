mod optimistic_rollback {
    use super::*;
    case!(create_view_does_not_invent_folder, {
        let h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=folders]").await;
        h.engine.borrow_mut().hold = "/api/folders".into();
        input("#form-newFolder", "Ads");
        click("[data-action=create-folder]").await;
        assert!(!texts(".ps-fold-name").contains(&"Ads".into()));
        assert!(texts(".ps-fold-name").contains(&"Work".into()));
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .find(|c| c.path == "/api/folders")
                .unwrap()
                .body,
            json!({"name":"Ads"})
        );
        finish_held(&h);
        until(|| texts(".ps-fold-name").contains(&"Ads".into())).await;
    });
    case!(refused_create_retains_authoritative_folders, {
        let mut e = Engine::default();
        e.errors.insert(
            "/api/folders".into(),
            (400, "read-only".into(), String::new()),
        );
        let _h = mount(e, false).await;
        click(".ps-navitem[data-id=folders]").await;
        input("#form-newFolder", "Ghost");
        click("[data-action=create-folder]").await;
        assert!(!texts(".ps-fold-name").contains(&"Ghost".into()));
        assert!(texts(".ps-fold-name").contains(&"Work".into()));
        assert!(visible().contains("read-only"));
    });
    case!(rename_awaits_authority, {
        let h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=folders]").await;
        click("[data-action=rename-folder][data-id=Work]").await;
        input("#form-to", "Client A");
        h.engine.borrow_mut().hold = "/api/folders/rename".into();
        click("[data-action=commit-folder]").await;
        assert!(texts(".ps-fold-name").contains(&"Work".into()));
        assert!(!texts(".ps-fold-name").contains(&"Client A".into()));
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .find(|c| c.path == "/api/folders/rename")
                .unwrap()
                .body,
            json!({"from":"Work","to":"Client A"})
        );
        finish_held(&h);
        until(|| texts(".ps-fold-name").contains(&"Client A".into())).await;
    });
    case!(blank_create_name_refused, {
        let h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=folders]").await;
        input("#form-newFolder", "   ");
        click("[data-action=create-folder]").await;
        assert!(
            !h.engine
                .borrow()
                .calls
                .iter()
                .any(|c| c.path == "/api/folders")
        );
    });
}
mod proxy_pool_password_intent {
    use super::*;
    async fn edit_password(stored: Value, edited: Option<&str>, expected: Value) {
        let mut e = Engine::default();
        e.pool[0]["pass"] = stored;
        let h = mount(e, false).await;
        click(".ps-navitem[data-id=proxies]").await;
        click("[data-action=edit-proxy]").await;
        if let Some(edited) = edited {
            input("#form-pass", edited)
        }
        click("[data-action=save-proxy]").await;
        assert!(!exists(".ps-modal"));
        let e = h.engine.borrow();
        let call = e
            .calls
            .iter()
            .find(|c| c.path == "/api/proxies/pool1" && c.method == "PUT")
            .unwrap();
        assert_eq!(call.body["pass"], expected);
    }
    case!(pool_clear_is_preserved, {
        edit_password(json!("fixture-password"), Some(""), json!("")).await;
        proxy_pool_edit_archive_restore_and_delete_use_real_routes().await;
    });
    case!(pool_replace_is_preserved, {
        edit_password(
            json!("fixture-password"),
            Some("replacement"),
            json!("replacement"),
        )
        .await;
    });
    case!(pool_withheld_remains_null, {
        edit_password(Value::Null, None, Value::Null).await;
    });
}
mod tag_search {
    use super::*;
    async fn find(query: &str) {
        let mut e = Engine::default();
        e.profiles[0]["tags"] = json!(["Work"]);
        e.profiles[1]["tags"] = json!(["personal"]);
        let _h = mount(e, false).await;
        input("#profile-search", query);
        assert_eq!(texts(".ps-pname"), ["alpha"]);
    }
    case!(lowercase_finds_mixed_tag, {
        find("work").await;
    });
    case!(titlecase_finds_mixed_tag, {
        find("Work").await;
    });
    case!(uppercase_finds_mixed_tag, {
        find("WORK").await;
    });
    case!(arbitrary_case_finds_mixed_tag, {
        find("WoRk").await;
    });
    case!(search_does_not_rewrite_tag, {
        let mut e = Engine::default();
        e.profiles[0]["tags"] = json!(["Work"]);
        let h = mount(e, false).await;
        input("#profile-search", "work");
        assert_eq!(texts(".ps-pname"), ["alpha"]);
        assert_eq!(texts(".ps-tag"), ["Work"]);
        assert_eq!(h.engine.borrow().profiles[0]["tags"], json!(["Work"]));
    });
    case!(numeric_legacy_tag_cannot_crash, {
        let mut e = Engine::default();
        e.profiles[0]["tags"] = json!([42]);
        e.profiles[1]["tags"] = json!(["work"]);
        let _h = mount(e, false).await;
        assert_eq!(texts(".ps-pname"), ["alpha", "bravo"]);
        input("#profile-search", "work");
        assert_eq!(texts(".ps-pname"), ["bravo"]);
        input("#profile-search", "alpha");
        assert_eq!(texts(".ps-pname"), ["alpha"]);
    });
}
mod timezone_auto_or_searchable {
    use super::*;
    async fn field() -> Harness {
        let h = mount(Engine::default(), false).await;
        open().await;
        click("[data-action=tab][data-id=fingerprint]").await;
        h
    }
    case!(locale_defaults_all_offered, {
        let _h = field().await;
        let (zones, valid) = timezones("Asia/Tehran");
        assert!(valid);
        assert!(zones.len() > 50);
        for (_, z) in LOCALES {
            assert!(zones.contains(&z.into()));
            assert!(timezones(z).1)
        }
        assert!(zones.contains(&"Asia/Tehran".into()));
        assert!(zones.contains(&"America/Los_Angeles".into()));
    });
    case!(every_zone_uses_searchable_datalist, {
        let _h = field().await;
        assert!(exists("input[list=numera-timezones]"));
        assert!(!exists("select[name=timezone]"));
        let values = document()
            .query_selector_all("#numera-timezones option")
            .unwrap();
        let actual = (0..values.length())
            .map(|i| {
                values
                    .item(i)
                    .unwrap()
                    .dyn_into::<Element>()
                    .unwrap()
                    .get_attribute("value")
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, timezones("America/New_York").0);
    });
    case!(auto_manual_are_distinct_buttons, {
        let _h = field().await;
        assert_eq!(
            node("[data-action=timezone-auto]").text_content().unwrap(),
            "Auto"
        );
        assert_eq!(
            node("[data-action=timezone-manual]")
                .text_content()
                .unwrap(),
            "Manual"
        );
        assert!(
            node("[data-action=timezone-manual]")
                .get_attribute("class")
                .unwrap()
                .contains("on")
        );
        assert!(
            !node("[data-action=timezone-auto]")
                .get_attribute("class")
                .unwrap()
                .contains("on")
        );
    });
    case!(auto_signal_does_not_remove_language_signal, {
        let _h = field().await;
        click("[data-action=ip][data-id=languages]").await;
        click("[data-action=timezone-auto]").await;
        assert_eq!(
            snapshot().editor.unwrap().draft["ipBased"],
            json!(["languages", "timezone"])
        );
        click("[data-action=timezone-manual]").await;
        assert_eq!(
            snapshot().editor.unwrap().draft["ipBased"],
            json!(["languages"])
        );
    });
    case!(auto_without_proxy_never_locks_input, {
        let mut e = Engine::default();
        e.profiles[0]["proxy"] = Value::Null;
        e.profiles[0]["ipBased"] = json!(["timezone"]);
        let h = mount(e, false).await;
        open().await;
        click("[data-action=tab][data-id=fingerprint]").await;
        assert!(node("[data-action=timezone-auto]").has_attribute("disabled"));
        assert!(!node("#editor-timezone").has_attribute("disabled"));
        drop(h);
        let mut e = Engine::default();
        e.profiles[0]["ipBased"] = json!(["timezone"]);
        let _h = mount(e, false).await;
        open().await;
        click("[data-action=tab][data-id=fingerprint]").await;
        assert!(node("#editor-timezone").has_attribute("disabled"));
    });
    case!(unknown_timezone_is_flagged, {
        timezone_auto_manual_and_unknown_zone_work_in_chromium().await;
    });
}
mod update_consent {
    use super::*;
    case!(source_mode_has_no_update_action, {
        let h = mount(Engine::default(), false).await;
        click(".ps-navitem[data-id=settings]").await;
        assert!(visible().contains("Source and server installations"));
        assert!(!exists("[data-action=update-check]"));
        assert!(!h.engine.borrow().calls.iter().any(|c| c.path == "/status"));
    });
    case!(exact_update_target_needs_each_confirmation, {
        update_download_and_restart_each_require_exact_target_confirmation().await;
    });
    case!(http_refusal_retains_safe_deferred_state, {
        let mut e = Engine::default();
        e.update_http_status = 409;
        e.update["state"] = json!("deferred");
        e.update["message"] = json!("Close active sessions before installing.");
        let h = mount(e, true).await;
        until(|| snapshot().update.is_object()).await;
        click(".ps-navitem[data-id=settings]").await;
        assert!(visible().contains("Close active sessions before installing."));
        assert_eq!(
            h.engine
                .borrow()
                .calls
                .iter()
                .filter(|c| c.path == "/status")
                .count(),
            1
        );
        assert!(!h.engine.borrow().calls.iter().any(|c| c.path == "/install"));
    });
    case!(request_lifetime_owns_abort_controller, {
        update_request_is_aborted_when_dashboard_unmounts().await;
    });
}
