use serde_json::{Value, json};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bridge {
    pub endpoint: String,
    pub token: String,
}
fn text(v: &Value, max: usize) -> bool {
    v.as_str().is_some_and(|s| {
        s.encode_utf16().count() <= max && s.chars().all(|c| c >= ' ' && c != '\u{7f}')
    })
}
fn count(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
}
pub fn bridge(candidate: &Value) -> Option<Bridge> {
    let endpoint = candidate["endpoint"].as_str()?;
    let token = candidate["token"].as_str()?;
    if !text(&candidate["endpoint"], 64)
        || !(32..=256).contains(&token.len())
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return None;
    }
    let url = url::Url::parse(endpoint).ok()?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none_or(|p| p == 0)
        || url.path() != "/"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || endpoint != url.origin().ascii_serialization()
    {
        return None;
    }
    Some(Bridge {
        endpoint: endpoint.into(),
        token: token.into(),
    })
}
pub const STATES: [&str; 12] = [
    "idle",
    "checking",
    "available",
    "current",
    "unavailable",
    "downloading",
    "ready",
    "deferred",
    "activating",
    "complete",
    "cancelled",
    "error",
];
pub fn status(value: Value) -> Result<Value, String> {
    let valid = || {
        if !value.is_object()
            || !STATES.contains(&value["state"].as_str().unwrap_or(""))
            || !text(&value["message"], 2048)
            || !value["current"].is_object()
            || !["version", "sourceSha", "platform"]
                .iter()
                .all(|k| text(&value["current"][k], 80))
            || !count(&value["current"]["generation"])
            || !value["current"]["portable"].is_boolean()
            || !value["progress"].is_object()
            || !count(&value["progress"]["receivedBytes"])
            || !count(&value["progress"]["totalBytes"])
            || !value["canCancel"].is_boolean()
        {
            return false;
        }
        let Some(offer) = value.get("offer") else {
            return false;
        };
        if !offer.is_null() {
            let sha = offer["sourceSha"].as_str().unwrap_or("");
            if !offer.is_object()
                || !text(&offer["id"], 256)
                || offer["id"].as_str() == Some("")
                || !text(&offer["version"], 80)
                || sha.len() != 40
                || !sha
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || !count(&offer["generation"])
                || offer["generation"] == 0
                || !count(&offer["downloadBytes"])
                || !count(&offer["expandedBytes"])
                || !offer["components"].is_object()
                || offer["restartRequired"] != true
            {
                return false;
            }
        }
        true
    };
    if valid() {
        Ok(value)
    } else {
        Err("Invalid update status".into())
    }
}
pub fn response(bytes: &[u8], ok: bool) -> Result<Value, String> {
    if bytes.len() > 65536 {
        return Err("Invalid update status".into());
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "Invalid update status")?;
    let mut v = status(v)?;
    if !ok && !["error", "deferred", "unavailable"].contains(&v["state"].as_str().unwrap_or("")) {
        v["state"] = json!("error");
        v["message"] = json!("The update operation was refused. Check again before retrying.");
    }
    Ok(v)
}
pub fn action_body(action: &str, id: &str) -> Value {
    match action {
        "download" | "install" => json!({"offerId":id,"approve":true}),
        "cancel" => json!({"offerId":id}),
        _ => json!({}),
    }
}
pub fn approval_valid(status: &Value, approval: &Value, busy: bool, error: &str) -> bool {
    let ready = ["ready", "deferred"].contains(&status["state"].as_str().unwrap_or(""));
    !busy
        && error.is_empty()
        && !active(status)
        && status["offer"].is_object()
        && status["offer"]["id"] == approval["offer"]["id"]
        && approval["install"].as_bool() == Some(ready)
}
pub fn active(status: &Value) -> bool {
    ["checking", "downloading", "activating"].contains(&status["state"].as_str().unwrap_or(""))
}
pub fn bytes(n: u64) -> String {
    if n >= 1_073_741_824 {
        format!("{:.1} GiB", n as f64 / 1_073_741_824.)
    } else if n >= 1_048_576 {
        format!("{:.1} MiB", n as f64 / 1_048_576.)
    } else {
        format!("{} KiB", n.div_ceil(1024))
    }
}
