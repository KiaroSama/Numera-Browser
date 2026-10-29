use crate::{conversion::truth, devices::catalog, fingerprint, models::Fingerprint};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io};

pub const FIELDS: &[&str] = &[
    "os",
    "is_mobile",
    "os_version",
    "device_model",
    "chrome_version",
    "platform",
    "ch_platform",
    "user_agent",
    "hardware_concurrency",
    "device_memory",
    "max_touch_points",
    "device_pixel_ratio",
    "webgl_vendor",
    "webgl_renderer",
    "screen_width",
    "screen_height",
    "viewport_width",
    "viewport_height",
    "cameras",
    "microphones",
    "languages",
    "locale",
    "timezone",
    "fonts",
];
fn apply(fp: &mut Value, key: &str, value: Value, label: &str, changed: &mut BTreeSet<String>) {
    if !value.is_null() && fp[key] != value {
        fp[key] = value;
        changed.insert(label.into());
    }
}
fn os(platform: &str, fallback: &str) -> String {
    let platform = platform.to_lowercase();
    for (tokens, os) in [
        (&["win"][..], "windows"),
        (&["mac"][..], "macos"),
        (&["arm", "android", "aarch"][..], "android"),
        (&["linux", "x11"][..], "linux"),
    ] {
        if tokens.iter().any(|token| platform.contains(token)) {
            return os.into();
        }
    }
    fallback.into()
}
fn chrome(ua: &str) -> Option<String> {
    let mut rest = ua;
    while let Some((_, tail)) = rest.split_once("Chrome/") {
        let mut length = 0;
        for component in 0..4 {
            let count = tail[length..]
                .chars()
                .take_while(|c| crate::pool::decimal_ascii(&c.to_string()).is_some())
                .map(char::len_utf8)
                .sum::<usize>();
            if count == 0 {
                break;
            }
            length += count;
            if component == 3 {
                return Some(tail[..length].into());
            }
            if tail.as_bytes().get(length) != Some(&b'.') {
                break;
            }
            length += 1;
        }
        rest = tail;
    }
    None
}
fn vendor(renderer: &str) -> String {
    let renderer = renderer.to_lowercase();
    for (token, vendor) in [
        ("nvidia", "NVIDIA"),
        ("amd", "AMD"),
        ("radeon", "AMD"),
        ("intel", "Intel"),
        ("apple", "Apple"),
        ("adreno", "Qualcomm"),
        ("qualcomm", "Qualcomm"),
        ("mali", "ARM"),
        ("arm", "ARM"),
    ] {
        if renderer.contains(token) {
            return format!("Google Inc. ({vendor})");
        }
    }
    "Google Inc.".into()
}
pub fn candidates() -> io::Result<Vec<String>> {
    let fonts = catalog().map_err(io::Error::other)?["catalog"]["FONTS"]
        .as_object()
        .ok_or_else(|| io::Error::other("Font catalog missing"))?;
    let mut result = BTreeSet::new();
    for pool in fonts.values() {
        for font in pool
            .as_array()
            .ok_or_else(|| io::Error::other("Font pool missing"))?
        {
            result.insert(
                font.as_str()
                    .ok_or_else(|| io::Error::other("Font name is not text"))?
                    .to_owned(),
            );
        }
    }
    Ok(result.into_iter().collect())
}
pub fn rewrite(original: &Fingerprint, seen: &Value) -> io::Result<(Fingerprint, Vec<String>)> {
    let data = &catalog().map_err(io::Error::other)?["catalog"];
    let mut fp = serde_json::to_value(original).map_err(io::Error::other)?;
    let mut changed = BTreeSet::new();
    let os = os(seen["platform"].as_str().unwrap_or(""), &original.os);
    let transition = os != original.os;
    if transition {
        fp["os"] = os.clone().into();
        fp["is_mobile"] = (os == "android").into();
        changed.insert("os".into());
    }
    let chrome = chrome(seen["userAgent"].as_str().unwrap_or(""))
        .unwrap_or_else(|| original.chrome_version.clone());
    fp["chrome_version"] = chrome.clone().into();
    apply(
        &mut fp,
        "platform",
        data["NAV_PLATFORM"]
            .get(&os)
            .cloned()
            .unwrap_or_else(|| seen["platform"].clone()),
        "platform",
        &mut changed,
    );
    if let Some(platform) = data["CH_PLATFORM"].get(&os) {
        fp["ch_platform"] = platform.clone();
    }
    if transition || original.os_version.is_empty() {
        fp["os_version"] = data["OS_VERSIONS"][&os][0][0].clone();
        if os != "android" {
            fp["device_model"] = "".into();
        } else if original.device_model.is_empty() {
            fp["device_model"] = data["ANDROID_MODELS"][0].clone();
        }
    }
    let ua = fingerprint::user_agent(
        &os,
        &chrome,
        fp["os_version"].as_str().unwrap_or(""),
        fp["device_model"].as_str().unwrap_or(""),
    )
    .map_err(io::Error::other)?;
    apply(&mut fp, "user_agent", ua.into(), "userAgent", &mut changed);
    for (key, source, label) in [
        ("hardware_concurrency", "cores", "cores"),
        ("device_memory", "memory", "memory"),
        ("max_touch_points", "maxTouchPoints", "maxTouchPoints"),
        ("device_pixel_ratio", "devicePixelRatio", "devicePixelRatio"),
        ("cameras", "cameras", "cameras"),
        ("microphones", "microphones", "microphones"),
    ] {
        let fallback = if transition && seen[source].is_null() {
            match source {
                "maxTouchPoints" => data["TOUCH_POINTS"][&os][0].clone(),
                "devicePixelRatio" => data["PIXEL_RATIOS"][&os][0].clone(),
                _ => Value::Null,
            }
        } else {
            seen[source].clone()
        };
        apply(&mut fp, key, fallback, label, &mut changed);
    }
    if truth(&seen["webglRenderer"]) {
        apply(
            &mut fp,
            "webgl_renderer",
            seen["webglRenderer"].clone(),
            "webglRenderer",
            &mut changed,
        );
        fp["webgl_vendor"] = if truth(&seen["webglVendor"]) {
            seen["webglVendor"].clone()
        } else {
            vendor(seen["webglRenderer"].as_str().unwrap_or("")).into()
        };
    }
    if truth(&seen["screenW"])
        && truth(&seen["screenH"])
        && (fp["screen_width"] != seen["screenW"] || fp["screen_height"] != seen["screenH"])
    {
        for (screen, viewport, source) in [
            ("screen_width", "viewport_width", "screenW"),
            ("screen_height", "viewport_height", "screenH"),
        ] {
            let size = seen[source]
                .as_i64()
                .ok_or_else(|| io::Error::other("Observed screen size is not integral"))?;
            let current = fp[viewport]
                .as_i64()
                .filter(|value| *value != 0)
                .unwrap_or(size);
            fp[screen] = size.into();
            fp[viewport] = current.min(size).into();
        }
        changed.insert("screen".into());
    }
    if truth(&seen["languages"]) {
        apply(
            &mut fp,
            "languages",
            seen["languages"].clone(),
            "languages",
            &mut changed,
        );
    }
    if let Some(timezone) = seen["timezone"].as_str().filter(|value| !value.is_empty()) {
        let locales = data["LOCALES"]
            .as_object()
            .ok_or_else(|| io::Error::other("Locale catalog missing"))?;
        if let Some((locale, _)) = locales.iter().find(|(_, value)| value[0] == timezone) {
            apply(
                &mut fp,
                "timezone",
                timezone.into(),
                "timezone",
                &mut changed,
            );
            apply(
                &mut fp,
                "locale",
                locale.clone().into(),
                "locale",
                &mut changed,
            );
        }
    }
    let pool = data["FONTS"][&os].as_array().cloned().unwrap_or_default();
    let rendered = seen["fontsRendered"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut fonts = pool
        .iter()
        .filter(|font| rendered.contains(font))
        .cloned()
        .collect::<Vec<_>>();
    if fonts.is_empty() {
        fonts = pool;
    }
    fonts.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    let mut old = fp["fonts"].as_array().cloned().unwrap_or_default();
    old.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
    if old != fonts {
        fp["fonts"] = json!(fonts);
        changed.insert("fonts".into());
    }
    Ok((
        serde_json::from_value(fp).map_err(io::Error::other)?,
        changed.into_iter().collect(),
    ))
}
pub fn patch(current: &mut Fingerprint, aligned: &Fingerprint) -> io::Result<()> {
    let mut value = serde_json::to_value(&*current).map_err(io::Error::other)?;
    let aligned = serde_json::to_value(aligned).map_err(io::Error::other)?;
    for key in FIELDS {
        value[*key] = aligned[*key].clone();
    }
    *current = serde_json::from_value(value).map_err(io::Error::other)?;
    Ok(())
}
