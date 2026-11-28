#![allow(clippy::field_reassign_with_default, clippy::unwrap_used)]
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
    update_approval: String,
    redirect: String,
}
struct Engine {
    profiles: Vec<Value>,
    config: Value,
    pool: Vec<Value>,
    schedules: Vec<Value>,
    calls: Vec<Recorded>,
    errors: BTreeMap<String, (u16, String, String)>,
    tokens: Vec<Value>,
    offer: Value,
    update: Value,
    preferences: Value,
    release_offer_id: String,
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
            schedules: vec![],
            calls: vec![],
            errors: BTreeMap::new(),
            tokens: vec![],
            offer: offer(),
            update: update(),
            preferences: json!({"app_auto_update":false,"browser_core_auto_update":false,"clean_old_versions_on_update":true}),
            release_offer_id: "1".repeat(64),
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
            ("GET", "/api/updates/preferences") => self.preferences.clone(),
            ("PUT", "/api/updates/preferences") => {
                if (body["app_auto_update"] == true || body["browser_core_auto_update"] == true)
                    && call.update_approval != "a".repeat(64)
                {
                    return response(json!({"detail":"Desktop approval is required"}), 403);
                }
                self.preferences = body.clone();
                self.preferences.clone()
            }
            ("GET", "/api/updates/check?channel=app") => {
                json!({"channel":"app","offerId":self.release_offer_id,"requiresCombined":false,"app":{"current":"1.0.0","latest":"1.1.0","update_available":true,"verified":true}})
            }
            ("GET", "/api/updates/check?channel=browsers") => {
                json!({"channel":"browsers","offerId":self.release_offer_id,"browsers":[{"id":"chromium","current":"158","latest":"159","update_available":true},{"id":"camoufox","current":"1","latest":"1","update_available":false}]})
            }
            ("GET", "/api/updates/check?channel=all") => {
                json!({"channel":"all","offerId":self.release_offer_id,"requiresCombined":true,"app":{"current":"1.0.0","latest":"1.1.0","update_available":true,"verified":true},"browsers":[{"id":"chromium","current":"158","latest":"159","update_available":true}]})
            }
            ("POST", "/api/updates/apply") => {
                if body["offerId"] != self.release_offer_id {
                    return response(
                        json!({"detail":"The reviewed update offer changed. Check again."}),
                        409,
                    );
                }
                json!({"ok":call.update_approval == "a".repeat(64),"channel":body["channel"],"data_preserved":true,"message":"Applied fixture update."})
            }
            ("POST", "/api/mcp") => json!({"jsonrpc":"2.0","id":body["id"],"result":{}}),
            ("GET", "/api/profiles/alpha/ip-check") => {
                json!({"profile_id":"alpha","ip":"192.0.2.1","ready":true,"country_code":"US","country_name":"United States","country_flag":"\u{1F1FA}\u{1F1F8}"})
            }
            ("POST", "/api/fleet/sync") => {
                let ids = strings(&body["profile_ids"]);
                for p in &mut self.profiles {
                    if ids.iter().any(|id| p["id"] == *id) {
                        p["status"] = json!("stopped");
                    }
                }
                json!({"action":body["action"], "results":ids.iter().map(|id| json!({"profile_id":id,"ok":true})).collect::<Vec<_>>()})
            }
            ("GET", p) if p.starts_with("/api/profiles/alpha/cookies/export?format=") => {
                let (_, format) = p.split_once('=').unwrap();
                json!({"profile_id":"alpha","format":format,"count":1,"content":if format=="json" {"{\"cookies\":[]}"} else {"# Netscape HTTP Cookie File\n"}})
            }
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
            ("GET", "/api/schedules") => json!(self.schedules),
            ("POST", "/api/schedules") => {
                let mut schedule = body.clone();
                schedule["id"] = json!(format!("schedule-{}", self.schedules.len() + 1));
                self.schedules.push(schedule.clone());
                schedule
            }
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
            _ if p.starts_with("/api/schedules/") && method == "PUT" => {
                let id = p.rsplit('/').next().unwrap();
                let Some(schedule) = self.schedules.iter_mut().find(|s| s["id"] == id) else {
                    return response(json!({"detail":"schedule not found"}), 404);
                };
                for (key, value) in body.as_object().unwrap() {
                    schedule[key] = value.clone();
                }
                schedule.clone()
            }
            _ if p.starts_with("/api/schedules/") && method == "DELETE" => {
                self.schedules
                    .retain(|s| s["id"] != p.rsplit('/').next().unwrap());
                json!({"ok":true})
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
                    if ["host", "port", "user", "pass", "type"]
                        .iter()
                        .any(|field| body.get(field).is_some())
                    {
                        p["status"] = json!("untested");
                        p["lastCheck"] = Value::Null;
                        p["checkedAt"] = Value::Null;
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
            _ if p.ends_with("/cookies") && method == "POST" => json!({"imported":1}),
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
        let path = if url.starts_with("/api/updates/check?") || url.contains("/cookies/export?") {
            url.clone()
        } else if url.starts_with("http") {
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
            update_approval: headers
                .get("X-Numera-Update-Approval")
                .unwrap()
                .unwrap_or_default(),
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

include!("browser_smoke_tests.rs");
#[path = "browser_api_contract_tests.rs"]
mod api_contract_tests;
#[path = "browser_async_scope_tests.rs"]
mod async_scope_tests;
#[path = "browser_cases.rs"]
mod cases;
#[path = "browser_release_tests.rs"]
mod release_tests;
