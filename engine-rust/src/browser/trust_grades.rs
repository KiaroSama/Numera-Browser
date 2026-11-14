use crate::{
    conversion::{truth, value_text},
    models::Fingerprint,
    validation::repr,
};
use serde_json::{Value, json};
use std::io;

pub(super) const ALIEN_FONT: &str = "Numera Nonexistent Grotesk";
fn check(name: &str, ok: bool, detail: impl Into<String>) -> Value {
    json!({"name":name,"ok":ok,"detail":detail.into()})
}
pub fn grade(fp: &Fingerprint, seen: &Value, engine: &str) -> io::Result<Value> {
    let required = [
        "webdriver",
        "webdriverOwn",
        "hasChrome",
        "plugins",
        "userAgent",
        "platform",
        "languages",
        "cores",
        "memory",
        "timezone",
        "toStringNative",
        "permissionsAgree",
    ];
    if !seen.is_object() || required.iter().any(|key| seen.get(*key).is_none()) {
        return Err(io::Error::other("Trust observation is incomplete"));
    }
    let plugins = seen["plugins"]
        .as_f64()
        .ok_or_else(|| io::Error::other("Plugin count is not numeric"))?;
    let mut checks = vec![
        check(
            "navigator.webdriver is false",
            seen["webdriver"] == false,
            format!("reads {}", repr(&seen["webdriver"])),
        ),
        check(
            "webdriver isn't an own property of navigator",
            !truth(&seen["webdriverOwn"]),
            "real Chrome defines it on the prototype",
        ),
        check(
            "window.chrome exists",
            truth(&seen["hasChrome"]),
            "missing on plain automated Chrome",
        ),
        check(
            "navigator.plugins isn't empty",
            plugins > 0.0,
            format!("{} plugin(s)", value_text(&seen["plugins"])),
        ),
        check(
            "User-Agent matches the profile",
            seen["userAgent"] == fp.user_agent,
            value_text(&seen["userAgent"]),
        ),
        check(
            "navigator.platform matches the OS",
            seen["platform"] == fp.platform,
            format!(
                "expected {}, got {}",
                fp.platform,
                value_text(&seen["platform"])
            ),
        ),
        check(
            "Languages match the locale",
            seen["languages"] == json!(fp.languages),
            format!(
                "expected {}, got {}",
                repr(&json!(fp.languages)),
                repr(&seen["languages"])
            ),
        ),
        check(
            "CPU cores match",
            seen["cores"].as_f64() == Some(fp.hardware_concurrency as f64),
            format!(
                "expected {}, got {}",
                fp.hardware_concurrency,
                value_text(&seen["cores"])
            ),
        ),
        check(
            "Device memory matches",
            seen["memory"].as_f64() == fp.device_memory.as_f64(),
            format!(
                "expected {}, got {}",
                value_text(&fp.device_memory),
                value_text(&seen["memory"])
            ),
        ),
        check(
            "Timezone matches the locale",
            super::network_grades::zone_matches(
                seen["timezone"].as_str().unwrap_or(""),
                &fp.timezone,
            )?,
            format!(
                "expected {}, got {}",
                fp.timezone,
                value_text(&seen["timezone"])
            ),
        ),
        check(
            "Patched functions still look native",
            truth(&seen["toStringNative"]),
            "toString() on a patched function must read '[native code]'",
        ),
        check(
            "permissions.query agrees with Notification.permission",
            truth(&seen["permissionsAgree"]),
            "",
        ),
        check(
            "Canvas reads the same twice",
            truth(&seen["canvasStable"]),
            "noise that changes between reads is a detection signal",
        ),
    ];
    if engine != "camoufox" {
        if seen.get("webglRenderer").is_none() {
            return Err(io::Error::other("WebGL observation missing"));
        }
        checks.push(check(
            "WebGL renderer matches the GPU",
            seen["webglRenderer"] == fp.webgl_renderer,
            format!(
                "expected {}, got {}",
                fp.webgl_renderer,
                value_text(&seen["webglRenderer"])
            ),
        ));
        let known = fp.fonts.first().map_or("Arial", String::as_str);
        let (font_known, font_alien) = if let Some(fonts) = seen.get("fontsRendered") {
            let fonts = fonts
                .as_array()
                .ok_or_else(|| io::Error::other("Rendered fonts are not a list"))?;
            (
                Some(!fp.fonts.is_empty() && fonts.contains(&json!(known))),
                fonts.contains(&json!(ALIEN_FONT)),
            )
        } else {
            (
                seen.get("fontKnown")
                    .filter(|value| !value.is_null())
                    .map(truth),
                truth(&seen["fontAlien"]),
            )
        };
        if let Some(font_known) = font_known {
            checks.push(check(
                "Font probing answers from this OS's font set",
                font_known && !font_alien,
                format!("'{known}' should exist, '{ALIEN_FONT}' should not"),
            ));
        }
        if !seen["cameras"].is_null() {
            checks.push(check(
                "Media devices match the profile",
                seen["cameras"].as_f64() == Some(fp.cameras as f64)
                    && seen["microphones"].as_f64() == Some(fp.microphones as f64),
                format!(
                    "expected {} camera(s)/{} mic(s), got {}/{}",
                    fp.cameras,
                    fp.microphones,
                    value_text(&seen["cameras"]),
                    value_text(&seen["microphones"])
                ),
            ));
        }
    }
    let passed = checks.iter().filter(|check| check["ok"] == true).count();
    let score = (100.0 * passed as f64 / checks.len() as f64).round_ties_even() as u64;
    let grade = if score >= 95 {
        "A"
    } else if score >= 85 {
        "B"
    } else if score >= 70 {
        "C"
    } else {
        "D"
    };
    Ok(json!({"score":score,"grade":grade,"passed":passed,"total":checks.len(),"checks":checks}))
}
