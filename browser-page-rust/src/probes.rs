//! DOM/network diagnostics translated from the six original browser probes.
use js_sys::{Array, Function, Object, Promise, Reflect};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, future_to_promise};

fn get(target: &JsValue, key: &str) -> Result<JsValue, JsValue> {
    if target.is_object() || target.is_function() || target.is_null() || target.is_undefined() {
        Reflect::get(target, &JsValue::from_str(key))
    } else {
        // Ordinary JavaScript property access boxes primitives; Reflect.get
        // itself does not. Null/undefined deliberately retain their TypeError.
        let constructor: Function =
            Reflect::get(&js_sys::global(), &"Object".into())?.dyn_into()?;
        let boxed = constructor.call1(&JsValue::UNDEFINED, target)?;
        Reflect::get(&boxed, &JsValue::from_str(key))
    }
}
fn set(target: &JsValue, key: &str, value: &JsValue) -> Result<(), JsValue> {
    Reflect::set(target, &JsValue::from_str(key), value).map(|_| ())
}
fn truth(value: &JsValue) -> bool {
    value.is_truthy()
}
fn or(values: impl IntoIterator<Item = JsValue>) -> JsValue {
    values.into_iter().find(truth).unwrap_or(JsValue::NULL)
}
fn string(value: &JsValue) -> Result<String, JsValue> {
    let constructor: Function = get(&js_sys::global(), "String")?.dyn_into()?;
    constructor
        .call1(&JsValue::UNDEFINED, value)?
        .as_string()
        .ok_or_else(|| JsValue::from_str("String conversion failed"))
}
fn contains_ascii_case(text: &str, needle: &str) -> bool {
    text.as_bytes()
        .windows(needle.len())
        .any(|part| part.eq_ignore_ascii_case(needle.as_bytes()))
}
fn call(target: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let method: Function = get(target, method)?.dyn_into()?;
    method.apply(target, &args.iter().cloned().collect::<Array>())
}
fn window() -> Result<web_sys::Window, JsValue> {
    // Firefox evaluates through a sandbox whose global is not instanceof
    // Window. The source reads its actual `window` binding in that realm.
    let window = get(&js_sys::global(), "window")?;
    if window.is_null() || window.is_undefined() {
        Err(JsValue::from_str("Window unavailable"))
    } else {
        Ok(window.unchecked_into())
    }
}
fn document() -> Result<web_sys::Document, JsValue> {
    window()?
        .document()
        .ok_or_else(|| JsValue::from_str("Document unavailable"))
}
fn error(message: &str) -> Result<JsValue, JsValue> {
    let out = Object::new();
    set(&out, "error", &message.into())?;
    Ok(out.into())
}
fn iterable(value: &JsValue) -> Result<Vec<JsValue>, JsValue> {
    iterator(value)?.collect()
}
fn iterator(value: &JsValue) -> Result<js_sys::IntoIter, JsValue> {
    js_sys::try_iter(value)?
        .ok_or_else(|| JsValue::from(js_sys::TypeError::new("Value is not iterable")))
}
fn properties_or(properties: &[(&JsValue, &str)]) -> Result<JsValue, JsValue> {
    for (target, key) in properties {
        let value = get(target, key)?;
        if truth(&value) {
            return Ok(value);
        }
    }
    Ok(JsValue::NULL)
}
fn fetch(url: &JsValue, no_cors: bool) -> Result<Promise, JsValue> {
    let options = Object::new();
    set(&options, "cache", &"no-store".into())?;
    if no_cors {
        set(&options, "mode", &"no-cors".into())?;
    }
    call(&window()?.into(), "fetch", &[url.clone(), options.into()])?.dyn_into()
}
async fn response(url: &JsValue, no_cors: bool, method: &str) -> Result<JsValue, JsValue> {
    let response = JsFuture::from(fetch(url, no_cors)?).await?;
    JsFuture::from(call(&response, method, &[])?.dyn_into::<Promise>()?).await
}
fn trim(value: &JsValue) -> Result<JsValue, JsValue> {
    call(value, "trim", &[])
}
fn replace(value: &JsValue, token: &str, replacement: &JsValue) -> Result<JsValue, JsValue> {
    call(value, "replace", &[token.into(), replacement.clone()])
}

#[wasm_bindgen]
pub fn location_host() -> Result<JsValue, JsValue> {
    get(&get(window()?.as_ref(), "location")?, "host")
}

/// The actual Camoufox host screen and WebGL pair used to condition model
/// selection. This is separate from the profile identity's reported values.
#[wasm_bindgen]
pub fn host_environment() -> Result<JsValue, JsValue> {
    let canvas = call(&document()?.into(), "createElement", &["canvas".into()])?;
    let gl = call(&canvas, "getContext", &["webgl".into()])?;
    let extension = if truth(&gl) {
        call(&gl, "getExtension", &["WEBGL_debug_renderer_info".into()])?
    } else {
        JsValue::NULL
    };
    let gpu = if truth(&extension) {
        Array::of2(
            &call(
                &gl,
                "getParameter",
                &[get(&extension, "UNMASKED_VENDOR_WEBGL")?],
            )?,
            &call(
                &gl,
                "getParameter",
                &[get(&extension, "UNMASKED_RENDERER_WEBGL")?],
            )?,
        )
        .into()
    } else {
        JsValue::NULL
    };
    let screen = get(window()?.as_ref(), "screen")?;
    let out = Object::new();
    set(&out, "width", &get(&screen, "width")?)?;
    set(&out, "height", &get(&screen, "height")?)?;
    set(&out, "gpu", &gpu)?;
    Ok(out.into())
}

/// The image acceptance phase still executes a compiled browser program.
#[wasm_bindgen]
pub fn smoke_value() -> u32 {
    6 * 7
}

/// Preserve DOM order, duplicates and the post-filter40-link limit. All hrefs
/// are read before filtering, matching the source map/filter/slice sequence.
#[wasm_bindgen]
pub fn internal_links(host: JsValue) -> Result<JsValue, JsValue> {
    let anchors = call(&document()?.into(), "querySelectorAll", &["a[href]".into()])?;
    let hrefs = iterable(&anchors)?
        .into_iter()
        .map(|anchor| get(&anchor, "href"))
        .collect::<Result<Vec<_>, _>>()?;
    let urls: Function = get(&js_sys::global(), "URL")?.dyn_into()?;
    let links = Array::new();
    for href in hrefs {
        if !truth(&call(&href, "startsWith", &["http".into()])?) {
            continue;
        }
        let url = Reflect::construct(&urls, &Array::of1(&href))?;
        if get(&url, "host")? == host {
            links.push(&href);
        }
    }
    call(&links, "slice", &[0.into(), 40.into()])
}

#[wasm_bindgen]
pub async fn inspect_identity() -> Result<JsValue, JsValue> {
    let window = window()?;
    let navigator = get(window.as_ref(), "navigator")?;
    let out = Object::new();
    for (source, destination) in [
        ("userAgent", "userAgent"),
        ("platform", "platform"),
        ("language", "locale"),
    ] {
        set(&out, destination, &get(&navigator, source)?)?;
    }
    let array = get(&js_sys::global(), "Array")?;
    set(
        &out,
        "languages",
        &call(&array, "from", &[get(&navigator, "languages")?])?,
    )?;
    set(&out, "cores", &get(&navigator, "hardwareConcurrency")?)?;
    set(&out, "width", &get(window.as_ref(), "innerWidth")?)?;
    set(&out, "height", &get(window.as_ref(), "innerHeight")?)?;
    let data = get(&navigator, "userAgentData")?;
    let data = if truth(&data) {
        let data = get(&navigator, "userAgentData")?;
        let hints = [
            "platformVersion",
            "architecture",
            "bitness",
            "model",
            "uaFullVersion",
            "fullVersionList",
            "wow64",
        ]
        .into_iter()
        .map(JsValue::from_str)
        .collect::<Array>();
        JsFuture::from(call(&data, "getHighEntropyValues", &[hints.into()])?.dyn_into::<Promise>()?)
            .await?
    } else {
        JsValue::NULL
    };
    set(&out, "userAgentData", &data)?;
    Ok(out.into())
}

/// A single real rendered span, with the exact three baseline families and
/// sample string used by the source probes.
struct FontMeasurement {
    host: JsValue,
    span: JsValue,
    style: JsValue,
    bases: [JsValue; 3],
}
const FONT_BASES: [&str; 3] = ["monospace", "serif", "sans-serif"];
impl FontMeasurement {
    fn new() -> Result<Self, JsValue> {
        let document: JsValue = document()?.into();
        let host = properties_or(&[(&document, "body"), (&document, "documentElement")])?;
        let span = call(&document, "createElement", &["span".into()])?;
        let style = get(&span, "style")?;
        set(
            &style,
            "cssText",
            &"position:absolute;left:-9999px;top:-9999px;font-size:72px;white-space:nowrap;".into(),
        )?;
        set(&span, "textContent", &"mmmmmmmmmmlli WwMiI0129@".into())?;
        call(&host, "appendChild", std::slice::from_ref(&span))?;
        let mut bases = [JsValue::UNDEFINED, JsValue::UNDEFINED, JsValue::UNDEFINED];
        for (index, family) in FONT_BASES.iter().enumerate() {
            set(&style, "fontFamily", &(*family).into())?;
            bases[index] = get(&span, "offsetWidth")?;
        }
        Ok(Self {
            host,
            span,
            style,
            bases,
        })
    }
    fn installed(&self, family: &JsValue) -> Result<bool, JsValue> {
        let family = string(family)?;
        for (index, base) in FONT_BASES.iter().enumerate() {
            set(
                &self.style,
                "fontFamily",
                &format!("\"{family}\",{base}").into(),
            )?;
            if get(&self.span, "offsetWidth")? != self.bases[index] {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn close(self) -> Result<(), JsValue> {
        call(&self.host, "removeChild", &[self.span]).map(|_| ())
    }
}

/// Preserve input order and duplicates; failed measurement returns the partial
/// result collected before the same source catch boundary.
#[wasm_bindgen]
pub async fn font_probe(fonts: JsValue) -> Result<JsValue, JsValue> {
    let out = Array::new();
    let _ = (|| -> Result<(), JsValue> {
        let measurement = FontMeasurement::new()?;
        for font in iterator(&fonts)? {
            let font = font?;
            if measurement.installed(&font)? {
                out.push(&font);
            }
        }
        measurement.close()
    })();
    Ok(out.into())
}

fn native_function(function: &JsValue) -> Result<bool, JsValue> {
    let constructor = get(&js_sys::global(), "Function")?;
    let prototype = get(&constructor, "prototype")?;
    let to_string: Function = get(&prototype, "toString")?.dyn_into()?;
    let text = to_string
        .call0(function)?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Function string unavailable"))?;
    Ok(js_sys::RegExp::new(r"^function [^(]*\(\) \{ \[native code\] \}$", "").test(&text))
}

#[wasm_bindgen]
pub async fn trust_probe(probe: JsValue) -> Result<JsValue, JsValue> {
    let window = window()?;
    let navigator: JsValue = window.navigator().into();
    let out = Object::new();
    for (target, source, destination) in [
        (&navigator, "webdriver", "webdriver"),
        (&navigator, "platform", "platform"),
        (&navigator, "userAgent", "userAgent"),
        (&navigator, "hardwareConcurrency", "cores"),
        (&navigator, "deviceMemory", "memory"),
        (&navigator, "maxTouchPoints", "maxTouchPoints"),
        (window.as_ref(), "devicePixelRatio", "devicePixelRatio"),
    ] {
        set(&out, destination, &get(target, source)?)?;
    }
    let object = get(&js_sys::global(), "Object")?;
    let object_prototype = get(&object, "prototype")?;
    let has_own: Function = get(&object_prototype, "hasOwnProperty")?.dyn_into()?;
    set(
        &out,
        "webdriverOwn",
        &has_own.call1(&navigator, &"webdriver".into())?,
    )?;
    set(
        &out,
        "hasChrome",
        &truth(&get(window.as_ref(), "chrome")?).into(),
    )?;
    set(
        &out,
        "plugins",
        &get(&get(&navigator, "plugins")?, "length")?,
    )?;
    let languages = iterable(&get(&navigator, "languages")?)?
        .into_iter()
        .collect::<Array>();
    set(&out, "languages", &languages.into())?;
    let screen = get(window.as_ref(), "screen")?;
    set(&out, "screenW", &get(&screen, "width")?)?;
    set(&out, "screenH", &get(&screen, "height")?)?;
    let intl = get(&js_sys::global(), "Intl")?;
    let format: Function = get(&intl, "DateTimeFormat")?.dyn_into()?;
    let format = format.call0(&intl)?;
    set(
        &out,
        "timezone",
        &get(&call(&format, "resolvedOptions", &[])?, "timeZone")?,
    )?;
    let gl = (|| -> Result<(JsValue, JsValue), JsValue> {
        let canvas = call(&document()?.into(), "createElement", &["canvas".into()])?;
        let gl = call(&canvas, "getContext", &["webgl".into()])?;
        if !truth(&gl) {
            return Ok((JsValue::NULL, JsValue::NULL));
        }
        if truth(&call(
            &gl,
            "getExtension",
            &["WEBGL_debug_renderer_info".into()],
        )?) {
            Ok((
                call(&gl, "getParameter", &[37445.into()])?,
                call(&gl, "getParameter", &[37446.into()])?,
            ))
        } else {
            Ok((JsValue::NULL, JsValue::NULL))
        }
    })()
    .unwrap_or((JsValue::NULL, JsValue::NULL));
    set(&out, "webglVendor", &gl.0)?;
    set(&out, "webglRenderer", &gl.1)?;
    let permissions = get(&navigator, "permissions")?;
    let to_string_native = native_function(&get(&permissions, "query")?)?;
    let to_string_native = if to_string_native {
        let function = get(&js_sys::global(), "Function")?;
        native_function(&get(&get(&function, "prototype")?, "toString")?)?
    } else {
        false
    };
    let to_string_native = if to_string_native {
        let navigator_ctor = get(&js_sys::global(), "Navigator")?;
        let descriptor = call(
            &object,
            "getOwnPropertyDescriptor",
            &[get(&navigator_ctor, "prototype")?, "webdriver".into()],
        )?;
        native_function(&get(&descriptor, "get")?)?
    } else {
        false
    };
    set(&out, "toStringNative", &to_string_native.into())?;
    let agree = async {
        let descriptor = Object::new();
        set(&descriptor, "name", &"notifications".into())?;
        let status = JsFuture::from(
            call(&permissions, "query", &[descriptor.into()])?.dyn_into::<Promise>()?,
        )
        .await?;
        let notification = get(&js_sys::global(), "Notification")?;
        Ok::<_, JsValue>(get(&status, "state")? == get(&notification, "permission")?)
    }
    .await
    .unwrap_or(false);
    set(&out, "permissionsAgree", &agree.into())?;
    let canvas = (|| -> Result<(bool, bool), JsValue> {
        let canvas = call(&document()?.into(), "createElement", &["canvas".into()])?;
        set(&canvas, "width", &240.into())?;
        set(&canvas, "height", &60.into())?;
        let context = call(&canvas, "getContext", &["2d".into()])?;
        set(&context, "textBaseline", &"top".into())?;
        set(&context, "font", &"14px Arial".into())?;
        set(&context, "fillStyle", &"#f60".into())?;
        call(
            &context,
            "fillRect",
            &[0.into(), 0.into(), 100.into(), 30.into()],
        )?;
        set(&context, "fillStyle", &"#069".into())?;
        call(
            &context,
            "fillText",
            &["numera-probe".into(), 2.into(), 15.into()],
        )?;
        Ok((
            call(&canvas, "toDataURL", &[])? == call(&canvas, "toDataURL", &[])?,
            get(&call(&canvas, "toDataURL", &[])?, "length")?
                .as_f64()
                .is_some_and(|n| n < 100.0),
        ))
    })();
    match canvas {
        Ok((stable, empty)) => {
            set(&out, "canvasStable", &stable.into())?;
            set(&out, "canvasEmpty", &empty.into())?;
        }
        Err(_) => set(&out, "canvasStable", &false.into())?,
    }
    let fonts = (|| -> Result<(bool, bool), JsValue> {
        let measurement = FontMeasurement::new()?;
        let known = measurement.installed(&get(&probe, "knownFont")?)?;
        let alien = measurement.installed(&get(&probe, "alienFont")?)?;
        measurement.close()?;
        Ok((known, alien))
    })();
    match fonts {
        Ok((known, alien)) => {
            set(&out, "fontKnown", &known.into())?;
            set(&out, "fontAlien", &alien.into())?;
        }
        Err(_) => {
            set(&out, "fontKnown", &JsValue::NULL)?;
            set(&out, "fontAlien", &JsValue::NULL)?;
        }
    }
    let devices = async {
        let media = get(&navigator, "mediaDevices")?;
        let devices =
            JsFuture::from(call(&media, "enumerateDevices", &[])?.dyn_into::<Promise>()?).await?;
        let mut cameras = 0;
        let mut microphones = 0;
        for device in iterable(&devices)? {
            let kind = get(&device, "kind")?;
            cameras += usize::from(kind == JsValue::from_str("videoinput"));
            microphones += usize::from(kind == JsValue::from_str("audioinput"));
        }
        Ok::<_, JsValue>((cameras, microphones))
    }
    .await;
    match devices {
        Ok((cameras, microphones)) => {
            set(&out, "cameras", &(cameras as f64).into())?;
            set(&out, "microphones", &(microphones as f64).into())?;
        }
        Err(_) => {
            set(&out, "cameras", &JsValue::NULL)?;
            set(&out, "microphones", &JsValue::NULL)?;
        }
    }
    Ok(out.into())
}

/// Perform service-issued unique DNS lookups concurrently, retaining the
/// source's distinct id-fetch, empty-id and result-fetch error objects.
#[wasm_bindgen]
pub async fn dns_probe(config: JsValue) -> Result<JsValue, JsValue> {
    let id_url = get(&config, "idUrl")?;
    let sub_template = get(&config, "subTmpl")?;
    let result_template = get(&config, "resultTmpl")?;
    let n = get(&config, "n")?;
    let id = match async { trim(&response(&id_url, false, "text").await?) }.await {
        Ok(id) => id,
        Err(_) => return error("could not reach the DNS test service"),
    };
    if !truth(&id) {
        return error("no test id issued");
    }
    let number: Function = get(&js_sys::global(), "Number")?.dyn_into()?;
    let count = number
        .call1(&JsValue::UNDEFINED, &n)?
        .as_f64()
        .unwrap_or(f64::NAN);
    let jobs = Array::new();
    let mut index = 1.0;
    while index <= count {
        let url = replace(&replace(&sub_template, "{i}", &index.into())?, "{id}", &id)?;
        // Creating all fetches before awaiting preserves concurrent resolution.
        // Every rejection is consumed just as source fetch(...).catch(() => {}).
        let pending = fetch(&url, true)?;
        jobs.push(&future_to_promise(async move {
            let _ = JsFuture::from(pending).await;
            Ok(JsValue::UNDEFINED)
        }));
        index += 1.0;
    }
    JsFuture::from(Promise::all(&jobs)).await?;
    let rows = async {
        let url = replace(&result_template, "{id}", &id)?;
        response(&url, false, "json").await
    }
    .await;
    let out = Object::new();
    match rows {
        Ok(rows) => {
            set(&out, "id", &id)?;
            set(&out, "rows", &rows)?;
        }
        Err(_) => {
            set(&out, "error", &"could not read the DNS result".into())?;
            set(&out, "id", &id)?;
        }
    }
    Ok(out.into())
}

/// Try the endpoints sequentially, using only the browser's observed exit.
#[wasm_bindgen]
pub async fn ip_probe(urls: JsValue) -> Result<JsValue, JsValue> {
    for url in iterator(&urls)? {
        let url = url?;
        let result = async {
            let response = JsFuture::from(fetch(&url, false)?).await?;
            if !truth(&get(&response, "ok")?) {
                return Ok(None);
            }
            let text = JsFuture::from(call(&response, "text", &[])?.dyn_into::<Promise>()?).await?;
            let text = trim(&text)?;
            let data = match js_sys::JSON::parse(&string(&text)?) {
                Ok(data) => data,
                Err(_) => {
                    let out = Object::new();
                    set(&out, "ip", &text)?;
                    out.into()
                }
            };
            let ip = properties_or(&[(&data, "ip"), (&data, "query"), (&data, "IPv4")])?;
            if !truth(&ip) {
                return Ok(None);
            }
            let timezone = get(&data, "timezone")?;
            let timezone_id = if timezone.is_null() || timezone.is_undefined() {
                JsValue::UNDEFINED
            } else {
                get(&timezone, "id")?
            };
            let timezone = or([timezone_id, timezone]);
            let out = Object::new();
            set(&out, "source", &url)?;
            set(&out, "ip", &ip)?;
            set(
                &out,
                "country",
                &properties_or(&[
                    (&data, "country_code"),
                    (&data, "country"),
                    (&data, "countryCode"),
                ])?,
            )?;
            set(&out, "city", &properties_or(&[(&data, "city")])?)?;
            set(&out, "timezone", &timezone)?;
            Ok::<_, JsValue>(Some(out.into()))
        }
        .await;
        if let Ok(Some(out)) = result {
            return Ok(out);
        }
    }
    Ok(JsValue::NULL)
}

/// ICE candidate observation owns both callbacks until completion and cancels
/// its timer/handler after the same three-second bound or terminal candidate.
#[wasm_bindgen]
pub async fn webrtc_probe() -> Result<JsValue, JsValue> {
    let config = Object::new();
    let server = Object::new();
    set(&server, "urls", &"stun:stun.l.google.com:19302".into())?;
    set(&config, "iceServers", &Array::of1(&server).into())?;
    let constructor =
        get(&js_sys::global(), "RTCPeerConnection").and_then(|value| value.dyn_into::<Function>());
    let peer =
        constructor.and_then(|constructor| Reflect::construct(&constructor, &Array::of1(&config)));
    let out = Object::new();
    let Ok(peer) = peer else {
        set(&out, "supported", &false.into())?;
        set(&out, "ips", &Array::new().into())?;
        return Ok(out.into());
    };
    let found = Rc::new(RefCell::new(Vec::<String>::new()));
    let resolve_slot = Rc::new(RefCell::new(None::<Function>));
    let done_peer = peer.clone();
    let done_slot = resolve_slot.clone();
    let done_found = found.clone();
    let done = Rc::new(move || {
        let _ = call(&done_peer, "close", &[]);
        if let Some(resolve) = done_slot.borrow_mut().take() {
            let values = done_found
                .borrow()
                .iter()
                .map(|value| JsValue::from_str(value))
                .collect::<Array>();
            let _ = resolve.call1(&JsValue::UNDEFINED, &values.into());
        }
    });
    let pending = Promise::new(&mut {
        let resolve_slot = resolve_slot.clone();
        move |resolve, _reject| {
            *resolve_slot.borrow_mut() = Some(resolve);
        }
    });
    let event_found = found.clone();
    let event_done = done.clone();
    let candidate = Closure::<dyn FnMut(JsValue)>::new(move |event| {
        let Ok(candidate) = get(&event, "candidate") else {
            return;
        };
        if !truth(&candidate) {
            event_done();
            return;
        }
        let value = get(&candidate, "candidate").unwrap_or(JsValue::UNDEFINED);
        let value = if truth(&value) { value } else { "".into() };
        let Ok(value) = string(&value) else {
            return;
        };
        let regex = js_sys::RegExp::new(
            r"([0-9]{1,3}(\.[0-9]{1,3}){3}|[a-f0-9]{1,4}(:[a-f0-9]{0,4}){2,7})",
            "i",
        );
        if let Some(matches) = regex.exec(&value)
            && let Some(value) = matches.get(0).as_string()
            && !value.ends_with(".local")
        {
            let mut found = event_found.borrow_mut();
            if !found.contains(&value) {
                found.push(value);
            }
        }
    });
    if let Err(error) = set(&peer, "onicecandidate", candidate.as_ref()) {
        let _ = set(&peer, "onicecandidate", &JsValue::NULL);
        done();
        return Err(error);
    }
    // The source ignores createOffer/setLocalDescription rejections. Consume
    // those failures without manufacturing a candidate or resolving early.
    let offer = call(&peer, "createDataChannel", &["probe".into()])
        .and_then(|_| call(&peer, "createOffer", &[]))
        .and_then(|value| value.dyn_into::<Promise>());
    if let Ok(offer) = offer {
        let offer_peer = peer.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(offer) = JsFuture::from(offer).await
                && let Ok(pending) = call(&offer_peer, "setLocalDescription", &[offer])
                && let Ok(pending) = pending.dyn_into::<Promise>()
            {
                let _ = JsFuture::from(pending).await;
            }
        });
    } else {
        done();
    }
    let timer_done = done.clone();
    let timer = Closure::<dyn FnMut()>::new(move || timer_done());
    let window = window()?;
    let timer_id = match window
        .set_timeout_with_callback_and_timeout_and_arguments_0(timer.as_ref().unchecked_ref(), 3000)
    {
        Ok(id) => id,
        Err(error) => {
            let _ = set(&peer, "onicecandidate", &JsValue::NULL);
            done();
            return Err(error);
        }
    };
    let ips = JsFuture::from(pending).await;
    window.clear_timeout_with_handle(timer_id);
    let _ = set(&peer, "onicecandidate", &JsValue::NULL);
    drop(candidate);
    drop(timer);
    set(&out, "supported", &true.into())?;
    set(&out, "ips", &ips?)?;
    Ok(out.into())
}

/// Read the selected TLS echo page. Invalid JSON remains a null observation;
/// invalid host property operations still reject the Promise as in the source.
#[wasm_bindgen]
pub async fn tls_probe() -> Result<JsValue, JsValue> {
    let document: JsValue = document()?.into();
    let body = get(&document, "body")?;
    let raw = if truth(&body) {
        string(&get(&body, "innerText")?)?
    } else {
        String::new()
    };
    let Ok(data) = js_sys::JSON::parse(&raw) else {
        return Ok(JsValue::NULL);
    };
    let tls = or([get(&data, "tls")?, data.clone()]);
    let ja3 = properties_or(&[(&tls, "ja3"), (&data, "ja3"), (&data, "fingerprint")])?;
    let ja3_hash = properties_or(&[
        (&tls, "ja3_hash"),
        (&data, "ja3_hash"),
        (&data, "hash"),
        (&data, "ja3_digest"),
    ])?;
    let mut ja4 = properties_or(&[(&tls, "ja4"), (&data, "ja4")])?;
    if !truth(&ja4) {
        let raw = get(&data, "ja4_r")?;
        if truth(&raw) {
            ja4 = JsValue::from_str(string(&raw)?.split('_').next().unwrap_or_default());
        }
    }
    let mut cipher_count = JsValue::from_f64(0.0);
    if truth(&ja3) {
        let includes: Function = get(&ja3, "includes")?.dyn_into()?;
        if truth(&includes.call1(&ja3, &JsValue::from_str(","))?) {
            let fields = call(&ja3, "split", &[",".into()])?;
            let field = get(&fields, "1")?;
            let field = if truth(&field) { field } else { "".into() };
            let fields = call(&field, "split", &["-".into()])?;
            cipher_count = (iterable(&fields)?
                .iter()
                .filter(|value| truth(value))
                .count() as f64)
                .into();
        } else {
            let ciphers = get(&tls, "ciphers")?;
            if truth(&ciphers) {
                cipher_count = get(&ciphers, "length")?;
            }
        }
    } else {
        let ciphers = get(&tls, "ciphers")?;
        if truth(&ciphers) {
            cipher_count = get(&ciphers, "length")?;
        }
    }
    let grease_word = contains_ascii_case(&raw, "grease");
    let grease_hex = raw.as_bytes().windows(6).any(|part| {
        part[0] == b'0'
            && part[1].eq_ignore_ascii_case(&b'x')
            && part[2].is_ascii_hexdigit()
            && part[3].eq_ignore_ascii_case(&b'a')
            && part[4].is_ascii_hexdigit()
            && part[5].eq_ignore_ascii_case(&b'a')
    });
    let http2 = get(&data, "http2")?;
    let http2 = if truth(&http2) {
        properties_or(&[
            (&http2, "akamai_fingerprint_hash"),
            (&http2, "akamai_fingerprint"),
        ])?
    } else {
        JsValue::NULL
    };
    let out = Object::new();
    for (key, value) in [
        ("ja3", ja3),
        ("ja3Hash", ja3_hash),
        ("ja4", ja4),
        ("http2", http2),
        ("grease", JsValue::from_bool(grease_word || grease_hex)),
        (
            "greaseKnown",
            JsValue::from_bool(grease_word || contains_ascii_case(&raw, "cipher")),
        ),
        ("cipherCount", cipher_count),
    ] {
        set(&out, key, &value)?;
    }
    Ok(out.into())
}
