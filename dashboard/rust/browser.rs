//! The browser is a thin adapter: DOM events enter the Rust state machine;
//! authenticated requests and owned resources leave it through this module.
use crate::{
    contracts::{self, ApiError},
    model::*,
    render, updates,
};
use js_sys::{Array, Function, Reflect, Uint8Array};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    AbortController, Blob, Document, Element, Event, HtmlAnchorElement, HtmlElement,
    HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement, KeyboardEvent, RequestInit, Response,
};

type Listener = (web_sys::EventTarget, String, Closure<dyn FnMut(Event)>);
struct Runtime {
    state: App,
    listeners: Vec<Listener>,
    timers: Vec<i32>,
    controllers: Vec<AbortController>,
    update_sequence: u64,
    update_controller: Option<AbortController>,
    timer_callbacks: Vec<Closure<dyn FnMut()>>,
    startup_error: Option<Closure<dyn FnMut(JsValue)>>,
    last_poll: f64,
    update_next: f64,
    disposed: bool,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        for (t, event, c) in &self.listeners {
            let _ = t.remove_event_listener_with_callback(event, c.as_ref().unchecked_ref());
            if let Some(input) = t.dyn_ref::<HtmlInputElement>()
                && input.id() == "numera-file-picker"
            {
                input.remove();
            }
        }
        for id in &self.timers {
            window().clear_timeout_with_handle(*id);
            window().clear_interval_with_handle(*id);
        }
        for c in &self.controllers {
            c.abort();
        }
        for url in self.state.icons.values() {
            let _ = web_sys::Url::revoke_object_url(url);
        }
        if let Some(callback) = &self.startup_error
            && Reflect::get(window().as_ref(), &JsValue::from_str("numeraError"))
                .ok()
                .as_ref()
                == Some(callback.as_ref())
        {
            let _ = Reflect::delete_property(window().as_ref(), &JsValue::from_str("numeraError"));
        }
    }
}
thread_local! {static RUNTIME:RefCell<Option<Rc<RefCell<Runtime>>>>=const{RefCell::new(None)};static TOKEN:RefCell<String>=const{RefCell::new(String::new())};}
fn window() -> web_sys::Window {
    web_sys::window().expect("browser window")
}
fn document() -> Document {
    window().document().expect("browser document")
}
fn runtime() -> Rc<RefCell<Runtime>> {
    RUNTIME.with(|r| r.borrow().as_ref().expect("mounted dashboard").clone())
}
pub fn snapshot() -> App {
    RUNTIME
        .with(|r| r.borrow().as_ref().map(|r| r.borrow().state.clone()))
        .unwrap_or_else(|| App {
            mode: "disposed".into(),
            profiles: vec![],
            ..App::default()
        })
}
fn change(f: impl FnOnce(&mut App)) {
    let r = RUNTIME.with(|r| r.borrow().clone());
    if let Some(r) = r
        && !r.borrow().disposed
    {
        f(&mut r.borrow_mut().state);
    }
}
fn alive() -> bool {
    RUNTIME.with(|r| r.borrow().as_ref().is_some_and(|r| !r.borrow().disposed))
}
fn auth() -> String {
    TOKEN.with(|t| t.borrow().clone())
}
fn set_auth(token: &str) {
    let token = token.trim().to_owned();
    TOKEN.with(|t| *t.borrow_mut() = token.clone());
    if let Ok(Some(storage)) = window().local_storage() {
        if token.is_empty() {
            let _ = storage.remove_item("numera.token");
        } else {
            let _ = storage.set_item("numera.token", &token);
        }
    }
}
fn network(e: JsValue) -> ApiError {
    ApiError {
        message: e.as_string().unwrap_or_else(|| format!("{e:?}")),
        status: 0,
        kind: String::new(),
    }
}
fn fail(s: impl Into<String>) -> ApiError {
    ApiError {
        message: s.into(),
        status: 0,
        kind: String::new(),
    }
}
fn call(target: &JsValue, name: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f = Reflect::get(target, &JsValue::from_str(name))?.dyn_into::<Function>()?;
    f.apply(target, &args.iter().cloned().collect::<Array>())
}
fn token_headers(token: &str) -> Result<web_sys::Headers, ApiError> {
    let h = web_sys::Headers::new().map_err(network)?;
    if !token.is_empty() {
        h.set("Authorization", &format!("Bearer {token}"))
            .map_err(network)?;
    }
    Ok(h)
}
struct Deadline {
    controller: AbortController,
    id: i32,
    _callback: Closure<dyn FnMut()>,
}
impl Deadline {
    fn new(ms: i32) -> Result<Self, ApiError> {
        if !alive() {
            return Err(fail("The dashboard was closed."));
        }
        let controller = AbortController::new().map_err(network)?;
        let c = controller.clone();
        let callback = Closure::wrap(Box::new(move || c.abort()) as Box<dyn FnMut()>);
        let id = window()
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                ms,
            )
            .map_err(network)?;
        runtime().borrow_mut().controllers.push(controller.clone());
        Ok(Self {
            controller,
            id,
            _callback: callback,
        })
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        window().clear_timeout_with_handle(self.id);
        if alive() {
            runtime()
                .borrow_mut()
                .controllers
                .retain(|c| c != &self.controller);
        }
    }
}
async fn send(
    method: &str,
    path: &str,
    body: Option<&Value>,
    deadline: &Deadline,
    token: &str,
    update: bool,
) -> Result<Response, ApiError> {
    let init = RequestInit::new();
    init.set_method(method);
    let headers = token_headers(token)?;
    if body.is_some() {
        headers
            .set("Content-Type", "application/json")
            .map_err(network)?;
    }
    init.set_headers(&headers);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    init.set_signal(Some(&deadline.controller.signal()));
    if update {
        init.set_redirect(web_sys::RequestRedirect::Error);
        init.set_credentials(web_sys::RequestCredentials::Omit);
        init.set_cache(web_sys::RequestCache::NoStore);
    }
    let response = JsFuture::from(window().fetch_with_str_and_init(path, &init))
        .await
        .map_err(network)?
        .dyn_into::<Response>()
        .map_err(network)?;
    Ok(response)
}
async fn parse(response: &Response) -> Result<Value, ApiError> {
    let text = JsFuture::from(response.text().map_err(network)?)
        .await
        .map_err(network)?
        .as_string()
        .unwrap_or_default();
    if response.status() == 204 {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| fail(format!("Invalid engine response: {e}")))
}
async fn request(method: &str, path: &str, body: Option<Value>) -> Result<Value, ApiError> {
    let deadline = Deadline::new(
        if (path == "/api/engines/cloak/acquisition" && method == "POST")
            || path.ends_with("/warmup")
            || path.ends_with("/align")
            || path.ends_with("/trust")
            || path.ends_with("/cookies")
        {
            310000
        } else if path == "/api/engines/cloak/acquisition" {
            10000
        } else {
            30000
        },
    )?;
    let response = send(
        method,
        &contracts::api_url(path),
        body.as_ref(),
        &deadline,
        &auth(),
        false,
    )
    .await.map_err(|error|if path=="/api/engines/cloak/acquisition"&&deadline.controller.signal().aborted(){fail("The vendor operation exceeded its UI deadline. Review local state before retrying; cancellation is not proof the vendor helper stopped.")}else{error})?;
    if !response.ok() {
        let body = parse(&response).await.unwrap_or(Value::Null);
        let error = contracts::error(method, path, response.status(), &body);
        if response.status() == 401 && alive() {
            change(|a| {
                a.mode = "locked".into();
                a.auth_error = error.message.clone();
                a.form = json!({"token":auth()});
            });
            draw();
        }
        return Err(error);
    }
    let v = parse(&response).await?;
    if let Some(kind) = contracts::response_kind(method, path) {
        contracts::validate(v, kind).map_err(fail)
    } else {
        Ok(v)
    }
}
async fn refresh() -> Result<(), ApiError> {
    let profiles = request("GET", "/api/profiles", None).await?;
    if alive() {
        change(|a| a.profiles = list(&profiles));
    }
    Ok(())
}
async fn refresh_config() -> Result<(), ApiError> {
    let cfg = request("GET", "/api/config", None).await?;
    if alive() {
        change(|a| a.set_config(cfg));
    }
    Ok(())
}
async fn health() -> &'static str {
    let Ok(d) = Deadline::new(10000) else {
        return "off";
    };
    match send(
        "GET",
        &contracts::api_url("/api/health"),
        None,
        &d,
        &auth(),
        false,
    )
    .await
    {
        Ok(r) if r.ok() => "ok",
        Ok(r) if r.status() == 401 => "locked",
        _ => "off",
    }
}

#[cfg_attr(not(test), wasm_bindgen(start))]
pub fn start() -> Result<(), JsValue> {
    shutdown();
    let mut state = App::default();
    if let Ok(Some(storage)) = window().local_storage() {
        TOKEN.with(|t| {
            *t.borrow_mut() = storage
                .get_item("numera.token")
                .ok()
                .flatten()
                .unwrap_or_default()
        });
        state.theme = storage
            .get_item("ps-theme")
            .ok()
            .flatten()
            .unwrap_or_else(|| "dark".into());
    }
    let bridge = Reflect::get(window().as_ref(), &JsValue::from_str("__NUMERA_UPDATES__"))
        .unwrap_or(JsValue::UNDEFINED);
    state.update_bridge = js_value(&bridge).and_then(|v| updates::bridge(&v));
    let r = Rc::new(RefCell::new(Runtime {
        state,
        listeners: vec![],
        timers: vec![],
        controllers: vec![],
        update_sequence: 0,
        update_controller: None,
        timer_callbacks: vec![],
        startup_error: None,
        last_poll: js_sys::Date::now(),
        update_next: f64::INFINITY,
        disposed: false,
    }));
    RUNTIME.with(|global| *global.borrow_mut() = Some(r));
    let startup_error = Closure::wrap(Box::new(move |message: JsValue| {
        let message = message
            .as_string()
            .unwrap_or_default()
            .chars()
            .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
            .take(16384)
            .collect::<String>();
        if alive() {
            change(|a| {
                a.mode = "startup-error".into();
                a.auth_error = message;
            });
            draw();
        }
    }) as Box<dyn FnMut(JsValue)>);
    Reflect::set(
        window().as_ref(),
        &JsValue::from_str("numeraError"),
        startup_error.as_ref(),
    )?;
    runtime().borrow_mut().startup_error = Some(startup_error);
    let root = document()
        .get_element_by_id("root")
        .ok_or_else(|| JsValue::from_str("Missing root"))?;
    listen(root.clone().into(), "click", move |ev| {
        let Some(target) = ev
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest("[data-action]").ok().flatten())
        else {
            return;
        };
        if target.has_attribute("disabled") {
            return;
        }
        ev.prevent_default();
        let act = target.get_attribute("data-action").unwrap_or_default();
        let id = target.get_attribute("data-id").unwrap_or_default();
        spawn_local(async move {
            if let Err(err) = handle(&act, &id).await
                && alive()
            {
                change(|a| a.say(err.message, true));
            }
            if alive() {
                draw();
            }
        });
    });
    for event in ["input", "change"] {
        listen(root.clone().into(), event, move |ev| {
            let Some(target) = ev.target().and_then(|t| t.dyn_into::<Element>().ok()) else {
                return;
            };
            let Some(scope) = target.get_attribute("data-scope") else {
                return;
            };
            let key = target.get_attribute("data-field").unwrap_or_default();
            let value = value_of(&target);
            let is_select = target.dyn_ref::<HtmlSelectElement>().is_some();
            if ev.type_() == "change" && !is_select {
                return;
            }
            if let Err(err) = edit(&scope, &key, &value) {
                change(|a| a.say(err, true));
            }
            draw();
        });
    }
    listen(root.into(), "focusout", move |ev| {
        let target = ev.target().and_then(|t| t.dyn_into::<Element>().ok());
        if target.is_some_and(|t| t.id() == "rename-profile") {
            spawn_local(async {
                let _ = handle("commit-rename", "").await;
                draw();
            });
        }
    });
    listen(document().into(), "keydown", move |ev| {
        let Ok(key) = ev.dyn_into::<KeyboardEvent>() else {
            return;
        };
        if (key.ctrl_key() || key.meta_key()) && key.key().eq_ignore_ascii_case("k") {
            key.prevent_default();
            change(|a| {
                a.modal = if a.modal == "commands" {
                    String::new()
                } else {
                    "commands".into()
                };
                a.command.clear();
                a.command_index = 0;
            });
            draw();
        } else if key.key() == "Escape" {
            change(|a| {
                if !a.update_approval.is_null() {
                    a.update_approval = Value::Null;
                } else if a.modal == "commands" || !a.busy {
                    a.modal.clear();
                }
                if !a.rename.is_empty() {
                    a.rename.clear();
                }
            });
            draw();
        } else if snapshot().modal == "commands"
            && ["ArrowDown", "ArrowUp", "Enter"].contains(&key.key().as_str())
        {
            key.prevent_default();
            if key.key() == "Enter" {
                let i = snapshot().command_index;
                spawn_local(async move {
                    let _ = handle("command", &i.to_string()).await;
                    draw();
                });
            } else {
                change(|a| {
                    a.command_index = if key.key() == "ArrowDown" {
                        (a.command_index + 1).min(a.commands().len().saturating_sub(1))
                    } else {
                        a.command_index.saturating_sub(1)
                    }
                });
                draw();
            }
        } else if key.key() == "Enter" {
            let id = document()
                .active_element()
                .map(|e| e.id())
                .unwrap_or_default();
            let action = match id.as_str() {
                "rename-profile" => "commit-rename",
                "form-newFolder" => "create-folder",
                "form-extension" => "add-extension",
                "form-tokenName" => "create-token",
                "form-token" => "unlock",
                _ => "",
            };
            if !action.is_empty() {
                key.prevent_default();
                spawn_local(async move {
                    let _ = handle(action, "").await;
                    draw();
                });
            }
        } else if key.key() == "Tab" && !snapshot().update_approval.is_null() {
            trap_focus(&key);
        }
    });
    listen(window().into(), "numera-updates-ready", move |_| {
        if snapshot().update_bridge.is_none()
            && let Ok(v) = Reflect::get(window().as_ref(), &JsValue::from_str("__NUMERA_UPDATES__"))
        {
            change(|a| a.update_bridge = js_value(&v).and_then(|v| updates::bridge(&v)));
            spawn_local(async {
                let _ = run_update("status", "").await;
            });
        }
    });
    draw();
    spawn_local(async {
        connect().await;
    });
    let callback = Closure::wrap(Box::new(move || {
        if !alive() {
            return;
        }
        let (can_poll, poll_every, has_updates, expired) = {
            let r = runtime();
            let r = r.borrow();
            let a = &r.state;
            (
                a.can_poll(),
                a.poll_every(),
                a.update_bridge.is_some(),
                a.toasts.iter().any(|t| t.expires <= js_sys::Date::now()),
            )
        };
        runtime()
            .borrow_mut()
            .listeners
            .retain(|(target, event, callback)| {
                let detached = target
                    .dyn_ref::<web_sys::Node>()
                    .is_some_and(|n| !n.is_connected());
                if detached {
                    let _ = target.remove_event_listener_with_callback(
                        event,
                        callback.as_ref().unchecked_ref(),
                    );
                }
                !detached
            });
        if expired {
            change(|a| a.toasts.retain(|t| t.expires > js_sys::Date::now()));
            draw();
        }
        let elapsed = js_sys::Date::now() - runtime().borrow().last_poll;
        if !can_poll {
            runtime().borrow_mut().last_poll = js_sys::Date::now();
        }
        if can_poll && elapsed >= f64::from(poll_every) {
            runtime().borrow_mut().last_poll = js_sys::Date::now();
            spawn_local(async {
                let _ = refresh().await;
                if alive() {
                    draw();
                }
            });
        }
        if has_updates && js_sys::Date::now() >= runtime().borrow().update_next {
            runtime().borrow_mut().update_next = f64::INFINITY;
            spawn_local(async {
                let _ = run_update("status", "").await;
            });
        }
    }) as Box<dyn FnMut()>);
    let id = window().set_interval_with_callback_and_timeout_and_arguments_0(
        callback.as_ref().unchecked_ref(),
        1000,
    )?;
    runtime().borrow_mut().timers.push(id);
    runtime().borrow_mut().timer_callbacks.push(callback);
    if snapshot().update_bridge.is_some() {
        spawn_local(async {
            let _ = run_update("status", "").await;
        });
    }
    Ok(())
}
fn js_value(v: &JsValue) -> Option<Value> {
    js_sys::JSON::stringify(v)
        .ok()
        .and_then(|s| s.as_string())
        .and_then(|s| serde_json::from_str(&s).ok())
}
fn listen(target: web_sys::EventTarget, event: &str, callback: impl FnMut(Event) + 'static) {
    let callback = Closure::wrap(Box::new(callback) as Box<dyn FnMut(Event)>);
    let _ = target.add_event_listener_with_callback(event, callback.as_ref().unchecked_ref());
    runtime()
        .borrow_mut()
        .listeners
        .push((target, event.into(), callback));
}
#[wasm_bindgen]
pub fn shutdown() {
    RUNTIME.with(|r| {
        if let Some(runtime) = r.borrow_mut().take() {
            runtime.borrow_mut().disposed = true;
            runtime
                .borrow_mut()
                .controllers
                .iter()
                .for_each(AbortController::abort);
            drop(runtime);
        }
    });
}
pub fn draw() {
    if !alive() {
        return;
    }
    let doc = document();
    let Some(root) = doc.get_element_by_id("root") else {
        return;
    };
    let active = doc.active_element();
    let focus = active.as_ref().map(Element::id).unwrap_or_default();
    let selection = active
        .as_ref()
        .and_then(|el| el.dyn_ref::<HtmlInputElement>())
        .map(|el| {
            (
                el.selection_start().ok().flatten(),
                el.selection_end().ok().flatten(),
            )
        })
        .or_else(|| {
            active
                .as_ref()
                .and_then(|el| el.dyn_ref::<HtmlTextAreaElement>())
                .map(|el| {
                    (
                        el.selection_start().ok().flatten(),
                        el.selection_end().ok().flatten(),
                    )
                })
        });
    let (markup, theme, commands, approval) = {
        let r = runtime();
        let r = r.borrow();
        let a = &r.state;
        (
            render::app(a),
            a.theme.clone(),
            a.modal == "commands",
            !a.update_approval.is_null(),
        )
    };
    if let Some(html) = doc.document_element() {
        let _ = html.set_attribute("data-theme", &theme);
    }
    root.set_inner_html(&markup);
    if let Ok(images) = root.query_selector_all("img.ps-ext-logo") {
        for i in 0..images.length() {
            if let Some(img) = images.item(i).and_then(|n| n.dyn_into::<Element>().ok()) {
                let url = img.get_attribute("src").unwrap_or_default();
                listen(img.into(), "error", move |_| {
                    change(|a| a.icons.retain(|_, value| value != &url));
                    let _ = web_sys::Url::revoke_object_url(&url);
                    draw();
                });
            }
        }
    }
    let target = if !focus.is_empty() {
        doc.get_element_by_id(&focus)
    } else {
        None
    }
    .or_else(|| {
        if commands {
            doc.get_element_by_id("command-query")
        } else if approval {
            doc.query_selector(".ps-update-dialog button")
                .ok()
                .flatten()
        } else {
            None
        }
    });
    if let Some(el) = target {
        if let Some(h) = el.dyn_ref::<HtmlElement>() {
            let _ = h.focus();
        }
        if let Some((Some(start), Some(end))) = selection {
            if let Some(i) = el.dyn_ref::<HtmlInputElement>() {
                let _ = i.set_selection_range(start, end);
            }
            if let Some(i) = el.dyn_ref::<HtmlTextAreaElement>() {
                let _ = i.set_selection_range(start, end);
            }
        }
    }
}
fn trap_focus(key: &KeyboardEvent) {
    let Ok(Some(dialog)) = document().query_selector(".ps-update-dialog") else {
        return;
    };
    let Ok(nodes) = dialog.query_selector_all("button:not(:disabled)") else {
        return;
    };
    let Some(first) = nodes.item(0) else { return };
    let Some(last) = nodes.item(nodes.length().saturating_sub(1)) else {
        return;
    };
    let active = document().active_element();
    let target = if key.shift_key()
        && active
            .as_ref()
            .is_some_and(|e| e.is_same_node(Some(&first)))
    {
        Some(last)
    } else if !key.shift_key() && active.as_ref().is_some_and(|e| e.is_same_node(Some(&last))) {
        Some(first)
    } else {
        None
    };
    if let Some(target) = target.and_then(|n| n.dyn_into::<HtmlElement>().ok()) {
        key.prevent_default();
        let _ = target.focus();
    }
}
async fn connect() {
    let state = health().await;
    if !alive() || snapshot().mode == "startup-error" {
        return;
    }
    match state {
        "locked" => change(|a| {
            a.mode = "locked".into();
            a.form = json!({"token":auth()});
        }),
        "off" => change(|a| a.mode = "demo".into()),
        _ => match refresh().await {
            Ok(()) => {
                if let Ok(e) = request("GET", "/api/engines", None).await {
                    change(|a| a.engines = e);
                }
                let _ = refresh_config().await;
                change(|a| {
                    if a.mode != "locked" {
                        a.mode = "live".into();
                    }
                });
            }
            Err(err) if err.status == 401 => {}
            Err(_) => change(|a| a.mode = "demo".into()),
        },
    }
    draw();
}
fn value_of(el: &Element) -> String {
    if let Some(input) = el.dyn_ref::<HtmlInputElement>() {
        input.value()
    } else if let Some(input) = el.dyn_ref::<HtmlSelectElement>() {
        input.value()
    } else if let Some(input) = el.dyn_ref::<HtmlTextAreaElement>() {
        input.value()
    } else {
        String::new()
    }
}
pub fn edit(scope: &str, key: &str, value: &str) -> Result<(), String> {
    change(|a| match scope {
        "app" => match key {
            "query" => {
                a.query = value.into();
                a.page = 0
            }
            "command" => {
                a.command = value.into();
                a.command_index = 0
            }
            _ => {}
        },
        "editor" => {
            if let Some(ed) = a.editor.as_mut() {
                if !ed.editable() {
                    return;
                }
                match key {
                    "os" => change_os(&mut ed.draft, value),
                    "locale" => change_locale(&mut ed.draft, value),
                    "tags" => ed.draft["tags"] = json!(split_tags(value)),
                    "webglRenderer" => {
                        ed.draft[key] = json!(value);
                        ed.draft["webglVendor"] = json!(vendor(value));
                    }
                    _ => {
                        let v = if [
                            "cores",
                            "memory",
                            "mediaVideo",
                            "mediaAudio",
                            "mediaSpeakers",
                        ]
                        .contains(&key)
                        {
                            value
                                .parse::<i64>()
                                .map(|n| json!(n))
                                .unwrap_or_else(|_| json!(value.parse::<f64>().unwrap_or(0.)))
                        } else if key == "dns" && value == "custom" {
                            json!("https://")
                        } else {
                            json!(value)
                        };
                        ed.set(&key.split('.').map(str::to_owned).collect::<Vec<_>>(), v);
                    }
                }
            }
        }
        "form" => {
            a.form[key] = json!(value);
            if key == "poolPick"
                && let Some(p) = a.pool.iter().find(|p| s(p, "id") == value)
            {
                let proxy = json!({"type":p["type"],"host":p["host"],"port":text(&p["port"]),"user":p["user"].as_str().unwrap_or(""),"pass":p["pass"].as_str().unwrap_or(""),"country":p["country"].as_str().unwrap_or("")});
                if let Some(ed) = a.editor.as_mut()
                    && ed.editable()
                {
                    ed.draft["proxy"] = proxy;
                    ed.draft["_passwordSet"] = json!(true);
                }
            }
        }
        "batch" => match key {
            "count" => a.batch.count = value.parse::<usize>().unwrap_or(1).clamp(1, 10000),
            "start" => a.batch.start = value.parse::<usize>().unwrap_or(0),
            "prefix" => a.batch.prefix = value.into(),
            "tags" => a.batch.tags = value.into(),
            "engine" => a.batch.engine = value.into(),
            _ => {}
        },
        "schedule" => {
            let id = s(&a.schedule, "id").to_owned();
            let key = key.to_owned();
            let value = if key == "everyHours" {
                json!(value.parse::<u64>().unwrap_or(1).max(1))
            } else {
                json!(value)
            };
            spawn_local(async move {
                let _ = request(
                    "PUT",
                    &format!("/api/schedules/{id}"),
                    Some(json!({key:value})),
                )
                .await
                .map(|s| change(|a| a.schedule = s));
                draw();
            });
        }
        _ => {}
    });
    Ok(())
}
async fn load_view() -> Result<(), ApiError> {
    let a = snapshot();
    if !a.live() {
        return Ok(());
    }
    match a.view.as_str() {
        "proxies" => {
            let p = request("GET", "/api/proxies", None).await?;
            change(|a| {
                a.pool = list(&p);
                a.pool_selected
                    .retain(|id| a.pool.iter().any(|p| s(p, "id") == id));
            });
        }
        "health" => {
            let h = request("GET", "/api/profiles/health", None).await?;
            change(|a| a.health = h);
        }
        "settings" => {
            let t = request("GET", "/api/tokens", None).await?;
            change(|a| a.tokens = t);
        }
        "extensions" => load_extensions().await?,
        _ => {}
    }
    Ok(())
}
async fn load_extensions() -> Result<(), ApiError> {
    let v = request("GET", "/api/extensions/installed", None).await?;
    let extensions = list(&v["extensions"]);
    change(|a| a.extensions = extensions.clone());
    for meta in extensions {
        let path = s(&meta, "path");
        if s(&meta, "icon").is_empty() || snapshot().icons.contains_key(path) {
            continue;
        }
        let deadline = Deadline::new(10000)?;
        let url = format!(
            "/api/extensions/icon?path={}",
            js_sys::encode_uri_component(path)
        );
        let response = send(
            "GET",
            &contracts::api_url(&url),
            None,
            &deadline,
            &auth(),
            false,
        )
        .await?;
        if response.status() == 401 {
            let err = contracts::error(
                "GET",
                &url,
                401,
                &parse(&response).await.unwrap_or(Value::Null),
            );
            change(|a| {
                a.mode = "locked".into();
                a.auth_error = err.message;
            });
            break;
        }
        if !response.ok() {
            continue;
        }
        let blob = JsFuture::from(response.blob().map_err(network)?)
            .await
            .map_err(network)?
            .dyn_into::<Blob>()
            .map_err(network)?;
        let made = web_sys::Url::create_object_url_with_blob(&blob).map_err(network)?;
        if snapshot().view != "extensions" {
            let _ = web_sys::Url::revoke_object_url(&made);
        } else {
            change(|a| {
                a.icons.insert(path.into(), made);
            });
        }
    }
    Ok(())
}
fn release_icons() {
    change(|a| {
        for url in a.icons.values() {
            let _ = web_sys::Url::revoke_object_url(url);
        }
        a.icons.clear();
    });
}
async fn open_editor(p: Value) {
    change(|a| a.open_editor(p));
    draw();
    if snapshot().live() {
        if let Ok(pool) = request("GET", "/api/proxies", None).await {
            change(|a| a.pool = list(&pool));
        }
        if let Ok(schedules) = request("GET", "/api/schedules", None).await {
            let id = snapshot()
                .editor
                .as_ref()
                .map(|e| s(&e.draft, "id").to_owned())
                .unwrap_or_default();
            change(|a| {
                a.schedule = list(&schedules)
                    .into_iter()
                    .find(|p| s(p, "profileId") == id)
                    .unwrap_or(Value::Null)
            });
        }
    }
}
fn confirm(operation: &str, ids: Vec<String>, title: &str, body: &str, label: &str) {
    change(|a| {
        a.modal = "confirm".into();
        a.form = json!({"operation":operation,"ids":ids,"title":title,"body":body,"label":label});
    });
}
fn chosen_profiles(id: &str) -> Vec<Value> {
    let a = snapshot();
    if id.is_empty() {
        a.selected_profiles()
    } else {
        a.profiles
            .into_iter()
            .filter(|p| s(p, "id") == id)
            .collect()
    }
}
fn chosen_proxies(id: &str, all: bool) -> Vec<Value> {
    let a = snapshot();
    a.pool
        .into_iter()
        .filter(|p| {
            all || if id.is_empty() {
                a.pool_selected.contains(s(p, "id"))
            } else {
                s(p, "id") == id
            }
        })
        .collect()
}
fn preset() -> String {
    let a = snapshot();
    if s(&a.form, "preset").is_empty() {
        "general".into()
    } else {
        s(&a.form, "preset").into()
    }
}
async fn save() -> Result<(), ApiError> {
    let a = snapshot();
    let Some(mut ed) = a.editor.clone() else {
        return Ok(());
    };
    let Some(p) = ed.begin_save(a.live()) else {
        return Ok(());
    };
    change(|a| a.editor = Some(ed.clone()));
    draw();
    if !a.live() {
        let id = s(&ed.draft, "id").to_owned();
        let before = a.profiles.iter().find(|p| s(p, "id") == id);
        let mut saved = without_marker(p);
        saved["_new"] = json!(false);
        change(|a| {
            if !s(&saved, "engine").is_empty()
                && before.is_none_or(|b| b["engine"] != saved["engine"])
            {
                a.config["default_engine"] = saved["engine"].clone();
            }
            if let Some(index) = a.profiles.iter().position(|p| s(p, "id") == id) {
                a.profiles[index] = saved;
            } else {
                a.profiles.push(saved);
            }
            a.editor = None;
        });
        return Ok(());
    }
    let existing = ed.draft["_new"] != true && a.profiles.iter().any(|x| x["id"] == ed.draft["id"]);
    let id = s(&ed.draft, "id");
    let result = if existing {
        request("PUT", &format!("/api/profiles/{id}"), Some(for_write(&p))).await
    } else {
        let mut p = without_marker(p);
        p.as_object_mut().map(|p| p.remove("id"));
        request("POST", "/api/profiles", Some(p)).await
    };
    match result {
        Ok(saved) => {
            change(|a| a.commit_profile(saved));
            if let Err(err) = refresh().await {
                change(|a| a.say(format!("Profile saved, but refresh failed: {err}"), true));
            }
            let _ = refresh_config().await;
        }
        Err(err) if err.status == 409 && err.kind == "StaleProfile" => {
            match request("GET", "/api/profiles", None).await {
                Ok(latest) => {
                    if let Some(latest) = list(&latest).into_iter().find(|p| s(p, "id") == id) {
                        change(|a| {
                            if let Some(ed) = a.editor.as_mut() {
                                ed.stale(latest);
                            }
                        });
                    } else {
                        change(|a| {
                            if let Some(ed) = a.editor.as_mut() {
                                ed.saving = false;
                                ed.message="Could not load the latest profile: This profile was removed. Your draft is still open.".into();
                            }
                        });
                    }
                }
                Err(err) => change(|a| {
                    if let Some(ed) = a.editor.as_mut() {
                        ed.saving = false;
                        ed.message = format!(
                            "Could not load the latest profile: {err}. Your draft is preserved; retry when it is available."
                        );
                    }
                }),
            }
        }
        Err(err) => {
            change(|a| {
                if let Some(ed) = a.editor.as_mut() {
                    ed.failed(&err);
                }
                a.say(format!("Save failed: {err}"), true);
            });
            let _ = refresh_config().await;
        }
    }
    Ok(())
}
fn local_id() -> String {
    format!(
        "local-{}-{}",
        js_sys::Date::now(),
        (js_sys::Math::random() * u32::MAX as f64) as u32
    )
}
async fn profile_session(id: &str, launch: bool) -> Result<(), ApiError> {
    if snapshot().live() {
        request(
            "POST",
            &format!(
                "/api/profiles/{id}/{}",
                if launch { "launch" } else { "stop" }
            ),
            None,
        )
        .await?;
        refresh().await?;
    } else {
        change(|a| {
            if let Some(p) = a.profiles.iter_mut().find(|p| s(p, "id") == id) {
                p["status"] = json!(if launch { "running" } else { "stopped" });
                p["lastActive"] = json!("now");
            }
        });
    }
    Ok(())
}
async fn bulk_sessions(warm: bool) -> Result<(), ApiError> {
    let a = snapshot();
    if a.busy {
        return Ok(());
    }
    if warm && !a.live() {
        change(|a| {
            a.say(
                "Warm-up runs against the live engine — start `numera serve` first.",
                false,
            )
        });
        return Ok(());
    }
    let selected = a.selected_profiles();
    let ids = if warm {
        selected
            .iter()
            .map(|p| s(p, "id").to_string())
            .collect::<Vec<_>>()
    } else {
        a.launch_ids()
    };
    if ids.is_empty() {
        change(|a| {
            a.say(
                format!(
                    "All {} selected profiles are already running.",
                    selected.len()
                ),
                false,
            )
        });
        return Ok(());
    }
    change(|a| a.busy = true);
    draw();
    let mut settled = vec![];
    let live = a.live();
    for chunk in ids.chunks(4) {
        let promises = chunk
            .iter()
            .map(|id| {
                let id = id.clone();
                wasm_bindgen_futures::future_to_promise(async move {
                    let result = if warm {
                        request(
                            "POST",
                            &format!("/api/profiles/{id}/warmup"),
                            Some(json!({"minutes":5,"preset":"general"})),
                        )
                        .await
                        .map(|_| ())
                    } else if live {
                        request("POST", &format!("/api/profiles/{id}/launch"), None)
                            .await
                            .map(|_| ())
                    } else {
                        profile_session(&id, true).await
                    };
                    Ok(JsValue::from_str(
                        &serde_json::to_string(&(id, result.err().map(|e| e.message))).unwrap(),
                    ))
                })
            })
            .collect::<Array>();
        let results = JsFuture::from(js_sys::Promise::all(&promises))
            .await
            .map_err(network)?;
        for result in Array::from(&results).iter() {
            if let Some((id, error)) = result
                .as_string()
                .and_then(|s| serde_json::from_str::<(String, Option<String>)>(&s).ok())
            {
                settled.push((id, error.map_or(Ok(()), Err)));
            }
        }
    }
    change(|a| {
        let verb = if warm { "Warmed up" } else { "Launched" };
        let skipped = selected.len().saturating_sub(ids.len());
        a.bulk_result(verb, &settled, skipped);
    });
    if a.live() {
        let _ = refresh().await;
    }
    Ok(())
}

pub async fn handle(act: &str, id: &str) -> Result<(), ApiError> {
    if !alive() {
        return Ok(());
    }
    let managed = matches!(
        act,
        "create-token"
            | "add-extension"
            | "remove-extension"
            | "save-proxy"
            | "add-proxies"
            | "test-proxy"
            | "test-proxies"
            | "test-all-proxies"
            | "rotate-proxy"
            | "rotate-proxies"
            | "archive-proxy"
            | "archive-proxies"
            | "attach"
            | "detach"
            | "export-cookies"
            | "schedule"
    );
    if managed {
        if snapshot().busy {
            return Ok(());
        }
        change(|a| a.busy = true);
        draw();
    }
    let result = handle_action(act, id).await;
    if managed && alive() {
        change(|a| a.busy = false);
    }
    result
}
async fn handle_action(act: &str, id: &str) -> Result<(), ApiError> {
    if !alive() {
        return Ok(());
    }
    let a = snapshot();
    match act {
        "nav" => {
            release_icons();
            change(|a| {
                a.view = id.into();
                a.modal.clear();
                a.rename.clear();
            });
            draw();
            let _ = load_view().await;
        }
        "folder" => change(|a| {
            a.folder = id.into();
            a.view = "profiles".into();
            a.page = 0;
            a.modal.clear();
        }),
        "theme" => {
            change(|a| {
                a.theme = if a.theme == "light" {
                    "dark".into()
                } else {
                    "light".into()
                }
            });
            if let Ok(Some(storage)) = window().local_storage() {
                let _ = storage.set_item("ps-theme", &snapshot().theme);
            }
        }
        "new" => {
            let mut p = new_profile(&a.folder, a.default_engine());
            p["id"] = json!(local_id());
            open_editor(p).await;
        }
        "edit" => {
            if let Some(p) = a.profiles.iter().find(|p| s(p, "id") == id) {
                open_editor(p.clone()).await;
            }
        }
        "close-editor" => {
            if a.editor.as_ref().is_none_or(|e| !e.saving) {
                change(|a| a.editor = None);
            }
        }
        "save" => save().await?,
        "tab" => change(|a| {
            if let Some(ed) = a.editor.as_mut()
                && ed.editable()
            {
                ed.tab = id.into();
            }
        }),
        "resolve-mine" | "resolve-latest" => change(|a| {
            if let Some(ed) = a.editor.as_mut() {
                ed.resolve(id.parse().unwrap_or(0), act == "resolve-latest");
            }
        }),
        "toggle-proxy" => change(|a| {
            if let Some(ed) = a.editor.as_mut()
                && ed.editable()
            {
                if ed.draft["proxy"].is_object() {
                    ed.draft["proxy"] = Value::Null;
                } else {
                    ed.draft["proxy"] = json!({"type":"HTTP","host":"","port":"","user":"","pass":"","country":"US"});
                    ed.draft["_passwordSet"] = json!(true);
                }
            }
        }),
        "dnt" => change(|a| {
            if let Some(ed) = a.editor.as_mut() {
                let next = ed.draft["dnt"] != true;
                ed.set(&["dnt".into()], json!(next));
            }
        }),
        "pick" => {
            let parts = id.splitn(3, '|').collect::<Vec<_>>();
            if parts.len() == 3 {
                edit(parts[0], parts[1], parts[2]).map_err(fail)?;
            }
        }
        "ip" | "timezone-auto" | "timezone-manual" => change(|a| {
            if let Some(ed) = a.editor.as_mut() {
                if !ed.editable() {
                    return;
                }
                let signal = if act == "ip" { id } else { "timezone" };
                let mut v = strings(&ed.draft["ipBased"]);
                let on = v.contains(&signal.to_owned());
                if act == "timezone-manual" || on && act == "ip" {
                    v.retain(|v| v != signal);
                } else if ed.draft["proxy"].is_object() && !on {
                    v.push(signal.into());
                }
                ed.draft["ipBased"] = json!(v);
            }
        }),
        "keep" => {
            if let Some((key, value)) = id.split_once('|') {
                change(|a| {
                    if let Some(ed) = a.editor.as_mut() {
                        if !ed.editable() {
                            return;
                        }
                        let mut v = strings(&ed.draft[key]);
                        if v.contains(&value.into()) {
                            v.retain(|v| v != value);
                        } else {
                            v.push(value.into());
                        }
                        ed.draft[key] = json!(v);
                    }
                });
            }
        }
        "select" => change(|a| App::toggle_set(&mut a.selected, id.into())),
        "select-all" => change(App::toggle_all),
        "clear-selection" => change(|a| a.selected.clear()),
        "filters" => change(|a| a.filter_open = !a.filter_open),
        "filter" => {
            if let Some((key, value)) = id.split_once('|') {
                change(|a| {
                    let v = if a.filters.get(key).is_some_and(|v| v == value) {
                        String::new()
                    } else {
                        value.into()
                    };
                    a.filters.insert(key.into(), v);
                    a.page = 0;
                });
            }
        }
        "clear-filters" | "clear-search" => change(|a| {
            a.filters.clear();
            a.query.clear();
            a.page = 0;
        }),
        "page" => change(|a| a.page = id.parse().unwrap_or(0)),
        "run" => {
            let running = a
                .profiles
                .iter()
                .find(|p| s(p, "id") == id)
                .is_some_and(|p| s(p, "status") == "running");
            if let Err(e) = profile_session(id, !running).await {
                change(|a| {
                    a.say(
                        format!("{} failed: {e}", if running { "Stop" } else { "Launch" }),
                        true,
                    )
                });
                let _ = refresh().await;
            }
        }
        "clone" => {
            if let Some(p) = a.profiles.iter().find(|p| s(p, "id") == id) {
                let mut copy = p.clone();
                copy.as_object_mut().map(|p| p.remove("id"));
                copy["name"] = json!(format!("{}-copy", text(&p["name"])));
                copy["status"] = json!("stopped");
                copy["lastActive"] = json!("never");
                copy["_new"] = json!(true);
                if a.live() {
                    request("POST", "/api/profiles", Some(without_marker(copy))).await?;
                    refresh().await?;
                } else {
                    copy["id"] = json!(local_id());
                    change(|a| a.profiles.push(copy));
                }
            }
        }
        "rename-profile" => change(|a| {
            a.rename = id.into();
            a.form["rename"] = a
                .profiles
                .iter()
                .find(|p| s(p, "id") == id)
                .map(|p| json!(text(&p["name"])))
                .unwrap_or(Value::Null);
        }),
        "commit-rename" => {
            if let Some(p) = a.profiles.iter().find(|p| s(p, "id") == a.rename) {
                let name = s(&a.form, "rename").trim();
                if !name.is_empty() && p["name"] != name {
                    let mut patch = p.clone();
                    patch["name"] = json!(name);
                    if a.live() {
                        match request(
                            "PUT",
                            &format!("/api/profiles/{}", a.rename),
                            Some(for_write(&patch)),
                        )
                        .await
                        {
                            Ok(saved) => change(|a| {
                                if let Some(p) =
                                    a.profiles.iter_mut().find(|p| s(p, "id") == a.rename)
                                {
                                    *p = saved;
                                }
                            }),
                            Err(err) => change(|a| a.say(err.message, true)),
                        }
                    } else {
                        change(|a| {
                            if let Some(p) = a.profiles.iter_mut().find(|p| s(p, "id") == a.rename)
                            {
                                p["name"] = json!(name);
                            }
                        });
                    }
                }
            }
            change(|a| a.rename.clear());
        }
        "bulk-launch" => bulk_sessions(false).await?,
        "bulk-warmup" => bulk_sessions(true).await?,
        "delete-profile" | "delete-selected" => {
            let rows = chosen_profiles(if act == "delete-profile" { id } else { "" });
            let n = rows.len();
            let running = rows.iter().filter(|p| s(p, "status") == "running").count();
            confirm(
                "delete-profiles",
                rows.iter().map(|p| s(p, "id").into()).collect(),
                &if n == 1 {
                    "Delete this profile?".into()
                } else {
                    format!("Delete {n} profiles?")
                },
                &format!(
                    "Deleting {n} profiles. The browser session goes too — cookies, logins and local storage. There is no undo. {running} of them are running right now."
                ),
                "Delete",
            );
        }
        "export-profiles" => {
            let (file, msg) = export_profiles(&a.selected_profiles());
            download(
                &format!("numera-profiles-{}.json", a.selected_profiles().len()),
                &serde_json::to_string_pretty(&file).unwrap(),
                "application/json",
            )?;
            change(|a| a.say(msg, false));
        }
        "import-profiles" => {
            if !a.live() {
                change(|a| a.say("Start the engine (numera serve) to import profiles.", false));
            } else {
                pick_file("profiles", ".json,application/json")?;
            }
        }
        "bulk" => change(|a| {
            a.modal = "bulk".into();
            a.batch = Batch::default();
            a.batch.engine = a.default_engine().into();
        }),
        "sync" => change(|a| {
            a.modal = "sync".into();
            a.form = json!({});
        }),
        "batch-chip" => {
            if let Some((key, value)) = id.split_once('|') {
                change(|a| {
                    let v = match key {
                        "os" => &mut a.batch.os,
                        "countries" => &mut a.batch.countries,
                        _ => &mut a.batch.versions,
                    };
                    if v.contains(&value.into()) {
                        if v.len() > 1 {
                            v.retain(|v| v != value);
                        }
                    } else {
                        v.push(value.into());
                    }
                });
            }
        }
        "batch-randomize" => change(|a| a.batch.randomize = !a.batch.randomize),
        "create-batch" => {
            if a.busy {
                return Ok(());
            }
            change(|a| {
                a.busy = true;
                a.batch_progress = 0;
            });
            draw();
            let mut profiles = a
                .batch
                .build(&a.folder, |n| (js_sys::Math::random() * n as f64) as usize);
            let result = async {
                if a.live() {
                    for (chunk_index, chunk) in profiles.chunks(500).enumerate() {
                        request(
                            "POST",
                            "/api/profiles/batch",
                            Some(json!({"profiles":chunk})),
                        )
                        .await?;
                        change(|a| {
                            a.batch_progress = ((chunk_index + 1) * 500).min(profiles.len())
                        });
                        draw();
                    }
                    refresh().await?;
                } else {
                    for p in &mut profiles {
                        p["id"] = json!(local_id());
                    }
                    change(|a| a.profiles.extend(profiles));
                }
                Ok::<_, ApiError>(())
            }
            .await;
            change(|a| {
                a.busy = false;
                if result.is_ok() {
                    a.modal.clear();
                }
            });
            result?;
        }
        "apply-sync" => {
            let patch = sync_patch(&a.form);
            if patch.as_object().is_none_or(|o| o.is_empty()) || a.busy {
                return Ok(());
            }
            let ids = a
                .selected_profiles()
                .iter()
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            if a.live() {
                match request(
                    "POST",
                    "/api/profiles/bulk",
                    Some(json!({"ids":ids,"patch":patch})),
                )
                .await
                {
                    Ok(saved) => {
                        let done = list(&saved).len();
                        change(|a| {
                            a.say(
                                if done < ids.len() {
                                    format!(
                                        "Edited {done} of {}. {} no longer existed.",
                                        ids.len(),
                                        ids.len() - done
                                    )
                                } else {
                                    format!("Edited {done} profile(s).")
                                },
                                done < ids.len(),
                            )
                        });
                        let _ = refresh().await;
                        let _ = refresh_config().await;
                    }
                    Err(e) => {
                        let _ = refresh_config().await;
                        change(|a| a.say(format!("Bulk edit failed: {e}"), true));
                    }
                }
            } else {
                change(|a| {
                    for p in &mut a.profiles {
                        if ids.contains(&s(p, "id").to_owned()) {
                            merge_patch(p, &patch);
                        }
                    }
                    if !s(&patch, "engine").is_empty() {
                        a.config["default_engine"] = patch["engine"].clone();
                    }
                });
            }
            change(|a| {
                a.selected.clear();
                a.modal.clear();
            });
        }
        "create-folder" => {
            let name = s(&a.form, "newFolder").trim();
            if !name.is_empty() && !a.folders().contains(&name.into()) {
                if a.live() {
                    match request("POST", "/api/folders", Some(json!({"name":name}))).await {
                        Ok(config) => change(|a| a.config = config),
                        Err(e) => {
                            let _ = refresh_config().await;
                            return Err(e);
                        }
                    }
                } else {
                    change(|a| {
                        let mut folders = a.folders();
                        folders.push(name.into());
                        a.config["folders"] = json!(folders);
                    });
                }
            }
            change(|a| a.form["newFolder"] = json!(""));
        }
        "rename-folder" => change(|a| {
            a.modal = "rename-folder".into();
            a.form = json!({"from":id,"to":id});
        }),
        "commit-folder" => {
            let from = s(&a.form, "from");
            let to = s(&a.form, "to").trim();
            if !to.is_empty() && to != from && !a.folders().contains(&to.into()) {
                if a.live() {
                    match request(
                        "POST",
                        "/api/folders/rename",
                        Some(json!({"from":from,"to":to})),
                    )
                    .await
                    {
                        Ok(config) => {
                            change(|a| {
                                a.config = config;
                                if a.folder == from {
                                    a.folder = to.into();
                                }
                            });
                            refresh().await?;
                        }
                        Err(e) => {
                            let _ = refresh_config().await;
                            let _ = refresh().await;
                            return Err(e);
                        }
                    }
                } else {
                    change(|a| {
                        a.config["folders"] = json!(
                            a.folders()
                                .into_iter()
                                .map(|f| if f == from { to.into() } else { f })
                                .collect::<Vec<_>>()
                        );
                        for p in &mut a.profiles {
                            if s(p, "folder") == from {
                                p["folder"] = json!(to);
                            }
                        }
                        if a.folder == from {
                            a.folder = to.into();
                        }
                    });
                }
            }
            change(|a| a.modal.clear());
        }
        "delete-folder" => confirm(
            "delete-folder",
            vec![id.into()],
            "Delete folder?",
            "Profiles inside it move to Unfiled. Their sessions remain.",
            "Delete folder",
        ),
        "default-engine" => {
            if a.live() {
                match request("PUT", "/api/config", Some(json!({"default_engine":id}))).await {
                    Ok(config) => change(|a| a.config = config),
                    Err(e) => {
                        let _ = refresh_config().await;
                        return Err(e);
                    }
                }
            } else {
                change(|a| {
                    a.config["default_engine"] = json!(id);
                    a.say(
                        "Connect the engine (numera serve) to save this setting.",
                        false,
                    );
                });
            }
        }
        "add-extension" => {
            let entry = s(&a.form, "extension").trim();
            if entry.is_empty() {
                return Ok(());
            }
            if entry.to_lowercase().starts_with("http://")
                || entry.to_lowercase().starts_with("https://")
            {
                if !a.live() {
                    return Err(fail("Installing from a link needs the engine running."));
                }
                let result = request(
                    "POST",
                    "/api/extensions/install",
                    Some(json!({"url":entry})),
                )
                .await?;
                change(|a| {
                    a.config["default_extensions"] = result["default_extensions"].clone();
                    a.form["extension"] = json!("");
                    a.say(
                        format!(
                            "{}: {}",
                            if result["reused"] == true {
                                "Already installed"
                            } else {
                                "Installed"
                            },
                            s(&result, "name")
                        ),
                        false,
                    );
                });
                load_extensions().await?;
            } else {
                let mut paths = strings(&a.config["default_extensions"]);
                if paths.contains(&entry.into()) {
                    return Err(fail("That folder is already in the list."));
                }
                paths.push(entry.into());
                save_extensions(paths).await?;
                change(|a| a.form["extension"] = json!(""));
            }
        }
        "remove-extension" => {
            let paths = strings(&a.config["default_extensions"])
                .into_iter()
                .filter(|p| p != id)
                .collect();
            save_extensions(paths).await?;
        }
        "apply-extensions" => {
            if !a.live() || a.busy {
                return Ok(());
            }
            change(|a| a.busy = true);
            let result = async {
                let all = request("GET", "/api/profiles", None).await?;
                let items = strings(&a.config["default_extensions"]);
                let (mut changed, mut skipped, mut failed) = (0, 0, 0);
                for mut p in list(&all) {
                    let mut have = s(&p, "extensions")
                        .lines()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    let missing = items
                        .iter()
                        .filter(|x| !have.contains(x))
                        .cloned()
                        .collect::<Vec<_>>();
                    if missing.is_empty() {
                        skipped += 1;
                        continue;
                    }
                    have.extend(missing);
                    p["extensions"] = json!(have.join("\n"));
                    if request(
                        "PUT",
                        &format!("/api/profiles/{}", s(&p, "id")),
                        Some(for_write(&p)),
                    )
                    .await
                    .is_ok()
                    {
                        changed += 1
                    } else {
                        failed += 1
                    }
                }
                let _ = refresh().await;
                change(|a| {
                    a.say(
                        format!("Updated {changed}, already had them {skipped}, failed {failed}."),
                        failed > 0,
                    )
                });
                Ok::<_, ApiError>(())
            }
            .await;
            change(|a| a.busy = false);
            result?;
        }
        "refresh-view" => {
            load_view().await?;
        }
        "probe" | "health-trust" | "health-align" => {
            if a.busy {
                return Ok(());
            }
            let is_health = act.starts_with("health-");
            let kind = if act == "health-trust" {
                "trust"
            } else if act == "health-align" {
                "align"
            } else {
                id
            };
            let pid = if is_health {
                id.to_owned()
            } else {
                a.editor
                    .as_ref()
                    .map(|e| s(&e.draft, "id").into())
                    .unwrap_or_default()
            };
            change(|a| {
                a.busy = true;
                if let Some(ed) = a.editor.as_mut() {
                    ed.probe = json!({"state":"running"});
                }
            });
            draw();
            let result = request("POST", &format!("/api/profiles/{pid}/{kind}"), None).await;
            change(|a| {
                a.busy = false;
                match &result {
                    Ok(v) => {
                        if let Some(ed) = a.editor.as_mut() {
                            ed.probe = v.clone();
                            if v["profile"].is_object()
                                && let Some(fields) = v["profile"].as_object()
                            {
                                for (k, v) in fields {
                                    ed.draft[k] = v.clone();
                                }
                            }
                        }
                        if is_health {
                            let name = a
                                .profiles
                                .iter()
                                .find(|p| s(p, "id") == pid)
                                .map(|p| text(&p["name"]))
                                .unwrap_or(pid.clone());
                            a.say(health_report(&name, kind, v), false);
                        }
                    }
                    Err(e) => {
                        if let Some(ed) = a.editor.as_mut() {
                            ed.probe = json!({"state":"error","error":e.message});
                        }
                        a.say(
                            format!(
                                "{} failed: {e}",
                                if kind == "trust" {
                                    "Trust check"
                                } else {
                                    "Auto-adjust"
                                }
                            ),
                            true,
                        );
                    }
                }
            });
            if is_health {
                let _ = load_view().await;
                let _ = refresh().await;
            }
        }
        "warm-now" => {
            let pid = a.editor.as_ref().map(|e| s(&e.draft, "id")).unwrap_or("");
            request(
                "POST",
                &format!("/api/profiles/{pid}/warmup"),
                Some(json!({"minutes":5,"preset":preset()})),
            )
            .await?;
            change(|a| {
                a.say(
                    "Warm-up started — runs in the background for ~5 minutes.",
                    false,
                )
            });
        }
        "schedule" => {
            if a.schedule.is_object() {
                request(
                    "DELETE",
                    &format!("/api/schedules/{}", s(&a.schedule, "id")),
                    None,
                )
                .await?;
                change(|a| a.schedule = Value::Null);
            } else {
                let pid = a.editor.as_ref().map(|e| s(&e.draft, "id")).unwrap_or("");
                let result = request(
                    "POST",
                    "/api/schedules",
                    Some(json!({"profileId":pid,"everyHours":12,"minutes":5,"preset":preset()})),
                )
                .await?;
                change(|a| a.schedule = result);
            }
        }
        "replace-cookies" => {
            change(|a| a.form["replaceCookies"] = json!(a.form["replaceCookies"] != true))
        }
        "export-cookies" => {
            let p = a
                .editor
                .as_ref()
                .map(|e| e.draft.clone())
                .unwrap_or(Value::Null);
            let result = request(
                "GET",
                &format!("/api/profiles/{}/cookies", s(&p, "id")),
                None,
            )
            .await?;
            download(
                &format!("{}-cookies.json", filename(&text(&p["name"]))),
                &json!({"cookies":result["cookies"],"origins":[]}).to_string(),
                "application/json",
            )?;
            change(|a| {
                a.say(
                    format!("Exported {} cookies.", list(&result["cookies"]).len()),
                    false,
                )
            });
        }
        "import-cookies" => pick_file("cookies", ".json,.txt")?,
        "headless" => change(|a| a.form["headless"] = json!(a.form["headless"] != true)),
        "attach" => {
            let result = request(
                "POST",
                &format!("/api/profiles/{id}/attach"),
                Some(json!({"headless":a.form["headless"]==true})),
            )
            .await?;
            change(|a| {
                a.connections.insert(id.into(), result.clone());
                a.say(format!("Attached on :{}", number(&result, "port")), false);
            });
            refresh().await?;
        }
        "detach" => {
            request("POST", &format!("/api/profiles/{id}/stop"), None).await?;
            change(|a| {
                a.connections.remove(id);
            });
            refresh().await?;
        }
        "copy-snippet" => {
            if let Some(c) = a.connections.get(id) {
                copy(&render::cdp_snippet(s(&a.form, "lang"), number(c, "port"))).await?;
            }
        }
        "proxy-tab" => change(|a| a.pool_tab = id.into()),
        "select-proxy" => change(|a| App::toggle_set(&mut a.pool_selected, id.into())),
        "select-all-proxies" => change(|a| {
            let ids = a
                .pool
                .iter()
                .filter(|p| match a.pool_tab.as_str() {
                    "archive" => p["archived"] == true,
                    "errors" => p["archived"] != true && s(p, "status") == "failed",
                    _ => p["archived"] != true,
                })
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            if ids.iter().all(|id| a.pool_selected.contains(id)) {
                a.pool_selected.clear();
            } else {
                a.pool_selected = ids.into_iter().collect();
            }
        }),
        "clear-proxies" => change(|a| a.pool_selected.clear()),
        "reveal-proxy" => change(|a| App::toggle_set(&mut a.reveal, id.into())),
        "copy-proxy" => {
            if let Some(p) = a.pool.iter().find(|p| s(p, "id") == id) {
                copy(s(p, "pass")).await?;
            }
        }
        "edit-proxy" => {
            if let Some(p) = a.pool.iter().find(|p| s(p, "id") == id) {
                change(|a| {
                    a.modal = "edit-proxy".into();
                    a.form = p.clone();
                });
            }
        }
        "save-proxy" => {
            if s(&a.form, "host").trim().is_empty() {
                return Err(fail("Host cannot be empty."));
            }
            if text(&a.form["port"]).trim().is_empty() {
                return Err(fail("Port cannot be empty."));
            }
            let patch = pool_password_patch(&a.form);
            request(
                "PUT",
                &format!("/api/proxies/{}", s(&a.form, "id")),
                Some(patch),
            )
            .await?;
            change(|a| {
                a.modal.clear();
                a.say("Saved.", false);
            });
            load_view().await?;
        }
        "import-proxies" => change(|a| {
            a.modal = "import-proxies".into();
            a.form = json!({"text":""});
        }),
        "load-proxy-file" => pick_file("proxies", ".txt,.csv,.json")?,
        "add-proxies" => {
            let result = request(
                "POST",
                "/api/proxies",
                Some(json!({"text":s(&a.form,"text")})),
            )
            .await?;
            change(|a| {
                a.modal.clear();
                a.say(
                    format!(
                        "Added {} proxy(ies). Pool now has {}.",
                        number(&result, "added"),
                        number(&result, "total")
                    ),
                    false,
                );
            });
            load_view().await?;
        }
        "test-proxy" | "test-proxies" | "test-all-proxies" => {
            let ids = chosen_proxies(
                if act == "test-proxy" { id } else { "" },
                act == "test-all-proxies",
            )
            .iter()
            .map(|p| s(p, "id").to_owned())
            .collect::<Vec<_>>();
            let r = if act == "test-proxy" {
                request("POST", &format!("/api/proxies/{id}/test"), None).await?
            } else {
                request("POST", "/api/proxies/test", Some(json!({"ids":ids}))).await?
            };
            change(|a| {
                a.say(
                    if act == "test-proxy" {
                        "Proxy checked.".into()
                    } else {
                        format!(
                            "Tested {}: {} live, {} not.",
                            number(&r, "tested"),
                            number(&r, "ok"),
                            number(&r, "tested").saturating_sub(number(&r, "ok"))
                        )
                    },
                    false,
                )
            });
            load_view().await?;
        }
        "rotate-proxy" | "rotate-proxies" => {
            let rows = chosen_proxies(if act == "rotate-proxy" { id } else { "" }, false);
            let ids = rows
                .iter()
                .filter(|p| !s(p, "rotateUrl").is_empty())
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            if ids.is_empty() {
                return Err(fail("None of these have a change-IP URL set."));
            }
            let r = if act == "rotate-proxy" {
                request("POST", &format!("/api/proxies/{id}/rotate"), None).await?
            } else {
                request("POST", "/api/proxies/rotate", Some(json!({"ids":ids}))).await?
            };
            change(|a| {
                a.say(
                    if act == "rotate-proxy" {
                        if r["rotated"] == true {
                            format!("New exit IP: {} → {}", text(&r["previousIp"]), s(&r, "ip"))
                        } else if !s(&r, "error").is_empty() {
                            s(&r, "error").into()
                        } else {
                            "The exit IP did not change.".into()
                        }
                    } else {
                        format!(
                            "{} of {} moved to a new IP.",
                            number(&r, "rotated"),
                            number(&r, "tried")
                        )
                    },
                    if act == "rotate-proxy" {
                        r["rotated"] != true
                    } else {
                        number(&r, "rotated") == 0
                    },
                )
            });
            load_view().await?;
        }
        "archive-proxy" | "archive-proxies" => {
            let rows = chosen_proxies(if act == "archive-proxy" { id } else { "" }, false);
            let archived = if act == "archive-proxy" {
                rows.first().is_none_or(|p| p["archived"] != true)
            } else {
                a.pool_tab != "archive"
            };
            request("POST","/api/proxies/archive",Some(json!({"ids":rows.iter().map(|p|s(p,"id")).collect::<Vec<_>>(),"archived":archived}))).await?;
            change(|a| {
                a.pool_selected.clear();
                a.say(
                    format!(
                        "{} {} proxies.",
                        if archived { "Archived" } else { "Restored" },
                        rows.len()
                    ),
                    false,
                );
            });
            load_view().await?;
        }
        "export-proxies" | "export-all-proxies" => {
            let ids = chosen_proxies("", act == "export-all-proxies")
                .iter()
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            let r = request("POST", "/api/proxies/export", Some(json!({"ids":ids}))).await?;
            download(
                &format!("numera-proxies-{}.txt", number(&r, "count")),
                s(&r, "text"),
                "text/plain",
            )?;
            change(|a| a.say(format!("Exported {} proxies.", number(&r, "count")), false));
        }
        "delete-proxy" | "delete-proxies" => {
            let rows = chosen_proxies(if act == "delete-proxy" { id } else { "" }, false);
            let used = rows.iter().map(|p| number(p, "profiles")).sum::<u64>();
            confirm(
                "delete-proxies",
                rows.iter().map(|p| s(p, "id").into()).collect(),
                &format!("Delete {} proxies?", rows.len()),
                &format!(
                    "Removing {} proxies from the pool. {used} profiles currently use them; those profiles keep their proxy settings. This cannot be undone, but you can re-import from an export.",
                    rows.len()
                ),
                "Delete",
            );
        }
        "assign-proxies" => confirm(
            "assign-proxies",
            vec![],
            "Assign proxies to profiles?",
            &format!(
                "Spreading {} proxies round-robin across all {} profiles. Each profile's locale and timezone will be aligned to its proxy's country.",
                a.pool.len(),
                a.profiles.len()
            ),
            "Assign",
        ),
        "create-token" => {
            let name = s(&a.form, "tokenName").trim();
            if name.is_empty() {
                return Ok(());
            }
            let made = request("POST", "/api/tokens", Some(json!({"name":name}))).await?;
            set_auth(&a.adopt_token(&auth(), &made));
            change(|a| {
                a.minted = made;
                a.form["tokenName"] = json!("");
            });
            load_view().await?;
        }
        "copy-token" => copy(s(&a.minted, "token")).await?,
        "dismiss-token" => change(|a| a.minted = Value::Null),
        "revoke-token" => {
            let name = list(&a.tokens["tokens"])
                .iter()
                .find(|t| s(t, "id") == id)
                .map(|t| text(&t["name"]))
                .unwrap_or_default();
            confirm(
                "revoke-token",
                vec![id.into()],
                "Revoke this token?",
                &format!(
                    "Anything using {name} stops working immediately. Other tokens are unaffected. There is no undo — mint a new one if you need it back."
                ),
                "Revoke",
            );
        }
        "unlock" => {
            set_auth(s(&a.form, "token"));
            change(|a| a.busy = true);
            let state = health().await;
            change(|a| {
                a.busy = false;
                a.auth_error = if state == "locked" {
                    "That token was not accepted.".into()
                } else if state == "off" {
                    "The engine stopped answering. Is it still running?".into()
                } else {
                    String::new()
                };
            });
            if state == "ok" {
                change(|a| a.mode = "connecting".into());
                connect().await;
            }
        }
        "cloak-review" => {
            if a.busy || !a.live() {
                return Ok(());
            }
            change(|a| {
                a.busy = true;
                a.cloak_approved = false;
            });
            draw();
            let r = request("GET", "/api/engines/cloak/acquisition", None).await;
            change(|a| {
                a.busy = false;
                a.cloak_approved = false;
            });
            change(|a| a.cloak_reviewed(r.as_ref().cloned().unwrap_or(Value::Null)));
            r?;
        }
        "cloak-consent" => change(|a| {
            if !a.busy && a.cloak["available"] == true {
                a.cloak_approved = !a.cloak_approved;
            }
        }),
        "cloak-acquire" => {
            let mut body = None;
            change(|a| body = a.begin_cloak());
            let Some(body) = body else { return Ok(()) };
            draw();
            let result = request("POST", "/api/engines/cloak/acquisition", Some(body)).await;
            change(|a| a.finish_cloak(result.as_ref().cloned().map_err(|e| e.message.clone())));
            result?;
        }
        "update-check" => run_update("check", "").await?,
        "update-cancel" => run_update("cancel", s(&a.update["offer"], "id")).await?,
        "update-review" => {
            if a.update["offer"].is_object() {
                change(
                    |a| a.update_approval = json!({"offer":a.update["offer"],"install":(["ready","deferred"].contains(&s(&a.update,"state")))}),
                );
            }
        }
        "update-later" => change(|a| a.update_approval = Value::Null),
        "update-approve" => {
            if updates::approval_valid(
                &a.update,
                &a.update_approval,
                a.update_busy,
                &a.update_error,
            ) {
                let action = if a.update_approval["install"] == true {
                    "install"
                } else {
                    "download"
                };
                let id = s(&a.update_approval["offer"], "id").to_owned();
                change(|a| a.update_approval = Value::Null);
                run_update(action, &id).await?;
            }
        }
        "dismiss-update" => change(|a| a.dismissed = s(&a.update["offer"], "id").into()),
        "commands" => change(|a| {
            a.modal = "commands".into();
            a.command.clear();
            a.command_index = 0;
        }),
        "command" => {
            if let Some((_, action, id)) =
                a.commands().get(id.parse::<usize>().unwrap_or(0)).cloned()
            {
                change(|a| a.modal.clear());
                Box::pin(handle(&action, &id)).await?;
            }
        }
        "close-modal" => {
            if !a.busy {
                change(|a| a.modal.clear());
            }
        }
        "clear-message" => change(|a| {
            a.toasts.retain(|t| t.id.to_string() != id);
            a.message.clear();
        }),
        "confirm" => execute_confirmation().await?,
        _ => {}
    }
    Ok(())
}
async fn save_extensions(paths: Vec<String>) -> Result<(), ApiError> {
    let a = snapshot();
    if a.live() {
        let cfg = request(
            "PUT",
            "/api/config",
            Some(json!({"default_extensions":paths})),
        )
        .await?;
        change(|a| a.config = cfg);
        load_extensions().await?;
    } else {
        change(|a| a.config["default_extensions"] = json!(paths));
    }
    Ok(())
}
async fn execute_confirmation() -> Result<(), ApiError> {
    let a = snapshot();
    if a.busy {
        return Ok(());
    }
    let ids = strings(&a.form["ids"]);
    change(|a| a.busy = true);
    draw();
    let result=async{match s(&a.form,"operation"){
"delete-profiles"=>{if a.live(){let r=request("POST","/api/profiles/delete",Some(json!({"ids":ids}))).await?;let removed=list(&r["removed"]).len();let missing=list(&r["missing"]).len();let failed=list(&r["failed"]);change(|a|a.say(format!("Deleted {removed} profile(s). {} no longer existed. {} could not be deleted. {}",missing,failed.len(),failed.first().map(|f|s(f,"error")).unwrap_or("")),!failed.is_empty()||missing>0));let _=refresh().await;}else{change(|a|{a.profiles.retain(|p|!ids.contains(&s(p,"id").to_owned()));a.say(format!("Deleted {} profiles.",ids.len()),false);});}change(|a|a.selected.clear());},
"delete-folder"=>{let name=ids.first().map(String::as_str).unwrap_or("");if a.live(){let cfg=request("POST","/api/folders/delete",Some(json!({"name":name}))).await?;change(|a|a.config=cfg);refresh().await?;}else{change(|a|{a.config["folders"]=json!(a.folders().into_iter().filter(|f|f!=name).collect::<Vec<_>>());for p in &mut a.profiles{if s(p,"folder")==name{p["folder"]=json!("Unfiled");}}});}change(|a|{if a.folder==name{a.folder="All profiles".into();}});},"delete-proxies"=>{let r=request("POST","/api/proxies/delete",Some(json!({"ids":ids}))).await?;change(|a|a.say(format!("Deleted {} proxies.",number(&r,"deleted")),false));load_view().await?;},"assign-proxies"=>{let r=request("POST","/api/proxies/assign",Some(json!({}))).await?;change(|a|a.say(format!("Assigned proxies to {} profile(s).",number(&r,"assigned")),false));refresh().await?;load_view().await?;},"revoke-token"=>{if let Some(id)=ids.first(){request("DELETE",&format!("/api/tokens/{id}"),None).await?;change(|a|a.say("Revoked.",false));load_view().await?;}},_=>{}}Ok::<_,ApiError>(())}.await;
    change(|a| {
        a.busy = false;
        if result.is_ok() {
            a.modal.clear();
        }
    });
    if result.is_err() {
        let _ = refresh_config().await;
    }
    result
}
async fn copy(text: &str) -> Result<(), ApiError> {
    JsFuture::from(window().navigator().clipboard().write_text(text))
        .await
        .map_err(network)?;
    change(|a| a.say("Copied.", false));
    Ok(())
}
fn filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect()
}
fn download(name: &str, text: &str, mime: &str) -> Result<(), ApiError> {
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type(mime);
    let parts = Array::new();
    parts.push(&JsValue::from_str(text));
    let blob = Blob::new_with_str_sequence_and_options(&parts, &bag).map_err(network)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(network)?;
    let anchor = document()
        .create_element("a")
        .map_err(network)?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|e| network(e.into()))?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    web_sys::Url::revoke_object_url(&url).map_err(network)
}
fn pick_file(kind: &str, accept: &str) -> Result<(), ApiError> {
    let input = document()
        .create_element("input")
        .map_err(network)?
        .dyn_into::<HtmlInputElement>()
        .map_err(|e| network(e.into()))?;
    input.set_type("file");
    input.set_accept(accept);
    input.set_id("numera-file-picker");
    input.set_hidden(true);
    document()
        .body()
        .ok_or_else(|| fail("No document body"))?
        .append_child(&input)
        .map_err(network)?;
    let kind = kind.to_owned();
    let selected = input.clone();
    listen(input.clone().into(), "change", move |_: Event| {
        selected.remove();
        if let Some(file) = selected.files().and_then(|files| files.get(0)) {
            let kind = kind.clone();
            spawn_local(async move {
                if kind == "cookies" {
                    change(|a| a.busy = true);
                    draw();
                }
                let result = async {
                    let text = JsFuture::from(file.text())
                        .await
                        .map_err(network)?
                        .as_string()
                        .unwrap_or_default();
                    match kind.as_str() {
                        "profiles" => {
                            let r =
                                request("POST", "/api/profiles/import", Some(json!({"text":text})))
                                    .await?;
                            let _ = refresh().await;
                            change(|a| a.say(import_report(&r), false));
                        }
                        "proxies" => change(|a| a.form["text"] = json!(text)),
                        "cookies" => {
                            let a = snapshot();
                            let id = a.editor.as_ref().map(|e| s(&e.draft, "id")).unwrap_or("");
                            let r = request(
                                "POST",
                                &format!("/api/profiles/{id}/cookies"),
                                Some(json!({"text":text,"clear":a.form["replaceCookies"]==true})),
                            )
                            .await?;
                            change(|a| {
                                a.say(
                                    format!(
                                        "Imported {} cookies from {}.",
                                        number(&r, "imported"),
                                        file.name()
                                    ),
                                    false,
                                )
                            });
                        }
                        _ => {}
                    }
                    Ok::<_, ApiError>(())
                }
                .await;
                if let Err(e) = result {
                    change(|a| a.say(format!("Import failed: {e}"), true));
                }
                if kind == "cookies" {
                    change(|a| a.busy = false);
                }
                draw();
            });
        }
    });
    let cancelled = input.clone();
    listen(input.clone().into(), "cancel", move |_| cancelled.remove());
    input.click();
    Ok(())
}
pub fn timezones(zone: &str) -> (Vec<String>, bool) {
    let intl =
        Reflect::get(&js_sys::global(), &JsValue::from_str("Intl")).unwrap_or(JsValue::UNDEFINED);
    let values = call(&intl, "supportedValuesOf", &[JsValue::from_str("timeZone")])
        .unwrap_or(JsValue::UNDEFINED);
    let mut zones = if Array::is_array(&values) {
        Array::from(&values)
            .iter()
            .filter_map(|v| v.as_string())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    zones.extend(LOCALES.map(|(_, z)| z.into()));
    zones.sort();
    zones.dedup();
    let options = js_sys::Object::new();
    let _ = Reflect::set(
        &options,
        &JsValue::from_str("timeZone"),
        &JsValue::from_str(zone),
    );
    let valid = Reflect::get(&intl, &JsValue::from_str("DateTimeFormat"))
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
        .is_some_and(|f| {
            let args = Array::new();
            args.push(&JsValue::from_str("en"));
            args.push(&options);
            Reflect::construct(&f, &args).is_ok()
        });
    (zones, valid)
}
struct StreamReader(JsValue);
impl Drop for StreamReader {
    fn drop(&mut self) {
        let _ = call(&self.0, "cancel", &[]);
        let _ = call(&self.0, "releaseLock", &[]);
    }
}
async fn bounded_bytes(response: &Response) -> Result<Vec<u8>, ApiError> {
    let stream = response
        .body()
        .ok_or_else(|| fail("Invalid update status"))?;
    let reader = call(stream.as_ref(), "getReader", &[]).map_err(network)?;
    let _owner = StreamReader(reader.clone());
    let mut bytes = vec![];
    loop {
        let promise = call(&reader, "read", &[])
            .map_err(network)?
            .dyn_into::<js_sys::Promise>()
            .map_err(network)?;
        let result = JsFuture::from(promise).await.map_err(network)?;
        if Reflect::get(&result, &JsValue::from_str("done")).unwrap_or(JsValue::FALSE)
            == JsValue::TRUE
        {
            break;
        }
        let chunk =
            Uint8Array::new(&Reflect::get(&result, &JsValue::from_str("value")).map_err(network)?);
        if bytes.len() + chunk.length() as usize > 65536 {
            return Err(fail("Invalid update status"));
        }
        bytes.extend(chunk.to_vec());
    }
    Ok(bytes)
}
async fn run_update(action: &str, id: &str) -> Result<(), ApiError> {
    let a = snapshot();
    let Some(bridge) = a.update_bridge else {
        return Ok(());
    };
    let sequence = {
        let r = runtime();
        let mut r = r.borrow_mut();
        r.update_sequence += 1;
        if let Some(controller) = r.update_controller.take() {
            controller.abort();
        }
        r.update_next = f64::INFINITY;
        r.update_sequence
    };
    change(|a| a.update_busy = action != "status");
    draw();
    let result = async {
        let d = Deadline::new(10000)?;
        runtime().borrow_mut().update_controller = Some(d.controller.clone());
        let body = updates::action_body(action, id);
        let response = send(
            if action == "status" { "GET" } else { "POST" },
            &format!("{}/{action}", bridge.endpoint),
            if action == "status" {
                None
            } else {
                Some(&body)
            },
            &d,
            &bridge.token,
            true,
        )
        .await?;
        let bytes = bounded_bytes(&response).await?;
        updates::response(&bytes, response.ok()).map_err(fail)
    }
    .await;
    if !alive() || runtime().borrow().update_sequence != sequence {
        return Ok(());
    }
    change(|a| {
        a.update_busy = false;
        match result{Ok(status)=>{a.update=status;a.update_error.clear();},Err(_)=>a.update_error="The desktop update service is unavailable. Check again before approving an update.".into()}
    });
    draw();
    let delay = if updates::active(&snapshot().update) {
        1000
    } else {
        15000
    };
    runtime().borrow_mut().update_next = js_sys::Date::now() + f64::from(delay);
    runtime().borrow_mut().update_controller = None;
    Ok(())
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
