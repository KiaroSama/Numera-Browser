//! Runtime checks at the HTTP boundary. Extra fields are retained unchanged.
use serde_json::Value;
pub fn api_url(path: &str) -> String {
    format!("{}{}", option_env!("NUMERA_API_URL").unwrap_or(""), path)
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub message: String,
    pub status: u16,
    pub kind: String,
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ApiError {}
pub fn error(method: &str, path: &str, status: u16, body: &Value) -> ApiError {
    ApiError {
        message: body["detail"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if status == 401 {
                    "This engine requires an API token.".into()
                } else {
                    format!("{method} {path} → {status}")
                }
            }),
        status,
        kind: body["kind"].as_str().unwrap_or("").into(),
    }
}
fn invalid(kind: &str) -> String {
    format!("The engine returned an invalid {kind}.")
}
fn fields(v: &Value, spec: &str) -> Result<(), String> {
    if !v.is_object() {
        return Err(invalid("object"));
    }
    for entry in spec.split_whitespace() {
        let (name, kind) = entry.split_once(':').unwrap();
        let optional = name.ends_with('?');
        let name = name.trim_end_matches('?');
        if optional && v.get(name).is_none() {
            continue;
        }
        check(&v[name], kind)?;
    }
    Ok(())
}
fn array(v: &Value, kind: &str) -> Result<(), String> {
    let a = v.as_array().ok_or_else(|| invalid("array"))?;
    for v in a {
        check(v, kind)?;
    }
    Ok(())
}
fn check(v: &Value, kind: &str) -> Result<(), String> {
    if let Some(k) = kind.strip_prefix('~') {
        return if v.is_null() { Ok(()) } else { check(v, k) };
    }
    if let Some(k) = kind.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return array(v, k);
    }
    match kind {
        "text" => {
            if v.is_string() {
                Ok(())
            } else {
                Err(invalid("string"))
            }
        }
        "number" => {
            if v.is_number() {
                Ok(())
            } else {
                Err(invalid("number"))
            }
        }
        "bool" => {
            if v.is_boolean() {
                Ok(())
            } else {
                Err(invalid("boolean"))
            }
        }
        "port" => {
            if v.is_number() || v.is_string() {
                Ok(())
            } else {
                Err(invalid("string"))
            }
        }
        "name" => {
            if v.is_null() || v.is_number() || v.is_string() {
                Ok(())
            } else {
                Err(invalid("string"))
            }
        }
        "tag" => {
            if v.is_number() || v.is_string() {
                Ok(())
            } else {
                Err(invalid("string"))
            }
        }
        "os" => {
            if ["Windows", "macOS", "Linux", "Android"].contains(&v.as_str().unwrap_or("")) {
                Ok(())
            } else {
                Err(invalid("operating system"))
            }
        }
        "json" => Ok(()),
        "object" => {
            if v.is_object() {
                Ok(())
            } else {
                Err(invalid("object"))
            }
        }
        "proxy" => fields(
            v,
            "id?:text type:text host:text port:port user?:~text pass?:~text country?:~text label?:~text rotateUrl?:~text archived?:bool profiles?:number status?:text lastCheck?:proxyCheck",
        ),
        "proxyCheck" => fields(v, "ip?:text country?:~text latencyMs?:number error?:text"),
        "profile" => {
            fields(v, "id:text")?;
            check(v, "draft")
        }
        "draft" => fields(
            v,
            "id?:text name:name folder:text status:text os:os tags:[tag] engine:~text browser:text browserVersion:text userAgent:text screen:text cores:number memory:number webglVendor:text webglRenderer:text canvas:text webrtc:text audio:text fonts:text locale:text timezone:text geo:text mediaVideo:number mediaAudio:number dnt:bool proxy:~proxy notes:text startupUrls:text extensions:text lastActive:text clientRects?:text webglImage?:text mediaDevices?:text mediaSpeakers?:number fontList?:text bookmarks?:text launchArgs?:text dns?:text ipBased?:[text] clearOnStart?:[text] switchedOff?:[text] _new?:bool _passwordSet?:bool _rev?:json",
        ),
        "config" => fields(
            v,
            "root?:text version?:text folders?:[text] default_engine?:text default_extensions?:[text] proxyCount?:number",
        ),
        "engines" => {
            let o = v.as_object().ok_or_else(|| invalid("object"))?;
            for v in o.values() {
                check(v, "bool")?;
            }
            Ok(())
        }
        "cloak" => fields(
            v,
            "engine:text available:bool installed:bool offer_id:~text sdk?:text version?:text platform?:text keyed?:bool reason:~text notice?:text",
        ),
        "acquired" => fields(v, "engine:text installed:bool version:text offer_id:text"),
        "grade" => fields(v, "grade?:text score:number passed?:number total?:number"),
        "probe" => fields(
            v,
            "before?:grade after?:grade grade?:text score?:number passed?:number total?:number checks?:[check] changed?:[text] os?:text ip?:text city?:~text country?:~text latencyMs?:number ja3Hash?:~text ja4?:~text countries?:[text] resolvers?:[json] error?:text profile?:profile",
        ),
        "check" => fields(v, "name:text ok:bool detail?:text"),
        "schedule" => fields(
            v,
            "id:text profileId:text everyHours:number minutes:number preset:text enabled?:bool",
        ),
        "health" => fields(v, "profiles:[healthRow] summary:healthSummary"),
        "healthRow" => fields(
            v,
            "id:text name:text engine:text status:text coherent:bool issues:[text] collides?:bool needsAttention?:bool lastActive?:text proxy?:~healthProxy trust?:~grade schedule?:~healthSchedule",
        ),
        "healthProxy" => fields(v, "set?:bool country?:~text type?:~text"),
        "healthSchedule" => fields(v, "enabled?:bool everyHours?:number preset?:text"),
        "healthSummary" => fields(
            v,
            "total?:number coherent?:number withProxy?:number scheduled?:number colliding?:number needsAttention?:number",
        ),
        "tokens" => fields(v, "tokens:[token] envToken:bool"),
        "token" => fields(v, "id:text name:text created:number lastUsed?:~number"),
        "minted" => fields(v, "id:text name:text created:number token:text"),
        "attachment" => fields(v, "port:number headless?:bool wsUrl?:text"),
        "rotation" => fields(v, "rotated:bool previousIp?:~text ip?:text error?:text"),
        "extensions" => fields(v, "extensions:[extension]"),
        "extension" => fields(v, "path:text name?:text icon?:~text"),
        "installed" => fields(v, "default_extensions:[text] reused:bool name:text"),
        "deletedProfiles" => fields(v, "removed:[removed] missing:[text] failed:[failed]"),
        "removed" => {
            if v.is_string() {
                Ok(())
            } else {
                fields(v, "id:text")
            }
        }
        "failed" => fields(v, "error:text"),
        "cookies" => fields(v, "cookies:[json]"),
        "cookiesImported" => fields(v, "imported:number"),
        "imported" => fields(v, "imported:number format:text results:[importResult]"),
        "importResult" => fields(v, "issues:[text]"),
        "proxiesAdded" => fields(v, "added:number total:number"),
        "proxiesDeleted" => fields(v, "deleted:number"),
        "proxiesExported" => fields(v, "text:text count:number"),
        "proxiesTested" => fields(v, "tested:number ok:number"),
        "proxiesAssigned" => fields(v, "assigned:number"),
        "proxiesRotated" => fields(v, "rotated:number tried:number"),
        _ => Err(format!("Unknown API schema: {kind}")),
    }
}
pub fn validate(v: Value, kind: &str) -> Result<Value, String> {
    check(&v, kind)?;
    Ok(v)
}
pub fn response_kind(method: &str, path: &str) -> Option<&'static str> {
    let path = path.split('?').next().unwrap_or(path);
    match (method, path) {
        ("GET", "/api/profiles") => Some("[profile]"),
        ("GET", "/api/config") | ("PUT", "/api/config") => Some("config"),
        ("GET", "/api/engines") => Some("engines"),
        ("GET", "/api/engines/cloak/acquisition") => Some("cloak"),
        ("POST", "/api/engines/cloak/acquisition") => Some("acquired"),
        ("GET", "/api/proxies") => Some("[proxy]"),
        ("POST", "/api/proxies") => Some("proxiesAdded"),
        ("GET", "/api/tokens") => Some("tokens"),
        ("POST", "/api/tokens") => Some("minted"),
        ("GET", "/api/schedules") => Some("[schedule]"),
        ("POST", "/api/schedules") => Some("schedule"),
        ("GET", "/api/extensions/installed") => Some("extensions"),
        ("POST", "/api/extensions/install") => Some("installed"),
        ("GET", "/api/profiles/health") => Some("health"),
        ("POST", "/api/profiles/delete") => Some("deletedProfiles"),
        ("POST", "/api/profiles/import") => Some("imported"),
        ("POST", "/api/profiles/bulk") => Some("[profile]"),
        ("POST", "/api/profiles") => Some("profile"),
        ("POST", "/api/proxies/delete") => Some("proxiesDeleted"),
        ("POST", "/api/proxies/export") => Some("proxiesExported"),
        ("POST", "/api/proxies/test") => Some("proxiesTested"),
        ("POST", "/api/proxies/assign") => Some("proxiesAssigned"),
        ("POST", "/api/proxies/rotate") => Some("proxiesRotated"),
        _ if path.starts_with("/api/folders") => Some("config"),
        _ if path.starts_with("/api/schedules/") && method == "PUT" => Some("schedule"),
        _ if path.starts_with("/api/profiles/") => match path.rsplit('/').next().unwrap_or("") {
            "trust" | "align" | "tls" | "dns" | "proxy-test" => Some("probe"),
            "attach" => Some("attachment"),
            "cookies" => Some(if method == "GET" {
                "cookies"
            } else {
                "cookiesImported"
            }),
            _ if method == "PUT" => Some("profile"),
            _ => None,
        },
        _ if path.starts_with("/api/proxies/") && path.ends_with("/rotate") => Some("rotation"),
        _ => None,
    }
}
