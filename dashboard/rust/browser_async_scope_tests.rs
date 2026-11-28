use super::*;

async fn hold_alignment(h: &Harness) {
    open().await;
    click("[data-action=tab][data-id=fingerprint]").await;
    h.engine.borrow_mut().hold = "/api/profiles/alpha/align".into();
    click("[data-action=probe][data-id=align]").await;
    until(|| !h.engine.borrow().pending.is_empty()).await;
}

async fn finish_alignment(h: &Harness) {
    let (resolve, profile) = {
        let mut engine = h.engine.borrow_mut();
        engine.hold.clear();
        engine.profiles[0]["screen"] = json!("1366x768");
        engine.profiles[0]["_rev"] = json!(6);
        (engine.pending.pop().unwrap(), engine.profiles[0].clone())
    };
    resolve
        .call1(
            &JsValue::NULL,
            &response(json!({"changed":["screen"],"profile":profile}), 200),
        )
        .unwrap();
    until(|| !snapshot().busy).await;
}

#[wasm_bindgen_test]
async fn delayed_alignment_cannot_replace_a_new_profile_draft() {
    let h = mount(Engine::default(), false).await;
    hold_alignment(&h).await;
    click("[data-action=close-editor]").await;
    click("[data-action=new]").await;
    input("#editor-name", "New independent profile");
    input("#editor-notes", "Keep this new draft");
    let new_id = snapshot().editor.unwrap().draft["id"].clone();

    finish_alignment(&h).await;
    let current = snapshot().editor.unwrap();
    assert_eq!(current.draft["id"], new_id);
    assert_eq!(current.draft["name"], "New independent profile");
    assert_eq!(current.draft["notes"], "Keep this new draft");
    assert!(current.probe.is_null());
    click("[data-action=save]").await;
    until(|| snapshot().editor.is_none()).await;
    assert_eq!(h.engine.borrow().profiles.len(), 3);
    assert!(writes(&h).is_empty());
}

#[wasm_bindgen_test]
async fn delayed_alignment_does_not_change_a_reopened_editor_for_the_same_profile() {
    let h = mount(Engine::default(), false).await;
    hold_alignment(&h).await;
    click("[data-action=close-editor]").await;
    click("[data-action=commands]").await;
    input("#command-query", "Open alpha");
    click("[data-action=command]").await;
    input("#editor-notes", "New edit after reopening");
    let draft = snapshot().editor.unwrap().draft;

    finish_alignment(&h).await;
    let current = snapshot().editor.unwrap();
    assert_eq!(current.draft, draft);
    assert!(current.probe.is_null());
}

#[wasm_bindgen_test]
async fn alignment_rebases_remote_changes_without_discarding_unsaved_notes() {
    let h = mount(Engine::default(), false).await;
    hold_alignment(&h).await;
    click("[data-action=tab][data-id=general]").await;
    input("#editor-notes", "My independent note");

    finish_alignment(&h).await;
    let editor = snapshot().editor.unwrap();
    assert_eq!(editor.draft["notes"], "My independent note");
    assert_eq!(editor.draft["screen"], "1366x768");
    assert_eq!(editor.base["_rev"], 6);
    assert!(editor.conflicts.is_empty());
    click("[data-action=save]").await;
    until(|| snapshot().editor.is_none()).await;
    assert_eq!(
        h.engine.borrow().profiles[0]["notes"],
        "My independent note"
    );
    assert_eq!(writes(&h)[0].body["_rev"], 6);
}

#[wasm_bindgen_test]
async fn cookie_import_keeps_the_selected_profile_and_replace_choice_while_reading() {
    let h = mount(Engine::default(), false).await;
    open().await;
    click("[data-action=tab][data-id=advanced]").await;
    click("[data-action=replace-cookies]").await;
    click("[data-action=import-cookies]").await;
    let picker = node("#numera-file-picker")
        .dyn_into::<HtmlInputElement>()
        .unwrap();
    let parts = Array::new();
    parts.push(&JsValue::from_str("[]"));
    let file = web_sys::File::new_with_str_sequence(&parts, "fixture-cookies.json").unwrap();
    let transfer = web_sys::DataTransfer::new().unwrap();
    transfer.items().add_with_file(&file).unwrap();
    picker.set_files(Some(&transfer.files().unwrap()));
    let file = picker.files().unwrap().get(0).unwrap();
    let waiting = Rc::new(RefCell::new(None::<Function>));
    let held = waiting.clone();
    let read = Closure::wrap(Box::new(move || {
        js_sys::Promise::new(&mut |resolve, _| *held.borrow_mut() = Some(resolve))
    }) as Box<dyn FnMut() -> js_sys::Promise>);
    Reflect::set(file.as_ref(), &JsValue::from_str("text"), read.as_ref()).unwrap();
    picker
        .dispatch_event(&Event::new("change").unwrap())
        .unwrap();
    until(|| waiting.borrow().is_some()).await;

    click("[data-action=close-editor]").await;
    click("[data-action=new]").await;
    let new_id = snapshot().editor.unwrap().draft["id"].clone();
    assert_eq!(snapshot().form["replaceCookies"], false);
    waiting
        .borrow_mut()
        .take()
        .unwrap()
        .call1(&JsValue::NULL, &JsValue::from_str("[]"))
        .unwrap();
    until(|| !snapshot().busy).await;

    let engine = h.engine.borrow();
    let imports: Vec<_> = engine
        .calls
        .iter()
        .filter(|call| call.method == "POST" && call.path.ends_with("/cookies"))
        .collect();
    assert_eq!(imports.len(), 1);
    assert_eq!(imports[0].path, "/api/profiles/alpha/cookies");
    assert_eq!(imports[0].body, json!({"text":"[]","clear":true}));
    assert_eq!(snapshot().editor.unwrap().draft["id"], new_id);
}

fn schedule() -> Value {
    json!({"id":"warm-alpha","profileId":"alpha","everyHours":12,"minutes":5,"preset":"general","enabled":true})
}

#[wasm_bindgen_test]
async fn schedule_edits_preserve_fractional_hours_and_report_failed_writes() {
    let mut engine = Engine::default();
    engine.schedules.push(schedule());
    let h = mount(engine, false).await;
    open().await;
    click("[data-action=tab][data-id=advanced]").await;
    until(|| exists("#schedule-everyHours")).await;
    h.engine.borrow_mut().errors.insert(
        "/api/schedules/warm-alpha".into(),
        (409, "Schedule is locked".into(), String::new()),
    );
    input("#schedule-everyHours", "0.5");
    assert!(node("#schedule-everyHours").has_attribute("disabled"));
    until(|| !snapshot().busy).await;
    assert!(visible().contains("Schedule failed: Schedule is locked"));
    assert_eq!(snapshot().schedule["everyHours"], 12);

    h.engine.borrow_mut().errors.clear();
    input("#schedule-everyHours", "0.5");
    until(|| !snapshot().busy).await;
    assert_eq!(snapshot().schedule["everyHours"], 0.5);
    assert_eq!(h.engine.borrow().schedules[0]["everyHours"], 0.5);
    assert!(!node("#schedule-everyHours").has_attribute("disabled"));
}

#[wasm_bindgen_test]
async fn pending_schedule_changes_cannot_attach_another_profiles_schedule() {
    for creating in [false, true] {
        let mut engine = Engine::default();
        if !creating {
            engine.schedules.push(schedule());
        }
        let h = mount(engine, false).await;
        open().await;
        click("[data-action=tab][data-id=advanced]").await;
        h.engine.borrow_mut().hold = if creating {
            "/api/schedules"
        } else {
            "/api/schedules/warm-alpha"
        }
        .into();
        if creating {
            click("[data-action=schedule]").await;
        } else {
            input("#schedule-everyHours", "24");
        }
        until(|| !h.engine.borrow().pending.is_empty()).await;
        h.engine.borrow_mut().hold.clear();
        click("[data-action=close-editor]").await;
        click("[data-action=new]").await;
        let resolve = h.engine.borrow_mut().pending.remove(0);
        let mut updated = schedule();
        updated["everyHours"] = json!(24);
        resolve
            .call1(&JsValue::NULL, &response(updated, 200))
            .unwrap();
        until(|| !snapshot().busy).await;
        assert!(snapshot().schedule.is_null());
        assert_eq!(snapshot().editor.unwrap().draft["_new"], true);
    }
}
