use crate::{devices::catalog, models::Fingerprint};
use serde_json::{Value, json};

pub fn platform_version(os: &str, version: &str) -> Result<String, serde_json::Error> {
    let root = catalog()?;
    if let Some(versions) = root["catalog"]["OS_VERSIONS"][os].as_array() {
        for release in versions {
            if release[0] == version {
                return Ok(release[1].as_str().unwrap_or("").to_owned());
            }
        }
    }
    Ok(version.to_owned())
}

fn full_version(fp: &Fingerprint) -> String {
    if fp.chrome_version.matches('.').count() == 3 {
        fp.chrome_version.clone()
    } else {
        format!(
            "{}.0.0.0",
            fp.chrome_version.split('.').next().unwrap_or("")
        )
    }
}

pub fn metadata(fp: &Fingerprint) -> Result<Value, serde_json::Error> {
    let major = fp.chrome_version.split('.').next().unwrap_or("");
    let full = full_version(fp);
    let brands = [
        ("Chromium", major),
        ("Google Chrome", major),
        ("Not-A.Brand", "99"),
    ];
    let arm = fp.os == "android"
        || fp.os == "ios"
        || fp.os == "macos" && fp.webgl_renderer.contains("Apple");
    let platform = fp.ch_platform.trim_matches('"');
    Ok(json!({
        "brands": brands.iter().map(|(brand, version)| json!({"brand":brand,"version":version})).collect::<Vec<_>>(),
        "fullVersionList": brands.iter().map(|(brand, _)| json!({"brand":brand,"version":if *brand == "Not-A.Brand" { "99.0.0.0" } else { &full }})).collect::<Vec<_>>(),
        "platform": if platform.is_empty() { if fp.os == "ios" { "iOS" } else { "Unknown" } } else { platform },
        "platformVersion": platform_version(&fp.os, &fp.os_version)?,
        "architecture": if arm { "arm" } else { "x86" }, "model": fp.device_model,
        "mobile": fp.is_mobile, "bitness":"64", "wow64":false
    }))
}

pub fn headers(fp: &Fingerprint) -> Result<Value, serde_json::Error> {
    if fp.os == "ios" {
        return Ok(json!({"low":{},"high":{}}));
    }
    let m = metadata(fp)?;
    let format_brands = |list: &Value| -> String {
        list.as_array()
            .into_iter()
            .flatten()
            .map(|b| {
                format!(
                    "\"{}\";v=\"{}\"",
                    b["brand"].as_str().unwrap_or(""),
                    b["version"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let quoted = |name: &str| format!("\"{}\"", m[name].as_str().unwrap_or(""));
    Ok(json!({"low":{"sec-ch-ua":format_brands(&m["brands"]),
        "sec-ch-ua-mobile":if fp.is_mobile { "?1" } else { "?0" },"sec-ch-ua-platform":quoted("platform")},
        "high":{"sec-ch-ua-platform-version":quoted("platformVersion"),"sec-ch-ua-arch":quoted("architecture"),
        "sec-ch-ua-bitness":quoted("bitness"),"sec-ch-ua-model":quoted("model"),
        "sec-ch-ua-full-version":format!("\"{}\"",full_version(fp)),
        "sec-ch-ua-full-version-list":format_brands(&m["fullVersionList"]),"sec-ch-ua-wow64":"?0"}}))
}

pub fn webgl_plausible(os: &str, vendor: &str, renderer: &str) -> Result<bool, serde_json::Error> {
    let root = catalog()?;
    if root["catalog"]["WEBGL"][os].as_array().is_some_and(|pool| {
        pool.iter()
            .any(|pair| pair[0] == vendor && pair[1] == renderer)
    }) {
        return Ok(true);
    }
    let r = renderer.to_lowercase();
    if r.is_empty() {
        return Ok(false);
    }
    let d3d = r.contains("direct3d") || r.contains("d3d11") || r.contains("d3d9");
    let gl = r.contains("opengl");
    Ok(match os {
        "windows" => d3d,
        "macos" => {
            gl && !d3d
                && ["apple", "intel", "amd", "metal", "opengl 4"]
                    .iter()
                    .any(|v| r.contains(v))
        }
        "linux" => gl && !d3d,
        "android" => ["opengl es", "adreno", "mali"]
            .iter()
            .any(|v| r.contains(v)),
        "ios" => r.contains("apple"),
        _ => false,
    })
}

pub fn validate(fp: &Fingerprint) -> Result<Vec<String>, serde_json::Error> {
    let root = catalog()?;
    let d = &root["catalog"];
    let mut issues = Vec::new();
    if d["NAV_PLATFORM"][&fp.os] != fp.platform {
        issues.push(format!(
            "platform '{}' does not match os '{}'",
            fp.platform, fp.os
        ));
    }
    if !webgl_plausible(&fp.os, &fp.webgl_vendor, &fp.webgl_renderer)? {
        issues.push("webgl vendor/renderer not valid for this os".into());
    }
    let screen = json!([fp.screen_width, fp.screen_height]);
    let curated = d["SCREENS"][&fp.os]
        .as_array()
        .is_some_and(|pool| pool.contains(&screen));
    if !curated
        && !((320..=8000).contains(&fp.screen_width) && (480..=5000).contains(&fp.screen_height))
    {
        issues.push("screen resolution not typical for this os".into());
    }
    if fp.viewport_width > fp.screen_width || fp.viewport_height > fp.screen_height {
        issues.push("viewport larger than screen".into());
    }
    if let Some(locale) = d["LOCALES"].get(&fp.locale) {
        if locale[0] != fp.timezone {
            issues.push("timezone does not match locale".into());
        }
        let base = |tag: &str| tag.split('-').next().unwrap_or("").to_lowercase();
        if fp
            .languages
            .first()
            .is_none_or(|v| base(v) != base(locale[1][0].as_str().unwrap_or("")))
        {
            issues.push("languages do not match locale".into());
        }
    } else {
        issues.push(format!("unknown locale '{}'", fp.locale));
    }
    let mobile = fp.os == "android" || fp.os == "ios";
    if fp.is_mobile != mobile {
        issues.push("is_mobile flag inconsistent with os".into());
    }
    if fp.is_mobile && fp.max_touch_points < 1 {
        issues.push("a phone with no touch points is implausible".into());
    }
    if !(0..=20).contains(&fp.max_touch_points) {
        issues.push("maxTouchPoints outside the range real devices report".into());
    }
    if fp
        .device_pixel_ratio
        .as_f64()
        .is_none_or(|value| !(0.5..=4.0).contains(&value))
    {
        issues.push("devicePixelRatio outside the range real displays report".into());
    }
    if fp.os == "ios" && fp.device_memory.as_f64().is_some_and(|v| v != 0.0) {
        issues.push("iOS does not expose navigator.deviceMemory".into());
    }
    if !fp.fonts.is_empty() {
        let mut alien: Vec<_> = fp
            .fonts
            .iter()
            .filter(|f| {
                !d["FONTS"][&fp.os]
                    .as_array()
                    .is_some_and(|pool| pool.iter().any(|v| v == *f))
            })
            .cloned()
            .collect();
        alien.sort();
        if !alien.is_empty() {
            issues.push(format!(
                "fonts not found on {}: {}",
                fp.os,
                alien.iter().take(3).cloned().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    if fp.cameras < 0 || fp.microphones < 0 {
        issues.push("media device counts cannot be negative".into());
    } else if fp.is_mobile && (fp.cameras == 0 || fp.microphones == 0) {
        issues.push("a phone with no camera or microphone is implausible".into());
    }
    Ok(issues)
}
