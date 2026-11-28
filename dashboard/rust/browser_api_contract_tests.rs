use super::*;

#[wasm_bindgen_test]
async fn edited_proxy_remains_usable_after_its_previous_check_is_cleared() {
    let mut engine = Engine::default();
    engine.pool[0]["status"] = json!("ok");
    engine.pool[0]["lastCheck"] = json!({"ip":"192.0.2.1","latencyMs":10});
    let h = mount(engine, false).await;
    click("[data-action=nav][data-id=proxies]").await;
    click("[data-action=edit-proxy][data-id=pool1]").await;
    input("#form-host", "changed.example");
    click("[data-action=save-proxy]").await;
    until(|| !snapshot().busy).await;

    assert_eq!(h.engine.borrow().pool[0]["lastCheck"], Value::Null);
    assert_eq!(snapshot().pool[0]["host"], "changed.example");
    assert_eq!(snapshot().pool[0]["lastCheck"], Value::Null);
    assert!(visible().contains("changed.example"));
    assert!(visible().contains("Untested"));
    assert!(!snapshot().message_error);

    click("[data-action=nav][data-id=profiles]").await;
    click("[data-action=nav][data-id=proxies]").await;
    assert!(snapshot().view_error.is_none());
    assert!(exists("[data-action=edit-proxy][data-id=pool1]"));
}
