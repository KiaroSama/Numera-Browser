use crate::{
    conversion::{truth, value_text},
    fingerprint::{GenerateOptions, generate},
    identity,
    models::Profile,
    proxy::Proxy,
    storage::archive_paths::random_hex,
    validation,
};
use serde_json::{Value, json};

fn first(record: &Value, names: &[&str], default: Value) -> Value {
    for name in names {
        let mut node = record;
        for part in name.split('.') {
            node = node.get(part).unwrap_or(&Value::Null);
        }
        if !node.is_null() && node != "" && node != &json!([]) && node != &json!({}) {
            return node.clone();
        }
    }
    default
}
fn usable(value: Value, default: Value, field: &str, issues: &mut Vec<String>) -> Value {
    if validation::kind(&value) == validation::kind(&default) {
        return value;
    }
    issues.push(format!(
        "{field} in this export was {}, not {}; the generated value was used instead - check it",
        validation::kind(&value),
        validation::kind(&default)
    ));
    default
}
fn hardware(value: Value, default: i64) -> i64 {
    value_text(&value)
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite() && (0.0..=4096.0).contains(number))
        .map_or(default, |number| number.trunc() as i64)
}
fn chrome(ua: &str) -> String {
    let Some(tail) = ua.split_once("Chrome/").map(|(_, tail)| tail) else {
        return String::new();
    };
    let mut parts = Vec::new();
    for part in tail.split('.').take(4) {
        let digits = part
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>();
        if digits.is_empty() {
            break;
        }
        let ended = digits.len() != part.len();
        parts.push(digits);
        if ended {
            break;
        }
    }
    if parts.is_empty() {
        return String::new();
    }
    while parts.len() < 4 {
        parts.push("0".into());
    }
    parts.join(".")
}
fn proxy(record: &Value) -> Result<(Option<Proxy>, Option<Value>), String> {
    let px = first(
        record,
        &["proxy", "proxyConfig", "user_proxy_config"],
        Value::Null,
    );
    if !px.is_object() {
        return Ok((None, None));
    }
    let host = first(&px, &["host", "ip", "proxy_host", "addr"], Value::Null);
    if !truth(&host) {
        return Ok((None, None));
    }
    let mode = value_text(&first(
        &px,
        &["mode", "type", "proxy_type", "proxy_soft"],
        "".into(),
    ))
    .to_lowercase();
    if ["none", "no_proxy", "direct", ""].contains(&mode.as_str())
        && !truth(&first(&px, &["type", "proxy_type"], Value::Null))
    {
        return Ok((None, None));
    }
    let declared = value_text(&first(&px, &["type", "proxy_type", "mode"], "".into()))
        .trim()
        .to_lowercase();
    let scheme = match declared.as_str() {
        "http" => "http",
        "https" => "https",
        "socks4" | "socks4a" => "socks4",
        "socks5" | "socks5h" | "socks" => "socks5",
        _ => {
            if mode.contains("socks") {
                "socks5"
            } else {
                "http"
            }
        }
    };
    let port = first(&px, &["port", "proxy_port"], "".into());
    let server = format!("{scheme}://{}:{}", value_text(&host), value_text(&port))
        .trim_end_matches(':')
        .to_owned();
    let record = json!({"server":server,"username":first(&px,&["username","user","proxy_user","login"],Value::Null),"password":first(&px,&["password","pass","proxy_password"],Value::Null),"country":first(&px,&["country","geo"],Value::Null)});
    let invalid = ["username", "password", "country"]
        .into_iter()
        .filter(|key| !record[*key].is_null() && !record[*key].is_string())
        .collect::<Vec<_>>();
    let mut safe = record.clone();
    for key in &invalid {
        safe[*key] = Value::Null;
    }
    let proxy = Proxy::from_record(&safe, false).map_err(|error| error.to_string())?;
    let mut dashboard = json!({});
    for (engine, client) in [
        ("username", "user"),
        ("password", "pass"),
        ("country", "country"),
    ] {
        if invalid.contains(&engine) {
            dashboard[client] = record[engine].clone();
        }
    }
    Ok((Some(proxy), Some(dashboard)))
}
/// Vendor import generation is explicit at this pure seam; HTTP supplies OS entropy.
pub fn profile(record: &Value, seed: i64, now: f64) -> Result<(Value, Vec<String>), String> {
    let (_, dashboard, issues) = parse(record, seed, now)?;
    Ok((dashboard, issues))
}
pub fn model(record: &Value, seed: i64, now: f64) -> Result<(Profile, Vec<String>), String> {
    let (mut model, dashboard, issues) = parse(record, seed, now)?;
    let mut original = serde_json::to_value(&model).map_err(|error| error.to_string())?;
    for (field, dashboard_field) in [
        ("timezone", "timezone"),
        ("webgl_vendor", "webglVendor"),
        ("webgl_renderer", "webglRenderer"),
    ] {
        original["fingerprint"][field] = dashboard[dashboard_field].clone();
    }
    original["notes"] = dashboard["notes"].clone();
    original["tags"] = dashboard["tags"].clone();
    if original["proxy"].is_object() {
        for (field, dashboard_field) in [
            ("username", "user"),
            ("password", "pass"),
            ("country", "country"),
        ] {
            let value = &dashboard["proxy"][dashboard_field];
            if !value.is_null() && !value.is_string() {
                original["proxy"][field] = value.clone();
            }
        }
    }
    let problems = validation::profile_problems(&original, "engine record")
        .map_err(|error| error.to_string())?;
    if !problems.is_empty() {
        return Err(problems.into_iter().take(8).collect::<Vec<_>>().join("; "));
    }
    model.notes = dashboard["notes"]
        .as_str()
        .ok_or("Imported notes are not text")?
        .into();
    model.tags =
        serde_json::from_value(dashboard["tags"].clone()).map_err(|error| error.to_string())?;
    Ok((model, issues))
}
fn parse(record: &Value, seed: i64, now: f64) -> Result<(Profile, Value, Vec<String>), String> {
    let mut issues = Vec::new();
    let ua = first(
        record,
        &[
            "navigator.userAgent",
            "navigator.user_agent",
            "user_agent",
            "ua",
            "useragent",
            "fingerprint.ua",
            "fingerprint.userAgent",
            "browser.userAgent",
            "userAgent",
        ],
        "".into(),
    );
    let ua = usable(
        if truth(&ua) { ua } else { "".into() },
        "".into(),
        "the user agent",
        &mut issues,
    );
    let ua = ua.as_str().ok_or("Imported user agent is not text")?;
    let os = first(
        record,
        &["os", "os_type", "osType", "navigator.platform"],
        Value::Null,
    );
    let text = format!(
        "{} {ua}",
        if truth(&os) {
            value_text(&os)
        } else {
            String::new()
        }
    )
    .to_lowercase();
    let os = if ["ios", "iphone", "ipad", "ipod"]
        .iter()
        .any(|token| text.contains(token))
    {
        "ios"
    } else if text.contains("android") {
        "android"
    } else if ["mac", "os x", "darwin"]
        .iter()
        .any(|token| text.contains(token))
    {
        "macos"
    } else if text.contains("linux") {
        "linux"
    } else {
        "windows"
    };
    let mut fp = generate(GenerateOptions {
        seed,
        os: Some(os),
        ..Default::default()
    })
    .map_err(|error| error.to_string())?;
    let resolution = value_text(&first(
        record,
        &[
            "navigator.resolution",
            "screen_resolution",
            "resolution",
            "screen",
        ],
        "".into(),
    ))
    .to_lowercase()
    .replace(' ', "");
    let dimensions = resolution
        .split('x')
        .take(2)
        .map(str::parse::<i64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()
        .filter(|parts| parts.len() == 2)
        .unwrap_or_else(|| vec![1920, 1080]);
    fp.screen_width = dimensions[0];
    fp.screen_height = dimensions[1];
    fp.viewport_width = dimensions[0];
    fp.viewport_height = (dimensions[1]
        - if os == "ios" {
            113
        } else if os == "android" {
            80
        } else {
            88
        })
    .max(1);
    let locale = usable(
        first(
            record,
            &["navigator.language", "language", "locale"],
            fp.locale.clone().into(),
        ),
        fp.locale.clone().into(),
        "the language",
        &mut issues,
    );
    let catalog = crate::devices::catalog().map_err(|error| error.to_string())?;
    if let Some(locale) = locale
        .as_str()
        .filter(|locale| !catalog["catalog"]["LOCALES"][*locale].is_null())
    {
        fp.locale = locale.into();
    }
    let zone = &catalog["catalog"]["LOCALES"][&fp.locale];
    let timezone = first(
        record,
        &[
            "timezone.timezone",
            "timezone.id",
            "timezone",
            "time_zone",
            "fingerprint.timezone",
        ],
        zone[0].clone(),
    );
    fp.timezone = timezone.as_str().unwrap_or("").into();
    fp.languages = zone[1]
        .as_array()
        .ok_or("Locale languages missing")?
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if !ua.is_empty() {
        fp.user_agent = ua.into();
    }
    let version = chrome(ua);
    if !version.is_empty() {
        fp.chrome_version = version;
    }
    fp.hardware_concurrency = hardware(
        first(
            record,
            &[
                "navigator.hardwareConcurrency",
                "hardware_concurrency",
                "cores",
            ],
            fp.hardware_concurrency.into(),
        ),
        fp.hardware_concurrency,
    );
    fp.device_memory = hardware(
        first(
            record,
            &["navigator.deviceMemory", "device_memory", "memory"],
            fp.device_memory.clone(),
        ),
        fp.device_memory.as_i64().unwrap_or(0),
    )
    .into();
    let vendor = first(
        record,
        &[
            "webGLMetadata.vendor",
            "webgl.vendor",
            "webGL.vendor",
            "graphic.vendor",
            "webgl_vendor",
            "unmaskedVendor",
            "fingerprint.webgl_vendor",
        ],
        fp.webgl_vendor.clone().into(),
    );
    let renderer = first(
        record,
        &[
            "webGLMetadata.renderer",
            "webgl.renderer",
            "webGL.renderer",
            "graphic.renderer",
            "webgl_renderer",
            "unmaskedRenderer",
            "fingerprint.webgl_renderer",
        ],
        fp.webgl_renderer.clone().into(),
    );
    fp.webgl_vendor = vendor.as_str().unwrap_or("").into();
    fp.webgl_renderer = renderer.as_str().unwrap_or("").into();
    // Imported models use dataclass signal defaults, not the generator's noise choices.
    fp.webrtc = "altered".into();
    fp.canvas = "noise".into();
    fp.audio = "noise".into();
    fp.font_mode = "masked".into();
    fp.geolocation = "prompt".into();
    fp.do_not_track = false;
    fp.client_rects = "noise".into();
    fp.webgl_image = "noise".into();
    fp.media_mode = "masked".into();
    fp.speakers = 1;
    fp.os_version = String::new();
    fp.device_model = String::new();
    let mut tags = first(record, &["tags"], json!([]));
    if !truth(&tags) {
        tags = json!([]);
    }
    if let Some(text) = tags.as_str() {
        tags = json!(
            text.split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .collect::<Vec<_>>()
        );
    }
    tags = usable(tags, json!([]), "the tag list", &mut issues);
    let group = first(record, &["group_name", "group", "folder"], Value::Null);
    if truth(&group) && !tags.as_array().is_some_and(|tags| tags.contains(&group)) {
        tags.as_array_mut()
            .ok_or("Imported tags are not a list")?
            .push(value_text(&group).into());
    }
    let notes = first(record, &["notes", "description"], "".into());
    let (proxy, proxy_view) = proxy(record)?;
    let mut model=Profile::from_record(&json!({"id":random_hex(6).map_err(|error|error.to_string())?,"name":first(record,&["name","title","profile_name"],"imported profile".into()),"fingerprint":fp,"proxy":null,"notes":"","tags":[],"created_at":now,"updated_at":now})).map_err(|error|error.to_string())?;
    model.proxy = proxy;
    issues.extend(identity::validate(&model.fingerprint).map_err(|error| error.to_string())?);
    let mut dashboard = crate::conversion::to_dashboard(&model);
    if let Some(proxy) = proxy_view
        && let Some(fields) = proxy.as_object()
    {
        for (key, value) in fields {
            dashboard["proxy"][key] = value.clone();
        }
    }
    dashboard["notes"] = if truth(&notes) { notes } else { "".into() };
    dashboard["tags"] = tags;
    dashboard["timezone"] = timezone;
    dashboard["webglVendor"] = vendor;
    dashboard["webglRenderer"] = renderer;
    dashboard
        .as_object_mut()
        .ok_or("Imported dashboard shape missing")?
        .remove("_new");
    Ok((model, dashboard, issues))
}
