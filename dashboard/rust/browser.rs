//! The browser is a thin adapter: DOM events enter the Rust state machine;
//! authenticated requests and owned resources leave it through this module.
use crate::{
    contracts::{self, ApiError},
    model::*,
    render, theme_sweep, updates,
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
            | "profile-ip-check"
            | "fleet-stop"
            | "release-check"
            | "release-preference"
            | "release-retry"
            | "retry-view"
            | "mcp-check"
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

#[path = "browser_http.rs"]
mod http;
use http::*;

#[path = "browser_runtime_dom.rs"]
mod runtime_dom;
use runtime_dom::*;
pub use runtime_dom::{draw, shutdown, start};

#[path = "browser_profile_io.rs"]
mod profile_io;
pub use profile_io::edit;
use profile_io::*;

#[path = "browser_platform.rs"]
mod platform;
pub use platform::timezones;
use platform::*;

#[path = "browser_profile_actions.rs"]
mod profile_actions;

#[path = "browser_resource_actions.rs"]
mod resource_actions;

#[path = "browser_proxy_actions.rs"]
mod proxy_actions;

#[path = "browser_settings_actions.rs"]
mod settings_actions;

#[path = "browser_release.rs"]
mod release;

async fn handle_action(act: &str, id: &str) -> Result<(), ApiError> {
    if !alive() {
        return Ok(());
    }
    let a = snapshot();
    if !release::dispatch(act, id, &a).await?
        && !profile_actions::dispatch(act, id, &a).await?
        && !resource_actions::dispatch(act, id, &a).await?
        && !proxy_actions::dispatch(act, id, &a).await?
    {
        settings_actions::dispatch(act, id, &a).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "browser_tests.rs"]
mod tests;
