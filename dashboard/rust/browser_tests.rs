#![allow(clippy::field_reassign_with_default)]
//! Run inside a real headless Chromium, exercising Rust/WASM DOM handlers and
//! fetch effects. The fake engine is Rust; there is no JavaScript test fixture.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[derive(Clone, Debug)]
struct Recorded {
    method: String,
    path: String,
    body: Value,
    bearer: String,
    redirect: String,
}
struct Engine {
    profiles: Vec<Value>,
    config: Value,
    pool: Vec<Value>,
    calls: Vec<Recorded>,
    errors: BTreeMap<String, (u16, String, String)>,
    tokens: Vec<Value>,
    offer: Value,
    update: Value,
    update_http_status: u16,
    offline: bool,
    hold: String,
    signals: Vec<web_sys::AbortSignal>,
    icons_missing: bool,
    import_answer: Value,
    trust_result: Value,
    align_result: Value,
    pending: Vec<Function>,
}
fn profile(id: &str) -> Value {
    let mut p = new_profile("Work", "playwright");
    p["id"] = json!(id);
    p["name"] = json!(id);
    p["_new"] = json!(false);
    p["_rev"] = json!(5);
    p["proxy"] = json!({"type":"HTTP","host":"localhost","port":"8080","user":"u","pass":"original","country":"US"});
    p
}
fn offer() -> Value {
    json!({"engine":"cloak","available":true,"installed":false,"offer_id":"f".repeat(64),"sdk":"0.5.12","version":"146.0.7680.177.5","platform":"linux-x64","reason":null,"notice":"Vendor license applies"})
}
fn update() -> Value {
    json!({"state":"available","message":"A verified update is available.","current":{"version":"0.36.0","sourceSha":"1".repeat(40),"generation":10,"platform":"linux-x64","portable":true},"offer":{"id":"offer-11","version":"0.36.1","sourceSha":"2".repeat(40),"generation":11,"downloadBytes":1048576,"expandedBytes":4194304,"components":{},"restartRequired":true},"progress":{"receivedBytes":0,"totalBytes":1048576},"canCancel":true})
}
impl Default for Engine {
    fn default() -> Self {
        Self {
            profiles: vec![profile("alpha"), profile("bravo")],
            config: json!({"folders":["Work"],"default_engine":"camoufox","default_extensions":[],"version":"0.36.0","root":"/data"}),
            pool: vec![
                json!({"id":"pool1","type":"SOCKS5","host":"pool.example","port":"1080","user":"pu","pass":"pooled","country":"GB"}),
            ],
            calls: vec![],
            errors: BTreeMap::new(),
            tokens: vec![],
            offer: offer(),
            update: update(),
            update_http_status: 200,
            offline: false,
            hold: String::new(),
            signals: vec![],
            icons_missing: false,
            import_answer: json!({"imported":2,"format":"numera","results":[]}),
            trust_result: json!({"grade":"C","score":81,"passed":13,"total":16}),
            align_result: json!({"before":{"grade":"C","score":81},"after":{"grade":"B","score":94},"changed":["os","screen","webglRenderer"]}),
            pending: vec![],
        }
    }
}
fn response(body: Value, status: u16) -> JsValue {
    let options = web_sys::ResponseInit::new();
    options.set_status(status);
    let encoded = body.to_string();
    Response::new_with_opt_str_and_init(if status == 204 { None } else { Some(&encoded) }, &options)
        .unwrap()
        .into()
}
impl Engine {
    fn answer(&mut self, call: &Recorded) -> JsValue {
        let p = call.path.as_str();
        let method = call.method.as_str();
        if !self.tokens.is_empty() && !self.tokens.iter().any(|t| s(t, "token") == call.bearer) {
            return response(json!({"detail":"invalid or missing API token"}), 401);
        }
        if let Some((status, detail, kind)) = self.errors.get(p) {
            return response(json!({"detail":detail,"kind":kind}), *status);
        }
        let body = &call.body;
        let value = match (method, p) {
            ("GET", "/api/health") => json!({"ok":true}),
            ("GET", "/api/engines") => {
                json!({"playwright":true,"cloak":false,"camoufox":true,"patchright":true})
            }
            ("GET", "/api/config") => self.config.clone(),
            ("PUT", "/api/config") => {
                for (k, v) in body.as_object().unwrap() {
                    self.config[k] = v.clone();
                }
                self.config.clone()
            }
            ("GET", "/api/profiles") => json!(self.profiles),
            ("GET", "/api/proxies") => json!(self.pool),
            ("GET", "/api/schedules") => json!([]),
            ("POST", "/api/profiles") => {
                let mut p = body.clone();
                p["id"] = json!(format!("created-{}", self.profiles.len()));
                p["_new"] = json!(false);
                p["_rev"] = json!(1);
                if p["engine"].is_null() {
                    p["engine"] = self.config["default_engine"].clone();
                }
                self.profiles.push(p.clone());
                p
            }
            ("POST", "/api/profiles/batch") => {
                for mut p in list(&body["profiles"]) {
                    p["id"] = json!(format!("batch-{}", self.profiles.len()));
                    p["_new"] = json!(false);
                    if p["engine"].is_null() {
                        p["engine"] = self.config["default_engine"].clone();
                    }
                    self.profiles.push(p);
                }
                json!({"created":list(&body["profiles"]).len()})
            }
            ("POST", "/api/profiles/bulk") => {
                let ids = strings(&body["ids"]);
                let mut saved = vec![];
                for p in &mut self.profiles {
                    if ids.contains(&s(p, "id").into()) {
                        merge_patch(p, &body["patch"]);
                        saved.push(p.clone());
                    }
                }
                json!(saved)
            }
            ("POST", "/api/profiles/delete") => {
                let ids = strings(&body["ids"]);
                self.profiles.retain(|p| !ids.contains(&s(p, "id").into()));
                json!({"removed":ids,"missing":[],"failed":[]})
            }
            ("POST", "/api/profiles/import") => self.import_answer.clone(),
            ("GET", "/api/tokens") => {
                json!({"tokens":self.tokens.iter().map(|t|{let mut t=t.clone();t.as_object_mut().unwrap().remove("token");t}).collect::<Vec<_>>(),"envToken":false})
            }
            ("POST", "/api/tokens") => {
                let t = json!({"id":format!("t{}",self.tokens.len()+1),"name":body["name"],"created":1,"token":format!("nmr_{}",s(body,"name"))});
                self.tokens.push(t.clone());
                t
            }
            ("GET", "/api/engines/cloak/acquisition") => self.offer.clone(),
            ("POST", "/api/engines/cloak/acquisition") => {
                json!({"engine":"cloak","installed":true,"version":self.offer["version"],"offer_id":body["offer_id"]})
            }
            ("GET", "/api/extensions/installed") => {
                json!({"extensions":strings(&self.config["default_extensions"]).iter().map(|p|json!({"path":p,"name":"uBlock Origin","icon":"icon.png"})).collect::<Vec<_>>()})
            }
            ("POST", "/api/extensions/install") => {
                self.config["default_extensions"] = json!(["/extensions/new"]);
                json!({"default_extensions":self.config["default_extensions"],"reused":false,"name":"New extension"})
            }
            ("POST", "/api/folders") => {
                let mut f = strings(&self.config["folders"]);
                f.push(s(body, "name").into());
                self.config["folders"] = json!(f);
                self.config.clone()
            }
            ("POST", "/api/folders/rename") => {
                let from = s(body, "from");
                let to = s(body, "to");
                self.config["folders"] = json!(
                    strings(&self.config["folders"])
                        .into_iter()
                        .map(|f| if f == from { to.into() } else { f })
                        .collect::<Vec<_>>()
                );
                for p in &mut self.profiles {
                    if s(p, "folder") == from {
                        p["folder"] = json!(to);
                    }
                }
                self.config.clone()
            }
            ("POST", "/api/folders/delete") => {
                let name = s(body, "name");
                self.config["folders"] = json!(
                    strings(&self.config["folders"])
                        .into_iter()
                        .filter(|f| f != name)
                        .collect::<Vec<_>>()
                );
                for p in &mut self.profiles {
                    if s(p, "folder") == name {
                        p["folder"] = json!("Unfiled");
                    }
                }
                self.config.clone()
            }
            ("GET", "/api/profiles/health") => {
                json!({"profiles":self.profiles.iter().map(|p|json!({"id":p["id"],"name":p["name"],"engine":p["engine"],"status":p["status"],"coherent":true,"issues":[],"trust":{"grade":"B","score":94}})).collect::<Vec<_>>(),"summary":{"total":self.profiles.len(),"coherent":self.profiles.len()}})
            }
            ("GET", "/status") | ("POST", "/check") => self.update.clone(),
            ("POST", "/download") => {
                self.update["state"] = json!("ready");
                self.update.clone()
            }
            ("POST", "/install") => {
                self.update["state"] = json!("deferred");
                self.update.clone()
            }
            ("POST", "/cancel") => {
                self.update["state"] = json!("cancelled");
                self.update.clone()
            }
            ("POST", "/api/proxies/test") => {
                json!({"tested":list(&body["ids"]).len(),"ok":list(&body["ids"]).len()})
            }
            ("POST", "/api/proxies/assign") => json!({"assigned":self.profiles.len()}),
            ("POST", "/api/proxies/rotate") => {
                json!({"rotated":1,"tried":list(&body["ids"]).len()})
            }
            ("POST", "/api/proxies/export") => {
                json!({"text":"socks5://pu:pooled@pool.example:1080","count":list(&body["ids"]).len()})
            }
            ("POST", "/api/proxies/archive") => {
                let ids = strings(&body["ids"]);
                for p in &mut self.pool {
                    if ids.contains(&s(p, "id").into()) {
                        p["archived"] = body["archived"].clone();
                    }
                }
                json!({"ok":true})
            }
            ("POST", "/api/proxies/delete") => {
                let ids = strings(&body["ids"]);
                self.pool.retain(|p| !ids.contains(&s(p, "id").into()));
                json!({"deleted":ids.len()})
            }
            ("POST", "/api/proxies") => json!({"added":1,"total":self.pool.len()+1}),
            _ if p.starts_with("/api/tokens/") && method == "DELETE" => {
                self.tokens
                    .retain(|t| s(t, "id") != p.rsplit('/').next().unwrap());
                return response(Value::Null, 204);
            }
            _ if p.starts_with("/api/profiles/") && method == "PUT" => {
                let Some(profile) = self
                    .profiles
                    .iter_mut()
                    .find(|v| s(v, "id") == p.rsplit('/').next().unwrap())
                else {
                    return response(json!({"detail":"not found"}), 404);
                };
                if body.get("_rev").is_some() && body["_rev"] != profile["_rev"] {
                    return response(
                        json!({"detail":"Profile changed since it was read","kind":"StaleProfile"}),
                        409,
                    );
                }
                let mut proxy = profile["proxy"].clone();
                for (k, v) in body.as_object().unwrap() {
                    if k == "proxy" && v.is_object() {
                        if !proxy.is_object() {
                            proxy = json!({});
                        }
                        for (k, v) in v.as_object().unwrap() {
                            proxy[k] = v.clone();
                        }
                        profile[k] = proxy.clone();
                    } else {
                        profile[k] = v.clone();
                    }
                }
                if body.get("engine").is_some() {
                    self.config["default_engine"] = profile["engine"].clone();
                }
                profile["_rev"] = json!(number(profile, "_rev") + 1);
                profile.clone()
            }
            _ if p.starts_with("/api/proxies/") && method == "PUT" => {
                let id = p.rsplit('/').next().unwrap();
                if let Some(p) = self.pool.iter_mut().find(|p| s(p, "id") == id) {
                    for (k, v) in body.as_object().unwrap() {
                        p[k] = v.clone();
                    }
                }
                json!({"ok":true})
            }
            _ if p.ends_with("/launch") || p.ends_with("/stop") => {
                let id = p.split('/').nth(3).unwrap_or("");
                if let Some(profile) = self.profiles.iter_mut().find(|p| s(p, "id") == id) {
                    profile["status"] = json!(if p.ends_with("/launch") {
                        "running"
                    } else {
                        "stopped"
                    });
                }
                json!({"ok":true})
            }
            _ if p.ends_with("/trust") => self.trust_result.clone(),
            _ if p.ends_with("/align") => self.align_result.clone(),
            _ if p.ends_with("/attach") => json!({"port":9222,"headless":body["headless"]}),
            _ if p.ends_with("/rotate") => {
                json!({"rotated":true,"previousIp":"192.0.2.1","ip":"192.0.2.2"})
            }
            _ if p.ends_with("/cookies") && method == "GET" => json!({"cookies":[]}),
            _ => json!({"ok":true}),
        };
        response(
            value,
            if ["/status", "/check", "/download", "/install", "/cancel"].contains(&p) {
                self.update_http_status
            } else {
                200
            },
        )
    }
}
struct Harness {
    engine: Rc<RefCell<Engine>>,
    original: JsValue,
    _fetch: Closure<dyn FnMut(JsValue, JsValue) -> js_sys::Promise>,
    style: Element,
    blobs: Rc<RefCell<Vec<Blob>>>,
    original_create_url: JsValue,
    _create_url: Closure<dyn FnMut(Blob) -> String>,
}
impl Drop for Harness {
    fn drop(&mut self) {
        shutdown();
        let _ = Reflect::set(
            window().as_ref(),
            &JsValue::from_str("fetch"),
            &self.original,
        );
        let _ =
            Reflect::delete_property(window().as_ref(), &JsValue::from_str("__NUMERA_UPDATES__"));
        self.style.remove();
        let url = Reflect::get(&js_sys::global(), &JsValue::from_str("URL")).unwrap();
        let _ = Reflect::set(
            &url,
            &JsValue::from_str("createObjectURL"),
            &self.original_create_url,
        );
    }
}
async fn delay() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        let callback = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 5)
            .unwrap();
    });
    JsFuture::from(promise).await.unwrap();
}
async fn until(mut condition: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if condition() {
            return;
        }
        delay().await;
    }
    panic!("condition did not complete; state {:?}", snapshot());
}
async fn mount(engine: Engine, bridge: bool) -> Harness {
    shutdown();
    if document().get_element_by_id("root").is_none() {
        let root = document().create_element("div").unwrap();
        root.set_id("root");
        document().body().unwrap().append_child(&root).unwrap();
    }
    if let Ok(Some(storage)) = window().local_storage() {
        storage.remove_item("numera.token").unwrap();
        storage.remove_item("ps-theme").unwrap();
    }
    let style = document().create_element("style").unwrap();
    style.set_text_content(Some(include_str!("../assets/app.css")));
    document().body().unwrap().append_child(&style).unwrap();
    if bridge {
        Reflect::set(
            window().as_ref(),
            &JsValue::from_str("__NUMERA_UPDATES__"),
            &js_sys::JSON::parse(
                &json!({"endpoint":"http://127.0.0.1:19876","token":"a".repeat(64)}).to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    } else {
        let _ =
            Reflect::delete_property(window().as_ref(), &JsValue::from_str("__NUMERA_UPDATES__"));
    }
    let engine = Rc::new(RefCell::new(engine));
    let state = engine.clone();
    let original = Reflect::get(window().as_ref(), &JsValue::from_str("fetch")).unwrap();
    let url = Reflect::get(&js_sys::global(), &JsValue::from_str("URL")).unwrap();
    let original_create_url = Reflect::get(&url, &JsValue::from_str("createObjectURL")).unwrap();
    let create = original_create_url.clone().dyn_into::<Function>().unwrap();
    let blobs = Rc::new(RefCell::new(vec![]));
    let captured = blobs.clone();
    let create_url = Closure::wrap(Box::new(move |blob: Blob| {
        captured.borrow_mut().push(blob.clone());
        create
            .call1(&JsValue::UNDEFINED, blob.as_ref())
            .unwrap()
            .as_string()
            .unwrap()
    }) as Box<dyn FnMut(Blob) -> String>);
    Reflect::set(
        &url,
        &JsValue::from_str("createObjectURL"),
        create_url.as_ref(),
    )
    .unwrap();
    let real_fetch = original.clone();
    let fetch = Closure::wrap(Box::new(move |url: JsValue, init: JsValue| {
        let url = url.as_string().unwrap();
        if url.starts_with("blob:") {
            return real_fetch
                .dyn_ref::<Function>()
                .unwrap()
                .call1(window().as_ref(), &JsValue::from_str(&url))
                .unwrap()
                .dyn_into::<js_sys::Promise>()
                .unwrap();
        }
        let path = if url.starts_with("http") {
            url::Url::parse(&url).unwrap().path().to_owned()
        } else {
            url.split('?').next().unwrap().to_owned()
        };
        let method = Reflect::get(&init, &JsValue::from_str("method"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "GET".into());
        let body = Reflect::get(&init, &JsValue::from_str("body"))
            .ok()
            .and_then(|v| v.as_string())
            .and_then(|v| serde_json::from_str(&v).ok())
            .unwrap_or(Value::Null);
        let headers = Reflect::get(&init, &JsValue::from_str("headers"))
            .unwrap()
            .dyn_into::<web_sys::Headers>()
            .unwrap();
        let bearer = headers
            .get("Authorization")
            .unwrap()
            .unwrap_or_default()
            .trim_start_matches("Bearer ")
            .to_owned();
        let redirect = Reflect::get(&init, &JsValue::from_str("redirect"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let call = Recorded {
            method,
            path: path.clone(),
            body,
            bearer,
            redirect,
        };
        let mut state = state.borrow_mut();
        state.calls.push(call.clone());
        if state.offline {
            return js_sys::Promise::reject(&JsValue::from_str("offline"));
        }
        if state
            .errors
            .get(&path)
            .is_some_and(|(status, _, _)| *status == 0)
        {
            return js_sys::Promise::reject(&JsValue::from_str("Network unavailable"));
        }
        if state.hold == path {
            let signal = Reflect::get(&init, &JsValue::from_str("signal"))
                .unwrap()
                .dyn_into::<web_sys::AbortSignal>()
                .unwrap();
            state.signals.push(signal.clone());
            return js_sys::Promise::new(&mut |resolve, reject| {
                state.pending.push(resolve);
                let cb = Closure::once(move |_: Event| {
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("aborted"));
                });
                signal
                    .add_event_listener_with_callback("abort", cb.as_ref().unchecked_ref())
                    .unwrap();
                cb.forget();
            });
        }
        if path == "/api/extensions/icon" {
            return js_sys::Promise::resolve(&if state.icons_missing {
                response(json!({"detail":"no icon"}), 404)
            } else {
                let bytes = Uint8Array::from(&include_bytes!("../public/logo-512.png")[..]);
                let parts = Array::new();
                parts.push(&bytes);
                let blob = Blob::new_with_u8_array_sequence(&parts).unwrap();
                Response::new_with_opt_blob(Some(&blob)).unwrap().into()
            });
        }
        js_sys::Promise::resolve(&state.answer(&call))
    })
        as Box<dyn FnMut(JsValue, JsValue) -> js_sys::Promise>);
    Reflect::set(
        window().as_ref(),
        &JsValue::from_str("fetch"),
        fetch.as_ref(),
    )
    .unwrap();
    start().unwrap();
    until(|| snapshot().mode != "connecting").await;
    Harness {
        engine,
        original,
        _fetch: fetch,
        style,
        blobs,
        original_create_url,
        _create_url: create_url,
    }
}
fn node(selector: &str) -> Element {
    document()
        .query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| {
            panic!(
                "missing {selector}; {}",
                document().get_element_by_id("root").unwrap().inner_html()
            )
        })
}
fn exists(selector: &str) -> bool {
    document().query_selector(selector).unwrap().is_some()
}
fn visible() -> String {
    document()
        .get_element_by_id("root")
        .unwrap()
        .text_content()
        .unwrap()
}
async fn click(selector: &str) {
    node(selector).dyn_into::<HtmlElement>().unwrap().click();
    for _ in 0..3 {
        delay().await;
    }
}
fn input(selector: &str, value: &str) {
    let target = node(selector);
    let select = target.dyn_ref::<HtmlSelectElement>();
    if let Some(input) = target.dyn_ref::<HtmlInputElement>() {
        input.set_value(value)
    } else if let Some(input) = target.dyn_ref::<HtmlTextAreaElement>() {
        input.set_value(value)
    } else if let Some(input) = select {
        input.set_value(value)
    }
    let event = web_sys::EventInit::new();
    event.set_bubbles(true);
    target
        .dispatch_event(
            &Event::new_with_event_init_dict(
                if select.is_some() { "change" } else { "input" },
                &event,
            )
            .unwrap(),
        )
        .unwrap();
}
async fn open() {
    click("[data-action=edit][data-id=alpha]").await;
    until(|| exists(".ps-editor")).await;
}
fn writes(h: &Harness) -> Vec<Recorded> {
    h.engine
        .borrow()
        .calls
        .iter()
        .filter(|c| c.method == "PUT" && c.path.starts_with("/api/profiles/"))
        .cloned()
        .collect()
}

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

#[path = "browser_cases.rs"]
mod cases;
