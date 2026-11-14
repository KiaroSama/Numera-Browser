//! Document-start identity patches. All patch decisions and algorithms live in
//! Rust; JavaScript functions below are wasm-bindgen closures, never source text.

use js_sys::{Array, Function, Object, Proxy, Reflect};
use std::{collections::HashSet, rc::Rc};
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};

type ResultValue = Result<JsValue, JsValue>;

fn get(object: &JsValue, key: &str) -> JsValue {
    if object.is_null() || object.is_undefined() {
        return JsValue::UNDEFINED;
    }
    let boxed;
    let receiver = if object.is_object() || object.is_function() {
        object
    } else {
        boxed = call(
            &get(&js_sys::global(), "Object"),
            &JsValue::UNDEFINED,
            &array(std::slice::from_ref(object)),
        )
        .unwrap_or(JsValue::UNDEFINED);
        &boxed
    };
    Reflect::get(receiver, &key.into()).unwrap_or(JsValue::UNDEFINED)
}
fn set(object: &JsValue, key: &str, value: &JsValue) -> Result<(), JsValue> {
    if Reflect::set(object, &key.into(), value)? {
        Ok(())
    } else {
        Err("Identity property is not writable".into())
    }
}
fn array(values: &[JsValue]) -> Array {
    values.iter().cloned().collect()
}
fn call(function: &JsValue, receiver: &JsValue, args: &Array) -> ResultValue {
    Reflect::apply(
        function
            .dyn_ref::<Function>()
            .ok_or_else(|| JsValue::from_str("Identity target is not callable"))?,
        receiver,
        args,
    )
}
fn method(receiver: &JsValue, key: &str, args: &[JsValue]) -> ResultValue {
    call(&get(receiver, key), receiver, &array(args))
}
fn truth(value: &JsValue) -> bool {
    value.is_truthy()
}
fn text(value: &JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            call(
                &get(&js_sys::global(), "String"),
                &JsValue::UNDEFINED,
                &array(std::slice::from_ref(value)),
            )
            .ok()
            .and_then(|value| value.as_string())
        })
        .unwrap_or_default()
}
fn number(value: &JsValue) -> f64 {
    value
        .as_f64()
        .or_else(|| {
            call(
                &get(&js_sys::global(), "Number"),
                &JsValue::UNDEFINED,
                &array(std::slice::from_ref(value)),
            )
            .ok()
            .and_then(|value| value.as_f64())
        })
        .unwrap_or(f64::NAN)
}
fn uint(value: f64) -> u32 {
    if value.is_finite() {
        value.trunc().rem_euclid(4294967296.0) as u32
    } else {
        0
    }
}
fn length(value: &JsValue) -> u32 {
    uint(number(&get(value, "length")))
}
fn object(fields: &[(&str, JsValue)]) -> JsValue {
    let out = Object::new();
    for (key, value) in fields {
        let _ = set(out.as_ref(), key, value);
    }
    out.into()
}
fn descriptor(object: &JsValue, key: &str) -> JsValue {
    Reflect::get_own_property_descriptor::<JsValue>(object.unchecked_ref(), &key.into())
        .unwrap_or(JsValue::UNDEFINED)
}
fn define(object: &JsValue, key: &str, value: &JsValue) -> Result<(), JsValue> {
    if Reflect::define_property::<JsValue>(
        object.unchecked_ref(),
        &key.into(),
        value.unchecked_ref(),
    )? {
        Ok(())
    } else {
        Err("Identity property cannot be configured".into())
    }
}
fn delete(object: &JsValue, key: &str) {
    let _ = Reflect::delete_property::<JsValue>(object.unchecked_ref(), &key.into());
}
fn prototype(global: &JsValue, name: &str) -> JsValue {
    get(&get(global, name), "prototype")
}
fn copy_array(value: &JsValue) -> ResultValue {
    method(value, "slice", &[])
}
fn callback<F>(function: F) -> JsValue
where
    F: FnMut(JsValue) -> ResultValue + 'static,
{
    // JS owns these closures. wasm-bindgen's weak finalization releases Rust
    // captures when the corresponding function becomes unreachable. Installed
    // prototype methods remain rooted by their page realm until navigation.
    Closure::wrap(Box::new(function) as Box<dyn FnMut(JsValue) -> ResultValue>).into_js_value()
}

struct Context {
    config: JsValue,
    global: JsValue,
    masked: JsValue,
    target: JsValue,
}
impl Context {
    fn inert_target(&self, name: &str, arity: u32) -> JsValue {
        // A bound native callable supplies a non-constructable target without
        // authoring or evaluating a JavaScript function body. Each target has
        // independent metadata; changing it never mutates a browser intrinsic.
        let Ok(target) = method(&self.target, "bind", &[JsValue::UNDEFINED]) else {
            return self.target.clone();
        };
        let _ = define(
            &target,
            "name",
            &object(&[("value", name.into()), ("configurable", true.into())]),
        );
        let _ = define(
            &target,
            "length",
            &object(&[("value", arity.into()), ("configurable", true.into())]),
        );
        target
    }
    fn cfg(&self, key: &str) -> JsValue {
        get(&self.config, key)
    }
    fn mode(&self, key: &str) -> String {
        text(&self.cfg(key))
    }
    fn seed(&self) -> u32 {
        uint(number(&self.cfg("seed")))
    }
    fn mask(&self, function: &JsValue, name: &str) {
        let _ = method(&self.masked, "set", &[function.clone(), name.into()]);
    }
    fn wrap<F>(&self, target: &JsValue, name: &str, function: F) -> JsValue
    where
        F: FnMut(JsValue, JsValue, Array) -> ResultValue + 'static,
    {
        let closure = Closure::wrap(
            Box::new(function) as Box<dyn FnMut(JsValue, JsValue, Array) -> ResultValue>
        )
        .into_js_value();
        let handler = object(&[("apply", closure)]);
        let proxy: JsValue = Proxy::new(target, handler.unchecked_ref()).into();
        self.mask(&proxy, name);
        proxy
    }
    fn getter<F>(&self, owner: &JsValue, key: &str, function: F) -> Result<(), JsValue>
    where
        F: FnMut(JsValue, JsValue, Array) -> ResultValue + 'static,
    {
        let getter = self.wrap(&self.inert_target("", 0), &format!("get {key}"), function);
        define(
            owner,
            key,
            &object(&[
                ("get", getter),
                ("configurable", true.into()),
                ("enumerable", true.into()),
            ]),
        )
    }
    fn constant(&self, owner: &JsValue, key: &str, value: JsValue) {
        let _ = self.getter(owner, key, move |_, _, _| Ok(value.clone()));
    }
}

#[derive(Clone)]
struct Random(u32);
impl Random {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0 as f64 / 4294967296.0
    }
    fn hex(&mut self, count: usize) -> String {
        (0..count)
            .map(|_| b"0123456789abcdef"[(self.next() * 16.0).floor() as usize] as char)
            .collect()
    }
}
fn geometry_seed(seed: u32, width: &JsValue, height: &JsValue) -> u32 {
    seed ^ uint(number(width) * 73856093.0) ^ uint(number(height) * 19349663.0)
}

/// Apply synchronously in the page's main realm before any page script runs.
/// Optional browser surfaces are patched independently, as with the prior
/// document-start implementation; unavailable APIs do not prevent later patches.
#[wasm_bindgen]
pub fn apply_identity(config: JsValue) -> Result<(), JsValue> {
    if !config.is_object() {
        return Err("Identity configuration must be an object".into());
    }
    let global: JsValue = js_sys::global().into();
    let target = get(&prototype(&global, "Function"), "toString");
    let weak_map = get(&global, "WeakMap");
    let masked: JsValue =
        Reflect::construct::<fn() -> JsValue>(weak_map.unchecked_ref(), &Array::new())?;
    let context = Rc::new(Context {
        config,
        global,
        masked,
        target,
    });
    let c = context.clone();
    let handler = Closure::wrap(Box::new(
        move |target: JsValue, receiver: JsValue, args: Array| -> ResultValue {
            if truth(&method(&c.masked, "has", std::slice::from_ref(&receiver))?) {
                let name = text(&method(&c.masked, "get", &[receiver])?);
                Ok(format!("function {name}() {{ [native code] }}").into())
            } else {
                call(&target, &receiver, &args)
            }
        },
    )
        as Box<dyn FnMut(JsValue, JsValue, Array) -> ResultValue>)
    .into_js_value();
    let proxy: JsValue = Proxy::new(
        &context.target,
        object(&[("apply", handler)]).unchecked_ref(),
    )
    .into();
    set(&prototype(&context.global, "Function"), "toString", &proxy)?;
    navigator(&context);
    let _ = user_agent(&context);
    let _ = chrome(&context);
    let _ = plugins(&context);
    let _ = permissions(&context);
    for name in ["WebGLRenderingContext", "WebGL2RenderingContext"] {
        let _ = webgl(&context, name);
    }
    let _ = canvas(&context);
    let _ = fonts(&context);
    let mut random = Random::new(context.seed());
    let _ = media(&context, &mut random);
    let _ = rects(&context, &mut random);
    let _ = audio(&context);
    let nav = prototype(&context.global, "Navigator");
    let c = context.clone();
    let getter = context.wrap(
        &context.inert_target("doNotTrack", 0),
        "doNotTrack",
        move |_, _, _| {
            Ok(if truth(&c.cfg("doNotTrack")) {
                "1".into()
            } else {
                JsValue::NULL
            })
        },
    );
    let _ = define(
        &nav,
        "doNotTrack",
        &object(&[("get", getter), ("configurable", true.into())]),
    );
    let _ = webrtc(&context);
    Ok(())
}

fn navigator(context: &Rc<Context>) {
    let nav = get(&context.global, "navigator");
    let proto: JsValue = Object::get_prototype_of(&nav).into();
    delete(&proto, "webdriver");
    delete(&nav, "webdriver");
    let proto = prototype(&context.global, "Navigator");
    context.constant(&proto, "webdriver", false.into());
    for key in ["platform", "hardwareConcurrency", "maxTouchPoints"] {
        context.constant(&proto, key, context.cfg(key));
    }
    let c = context.clone();
    let _ = context.getter(&proto, "languages", move |_, _, _| {
        let languages = copy_array(&c.cfg("languages"))?;
        method(&get(&c.global, "Object"), "freeze", &[languages])
    });
    if truth(&context.cfg("deviceMemory")) {
        context.constant(&proto, "deviceMemory", context.cfg("deviceMemory"));
    } else {
        delete(&proto, "deviceMemory");
        delete(&nav, "deviceMemory");
    }
    if number(&context.cfg("maxTouchPoints")) > 0.0 {
        if !Reflect::has(&context.global, &"ontouchstart".into()).unwrap_or(false) {
            let _ = define(
                &context.global,
                "ontouchstart",
                &object(&[
                    ("value", JsValue::NULL),
                    ("writable", true.into()),
                    ("configurable", true.into()),
                ]),
            );
        }
    } else {
        delete(&context.global, "ontouchstart");
    }
    context.constant(
        &context.global,
        "devicePixelRatio",
        context.cfg("devicePixelRatio"),
    );
}
fn brands(context: &Context, full: bool) -> JsValue {
    let input = context.cfg("uaBrands");
    let out = Array::new();
    for i in 0..length(&input) {
        let brand = get(&get(&input, &i.to_string()), "brand");
        let version = if full {
            if brand.as_string().as_deref() == Some("Not-A.Brand") {
                "99.0.0.0".into()
            } else {
                context.cfg("fullVersion")
            }
        } else {
            get(&get(&input, &i.to_string()), "version")
        };
        out.push(&object(&[("brand", brand), ("version", version)]));
    }
    out.into()
}
fn user_agent(context: &Rc<Context>) -> Result<(), JsValue> {
    if !truth(&get(&context.global, "NavigatorUAData")) {
        return Ok(());
    }
    if !truth(&context.cfg("hasUaData")) {
        delete(&prototype(&context.global, "Navigator"), "userAgentData");
        return Ok(());
    }
    let proto = prototype(&context.global, "NavigatorUAData");
    let c = context.clone();
    context.getter(&proto, "brands", move |_, _, _| Ok(brands(&c, false)))?;
    context.constant(&proto, "mobile", context.cfg("isMobile"));
    context.constant(&proto, "platform", context.cfg("uaPlatform"));
    let original = get(&proto, "getHighEntropyValues");
    let c = context.clone();
    let function = context.wrap(
        &original,
        "getHighEntropyValues",
        move |target, receiver, args| {
            let output = call(&target, &receiver, &array(&[args.get(0)]))?;
            let promise: JsValue = js_sys::Promise::resolve(&output).into();
            let recover = callback(|_| Ok(Object::new().into()));
            let promise = method(&promise, "catch", &[recover])?;
            let c = c.clone();
            let hints = args.get(0);
            let then = callback(move |base| {
                let out: JsValue = Object::new().into();
                method(&get(&c.global, "Object"), "assign", &[out.clone(), base])?;
                set(&out, "brands", &brands(&c, false))?;
                set(&out, "mobile", &c.cfg("isMobile"))?;
                set(&out, "platform", &c.cfg("uaPlatform"))?;
                let want = if Array::is_array(&hints) {
                    hints.clone()
                } else {
                    Array::new().into()
                };
                for (hint, key) in [
                    ("platformVersion", "platformVersion"),
                    ("architecture", "architecture"),
                    ("uaFullVersion", "fullVersion"),
                ] {
                    if truth(&method(&want, "includes", &[hint.into()])?) {
                        set(&out, hint, &c.cfg(key))?;
                    }
                }
                for (hint, value) in [
                    ("bitness", "64".into()),
                    (
                        "model",
                        if truth(&c.cfg("deviceModel")) {
                            c.cfg("deviceModel")
                        } else {
                            "".into()
                        },
                    ),
                    ("fullVersionList", brands(&c, true)),
                    ("wow64", false.into()),
                    (
                        "formFactors",
                        array(&[if truth(&c.cfg("isMobile")) {
                            "Mobile".into()
                        } else {
                            "Desktop".into()
                        }])
                        .into(),
                    ),
                ] {
                    if truth(&method(&want, "includes", &[hint.into()])?) {
                        set(&out, hint, &value)?;
                    }
                }
                Ok(out)
            });
            method(&promise, "then", &[then])
        },
    );
    set(&proto, "getHighEntropyValues", &function)
}
fn chrome(context: &Context) -> Result<(), JsValue> {
    if !truth(&get(&context.global, "chrome")) {
        set(&context.global, "chrome", &Object::new().into())?;
    }
    let chrome = get(&context.global, "chrome");
    if !truth(&get(&chrome, "runtime")) {
        set(&chrome, "runtime", &Object::new().into())?;
    }
    if !truth(&get(&chrome, "app")) {
        set(&chrome, "app", &object(&[("isInstalled", false.into())]))?;
    }
    Ok(())
}
fn collection(context: &Context, items: Vec<JsValue>, proto: &JsValue, key: &str) -> ResultValue {
    let out: JsValue = Object::new().into();
    for (i, value) in items.iter().enumerate() {
        set(&out, &i.to_string(), value)?;
    }
    define(
        &out,
        "length",
        &object(&[
            ("value", (items.len() as u32).into()),
            ("enumerable", false.into()),
        ]),
    )?;
    let by_index = items.clone();
    let function = context.wrap(&context.inert_target("", 1), "item", move |_, _, args| {
        let key = text(&args.get(0));
        Ok(key
            .parse::<usize>()
            .ok()
            .filter(|i| i.to_string() == key)
            .and_then(|i| by_index.get(i).cloned())
            .unwrap_or(JsValue::NULL))
    });
    set(&out, "item", &function)?;
    let key = key.to_owned();
    let function = context.wrap(
        &context.inert_target("", 1),
        "namedItem",
        move |_, _, args| {
            Ok(items
                .iter()
                .find(|value| Object::is(&get(value, &key), &args.get(0)))
                .cloned()
                .unwrap_or(JsValue::NULL))
        },
    );
    set(&out, "namedItem", &function)?;
    Reflect::set_prototype_of::<JsValue>(out.unchecked_ref(), proto.unchecked_ref())?;
    Ok(out)
}
fn plugins(context: &Rc<Context>) -> Result<(), JsValue> {
    let navigator = get(&context.global, "navigator");
    if length(&get(&navigator, "plugins")) != 0
        || ["PluginArray", "MimeTypeArray", "Plugin", "MimeType"]
            .iter()
            .any(|name| !truth(&get(&context.global, name)))
    {
        return Ok(());
    }
    let mut mimes = Vec::new();
    for mime in ["application/pdf", "text/pdf"] {
        let value = object(&[
            ("type", mime.into()),
            ("suffixes", "pdf".into()),
            ("description", "Portable Document Format".into()),
        ]);
        Reflect::set_prototype_of::<JsValue>(
            value.unchecked_ref(),
            prototype(&context.global, "MimeType").unchecked_ref(),
        )?;
        mimes.push(value);
    }
    let mut plugins = Vec::new();
    for name in [
        "PDF Viewer",
        "Chrome PDF Viewer",
        "Chromium PDF Viewer",
        "Microsoft Edge PDF Viewer",
        "WebKit built-in PDF",
    ] {
        // Native Plugin getters are read-only. Install the synthetic own fields
        // before adopting Plugin.prototype, exactly as the source did.
        let value = collection(
            context,
            mimes.clone(),
            &prototype(&context.global, "Object"),
            "type",
        )?;
        for (key, text) in [
            ("name", name),
            ("filename", "internal-pdf-viewer"),
            ("description", "Portable Document Format"),
        ] {
            set(&value, key, &text.into())?;
        }
        Reflect::set_prototype_of::<JsValue>(
            value.unchecked_ref(),
            prototype(&context.global, "Plugin").unchecked_ref(),
        )?;
        plugins.push(value);
    }
    let plugins = collection(
        context,
        plugins,
        &prototype(&context.global, "PluginArray"),
        "name",
    )?;
    let mimes = collection(
        context,
        mimes,
        &prototype(&context.global, "MimeTypeArray"),
        "type",
    )?;
    let proto = prototype(&context.global, "Navigator");
    context.constant(&proto, "plugins", plugins);
    context.constant(&proto, "mimeTypes", mimes);
    Ok(())
}
fn permissions(context: &Rc<Context>) -> Result<(), JsValue> {
    let permissions = get(&get(&context.global, "navigator"), "permissions");
    let original = get(&permissions, "query");
    if !original.is_function() {
        return Ok(());
    }
    let c = context.clone();
    let bound_receiver = permissions.clone();
    let function = context.wrap(&original, "query", move |target, _, args| {
        let params = args.get(0);
        if truth(&params) && get(&params, "name").as_string().as_deref() == Some("notifications") {
            Ok(js_sys::Promise::resolve(&object(&[
                ("state", get(&get(&c.global, "Notification"), "permission")),
                ("onchange", JsValue::NULL),
            ]))
            .into())
        } else {
            call(&target, &bound_receiver, &array(&[params]))
        }
    });
    set(&permissions, "query", &function)
}
fn webgl(context: &Rc<Context>, name: &str) -> Result<(), JsValue> {
    let proto = prototype(&context.global, name);
    if !truth(&proto) {
        return Ok(());
    }
    let c = context.clone();
    let original = get(&proto, "getParameter");
    let function = context.wrap(
        &original,
        "getParameter",
        move |target, receiver, args| match args.get(0).as_f64() {
            Some(37445.0) => Ok(c.cfg("webglVendor")),
            Some(37446.0) => Ok(c.cfg("webglRenderer")),
            _ => call(&target, &receiver, &array(&[args.get(0)])),
        },
    );
    set(&proto, "getParameter", &function)?;
    let original = get(&proto, "readPixels");
    if context.mode("webglImage") == "real" || !original.is_function() {
        return Ok(());
    }
    let seed = context.seed();
    let function = context.wrap(&original, "readPixels", move |target, receiver, args| {
        let out = call(&target, &receiver, &args)?;
        let pixels = args.get(6);
        if truth(&pixels) {
            let mut random = Random::new(geometry_seed(seed, &args.get(2), &args.get(3)));
            for i in (0..length(&pixels)).step_by(4) {
                if random.next() < 0.02 {
                    let key = i.to_string();
                    let _ = set(
                        &pixels,
                        &key,
                        &(uint(number(&get(&pixels, &key))) ^ 1).into(),
                    );
                }
            }
        }
        Ok(out)
    });
    set(&proto, "readPixels", &function)
}

struct Canvas {
    context: Rc<Context>,
    get_image: JsValue,
}
impl Canvas {
    fn blank(&self, canvas: &JsValue) -> ResultValue {
        let copy = method(
            &get(&self.context.global, "document"),
            "createElement",
            &["canvas".into()],
        )?;
        for key in ["width", "height"] {
            let value = get(canvas, key);
            set(&copy, key, &if truth(&value) { value } else { 1.into() })?;
        }
        Ok(copy)
    }
    fn noisy(&self, canvas: &JsValue) -> Result<Option<JsValue>, JsValue> {
        if self.context.mode("canvas") == "block" {
            return self.blank(canvas).map(Some);
        }
        let ctx = method(canvas, "getContext", &["2d".into()])?;
        let mut is_gl = false;
        if !truth(&ctx) && self.context.mode("webglImage") != "real" {
            for kind in ["webgl", "webgl2", "experimental-webgl"] {
                if truth(&method(canvas, "getContext", &[kind.into()])?) {
                    is_gl = true;
                    break;
                }
            }
        }
        let width = get(canvas, "width");
        let height = get(canvas, "height");
        if !(truth(&ctx) || is_gl) || !truth(&width) || !truth(&height) {
            return Ok(None);
        }
        let copy = self.blank(canvas)?;
        let ctx = method(&copy, "getContext", &["2d".into()])?;
        method(&ctx, "drawImage", &[canvas.clone(), 0.into(), 0.into()])?;
        let image = call(
            &self.get_image,
            &ctx,
            &array(&[0.into(), 0.into(), width.clone(), height.clone()]),
        )?;
        let pixels = get(&image, "data");
        let mut random = Random::new(geometry_seed(self.context.seed(), &width, &height));
        for i in (0..length(&pixels)).step_by(4) {
            if random.next() < 0.02 {
                for offset in 0..3 {
                    let key = (i + offset).to_string();
                    set(
                        &pixels,
                        &key,
                        &(uint(number(&get(&pixels, &key))) ^ uint(random.next() * 2.0)).into(),
                    )?;
                }
            }
        }
        method(&ctx, "putImageData", &[image, 0.into(), 0.into()])?;
        Ok(Some(copy))
    }
}
fn canvas(context: &Rc<Context>) -> Result<(), JsValue> {
    if context.mode("canvas") == "real" {
        return Ok(());
    }
    let proto = prototype(&context.global, "HTMLCanvasElement");
    let ctx_proto = prototype(&context.global, "CanvasRenderingContext2D");
    let original_get = get(&ctx_proto, "getImageData");
    let canvas = Rc::new(Canvas {
        context: context.clone(),
        get_image: original_get.clone(),
    });
    for name in ["toDataURL", "toBlob"] {
        let original = get(&proto, name);
        if !original.is_function() {
            continue;
        }
        let canvas = canvas.clone();
        let function = context.wrap(&original, name, move |target, receiver, args| {
            if let Ok(Some(copy)) = canvas.noisy(&receiver)
                && let Ok(out) = call(&target, &copy, &args)
            {
                return Ok(out);
            }
            call(&target, &receiver, &args)
        });
        set(&proto, name, &function)?;
    }
    if original_get.is_function() {
        let function = context.wrap(
            &original_get,
            "getImageData",
            move |target, receiver, args| {
                if let Ok(Some(copy)) = canvas.noisy(&get(&receiver, "canvas"))
                    && let Ok(ctx) = method(&copy, "getContext", &["2d".into()])
                    && let Ok(out) = call(&target, &ctx, &args)
                {
                    return Ok(out);
                }
                call(&target, &receiver, &args)
            },
        );
        set(&ctx_proto, "getImageData", &function)?;
    }
    Ok(())
}

const GENERIC_FONTS: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-sans-serif",
    "ui-serif",
    "ui-monospace",
    "ui-rounded",
    "-apple-system",
];
fn font_prefix(spec: &str) -> &str {
    let trimmed = spec.trim_start_matches(crate::webrtc_rules::ecma_whitespace);
    let end = trimmed
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'))
        .unwrap_or(trimmed.len());
    if end > 0 && trimmed[end..].starts_with(crate::webrtc_rules::ecma_whitespace) {
        trimmed[end..].trim_start_matches(crate::webrtc_rules::ecma_whitespace)
    } else {
        spec
    }
}
fn first_family(spec: &str) -> String {
    spec.split(',')
        .next()
        .unwrap_or("")
        .replace(['\'', '"'], "")
        .trim_matches(crate::webrtc_rules::ecma_whitespace)
        .to_lowercase()
}
fn hidden_font(spec: &str, installed: &HashSet<String>) -> bool {
    let family = first_family(spec);
    !family.is_empty() && !GENERIC_FONTS.contains(&family.as_str()) && !installed.contains(&family)
}
fn fallback(spec: &str) -> String {
    let remaining = spec.split(',').skip(1).collect::<Vec<_>>().join(",");
    let remaining = remaining.trim_matches(crate::webrtc_rules::ecma_whitespace);
    if remaining.is_empty() {
        "sans-serif".into()
    } else {
        remaining.into()
    }
}
fn fonts(context: &Rc<Context>) -> Result<(), JsValue> {
    let fonts = get(&get(&context.global, "document"), "fonts");
    let check = get(&fonts, "check");
    let configured = context.cfg("fonts");
    if context.mode("fontMode") == "real" || length(&configured) == 0 || !check.is_function() {
        return Ok(());
    }
    let installed: Rc<HashSet<String>> = Rc::new(
        (0..length(&configured))
            .map(|i| text(&get(&configured, &i.to_string())).to_lowercase())
            .collect(),
    );
    let available = installed.clone();
    let font_set = fonts.clone();
    let function = context.wrap(&check, "check", move |target, _, args| {
        let family = first_family(font_prefix(&text(&args.get(0))));
        if GENERIC_FONTS.contains(&family.as_str()) {
            call(&target, &font_set, &array(&[args.get(0), args.get(1)]))
        } else {
            Ok(available.contains(&family).into())
        }
    });
    set(&fonts, "check", &function)?;
    let proto = prototype(&context.global, "HTMLElement");
    for prop in ["offsetWidth", "offsetHeight"] {
        let d = descriptor(&proto, prop);
        let getter = get(&d, "get");
        if !getter.is_function() {
            continue;
        }
        let available = installed.clone();
        let function = context.wrap(&getter, prop, move |target, receiver, _| {
            let style = get(&receiver, "style");
            let previous = get(&style, "fontFamily");
            let spec = text(&previous);
            if truth(&previous)
                && hidden_font(&spec, &available)
                && set(&style, "fontFamily", &fallback(&spec).into()).is_ok()
                && let Ok(value) = call(&target, &receiver, &Array::new())
                && set(&style, "fontFamily", &previous).is_ok()
            {
                return Ok(value);
            }
            call(&target, &receiver, &Array::new())
        });
        define(
            &proto,
            prop,
            &object(&[
                ("get", function),
                ("enumerable", get(&d, "enumerable")),
                ("configurable", true.into()),
            ]),
        )?;
    }
    let proto = prototype(&context.global, "CanvasRenderingContext2D");
    let original = get(&proto, "measureText");
    if original.is_function() {
        let function = context.wrap(&original, "measureText", move |target, receiver, args| {
            let previous = get(&receiver, "font");
            let spec = if truth(&previous) {
                text(&previous)
            } else {
                String::new()
            };
            let family = font_prefix(&spec);
            if hidden_font(family, &installed) {
                let replacement = spec.replacen(family, &fallback(family), 1);
                if set(&receiver, "font", &replacement.into()).is_ok()
                    && let Ok(value) = call(&target, &receiver, &array(&[args.get(0)]))
                    && set(&receiver, "font", &previous).is_ok()
                {
                    return Ok(value);
                }
            }
            call(&target, &receiver, &array(&[args.get(0)]))
        });
        set(&proto, "measureText", &function)?;
    }
    Ok(())
}
fn media(context: &Rc<Context>, random: &mut Random) -> Result<(), JsValue> {
    if context.mode("mediaMode") == "real" {
        return Ok(());
    }
    let media = get(&get(&context.global, "navigator"), "mediaDevices");
    let original = get(&media, "enumerateDevices");
    if !original.is_function() {
        return Ok(());
    }
    let values: JsValue = Reflect::construct::<fn() -> JsValue>(
        get(&context.global, "WeakMap").unchecked_ref(),
        &Array::new(),
    )?;
    let proto = prototype(&context.global, "MediaDeviceInfo");
    for key in ["deviceId", "kind", "label", "groupId"] {
        let d = descriptor(&proto, key);
        let getter = get(&d, "get");
        if !getter.is_function() {
            continue;
        }
        let values = values.clone();
        let function = context.wrap(
            &getter,
            &format!("get {key}"),
            move |target, receiver, _| {
                let own = method(&values, "get", std::slice::from_ref(&receiver))?;
                if truth(&own) {
                    Ok(get(&own, key))
                } else {
                    call(&target, &receiver, &Array::new())
                }
            },
        );
        define(
            &proto,
            key,
            &object(&[
                ("get", function),
                ("enumerable", get(&d, "enumerable")),
                ("configurable", true.into()),
            ]),
        )?;
    }
    let devices = Array::new();
    for (key, kind) in [
        ("cameras", "videoinput"),
        ("microphones", "audioinput"),
        ("speakers", "audiooutput"),
    ] {
        let count = number(&context.cfg(key));
        let mut i = 0.0;
        while i < count {
            let base = if kind != "audiooutput" && truth(&get(&context.global, "InputDeviceInfo")) {
                prototype(&context.global, "InputDeviceInfo")
            } else {
                proto.clone()
            };
            let device = method(&get(&context.global, "Object"), "create", &[base])?;
            let own = object(&[
                ("deviceId", random.hex(64).into()),
                ("kind", kind.into()),
                ("label", "".into()),
                ("groupId", random.hex(64).into()),
            ]);
            method(&values, "set", &[device.clone(), own])?;
            devices.push(&device);
            i += 1.0;
        }
    }
    let function = context.wrap(&original, "enumerateDevices", move |_, _, _| {
        Ok(js_sys::Promise::resolve(&copy_array(devices.as_ref())?).into())
    });
    set(&media, "enumerateDevices", &function)
}
fn shift(context: &Context, rect: JsValue, jitter: f64) -> ResultValue {
    if !truth(&rect) {
        return Ok(rect);
    }
    let mut fields = Vec::new();
    for key in [
        "x", "y", "width", "height", "top", "right", "bottom", "left",
    ] {
        let value = get(&rect, key);
        let value = match value.as_f64() {
            Some(v) if v != 0.0 => JsValue::from_f64(v + v * jitter),
            _ => value,
        };
        fields.push((key, value));
    }
    for (key, value) in fields {
        let function = context.wrap(
            &context.inert_target("", 0),
            &format!("get {key}"),
            move |_, _, _| Ok(value.clone()),
        );
        let _ = define(
            &rect,
            key,
            &object(&[("get", function), ("configurable", true.into())]),
        );
    }
    Ok(rect)
}
fn rects(context: &Rc<Context>, random: &mut Random) -> Result<(), JsValue> {
    if context.mode("clientRects") == "real" {
        return Ok(());
    }
    let jitter = (random.next() - 0.5) * 0.0002;
    for (owner, name) in [
        ("Element", "getBoundingClientRect"),
        ("Element", "getClientRects"),
        ("Range", "getBoundingClientRect"),
    ] {
        let proto = prototype(&context.global, owner);
        let original = get(&proto, name);
        if !original.is_function() {
            continue;
        }
        let c = context.clone();
        let function = context.wrap(&original, name, move |target, receiver, args| {
            let rect = call(&target, &receiver, &args)?;
            if name == "getClientRects" {
                for i in 0..length(&rect) {
                    shift(&c, get(&rect, &i.to_string()), jitter)?;
                }
                Ok(rect)
            } else {
                shift(&c, rect, jitter)
            }
        });
        set(&proto, name, &function)?;
    }
    Ok(())
}
fn audio(context: &Rc<Context>) -> Result<(), JsValue> {
    if context.mode("audio") == "real" {
        return Ok(());
    }
    let proto = prototype(&context.global, "AnalyserNode");
    let original = get(&proto, "getFloatFrequencyData");
    if original.is_function() {
        let seed = context.seed();
        let function = context.wrap(
            &original,
            "getFloatFrequencyData",
            move |target, receiver, args| {
                let out = call(&target, &receiver, &args)?;
                let data = args.get(0);
                let mut random = Random::new(seed ^ 0x5eed1);
                for i in (0..length(&data)).step_by(100) {
                    let key = i.to_string();
                    set(
                        &data,
                        &key,
                        &(number(&get(&data, &key)) + (random.next() - 0.5) * 1e-7).into(),
                    )?;
                }
                Ok(out)
            },
        );
        set(&proto, "getFloatFrequencyData", &function)?;
    }
    let proto = prototype(&context.global, "AudioBuffer");
    let original = get(&proto, "getChannelData");
    if original.is_function() {
        let noised: JsValue = Reflect::construct::<fn() -> JsValue>(
            get(&context.global, "WeakSet").unchecked_ref(),
            &Array::new(),
        )?;
        let seed = context.seed();
        let function = context.wrap(
            &original,
            "getChannelData",
            move |target, receiver, args| {
                let channel = args.get(0);
                let data = call(&target, &receiver, &array(std::slice::from_ref(&channel)))?;
                if !truth(&method(&noised, "has", std::slice::from_ref(&data))?) {
                    method(&noised, "add", std::slice::from_ref(&data))?;
                    let mut random = Random::new(seed ^ 0x5eed2 ^ uint(number(&channel)));
                    for i in (0..length(&data)).step_by(500) {
                        let key = i.to_string();
                        set(
                            &data,
                            &key,
                            &(number(&get(&data, &key)) + (random.next() - 0.5) * 1e-7).into(),
                        )?;
                    }
                }
                Ok(data)
            },
        );
        set(&proto, "getChannelData", &function)?;
    }
    Ok(())
}

fn scrub_sdp(value: JsValue) -> JsValue {
    match value.as_string() {
        Some(sdp) => crate::webrtc_rules::scrub_sdp(&sdp).into(),
        None => value,
    }
}
fn drop_candidate(event: &JsValue) -> bool {
    let line = get(&get(event, "candidate"), "candidate");
    if truth(&line) {
        crate::webrtc_rules::drop_candidate(Some(&text(&line)))
    } else {
        crate::webrtc_rules::drop_candidate(None)
    }
}
fn proxy<F>(target: &JsValue, function: F) -> JsValue
where
    F: FnMut(JsValue, JsValue, Array) -> ResultValue + 'static,
{
    let closure =
        Closure::wrap(Box::new(function) as Box<dyn FnMut(JsValue, JsValue, Array) -> ResultValue>)
            .into_js_value();
    Proxy::new(target, object(&[("apply", closure)]).unchecked_ref()).into()
}
fn listener(function: &JsValue) -> JsValue {
    proxy(function, |target, receiver, args| {
        if drop_candidate(&args.get(0)) {
            Ok(JsValue::UNDEFINED)
        } else {
            call(&target, &receiver, &args)
        }
    })
}
fn description(context: &Context, desc: JsValue) -> ResultValue {
    if !truth(&desc) {
        return Ok(desc);
    }
    let kind = get(&desc, "type");
    let sdp = scrub_sdp(get(&desc, "sdp"));
    let out = object(&[("type", kind.clone()), ("sdp", sdp.clone())]);
    let function = proxy(&context.inert_target("toJSON", 0), move |_, _, _| {
        Ok(object(&[("type", kind.clone()), ("sdp", sdp.clone())]))
    });
    set(&out, "toJSON", &function)?;
    Ok(out)
}
fn webrtc(context: &Rc<Context>) -> Result<(), JsValue> {
    let mode = crate::webrtc_rules::mode(&context.mode("webrtc"));
    if mode == crate::webrtc_rules::Mode::Disabled {
        for key in [
            "RTCPeerConnection",
            "webkitRTCPeerConnection",
            "RTCDataChannel",
            "mozRTCPeerConnection",
        ] {
            if !Reflect::delete_property::<JsValue>(context.global.unchecked_ref(), &key.into())
                .unwrap_or(false)
            {
                let _ = set(&context.global, key, &JsValue::UNDEFINED);
            }
        }
        return Ok(());
    }
    let proto = prototype(&context.global, "RTCPeerConnection");
    if mode == crate::webrtc_rules::Mode::Real || !truth(&proto) {
        return Ok(());
    }
    let wrapped: JsValue = Reflect::construct::<fn() -> JsValue>(
        get(&context.global, "WeakMap").unchecked_ref(),
        &Array::new(),
    )?;
    let original = get(&proto, "addEventListener");
    let map = wrapped.clone();
    let function = context.wrap(
        &original,
        "addEventListener",
        move |target, receiver, args| {
            let callback = args.get(1);
            if args.get(0).as_string().as_deref() == Some("icecandidate") && callback.is_function()
            {
                let mut wrapped = method(&map, "get", std::slice::from_ref(&callback))?;
                if !truth(&wrapped) {
                    wrapped = listener(&callback);
                    method(&map, "set", &[callback, wrapped.clone()])?;
                }
                call(
                    &target,
                    &receiver,
                    &array(&[args.get(0), wrapped, args.get(2)]),
                )
            } else {
                call(&target, &receiver, &args)
            }
        },
    );
    set(&proto, "addEventListener", &function)?;
    let original = get(&proto, "removeEventListener");
    let function = context.wrap(
        &original,
        "removeEventListener",
        move |target, receiver, args| {
            if args.get(0).as_string().as_deref() == Some("icecandidate")
                && truth(&method(&wrapped, "has", &[args.get(1)])?)
            {
                call(
                    &target,
                    &receiver,
                    &array(&[
                        args.get(0),
                        method(&wrapped, "get", &[args.get(1)])?,
                        args.get(2),
                    ]),
                )
            } else {
                call(&target, &receiver, &args)
            }
        },
    );
    set(&proto, "removeEventListener", &function)?;
    let d = descriptor(&proto, "onicecandidate");
    let setter = get(&d, "set");
    if setter.is_function() {
        let handlers: JsValue = Reflect::construct::<fn() -> JsValue>(
            get(&context.global, "WeakMap").unchecked_ref(),
            &Array::new(),
        )?;
        let get_handlers = handlers.clone();
        let getter = proxy(&context.inert_target("get", 0), move |_, receiver, _| {
            let value = method(&get_handlers, "get", &[receiver])?;
            Ok(if truth(&value) { value } else { JsValue::NULL })
        });
        let setter = proxy(&setter, move |target, receiver, args| {
            let callback = args.get(0);
            method(&handlers, "set", &[receiver.clone(), callback.clone()])?;
            call(
                &target,
                &receiver,
                &array(&[if callback.is_function() {
                    listener(&callback)
                } else {
                    callback
                }]),
            )
        });
        define(
            &proto,
            "onicecandidate",
            &object(&[
                ("get", getter),
                ("set", setter),
                ("enumerable", get(&d, "enumerable")),
                ("configurable", true.into()),
            ]),
        )?;
    }
    for name in [
        "localDescription",
        "currentLocalDescription",
        "pendingLocalDescription",
    ] {
        let d = descriptor(&proto, name);
        let getter = get(&d, "get");
        if !getter.is_function() {
            continue;
        }
        let c = context.clone();
        let function = proxy(&getter, move |target, receiver, _| {
            description(&c, call(&target, &receiver, &Array::new())?)
        });
        define(
            &proto,
            name,
            &object(&[
                ("get", function),
                ("enumerable", get(&d, "enumerable")),
                ("configurable", true.into()),
            ]),
        )?;
    }
    for name in ["createOffer", "createAnswer"] {
        let original = get(&proto, name);
        if !original.is_function() {
            continue;
        }
        let function = context.wrap(&original, name, move |target, receiver, args| {
            let out = call(&target, &receiver, &args)?;
            if truth(&out) && get(&out, "then").is_function() {
                let then = callback(|desc| {
                    Ok(if truth(&desc) && truth(&get(&desc, "sdp")) {
                        object(&[
                            ("type", get(&desc, "type")),
                            ("sdp", scrub_sdp(get(&desc, "sdp"))),
                        ])
                    } else {
                        desc
                    })
                });
                method(&out, "then", &[then])
            } else {
                Ok(out)
            }
        });
        set(&proto, name, &function)?;
    }
    Ok(())
}
