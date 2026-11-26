use super::*;

#[cfg_attr(not(test), wasm_bindgen(start))]
pub fn start() -> Result<(), JsValue> {
    shutdown();
    let mut state = App::default();
    let origin = option_env!("NUMERA_API_URL")
        .map(str::to_owned)
        .or_else(|| window().location().origin().ok());
    state.mcp_bridge_command = origin
        .as_deref()
        .and_then(crate::release_controls::bridge_command);
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
pub(super) fn js_value(v: &JsValue) -> Option<Value> {
    js_sys::JSON::stringify(v)
        .ok()
        .and_then(|s| s.as_string())
        .and_then(|s| serde_json::from_str(&s).ok())
}
pub(super) fn listen(
    target: web_sys::EventTarget,
    event: &str,
    callback: impl FnMut(Event) + 'static,
) {
    let callback = Closure::wrap(Box::new(callback) as Box<dyn FnMut(Event)>);
    let _ = target.add_event_listener_with_callback(event, callback.as_ref().unchecked_ref());
    runtime()
        .borrow_mut()
        .listeners
        .push((target, event.into(), callback));
}
#[wasm_bindgen]
pub fn shutdown() {
    theme_sweep::stop(false);
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
pub(super) async fn connect() {
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
