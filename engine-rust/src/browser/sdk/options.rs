use serde_json::{Map, Value};
use std::io;

/// Preserve the native launch decision while translating Python-shaped wire
/// keys to the vendor's language-independent driver protocol.
pub fn persistent(options: &Map<String, Value>) -> io::Result<Value> {
    let mut result = Map::new();
    for (key, value) in options {
        let target = match key.as_str() {
            "user_data_dir" => "userDataDir",
            "executable_path" => "executablePath",
            "headless" | "channel" | "args" | "proxy" | "locale" | "screen" | "permissions"
            | "geolocation" | "viewport" => key,
            "ignore_default_args" => "ignoreDefaultArgs",
            "user_agent" => "userAgent",
            "timezone_id" => "timezoneId",
            "is_mobile" => "isMobile",
            "device_scale_factor" => "deviceScaleFactor",
            "has_touch" => "hasTouch",
            "accept_downloads" => "acceptDownloads",
            "color_scheme" => "colorScheme",
            "ignore_https_errors" => "ignoreHTTPSErrors",
            "firefox_user_prefs" => "firefoxUserPrefs",
            "extra_http_headers" => "extraHTTPHeaders",
            "env" => "env",
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Unsupported persistent context option",
                ));
            }
        };
        if value.is_null() {
            if key == "viewport" {
                result.insert("noDefaultViewport".into(), true.into());
            }
            continue;
        }
        let value = if key == "accept_downloads" {
            match value.as_bool() {
                Some(true) => "accept".into(),
                Some(false) => "deny".into(),
                None => return Err(io::Error::other("Download permission must be boolean")),
            }
        } else if matches!(key.as_str(), "env" | "extra_http_headers") {
            Value::Array(
                value
                    .as_object()
                    .ok_or_else(|| io::Error::other("SDK string map required"))?
                    .iter()
                    .map(|(name, value)| {
                        let value = value
                            .as_str()
                            .ok_or_else(|| io::Error::other("SDK string map value required"))?;
                        Ok(serde_json::json!({"name":name,"value":value}))
                    })
                    .collect::<io::Result<Vec<_>>>()?,
            )
        } else {
            value.clone()
        };
        result.insert(target.into(), value);
    }
    if result
        .get("userDataDir")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Persistent profile directory required",
        ));
    }
    Ok(Value::Object(result))
}
