use crate::{
    devices::catalog,
    models::{Fingerprint, Profile},
    proxy::Proxy,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{error::Error, fmt};

#[derive(Debug)]
pub struct ConversionError(String);
impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Error for ConversionError {}
fn text(v: &Value, default: &str) -> String {
    v.as_str().unwrap_or(default).into()
}
pub(crate) fn truth(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64().is_none_or(|v| v != 0.0),
    }
}
pub(crate) fn value_text(v: &Value) -> String {
    match v {
        Value::String(v) => v.clone(),
        Value::Null => "None".into(),
        Value::Bool(v) => if *v { "True" } else { "False" }.into(),
        _ => v.to_string(),
    }
}
fn integer_number(value: &Value) -> Option<Value> {
    let integer = match value {
        Value::String(text) => {
            let integer = crate::decimal::integer(text)?;
            if integer.trim_start_matches('-').len() > 4300 {
                return None;
            }
            integer
        }
        Value::Number(number) => {
            let text = number.to_string();
            if !text.contains(['.', 'e', 'E']) {
                text
            } else {
                format!(
                    "{:.0}",
                    number.as_f64().filter(|value| value.is_finite())?.trunc()
                )
            }
        }
        _ => return None,
    };
    let negative = integer.starts_with('-');
    let digits = integer.trim_start_matches('-').trim_start_matches('0');
    let normalized = if digits.is_empty() {
        "0".into()
    } else if negative {
        format!("-{digits}")
    } else {
        digits.to_owned()
    };
    normalized
        .parse::<serde_json::Number>()
        .ok()
        .map(Value::Number)
}
fn finite_number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => crate::browser::cookie_expiry::numeric_text(text),
        _ => None,
    }
    .filter(|number| number.is_finite())
}
fn number(v: &Value, default: f64, whole: bool) -> Value {
    if whole {
        return integer_number(v).unwrap_or_else(|| json!(default as i64));
    }
    let n = finite_number(v).unwrap_or(default);
    if n.fract() == 0.0 {
        integer_number(&json!(n)).expect("Finite whole conversion number")
    } else {
        json!(n)
    }
}
fn trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
}
fn lines(v: &Value) -> Vec<String> {
    let parts = if let Some(v) = v.as_str() {
        // Empty CRLF segments are discarded below, along with blank source lines.
        v.split(|c| {
            matches!(
                c,
                '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                    ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
        })
        .map(str::to_owned)
        .collect()
    } else {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    parts
        .into_iter()
        .map(|v| trim(&v).to_owned())
        .filter(|v| !v.is_empty())
        .collect()
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or(String::new(), |c| {
        c.to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect()
    })
}
fn dash_os(os: &str) -> &str {
    match os {
        "macos" => "macOS",
        "linux" => "Linux",
        "android" => "Android",
        "ios" => "iOS",
        _ => "Windows",
    }
}
fn engine_os(os: &str) -> &str {
    match os {
        "macOS" => "macos",
        "Linux" => "linux",
        "Android" => "android",
        "iOS" => "ios",
        _ => "windows",
    }
}
fn stable_seed(id: &str) -> u32 {
    let hash = Sha256::digest(id.as_bytes());
    u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]).max(1)
}
fn urls(v: &Value) -> Vec<String> {
    let mut entries = Vec::<String>::new();
    for line in lines(v) {
        if v.is_string() {
            let mut chunks = Vec::<String>::new();
            for chunk in line.split(',') {
                let opaque = chunks.last().is_some_and(|s| {
                    s.split_once("://")
                        .map_or(s.as_str(), |(_, host)| host)
                        .contains(['/', '?', '#'])
                });
                if opaque {
                    if let Some(last) = chunks.last_mut() {
                        last.push(',');
                        last.push_str(chunk);
                    }
                } else {
                    chunks.push(chunk.into());
                }
            }
            entries.extend(chunks);
        } else {
            entries.push(line);
        }
    }
    entries
        .into_iter()
        .map(|v| trim(&v).to_owned())
        .filter(|v| !v.is_empty())
        .map(|v| {
            if !v.contains("://") && !v.starts_with("about:") {
                format!("https://{v}")
            } else {
                v
            }
        })
        .collect()
}
fn bookmarks(v: &Value) -> Vec<Value> {
    lines(v)
        .into_iter()
        .filter_map(|line| {
            let (name, url) = line
                .rsplit_once('|')
                .map_or(("", line.as_str()), |(name, url)| (trim(name), trim(url)));
            if url.is_empty() {
                None
            } else {
                Some(json!({"name":if name.is_empty(){url}else{name},"url":url}))
            }
        })
        .collect()
}

pub fn secret_intent(record: &Value) -> &'static str {
    let Some(proxy) = record.get("proxy") else {
        return "preserve";
    };
    if proxy.is_null() {
        return "remove";
    }
    match proxy.get("pass") {
        None | Some(Value::Null) => "preserve",
        Some(Value::String(v)) if v.is_empty() => "clear",
        _ => "replace",
    }
}

pub fn to_engine(d: &Value, now: f64) -> Result<Profile, ConversionError> {
    let root = catalog().map_err(|_| ConversionError("Cannot parse device catalog".into()))?;
    let c = &root["catalog"];
    let os = engine_os(d["os"].as_str().unwrap_or("Windows"));
    let mobile = os == "ios" || os == "android";
    let sides: Option<Vec<i64>> = text(&d["screen"], "1920x1080")
        .to_lowercase()
        .split('x')
        .map(|value| crate::decimal::integer(value)?.parse::<i64>().ok())
        .collect();
    let (sw, sh) = match sides {
        Some(v) if v.len() == 2 => (v[0], v[1]),
        _ => (1920, 1080),
    };
    let locale = text(&d["locale"], "en-US");
    let language = c["LOCALES"].get(&locale).unwrap_or(&c["LOCALES"]["en-US"]);
    let mut renderer = trim(&text(&d["webglRenderer"], "")).to_owned();
    let mut vendor = trim(&text(&d["webglVendor"], "")).to_owned();
    if renderer.is_empty() {
        renderer = text(&c["WEBGL"][os][0][1], "");
        vendor = text(&c["WEBGL"][os][0][0], "");
    } else if vendor.is_empty() {
        vendor = "Google Inc.".into();
        let lower = renderer.to_lowercase();
        for (token, name) in [
            ("nvidia", "NVIDIA"),
            ("radeon", "AMD"),
            ("amd", "AMD"),
            ("intel", "Intel"),
            ("apple", "Apple"),
            ("adreno", "Qualcomm"),
            ("qualcomm", "Qualcomm"),
            ("mali", "ARM"),
            ("arm", "ARM"),
        ] {
            if lower.contains(token) {
                vendor = format!("Google Inc. ({name})");
                break;
            }
        }
    }
    let version = if truth(&d["osVersion"]) {
        value_text(&d["osVersion"])
    } else {
        text(&c["OS_VERSIONS"][os][0][0], "")
    };
    let model = if truth(&d["deviceModel"]) {
        value_text(&d["deviceModel"])
    } else {
        String::new()
    };
    let chrome = text(&d["browserVersion"], "128.0.0.0");
    let token = text(&c["OS_UA_TOKEN"][os], "")
        .replace("{v}", if version.is_empty() { "14" } else { &version })
        .replace(
            "{v_}",
            &if version.is_empty() {
                "18.5".into()
            } else {
                version.clone()
            }
            .replace('.', "_"),
        )
        .replace(
            "{model}",
            if model.is_empty() { "SM-S918B" } else { &model },
        );
    let ua = if truth(&d["userAgent"]) {
        value_text(&d["userAgent"])
    } else if os == "ios" {
        format!(
            "Mozilla/5.0 ({token}) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/{chrome} Mobile/15E148 Safari/604.1"
        )
    } else {
        format!(
            "Mozilla/5.0 ({token}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chrome}{} Safari/537.36",
            if os == "android" { " Mobile" } else { "" }
        )
    };
    let mut fonts = lines(&d["fontList"]);
    if fonts.is_empty() {
        fonts = strings(&c["FONTS"][os]);
    }
    let id = if truth(&d["id"]) {
        value_text(&d["id"])
    } else {
        "unsaved".into()
    };
    let seed = number(&d["seed"], 0.0, true);
    let seed = if truth(&seed) {
        seed
    } else {
        json!(stable_seed(&id))
    };
    let mode = |key: &str, default: &str| {
        if truth(&d[key]) {
            trim(&value_text(&d[key])).to_lowercase()
        } else {
            default.into()
        }
    };
    let ratio = finite_number(&d["devicePixelRatio"])
        .filter(|value| *value != 0.0)
        .map_or_else(|| c["PIXEL_RATIOS"][os][0].clone(), |value| json!(value));
    let fp=Fingerprint::from_record(&json!({"os":os,"user_agent":ua,"platform":c["NAV_PLATFORM"][os],"ch_platform":c["CH_PLATFORM"][os],"chrome_version":chrome,
        "screen_width":sw,"screen_height":sh,"viewport_width":sw,"viewport_height":(sh-if os=="ios"{113}else if mobile{80}else{88}).max(1),
        "hardware_concurrency":number(&d["cores"],8.0,true),"device_memory":if os=="ios"{json!(0)}else{number(&d["memory"],16.0,false)},
        "fonts":fonts,"cameras":number(&d["mediaVideo"],1.0,true),"microphones":number(&d["mediaAudio"],1.0,true),"speakers":number(&d["mediaSpeakers"],1.0,true),
        "webrtc":mode("webrtc","altered"),"canvas":mode("canvas","noise"),"audio":mode("audio","noise"),"client_rects":mode("clientRects","noise"),"webgl_image":mode("webglImage","noise"),"media_mode":mode("mediaDevices","masked"),"font_mode":mode("fonts","masked"),"geolocation":mode("geo","prompt"),"do_not_track":truth(&d["dnt"]),
        "webgl_vendor":vendor,"webgl_renderer":renderer,"locale":locale,"timezone":if truth(&d["timezone"]){d["timezone"].clone()}else{language[0].clone()},"languages":language[1],"is_mobile":mobile,"os_version":version,"device_model":model,"max_touch_points":number(&d["maxTouchPoints"],c["TOUCH_POINTS"][os][0].as_f64().unwrap_or(0.0),true),
        "device_pixel_ratio":ratio,"seed":seed})).map_err(|_|ConversionError("Converted fingerprint is invalid".into()))?;
    let px = &d["proxy"];
    let proxy = if px.is_object() && truth(&px["host"]) {
        let kind = text(&px["type"], "HTTP").to_uppercase();
        let scheme = match kind.as_str() {
            "HTTP" => "http",
            "HTTPS" => "https",
            "SOCKS4" => "socks4",
            "SOCKS5" => "socks5",
            _ => {
                return Err(ConversionError(format!(
                    "unsupported proxy type '{kind}'; expected one of ['HTTP', 'HTTPS', 'SOCKS4', 'SOCKS5']"
                )));
            }
        };
        let host = value_text(&px["host"]);
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host
        };
        let tail = if truth(&px["port"]) {
            format!(":{}", value_text(&px["port"]))
        } else {
            String::new()
        };
        Some(Proxy::from_record(&json!({"server":format!("{scheme}://{host}{tail}"),"username":if truth(&px["user"]){px["user"].clone()}else{Value::Null},"password":if truth(&px["pass"]){px["pass"].clone()}else{Value::Null},"country":if truth(&px["country"]){px["country"].clone()}else{Value::Null}}),true).map_err(|_|ConversionError("Converted proxy is invalid".into()))?)
    } else {
        None
    };
    let selected = |key: &str, allowed: &[&str]| {
        let values = strings(&d[key]);
        allowed
            .iter()
            .filter(|k| values.iter().any(|v| v == **k))
            .map(|v| (*v).to_owned())
            .collect::<Vec<_>>()
    };
    Ok(Profile {
        id,
        name: d
            .get("name")
            .cloned()
            .unwrap_or(Value::String("profile".into())),
        fingerprint: fp,
        proxy,
        notes: text(&d["notes"], ""),
        tags: strings(&d["tags"]),
        engine: text(&d["engine"], "playwright"),
        startup_urls: urls(&d["startupUrls"]),
        extensions: lines(&d["extensions"]),
        launch_args: lines(&d["launchArgs"]),
        bookmarks: bookmarks(&d["bookmarks"]),
        dns: if truth(&d["dns"]) {
            trim(&value_text(&d["dns"])).into()
        } else {
            String::new()
        },
        ip_based: selected("ipBased", &["timezone", "geolocation", "languages"]),
        clear_on_start: selected(
            "clearOnStart",
            &[
                "history",
                "passwords",
                "cookies",
                "localStorage",
                "indexedDb",
                "sessions",
                "extensionData",
                "bookmarks",
            ],
        ),
        switched_off: selected("switchedOff", &["googleServices", "systemExtensions"]),
        created_at: now,
        updated_at: now,
    })
}

pub fn to_dashboard(profile: &Profile) -> Value {
    let fp = &profile.fingerprint;
    let proxy = profile.proxy.as_ref().filter(|p| !p.server.is_empty()).map_or(Value::Null, |p| {
        let record = p.to_record();
        let server = trim(&p.server);
        let (scheme, hostport) = server.split_once("://").unwrap_or(("http", server));
        let (host, port) = if let Some(value) = hostport.strip_prefix('[') {
            value.split_once(']').map_or((hostport, ""), |(host, tail)| {
                (host, tail.strip_prefix(':').unwrap_or(""))
            })
        } else if hostport.matches(':').count() == 1 {
            hostport.split_once(':').unwrap_or((hostport, ""))
        } else {
            (hostport, "")
        };
        json!({"type":match scheme.to_lowercase().as_str(){"https"=>"HTTPS","socks4"=>"SOCKS4","socks"|"socks5h"|"socks5"=>"SOCKS5",_=>"HTTP"},"host":host,"port":port,"user":record["username"].as_str().unwrap_or(""),"pass":record["password"].as_str().unwrap_or(""),"country":record["country"].as_str().unwrap_or("")})
    });
    let mut output = json!({"name":profile.name,"folder":"Facebook Ads","status":"stopped","os":dash_os(&fp.os),"engine":if profile.engine.is_empty(){"playwright"}else{&profile.engine},"browser":"Chrome","browserVersion":fp.chrome_version,"userAgent":fp.user_agent,"screen":format!("{}x{}",fp.screen_width,fp.screen_height),"cores":fp.hardware_concurrency,"memory":fp.device_memory,"webglVendor":fp.webgl_vendor,"webglRenderer":fp.webgl_renderer});
    let signals = json!({"canvas":capitalize(&fp.canvas),"webrtc":capitalize(&fp.webrtc),"audio":capitalize(&fp.audio),"fonts":capitalize(&fp.font_mode),"clientRects":capitalize(&fp.client_rects),"webglImage":capitalize(&fp.webgl_image),"mediaDevices":capitalize(&fp.media_mode),"locale":fp.locale,"timezone":fp.timezone,"geo":capitalize(&fp.geolocation),"mediaVideo":fp.cameras,"mediaAudio":fp.microphones,"mediaSpeakers":fp.speakers,"fontList":fp.fonts.join("\n"),"osVersion":fp.os_version,"deviceModel":fp.device_model,"maxTouchPoints":fp.max_touch_points,"devicePixelRatio":fp.device_pixel_ratio,"seed":fp.seed,"dnt":fp.do_not_track});
    let extras = json!({"proxy":proxy,"notes":profile.notes,"tags":profile.tags,"startupUrls":profile.startup_urls.join("\n"),"extensions":profile.extensions.join("\n"),"launchArgs":profile.launch_args.join("\n"),"bookmarks":profile.bookmarks.iter().filter(|b|truth(&b["url"])).map(|b|{let url=text(&b["url"],"");let name=text(&b["name"],"");if name.is_empty()||name==url{url}else{format!("{name} | {url}")}}).collect::<Vec<_>>().join("\n"),"dns":profile.dns,"ipBased":profile.ip_based,"clearOnStart":profile.clear_on_start,"switchedOff":profile.switched_off,"lastActive":"never"});
    if let Some(object) = output.as_object_mut() {
        for block in [signals, extras] {
            if let Value::Object(fields) = block {
                object.extend(fields);
            }
        }
    }
    output
}
