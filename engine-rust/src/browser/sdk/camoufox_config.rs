// SPDX-License-Identifier: MPL-2.0
// Camoufox launch identity preparation, ported from daijro/camoufox
// 079a6ce8249bab98063e5ac9241e3879a3e6578d, fingerprints.py and utils.py.
use super::camoufox_identity::Os;
use regex::Regex;
use serde_json::{Map, Value};
use std::{io, sync::LazyLock};

#[derive(Clone, Copy)]
pub struct Display {
    pub width: i64,
    pub height: i64,
}
pub fn host_os() -> Os {
    if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Linux
    }
}
pub fn os_name(os: Os) -> &'static str {
    match os {
        Os::Windows => "windows",
        Os::Macos => "macos",
        Os::Linux => "linux",
    }
}
pub fn parse_os(value: &str) -> io::Result<Os> {
    match value {
        "windows" => Ok(Os::Windows),
        "macos" => Ok(Os::Macos),
        "linux" => Ok(Os::Linux),
        _ => Err(io::Error::other("Camoufox identity OS is unsupported")),
    }
}
fn truth(value: &Value) -> bool {
    crate::conversion::truth(value)
}
fn rewrite_version(text: &str, major: u32) -> String {
    static VERSION: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"1[0-9]{2}\.0").expect("Constant version pattern"));
    let mut result = String::new();
    let mut copied = 0;
    for found in VERSION.find_iter(text) {
        if text[..found.start()]
            .chars()
            .next_back()
            .is_some_and(char::is_numeric)
            || text[found.end()..]
                .chars()
                .next()
                .is_some_and(char::is_numeric)
        {
            continue;
        }
        result.push_str(&text[copied..found.start()]);
        result.push_str(&format!("{major}.0"));
        copied = found.end();
    }
    result.push_str(&text[copied..]);
    result
}
fn app_version(ua: &str) -> Option<String> {
    let block = ua.strip_prefix("Mozilla/5.0 (")?.split_once(')')?.0;
    let kept: Vec<_> = block
        .split(';')
        .map(str::trim)
        .filter_map(|token| {
            if token.starts_with("rv:")
                || token.starts_with("Linux ")
                || token.starts_with("Intel Mac OS X")
                || ["Win64", "x64", "Mobile", "Tablet"].contains(&token)
            {
                None
            } else if token.starts_with("Windows") {
                Some("Windows")
            } else {
                Some(token)
            }
        })
        .collect();
    (!kept.is_empty()).then(|| format!("5.0 ({})", kept.join("; ")))
}
fn number(config: &Map<String, Value>, key: &str) -> Option<i64> {
    config.get(key).and_then(Value::as_i64)
}
/// Convert the fields that upstream fpgen.yml maps; browser-owned defaults such
/// as productSub, inner geometry, privacy headers and history remain browser-owned.
pub fn from_fpgen(fp: &Value, major: u32) -> io::Result<Map<String, Value>> {
    let mut config = Map::new();
    for (group, names) in [
        (
            "navigator",
            &[
                "userAgent",
                "appVersion",
                "oscpu",
                "platform",
                "hardwareConcurrency",
                "maxTouchPoints",
            ][..],
        ),
        (
            "screen",
            &[
                "availLeft",
                "availTop",
                "availWidth",
                "availHeight",
                "height",
                "width",
                "colorDepth",
                "pixelDepth",
            ][..],
        ),
        (
            "window",
            &["outerHeight", "outerWidth", "screenX", "screenY"][..],
        ),
    ] {
        for name in names {
            let value = &fp[group][*name];
            if !truth(value) {
                continue;
            }
            let value = if let Some(text) = value.as_str() {
                rewrite_version(text, major).into()
            } else if group == "screen" && value.as_i64().is_some_and(|v| v < 0) {
                0.into()
            } else {
                value.clone()
            };
            config.insert(format!("{group}.{name}"), value);
        }
    }
    // Accept-Encoding is mapped by upstream, then deliberately removed for
    // generated identities: Firefox must negotiate its HTTP/HTTPS encodings.
    if !config.contains_key("window.screenY") {
        let x = fp["window"]["screenX"].as_i64().unwrap_or(0);
        let y = if (-50..=50).contains(&x) {
            x
        } else {
            let available = fp["screen"]["availHeight"].as_i64().unwrap_or(0)
                - fp["window"]["outerHeight"].as_i64().unwrap_or(0);
            random_between(available.min(0), available.max(0))?
        };
        config.insert("window.screenX".into(), x.into());
        config.insert("window.screenY".into(), y.into());
    }
    if let Some(derived) = config
        .get("navigator.userAgent")
        .and_then(Value::as_str)
        .and_then(app_version)
    {
        config.insert("navigator.appVersion".into(), derived.into());
    }
    Ok(config)
}
fn random_between(low: i64, high: i64) -> io::Result<i64> {
    if low == high {
        return Ok(low);
    }
    let size = u64::try_from(
        high.checked_sub(low)
            .ok_or_else(|| io::Error::other("Invalid fingerprint position"))?,
    )
    .map_err(|_| io::Error::other("Invalid fingerprint position"))?;
    let limit = u64::MAX - u64::MAX % size;
    loop {
        let mut bytes = [0; 8];
        getrandom::fill(&mut bytes)
            .map_err(|_| io::Error::other("Fingerprint entropy unavailable"))?;
        let value = u64::from_le_bytes(bytes);
        if value < limit {
            return Ok(low + (value % size) as i64);
        }
    }
}
pub fn normalize(
    config: &mut Map<String, Value>,
    os: Os,
    cpus: Option<u32>,
    display: Option<Display>,
) {
    if os == Os::Linux {
        let ua = config
            .get("navigator.userAgent")
            .and_then(Value::as_str)
            .unwrap_or("");
        let architecture = if ua.contains("Linux x86_64") {
            Some("Linux x86_64")
        } else if ua.contains("Linux i686") {
            Some("Linux i686")
        } else {
            None
        };
        if let Some(architecture) = architecture {
            config.insert("navigator.platform".into(), architecture.into());
            config.insert("navigator.oscpu".into(), architecture.into());
        }
    }
    if let Some(cpus) = cpus.filter(|n| *n > 0) {
        let constants: Value =
            serde_json::from_str(include_str!("camoufox-data/launcher-constants.json"))
                .expect("Compiled launcher constants");
        let counts = constants["plausibleCoreCounts"]
            .as_array()
            .expect("Compiled core counts");
        let count = counts
            .iter()
            .filter_map(Value::as_u64)
            .rfind(|n| *n <= u64::from(cpus))
            .unwrap_or_else(|| counts[0].as_u64().expect("Compiled minimum core count"));
        config.insert("navigator.hardwareConcurrency".into(), count.into());
    }
    super::camoufox_coherence::repair_screen_orientation(config);
    super::camoufox_coherence::raise_screen_to_modern_floor(config);
    if let Some(display) = display {
        for (axis, cap, avail) in [
            ("width", display.width, "availWidth"),
            ("height", display.height, "availHeight"),
        ] {
            let key = format!("screen.{axis}");
            let avail = format!("screen.{avail}");
            if let Some(screen) = number(config, &key).filter(|screen| cap > 0 && *screen > cap) {
                if let Some(available) = number(config, &avail).filter(|n| *n != 0) {
                    config.insert(avail, (cap - (screen - available).max(0)).max(1).into());
                }
                config.insert(key, cap.into());
            }
        }
    }
    if let (Some(width), Some(height)) = (
        number(config, "screen.width"),
        number(config, "screen.height"),
    ) {
        let taskbar = match os {
            Os::Windows => 48,
            Os::Macos => 76,
            Os::Linux => 0,
        };
        config.insert("screen.availWidth".into(), width.into());
        config.insert("screen.availHeight".into(), (height - taskbar).into());
    }
    if number(config, "navigator.maxTouchPoints").is_some_and(|n| n > 1) {
        config.insert("navigator.maxTouchPoints".into(), 5.into());
    }
    for (axis, position) in [("Width", "window.screenX"), ("Height", "window.screenY")] {
        let screen = format!("screen.{}", axis.to_lowercase());
        let avail = format!("screen.avail{axis}");
        let outer = format!("window.outer{axis}");
        let inner = format!("window.inner{axis}");
        if let (Some(screen), Some(available)) = (number(config, &screen), number(config, &avail))
            && available > screen
        {
            config.insert(avail.clone(), screen.into());
        }
        if let (Some(outer_value), Some(cap)) = (
            number(config, &outer),
            number(config, &avail).or_else(|| number(config, &screen)),
        ) && outer_value > cap
            && cap != 0
        {
            if let Some(inner_value) = number(config, &inner).filter(|v| *v != 0) {
                let chrome = (outer_value - inner_value).max(0);
                config.insert(inner.clone(), (cap - chrome).max(1).into());
            }
            config.insert(outer.clone(), cap.into());
        }
        if let (Some(inner_value), Some(outer_value)) =
            (number(config, &inner), number(config, &outer))
            && inner_value > outer_value
        {
            config.insert(inner, outer_value.into());
        }
        if let (Some(screen), Some(outer), Some(position_value)) = (
            number(config, &screen),
            number(config, &outer),
            number(config, position),
        ) {
            config.insert(
                position.into(),
                position_value.min(screen - outer).max(0).into(),
            );
        }
    }
}
pub fn identity_seed(config: &Map<String, Value>, salt: u64) -> u32 {
    static VERSION: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(rv:|Firefox/)[0-9][0-9.]*").expect("Constant UA version pattern")
    });
    let text = |key| {
        config
            .get(key)
            .map(crate::conversion::value_text)
            .unwrap_or_default()
    };
    let identity = [
        VERSION
            .replace_all(&text("navigator.userAgent"), "$1")
            .into_owned(),
        text("navigator.platform"),
        text("screen.width"),
        text("screen.height"),
        text("navigator.hardwareConcurrency"),
        salt.to_string(),
    ]
    .join("|");
    let mut crc = !0u32;
    for byte in identity.bytes() {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}
pub fn chunk_environment(prefix: &str, text: &str, windows: bool) -> Map<String, Value> {
    let size = if windows { 2047 } else { 32767 };
    let mut output = Map::new();
    let mut chunk = String::new();
    let mut count = 0;
    for c in text.chars() {
        if count == size {
            output.insert(
                format!("{prefix}_{}", output.len() + 1),
                std::mem::take(&mut chunk).into(),
            );
            count = 0;
        }
        chunk.push(c);
        count += 1;
    }
    if !chunk.is_empty() {
        output.insert(format!("{prefix}_{}", output.len() + 1), chunk.into());
    }
    output
}
pub fn ascii_json(value: &Value) -> io::Result<String> {
    let raw = serde_json::to_string(value).map_err(io::Error::other)?;
    let mut result = String::new();
    for c in raw.chars() {
        if c as u32 >= 0x7f {
            for unit in c.encode_utf16(&mut [0; 2]) {
                result.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            result.push(c);
        }
    }
    Ok(result)
}
