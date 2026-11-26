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

include!("cases_profile_lists.rs");
include!("cases_editor_saves.rs");
include!("cases_browser_surfaces.rs");
include!("cases_network_settings.rs");
