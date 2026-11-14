//! Original component-case inventory, executed in Chromium. Each test name
//! maps to one source case in tests/vitest-case-inventory.json.
use super::*;
macro_rules! case {($name:ident,$body:block)=>{#[wasm_bindgen_test(async)]async fn $name() $body};}
fn count(selector: &str) -> u32 {
    document().query_selector_all(selector).unwrap().length()
}
fn texts(selector: &str) -> Vec<String> {
    let nodes = document().query_selector_all(selector).unwrap();
    (0..nodes.length())
        .map(|i| nodes.item(i).unwrap().text_content().unwrap_or_default())
        .collect()
}
fn value(selector: &str) -> String {
    value_of(&node(selector))
}
async fn exported(h: &Harness) -> Value {
    let blob = h.blobs.borrow().last().unwrap().clone();
    let text = JsFuture::from(blob.text())
        .await
        .unwrap()
        .as_string()
        .unwrap();
    serde_json::from_str(&text).unwrap()
}
fn select_file(text: &str) {
    let parts = Array::new();
    parts.push(&JsValue::from_str(text));
    let file = web_sys::File::new_with_str_sequence(&parts, "fixture.json").unwrap();
    let transfer = web_sys::DataTransfer::new().unwrap();
    transfer.items().add_with_file(&file).unwrap();
    let input = node("#numera-file-picker")
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    input.set_files(transfer.files().as_ref());
    input
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
}
async fn import_file(text: &str) {
    click("[data-action=import-profiles]").await;
    select_file(text);
    for _ in 0..4 {
        delay().await;
    }
}
fn finish_held(h: &Harness) {
    let mut e = h.engine.borrow_mut();
    let resolve = e.pending.pop().unwrap();
    let call = e
        .calls
        .iter()
        .rev()
        .find(|c| c.path == e.hold)
        .unwrap()
        .clone();
    e.hold.clear();
    let result = e.answer(&call);
    resolve.call1(&JsValue::NULL, &result).unwrap();
}
async fn generate(
    count: &str,
    start: &str,
    os: &[&str],
    countries: &[&str],
    engine: &str,
) -> (Harness, Vec<Value>) {
    let h = mount(Engine::default(), false).await;
    click("[data-action=bulk]").await;
    input("#batch-count", count);
    input("#batch-start", start);
    for os in os {
        click(&format!("[data-id='os|{os}']")).await;
    }
    for c in countries {
        click(&format!("[data-id='countries|{c}']")).await;
    }
    input("#batch-engine", engine);
    click("[data-action=create-batch]").await;
    until(|| snapshot().modal.is_empty()).await;
    let profiles = h
        .engine
        .borrow()
        .calls
        .iter()
        .find(|c| c.path == "/api/profiles/batch")
        .unwrap()
        .body["profiles"]
        .clone();
    (h, list(&profiles))
}

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
