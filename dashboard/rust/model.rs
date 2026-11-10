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

#[derive(Clone, Default, Debug)]
pub struct Editor {
    pub base: Value,
    pub draft: Value,
    pub conflicts: Vec<FieldPath>,
    pub tab: String,
    pub message: String,
    pub saving: bool,
    pub probe: Value,
}
impl Editor {
    pub fn new(p: Value) -> Self {
        Self {
            base: p.clone(),
            draft: p,
            tab: "general".into(),
            ..Self::default()
        }
    }
    pub fn editable(&self) -> bool {
        !self.saving && self.conflicts.is_empty()
    }
    pub fn set(&mut self, path: &[String], value: Value) {
        if !self.editable() {
            return;
        }
        with_field(&mut self.draft, path, value);
        if path == ["proxy", "pass"] {
            self.draft["_passwordSet"] = json!(true)
        }
    }
    pub fn resolve(&mut self, index: usize, latest: bool) -> bool {
        let Some(path) = self.conflicts.get(index).cloned() else {
            return false;
        };
        if latest {
            with_field(&mut self.draft, &path, field(&self.base, &path).clone());
            if path[0] == "proxy" && (path.len() == 1 || path[1] == "pass") {
                self.draft["_passwordSet"] = json!(false);
            }
        } else if path == ["proxy"] && self.draft["proxy"].is_object() {
            if !self.draft["proxy"]["pass"].is_string() {
                self.message="The proxy password is unavailable. Use latest, or cancel and reload after unlocking the vault before restoring this proxy.".into();
                return false;
            }
            self.draft["_passwordSet"] = json!(true);
        }
        self.conflicts.remove(index);
        self.message = "Conflict resolved. Review your edits and save again.".into();
        true
    }
    pub fn begin_save(&mut self, live: bool) -> Option<Value> {
        if !self.editable() || text(&self.draft["name"]).is_empty() {
            return None;
        }
        self.saving = true;
        self.message.clear();
        Some(if live && self.draft["_new"] != true {
            profile_write(&self.base, &self.draft)
        } else {
            let mut p = self.draft.clone();
            p["_new"] = json!(false);
            p
        })
    }
    pub fn stale(&mut self, latest: Value) {
        let (p, c) = rebase(&self.base, &self.draft, &latest);
        self.base = latest;
        self.draft = p;
        self.conflicts = c;
        self.saving = false;
        self.message=if self.conflicts.is_empty(){"Your edits were rebased onto the latest profile. Review them and save again."}else{"You and another writer both changed these fields. Choose which edits to keep before saving."}.into();
    }
    pub fn failed(&mut self, error: &crate::contracts::ApiError) {
        self.saving = false;
        self.message = format!("Save failed: {error}");
    }
}

#[derive(Clone, Debug)]
pub struct Batch {
    pub count: usize,
    pub os: Vec<String>,
    pub countries: Vec<String>,
    pub versions: Vec<String>,
    pub engine: String,
    pub prefix: String,
    pub start: usize,
    pub tags: String,
    pub randomize: bool,
}
impl Default for Batch {
    fn default() -> Self {
        Self {
            count: 50,
            os: vec!["Windows".into()],
            countries: vec!["US".into()],
            versions: VERSIONS.map(str::to_owned).to_vec(),
            engine: String::new(),
            prefix: "Batch".into(),
            start: 1,
            tags: "bulk".into(),
            randomize: true,
        }
    }
}
impl Batch {
    pub fn build(&self, folder: &str, mut random: impl FnMut(usize) -> usize) -> Vec<Value> {
        let n = self.count.clamp(1, 10000);
        let pad = (self.start + n - 1).to_string().len().max(2);
        (0..n)
            .map(|i| {
                let os = &self.os[i % self.os.len()];
                let country = &self.countries[(i / self.os.len()) % self.countries.len()];
                let version = &self.versions[random(self.versions.len()) % self.versions.len()];
                let mut p = new_profile(folder, &self.engine);
                change_os(&mut p, os);
                change_locale(&mut p, country_locale(country));
                p["name"] = json!(format!("{}-{:0pad$}", self.prefix, self.start + i));
                p["tags"] = json!(split_tags(&self.tags));
                p["browserVersion"] = json!(version);
                p["userAgent"] = json!(ua(os, version));
                let g = if self.randomize {
                    random(gpus(os).len()) % gpus(os).len()
                } else {
                    0
                };
                p["webglRenderer"] = json!(gpus(os)[g]);
                p["webglVendor"] = json!(vendor(gpus(os)[g]));
                if self.randomize {
                    p["screen"] = json!(screens(os)[random(screens(os).len()) % screens(os).len()]);
                    p["cores"] = json!(cores(os)[random(cores(os).len()) % cores(os).len()]);
                    p["memory"] = json!(ram(os)[random(ram(os).len()) % ram(os).len()]);
                }
                p
            })
            .collect()
    }
}
pub fn sync_patch(values: &Value) -> Value {
    let mut p = json!({});
    for k in ["engine", "locale", "folder"] {
        if !s(values, k).is_empty() {
            p[k] = values[k].clone();
        }
    }
    let tags = split_tags(s(values, "tags"));
    if !tags.is_empty() {
        p["addTags"] = json!(tags);
    }
    p
}
pub fn merge_patch(p: &mut Value, patch: &Value) {
    if let Some(fields) = patch.as_object() {
        for (k, v) in fields {
            if k != "addTags" {
                p[k] = v.clone();
            }
        }
    }
    if !s(patch, "locale").is_empty() {
        p["timezone"] = json!(zone(s(patch, "locale")));
    }
    if patch["addTags"].is_array() {
        let mut tags = list(&p["tags"]);
        for t in list(&patch["addTags"]) {
            if !tags.contains(&t) {
                tags.push(t)
            }
        }
        p["tags"] = json!(tags);
    }
}
pub fn export_profiles(profiles: &[Value]) -> (Value, String) {
    let withheld = profiles
        .iter()
        .filter(|p| p["proxy"].is_object() && p["proxy"]["pass"].is_null())
        .count();
    (
        json!({"numeraProfiles":1,"profiles":profiles}),
        format!(
            "Exported {} profile configuration{}. Browser sessions — cookies and logins — are not included; use `numera backup` for those.{}",
            profiles.len(),
            if profiles.len() == 1 { "" } else { "s" },
            if withheld > 0 {
                format!(
                    " {withheld} proxy password{} locked in the vault and not in the file.",
                    if withheld == 1 { " is" } else { "s are" }
                )
            } else {
                String::new()
            }
        ),
    )
}
pub fn import_report(v: &Value) -> String {
    let n = number(v, "imported");
    let flagged = list(&v["results"])
        .iter()
        .filter(|r| !list(&r["issues"]).is_empty())
        .count();
    format!(
        "Imported {n} profile{}.{}{}",
        if n == 1 { "" } else { "s" },
        if s(v, "format") == "vendor" {
            " Read as a vendor export, so each fingerprint was regenerated from the fields that file did carry."
        } else {
            ""
        },
        if flagged > 0 {
            format!(" {flagged} flagged for review — open them to see the notes.")
        } else {
            String::new()
        }
    )
}
pub fn health_report(name: &str, action: &str, result: &Value) -> String {
    if action == "trust" {
        let g = if result["after"].is_object() {
            &result["after"]
        } else {
            result
        };
        format!(
            "{name}: trust {}.",
            if s(g, "grade").is_empty() {
                "no grade came back".into()
            } else {
                format!(
                    "{} ({}/100){}",
                    s(g, "grade"),
                    text(&g["score"]),
                    if number(g, "total") > 0 {
                        format!(
                            ", {} of {} checks passed",
                            text(&g["passed"]),
                            text(&g["total"])
                        )
                    } else {
                        String::new()
                    }
                )
            }
        )
    } else {
        let changed = strings(&result["changed"]);
        let before = s(&result["before"], "grade");
        let after = s(&result["after"], "grade");
        let moved = if !before.is_empty() && !after.is_empty() && before != after {
            format!(" {before} → {after}")
        } else {
            String::new()
        };
        if changed.is_empty() {
            format!("{name}: already aligned{moved} — nothing needed changing.")
        } else {
            format!(
                "{name}: aligned{moved} — rewrote {} to what the browser presents.",
                changed.join(", ")
            )
        }
    }
}
pub fn pool_password_patch(p: &Value) -> Value {
    let mut patch = json!({});
    for key in [
        "label",
        "type",
        "host",
        "port",
        "user",
        "pass",
        "country",
        "rotateUrl",
    ] {
        let value = p[key].clone();
        patch[key] = if key == "pass" {
            value
        } else if ["label", "user", "country", "rotateUrl"].contains(&key)
            && text(&value).is_empty()
        {
            Value::Null
        } else if key == "country" {
            json!(text(&value).to_uppercase())
        } else if key == "host" || key == "rotateUrl" {
            json!(text(&value).trim())
        } else {
            value
        };
    }
    patch
}
pub fn collision_ids(profiles: &[Value]) -> BTreeSet<String> {
    let mut by = BTreeMap::<String, Vec<String>>::new();
    for p in profiles {
        let sig = [
            "os",
            "userAgent",
            "screen",
            "cores",
            "memory",
            "webglRenderer",
        ]
        .map(|k| text(&p[k]))
        .join("|");
        by.entry(sig).or_default().push(s(p, "id").into());
    }
    by.into_values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .collect()
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub error: bool,
    pub expires: f64,
}
#[derive(Clone, Debug)]
pub struct App {
    pub mode: String,
    pub auth_error: String,
    pub profiles: Vec<Value>,
    pub config: Value,
    pub engines: Value,
    pub view: String,
    pub folder: String,
    pub query: String,
    pub filters: BTreeMap<String, String>,
    pub selected: BTreeSet<String>,
    pub page: usize,
    pub filter_open: bool,
    pub editor: Option<Editor>,
    pub modal: String,
    pub form: Value,
    pub batch: Batch,
    pub batch_progress: usize,
    pub pool: Vec<Value>,
    pub pool_selected: BTreeSet<String>,
    pub pool_tab: String,
    pub health: Value,
    pub tokens: Value,
    pub minted: Value,
    pub extensions: Vec<Value>,
    pub icons: BTreeMap<String, String>,
    pub connections: BTreeMap<String, Value>,
    pub theme: String,
    pub busy: bool,
    pub message: String,
    pub message_error: bool,
    pub toasts: Vec<Toast>,
    pub next_toast: u64,
    pub command: String,
    pub command_index: usize,
    pub cloak: Value,
    pub cloak_approved: bool,
    pub update_bridge: Option<crate::updates::Bridge>,
    pub update: Value,
    pub update_approval: Value,
    pub update_error: String,
    pub update_busy: bool,
    pub dismissed: String,
    pub reveal: BTreeSet<String>,
    pub rename: String,
    pub schedule: Value,
}
impl Default for App {
    fn default() -> Self {
        Self {
            mode: "connecting".into(),
            auth_error: String::new(),
            profiles: demo_profiles(),
            config: json!({"folders":["Facebook Ads","Amazon Stores","Crypto","QA Testing"]}),
            engines: json!({}),
            view: "profiles".into(),
            folder: "All profiles".into(),
            query: String::new(),
            filters: BTreeMap::new(),
            selected: BTreeSet::new(),
            page: 0,
            filter_open: false,
            editor: None,
            modal: String::new(),
            form: json!({}),
            batch: Batch::default(),
            batch_progress: 0,
            pool: vec![],
            pool_selected: BTreeSet::new(),
            pool_tab: "all".into(),
            health: json!({}),
            tokens: json!({}),
            minted: Value::Null,
            extensions: vec![],
            icons: BTreeMap::new(),
            connections: BTreeMap::new(),
            theme: "dark".into(),
            busy: false,
            message: String::new(),
            message_error: false,
            toasts: vec![],
            next_toast: 0,
            command: String::new(),
            command_index: 0,
            cloak: Value::Null,
            cloak_approved: false,
            update_bridge: None,
            update: Value::Null,
            update_approval: Value::Null,
            update_error: String::new(),
            update_busy: false,
            dismissed: String::new(),
            reveal: BTreeSet::new(),
            rename: String::new(),
            schedule: Value::Null,
        }
    }
}
impl App {
    pub fn commit_profile(&mut self, mut saved: Value) {
        saved["_new"] = json!(false);
        if let Some(index) = self.profiles.iter().position(|p| p["id"] == saved["id"]) {
            self.profiles[index] = saved;
        } else {
            self.profiles.push(saved);
        }
        self.editor = None;
    }
    pub fn bulk_result(
        &mut self,
        verb: &str,
        results: &[(String, Result<(), String>)],
        skipped: usize,
    ) {
        let failed: Vec<_> = results
            .iter()
            .filter(|(_, result)| result.is_err())
            .collect();
        self.selected = failed.iter().map(|(id, _)| id.clone()).collect();
        self.busy = false;
        let note = if skipped > 0 {
            format!(" {skipped} already running.")
        } else {
            String::new()
        };
        self.say(
            if failed.is_empty() {
                format!("{verb} {} profile(s).{note}", results.len())
            } else {
                format!(
                    "{verb} {} of {}. {} failed: {}.{note}",
                    results.len() - failed.len(),
                    results.len(),
                    failed.len(),
                    failed[0].1.as_ref().unwrap_err()
                )
            },
            !failed.is_empty(),
        );
    }
    pub fn cloak_reviewed(&mut self, offer: Value) {
        self.cloak = offer;
        self.cloak_approved = false;
    }
    pub fn begin_cloak(&mut self) -> Option<Value> {
        if self.busy
            || !self.live()
            || !self.cloak_approved
            || self.cloak["available"] != true
            || self.cloak["installed"] == true
            || s(&self.cloak, "offer_id").is_empty()
        {
            return None;
        }
        self.cloak_approved = false;
        self.busy = true;
        Some(json!({"offer_id": self.cloak["offer_id"], "approve": true}))
    }
    pub fn finish_cloak(&mut self, result: Result<Value, String>) {
        self.busy = false;
        match result {
            Ok(result) => {
                self.cloak["installed"] = json!(true);
                self.say(
                    format!("Approved browser acquired: {}.", s(&result, "version")),
                    false,
                );
            }
            Err(error) => {
                self.cloak = Value::Null;
                self.say(error, true);
            }
        }
    }
    pub fn live(&self) -> bool {
        self.mode == "live"
    }
    pub fn default_engine(&self) -> &str {
        s(&self.config, "default_engine")
    }
    pub fn folders(&self) -> Vec<String> {
        let mut out = strings(&self.config["folders"]);
        for p in &self.profiles {
            let f = s(p, "folder");
            if !f.is_empty() && !out.contains(&f.to_owned()) {
                out.push(f.into());
            }
        }
        out
    }
    pub fn selected_profiles(&self) -> Vec<Value> {
        self.selected
            .iter()
            .filter_map(|id| self.profiles.iter().find(|p| s(p, "id") == id))
            .cloned()
            .collect()
    }
    pub fn filtered(&self) -> Vec<&Value> {
        let q = self.query.to_lowercase();
        self.profiles
            .iter()
            .filter(|p| {
                (self.folder == "All profiles" || s(p, "folder") == self.folder)
                    && (text(&p["name"]).to_lowercase().contains(&q)
                        || strings(&p["tags"])
                            .iter()
                            .any(|t| t.to_lowercase().contains(&q)))
                    && self.filters.iter().all(|(k, v)| {
                        v.is_empty()
                            || match k.as_str() {
                                "proxy" => {
                                    if v == "with" {
                                        p["proxy"].is_object()
                                    } else {
                                        !p["proxy"].is_object()
                                    }
                                }
                                "engine" => {
                                    let e = s(p, "engine");
                                    (if e.is_empty() { "playwright" } else { e }) == v
                                }
                                "tag" => strings(&p["tags"]).contains(v),
                                _ => s(p, k) == v,
                            }
                    })
            })
            .collect()
    }
    pub fn toggle_all(&mut self) {
        let ids = self
            .filtered()
            .iter()
            .map(|p| s(p, "id").to_owned())
            .collect::<Vec<_>>();
        let all = !ids.is_empty() && ids.iter().all(|id| self.selected.contains(id));
        for id in ids {
            if all {
                self.selected.remove(&id);
            } else {
                self.selected.insert(id);
            }
        }
    }
    pub fn say(&mut self, msg: impl Into<String>, error: bool) {
        self.message = msg.into();
        self.message_error = error;
        self.next_toast += 1;
        #[cfg(target_arch = "wasm32")]
        let expires = js_sys::Date::now() + 3800.;
        #[cfg(not(target_arch = "wasm32"))]
        let expires = f64::INFINITY;
        self.toasts.push(Toast {
            id: self.next_toast,
            text: self.message.clone(),
            error,
            expires,
        });
    }
    pub fn toggle_set(set: &mut BTreeSet<String>, key: String) {
        if !set.remove(&key) {
            set.insert(key);
        }
    }
    pub fn poll_every(&self) -> u32 {
        if self.profiles.len() > 500 {
            12000
        } else {
            3000
        }
    }
    pub fn can_poll(&self) -> bool {
        self.live() && self.editor.is_none() && !["bulk", "sync"].contains(&self.modal.as_str())
    }
    pub fn open_editor(&mut self, p: Value) {
        self.editor = Some(Editor::new(p));
        self.schedule = Value::Null;
        self.form["preset"] = json!("general");
        self.form["replaceCookies"] = json!(false);
    }
    pub fn launch_ids(&self) -> Vec<String> {
        self.selected_profiles()
            .iter()
            .filter(|p| s(p, "status") != "running")
            .map(|p| s(p, "id").to_owned())
            .collect()
    }
    pub fn set_config(&mut self, c: Value) {
        self.config = c;
    }
    pub fn adopt_token(&self, old: &str, made: &Value) -> String {
        if list(&self.tokens["tokens"]).is_empty() && self.tokens["envToken"] != true {
            s(made, "token").into()
        } else {
            old.into()
        }
    }
    pub fn commands(&self) -> Vec<(String, String, String)> {
        let mut out = VIEWS
            .iter()
            .map(|(v, l)| (format!("Go to {l}"), "nav".into(), v.to_string()))
            .collect::<Vec<_>>();
        out.push(("New profile".into(), "new".into(), String::new()));
        out.push((
            "Toggle light / dark theme".into(),
            "theme".into(),
            String::new(),
        ));
        for p in self.profiles.iter().take(150) {
            out.push((
                format!("Open {}", text(&p["name"])),
                "edit".into(),
                s(p, "id").into(),
            ));
            out.push((
                format!(
                    "{} {}",
                    if s(p, "status") == "running" {
                        "Stop"
                    } else {
                        "Launch"
                    },
                    text(&p["name"])
                ),
                "run".into(),
                s(p, "id").into(),
            ));
        }
        out.into_iter()
            .filter(|c| {
                c.0.to_lowercase()
                    .contains(&self.command.trim().to_lowercase())
            })
            .take(40)
            .collect()
    }
}
