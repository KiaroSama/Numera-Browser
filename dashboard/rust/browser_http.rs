use super::*;

pub(super) fn network(e: JsValue) -> ApiError {
    ApiError {
        message: e.as_string().unwrap_or_else(|| format!("{e:?}")),
        status: 0,
        kind: String::new(),
    }
}
pub(super) fn fail(s: impl Into<String>) -> ApiError {
    ApiError {
        message: s.into(),
        status: 0,
        kind: String::new(),
    }
}
pub(super) fn call(target: &JsValue, name: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f = Reflect::get(target, &JsValue::from_str(name))?.dyn_into::<Function>()?;
    f.apply(target, &args.iter().cloned().collect::<Array>())
}
pub(super) fn token_headers(token: &str) -> Result<web_sys::Headers, ApiError> {
    let h = web_sys::Headers::new().map_err(network)?;
    if !token.is_empty() {
        h.set("Authorization", &format!("Bearer {token}"))
            .map_err(network)?;
    }
    Ok(h)
}
pub(super) struct Deadline {
    pub(super) controller: AbortController,
    id: i32,
    _callback: Closure<dyn FnMut()>,
}
impl Deadline {
    pub(super) fn new(ms: i32) -> Result<Self, ApiError> {
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
pub(super) async fn send(
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
    let apply = method == "POST" && path == contracts::api_url("/api/updates/apply");
    let preferences = method == "PUT" && path == contracts::api_url("/api/updates/preferences");
    if apply || preferences {
        let a = snapshot();
        if let Some(approval) = a
            .update_bridge
            .as_ref()
            .filter(|_| a.mcp_bridge_command.is_some())
        {
            headers
                .set("X-Numera-Update-Approval", &approval.token)
                .map_err(network)?;
        } else if apply
            || body.is_some_and(|p| {
                p["app_auto_update"] == true || p["browser_core_auto_update"] == true
            })
        {
            return Err(fail(
                "Open the desktop app to authorize this update change.",
            ));
        }
        init.set_redirect(web_sys::RequestRedirect::Error);
        init.set_cache(web_sys::RequestCache::NoStore);
    }
    init.set_headers(&headers);
    if let Some(body) = body {
        init.set_body(&JsValue::from_str(&body.to_string()));
    }
    init.set_signal(Some(&deadline.controller.signal()));
    if path.contains("/cookies/export?") {
        init.set_redirect(web_sys::RequestRedirect::Error);
        init.set_cache(web_sys::RequestCache::NoStore);
    }
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
pub(super) async fn parse(response: &Response) -> Result<Value, ApiError> {
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
pub(super) async fn request(
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, ApiError> {
    let deadline = Deadline::new(
        if (path == "/api/engines/cloak/acquisition" && method == "POST")
            || path.ends_with("/warmup")
            || path.ends_with("/align")
            || path.ends_with("/trust")
            || path.ends_with("/cookies")
            || path.starts_with("/api/updates/apply")
            || path == "/api/fleet/sync"
            || path.contains("/cookies/export?")
        {
            310000
        } else if path.starts_with("/api/updates/check?") {
            100000
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
    let v = if path.contains("/cookies/export?") {
        let bytes = bounded_bytes(
            &response,
            16 * 1024 * 1024,
            "Cookie export exceeded 16 MiB or was unreadable.",
        )
        .await?;
        serde_json::from_slice(&bytes)
            .map_err(|_| fail("The engine returned an invalid cookie export."))?
    } else {
        parse(&response).await?
    };
    if let Some(kind) = contracts::response_kind(method, path) {
        contracts::validate(v, kind).map_err(fail)
    } else {
        Ok(v)
    }
}
pub(super) async fn refresh() -> Result<(), ApiError> {
    let profiles = request("GET", "/api/profiles", None).await?;
    if alive() {
        change(|a| {
            a.profiles = list(&profiles);
            a.ip_checks
                .retain(|id, _| a.profiles.iter().any(|p| p["id"] == *id));
        });
    }
    Ok(())
}
pub(super) async fn refresh_config() -> Result<(), ApiError> {
    let cfg = request("GET", "/api/config", None).await?;
    if alive() {
        change(|a| a.set_config(cfg));
    }
    Ok(())
}
pub(super) async fn health() -> &'static str {
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

struct StreamReader(JsValue);
impl Drop for StreamReader {
    fn drop(&mut self) {
        let _ = call(&self.0, "cancel", &[]);
        let _ = call(&self.0, "releaseLock", &[]);
    }
}
async fn bounded_bytes(
    response: &Response,
    limit: usize,
    message: &str,
) -> Result<Vec<u8>, ApiError> {
    let stream = response.body().ok_or_else(|| fail(message))?;
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
        if bytes.len() + chunk.length() as usize > limit {
            return Err(fail(message));
        }
        bytes.extend(chunk.to_vec());
    }
    Ok(bytes)
}
pub(super) async fn run_update(action: &str, id: &str) -> Result<(), ApiError> {
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
        let bytes = bounded_bytes(&response, 65536, "Invalid update status").await?;
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
