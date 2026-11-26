use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const VIEWS: [(&str, &str); 8] = [
    ("profiles", "Profiles"),
    ("health", "Health"),
    ("engines", "Engines"),
    ("extensions", "Extensions"),
    ("proxies", "Proxies"),
    ("folders", "Folders"),
    ("automation", "Automation"),
    ("settings", "Settings"),
];
pub const ENGINES: [(&str, &str); 4] = [
    ("cloak", "CloakBrowser"),
    ("camoufox", "Camoufox"),
    ("patchright", "Patchright"),
    ("playwright", "Playwright"),
];
pub const OS: [&str; 4] = ["Windows", "macOS", "Linux", "Android"];
pub const LOCALES: [(&str, &str); 10] = [
    ("en-US", "America/New_York"),
    ("en-GB", "Europe/London"),
    ("en-PK", "Asia/Karachi"),
    ("en-IN", "Asia/Kolkata"),
    ("de-DE", "Europe/Berlin"),
    ("fr-FR", "Europe/Paris"),
    ("es-ES", "Europe/Madrid"),
    ("tr-TR", "Europe/Istanbul"),
    ("it-IT", "Europe/Rome"),
    ("nl-BE", "Europe/Brussels"),
];
pub const COUNTRIES: [&str; 10] = ["US", "GB", "PK", "IN", "DE", "FR", "ES", "TR", "IT", "BE"];
pub const VERSIONS: [&str; 4] = ["125.0.0.0", "126.0.0.0", "127.0.0.0", "128.0.0.0"];
pub const PRESETS: [(&str, &str); 6] = [
    ("general", "General"),
    ("ads", "Ads / media"),
    ("ecommerce", "E-commerce"),
    ("crypto", "Crypto"),
    ("social", "Social"),
    ("news", "News"),
];
pub fn text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => v.to_string(),
    }
}
pub fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
pub fn number(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap_or(0)
}
pub fn list(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}
pub fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
pub fn country_locale(country: &str) -> &str {
    match country {
        "GB" | "UK" => "en-GB",
        "PK" => "en-PK",
        "IN" => "en-IN",
        "DE" => "de-DE",
        "FR" => "fr-FR",
        "ES" => "es-ES",
        "TR" => "tr-TR",
        "IT" => "it-IT",
        "BE" => "nl-BE",
        _ => "en-US",
    }
}
pub fn zone(locale: &str) -> &str {
    LOCALES
        .iter()
        .find(|(l, _)| *l == locale)
        .map(|(_, z)| *z)
        .unwrap_or("")
}
pub fn engine_label(engine: &str) -> &str {
    ENGINES
        .iter()
        .find(|(id, _)| *id == engine)
        .map(|(_, n)| *n)
        .unwrap_or(if engine.is_empty() {
            "Playwright"
        } else {
            engine
        })
}
pub fn owns_identity(engine: &str) -> bool {
    engine == "camoufox"
}
pub fn gpus(os: &str) -> &'static [&'static str] {
    match os {
        "macOS" => &[
            "ANGLE (Apple M1, OpenGL 4.1)",
            "ANGLE (Apple M2, OpenGL 4.1)",
            "ANGLE (Intel Iris Plus Graphics, OpenGL 4.1)",
        ],
        "Linux" => &[
            "ANGLE (NVIDIA GeForce GTX 1050 Ti, OpenGL 4.5)",
            "ANGLE (Mesa Intel UHD Graphics, OpenGL 4.6)",
        ],
        "Android" => &[
            "ANGLE (Qualcomm Adreno 660, OpenGL ES 3.2)",
            "ANGLE (ARM Mali-G78, OpenGL ES 3.2)",
        ],
        _ => &[
            "ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)",
            "ANGLE (NVIDIA GeForce GTX 1660 Ti Direct3D11)",
            "ANGLE (Intel UHD Graphics 630 Direct3D11)",
            "ANGLE (AMD Radeon RX 6600 Direct3D11)",
        ],
    }
}
pub fn screens(os: &str) -> &'static [&'static str] {
    match os {
        "macOS" => &["2560x1600", "1440x900", "1728x1117"],
        "Linux" => &["1920x1080", "1366x768"],
        "Android" => &["360x800", "393x873", "412x915"],
        _ => &["1920x1080", "1366x768", "2560x1440", "1536x864"],
    }
}
pub fn cores(os: &str) -> &'static [u64] {
    match os {
        "macOS" => &[8, 10, 12],
        "Linux" => &[4, 8, 16],
        "Android" => &[6, 8],
        _ => &[4, 6, 8, 12, 16],
    }
}
pub fn ram(os: &str) -> &'static [u64] {
    match os {
        "Android" => &[4, 6, 8],
        "Linux" => &[8, 16],
        _ => &[8, 16, 32],
    }
}
pub fn vendor(renderer: &str) -> String {
    let r = renderer.to_lowercase();
    for (tok, name) in [
        ("nvidia", "NVIDIA"),
        ("radeon", "AMD"),
        ("amd", "AMD"),
        ("intel", "Intel"),
        ("apple", "Apple"),
        ("adreno", "Qualcomm"),
        ("mali", "ARM"),
    ] {
        if r.contains(tok) {
            return format!("Google Inc. ({name})");
        }
    }
    "Google Inc.".into()
}
pub fn ua(os: &str, version: &str) -> String {
    let tok = match os {
        "macOS" => "Macintosh; Intel Mac OS X 10_15_7",
        "Linux" => "X11; Linux x86_64",
        "Android" => "Linux; Android 13; Pixel 7",
        _ => "Windows NT 10.0; Win64; x64",
    };
    format!(
        "Mozilla/5.0 ({tok}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{version}{} Safari/537.36",
        if os == "Android" { " Mobile" } else { "" }
    )
}
pub fn change_os(p: &mut Value, os: &str) {
    p["os"] = json!(os);
    p["userAgent"] = json!(ua(os, "128.0.0.0"));
    p["screen"] = json!(screens(os)[0]);
    p["cores"] = json!(cores(os)[1]);
    p["memory"] = json!(ram(os)[1]);
    p["webglRenderer"] = json!(gpus(os)[0]);
    p["webglVendor"] = json!(vendor(gpus(os)[0]));
}
pub fn change_locale(p: &mut Value, locale: &str) {
    p["locale"] = json!(locale);
    p["timezone"] = json!(zone(locale));
}
pub fn new_profile(folder: &str, engine: &str) -> Value {
    let mut p = json!({"name":"","folder":if folder=="All profiles"{"Facebook Ads"}else{folder},"status":"stopped","os":"Windows","tags":[],"engine":if engine.is_empty(){Value::Null}else{json!(engine)},"browser":"Chrome","browserVersion":"128.0.0.0","canvas":"Noise","webrtc":"Altered","audio":"Noise","fonts":"Masked","locale":"en-US","timezone":"America/New_York","geo":"Prompt","mediaVideo":1,"mediaAudio":1,"dnt":false,"proxy":null,"notes":"","startupUrls":"","extensions":"","bookmarks":"","launchArgs":"","dns":"","ipBased":[],"clearOnStart":[],"switchedOff":[],"lastActive":"never","_new":true});
    change_os(&mut p, "Windows");
    p
}
pub fn demo_profiles() -> Vec<Value> {
    [("FB-Ads-01","Facebook Ads","running","Windows",vec!["client-a"],"US"),("FB-Ads-02","Facebook Ads","stopped","Windows",vec!["client-a"],"US"),("AMZ-Store-EU","Amazon Stores","running","macOS",vec!["eu"],"DE"),("Crypto-Air-07","Crypto","stopped","Linux",vec!["airdrop"],""),("PK-Mobile-01","QA Testing","running","Android",vec!["mobile","urdu"],"PK"),("QA-Sandbox","QA Testing","stopped","Windows",vec!["internal"],"GB")].into_iter().enumerate().map(|(i,(name,folder,status,os,tags,country))|{let mut p=new_profile(folder,"playwright");change_os(&mut p,os);change_locale(&mut p,country_locale(country));p["id"]=json!(format!("demo-{i}"));p["name"]=json!(name);p["status"]=json!(status);p["tags"]=json!(tags);p["_new"]=json!(false);p["lastActive"]=json!(if status=="running"{"now"}else{["2h ago","yesterday","3d ago"][name.len()%3]});if !country.is_empty(){p["proxy"]=json!({"type":"HTTP","host":format!("res-{}.proxy.io",country.to_lowercase()),"port":"8080","user":format!("u{}",name.len()),"pass":"•••••","country":country});}p}).collect()
}
pub fn webgl_plausible(os: &str, renderer: &str) -> bool {
    if renderer.is_empty() || gpus(os).contains(&renderer) {
        return true;
    }
    let r = renderer.to_lowercase();
    let d3d = r.contains("direct3d") || r.contains("d3d11");
    match os {
        "Windows" => d3d,
        "macOS" | "Linux" => r.contains("opengl") && !d3d,
        "Android" => r.contains("opengl es") || r.contains("adreno") || r.contains("mali"),
        _ => false,
    }
}
pub fn screen_plausible(os: &str, screen: &str) -> bool {
    if screen.is_empty() || screens(os).contains(&screen) {
        return true;
    }
    let Some((w, h)) = screen.split_once('x') else {
        return false;
    };
    matches!(
        (w.parse::<u32>(), h.parse::<u32>()),
        (Ok(320..=8000), Ok(480..=5000))
    )
}
pub fn coherence(p: &Value) -> Vec<String> {
    let mut issues = vec![];
    let os = s(p, "os");
    if !owns_identity(s(p, "engine")) {
        if !webgl_plausible(os, s(p, "webglRenderer")) {
            issues.push(format!("GPU doesn't belong to {os}"));
        }
        if !screen_plausible(os, s(p, "screen")) {
            issues.push(format!("Screen resolution unusual for {os}"));
        }
        let c = number(p, "cores");
        if c != 0 && !(2..=128).contains(&c) {
            issues.push(format!("CPU cores atypical for {os}"));
        }
    }
    if !s(p, "locale").is_empty()
        && !s(p, "timezone").is_empty()
        && zone(s(p, "locale")) != s(p, "timezone")
    {
        issues.push("Timezone doesn't match locale".into());
    }
    let c = s(&p["proxy"], "country");
    if (COUNTRIES.contains(&c) || c == "UK") && country_locale(c) != s(p, "locale") {
        issues.push(format!("Locale differs from proxy country ({c})"));
    }
    issues
}
pub fn split_tags(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}
pub fn without_marker(mut p: Value) -> Value {
    if let Some(o) = p.as_object_mut() {
        o.remove("_passwordSet");
    }
    p
}
pub fn for_write(p: &Value) -> Value {
    let mut r = without_marker(p.clone());
    if p["_passwordSet"] != true
        && let Some(proxy) = r.get_mut("proxy").and_then(Value::as_object_mut)
    {
        proxy.remove("pass");
    }
    r
}
pub type FieldPath = Vec<String>;
pub fn field<'a>(p: &'a Value, path: &[String]) -> &'a Value {
    path.iter().fold(p, |v, k| &v[k])
}
pub fn with_field(p: &mut Value, path: &[String], value: Value) {
    if let Some((key, rest)) = path.split_first() {
        if !p.is_object() {
            *p = json!({});
        }
        if rest.is_empty() {
            p[key] = value;
        } else {
            with_field(&mut p[key], rest, value);
        }
    }
}
pub fn changed_fields(base: &Value, draft: &Value) -> Vec<FieldPath> {
    fn visit(a: &Value, b: &Value, path: FieldPath, out: &mut Vec<FieldPath>) {
        if a == b {
            return;
        }
        if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
            for k in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                let mut next = path.clone();
                next.push(k.clone());
                visit(
                    a.get(k).unwrap_or(&Value::Null),
                    b.get(k).unwrap_or(&Value::Null),
                    next,
                    out,
                );
            }
        } else {
            out.push(path);
        }
    }
    let mut out = vec![];
    if let Some(d) = draft.as_object() {
        for (key, value) in d {
            if key.starts_with('_') || ["id", "status", "lastActive"].contains(&key.as_str()) {
                continue;
            }
            visit(&base[key], value, vec![key.clone()], &mut out);
        }
    }
    if draft["_passwordSet"] == true
        && draft["proxy"].is_object()
        && !out
            .iter()
            .any(|p| p[0] == "proxy" && (p.len() == 1 || p[1] == "pass"))
    {
        out.push(vec!["proxy".into(), "pass".into()]);
    }
    out
}
pub fn profile_write(base: &Value, draft: &Value) -> Value {
    let mut patch = json!({});
    for k in ["id", "_rev"] {
        let source = if k == "id" { draft } else { base };
        if let Some(v) = source.get(k) {
            patch[k] = v.clone();
        }
    }
    for path in changed_fields(base, draft) {
        patch[&path[0]] = draft[&path[0]].clone();
    }
    if draft["_passwordSet"] == true && patch["proxy"].is_object() {
        patch["_passwordSet"] = json!(true);
    }
    patch
}
pub fn rebase(base: &Value, draft: &Value, latest: &Value) -> (Value, Vec<FieldPath>) {
    let mut p = latest.clone();
    p["_new"] = draft["_new"].clone();
    p["_passwordSet"] = json!(draft["_passwordSet"] == true);
    let mut conflicts = vec![];
    for path in changed_fields(base, draft) {
        let target = if path[0] == "proxy" && path.len() > 1 && !latest["proxy"].is_object() {
            vec!["proxy".into()]
        } else {
            path
        };
        let a = field(base, &target);
        let b = field(draft, &target);
        let c = field(latest, &target);
        if a != c && b != c && !conflicts.contains(&target) {
            conflicts.push(target.clone());
        }
        with_field(&mut p, &target, b.clone());
    }
    (p, conflicts)
}

#[path = "model_editor.rs"]
mod editor;
#[path = "model_app.rs"]
mod state;
pub use editor::*;
pub use state::*;
