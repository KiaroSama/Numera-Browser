// SPDX-License-Identifier: MPL-2.0
// Native port of Camoufox whole-identity coherence and GPU/screen constraints.
// Source: daijro/camoufox, 079a6ce8249bab98063e5ac9241e3879a3e6578d.
use super::camoufox_identity::Os;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::{collections::HashSet, sync::OnceLock};

pub const BROWSER_CHROME_HEIGHT: i64 = 86;
pub const CSS_GEOMETRY_KEYS: &[&str] = &[
    "screen.width",
    "screen.height",
    "screen.availWidth",
    "screen.availHeight",
    "screen.availLeft",
    "screen.availTop",
    "window.outerWidth",
    "window.outerHeight",
    "window.innerWidth",
    "window.innerHeight",
    "window.screenX",
    "window.screenY",
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Violation {
    pub rule: String,
    pub detail: String,
    pub kept: Vec<String>,
}
fn constants() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("camoufox-data/launcher-constants.json"))
            .expect("Compiled Camoufox constants are valid JSON")
    })
}
fn number(config: &Map<String, Value>, key: &str) -> Option<f64> {
    config.get(key).and_then(Value::as_f64)
}
fn integer(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_bool().map(i64::from))
}
fn float(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_bool().map(|v| if v { 1.0 } else { 0.0 }))
        .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
}
fn numeric(value: f64, integer: bool) -> Value {
    if integer {
        json!(value as i64)
    } else {
        json!(value)
    }
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64().is_some_and(|v| v != 0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn renderer(config: &Map<String, Value>) -> &str {
    config
        .get("webGl:renderer")
        .and_then(Value::as_str)
        .unwrap_or("")
}
fn repr_str(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(quote);
    for c in value.chars() {
        match c {
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c == quote => {
                result.push('\\');
                result.push(c);
            }
            c => result.push(c),
        }
    }
    result.push(quote);
    result
}
fn display(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "None".into(),
        Some(Value::Bool(v)) => if *v { "True" } else { "False" }.into(),
        Some(Value::String(v)) => v.clone(),
        Some(value) => value.to_string(),
    }
}
fn allowed(key: &str, value: i64) -> bool {
    constants()[key]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|v| v.as_i64() == Some(value)))
}
pub fn gpu_fits_os(renderer: Option<&str>, os: Os) -> bool {
    let renderer = renderer.unwrap_or("");
    if renderer.is_empty() {
        return true;
    }
    match os {
        Os::Macos => !renderer.contains("ANGLE") && !renderer.contains("llvmpipe"),
        Os::Windows => renderer.starts_with("ANGLE"),
        Os::Linux => !renderer.contains("ANGLE") && !renderer.contains("Apple M"),
    }
}
fn intel_mac_misfit(
    renderer: &str,
    cores: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
) -> Option<String> {
    if renderer.is_empty() || renderer.contains("Apple M") {
        return None;
    }
    if let Some(cores) = cores {
        let fits = allowed("intelMacIgpCores", cores)
            || (!renderer.contains("Intel") && allowed("intelMacDgpuExtraCores", cores));
        if !fits {
            return Some(format!(
                "{} with hardwareConcurrency {cores}; no Intel Mac with that GPU reports it",
                repr_str(renderer)
            ));
        }
    }
    let silicon_panel = constants()["appleSiliconPanels"]
        .as_object()
        .is_some_and(|groups| {
            groups
                .values()
                .filter_map(Value::as_array)
                .flatten()
                .any(|panel| {
                    width.is_some()
                        && height.is_some()
                        && panel[0].as_i64() == width
                        && panel[1].as_i64() == height
                })
        });
    if silicon_panel {
        return Some(format!(
            "{} behind a {}x{} panel, which only Apple Silicon Macs have",
            repr_str(renderer),
            width.unwrap_or(0),
            height.unwrap_or(0)
        ));
    }
    None
}
pub fn gpu_fits_machine(
    renderer: Option<&str>,
    os: Os,
    cores: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
) -> bool {
    os != Os::Macos || intel_mac_misfit(renderer.unwrap_or(""), cores, width, height).is_none()
}
pub fn is_software_renderer(renderer: Option<&str>) -> bool {
    renderer.is_some_and(|r| {
        [
            "llvmpipe",
            "Microsoft Basic Render Driver",
            "SwiftShader",
            "Generic Renderer",
        ]
        .iter()
        .any(|s| r.contains(s))
    })
}
pub fn gpu_screen_is_plausible(
    renderer: Option<&str>,
    width: Option<i64>,
    height: Option<i64>,
) -> bool {
    let Some(renderer) = renderer.filter(|v| !v.is_empty()) else {
        return true;
    };
    let (Some(width), Some(height)) = (width.filter(|v| *v != 0), height.filter(|v| *v != 0))
    else {
        return true;
    };
    if is_software_renderer(Some(renderer)) {
        return true;
    }
    let mut core = renderer.strip_suffix(", or similar").unwrap_or(renderer);
    if let Some(angle) = core.strip_prefix("ANGLE (") {
        if let Some(d3d) = angle.strip_suffix(')') {
            if let Some((_, device)) = d3d.split_once(", ")
                && let Some((device, _)) = device.split_once(" Direct3D")
            {
                core = device;
            }
        } else if let Some(vulkan) = angle.strip_suffix(") on Vulkan") {
            core = vulkan;
        }
    }
    core = core
        .strip_suffix("/PCIe/SSE2")
        .or_else(|| core.strip_suffix("/PCI/SSE2"))
        .unwrap_or(core);
    core = core.strip_prefix("NVIDIA ").unwrap_or(core);
    ![
        "GeForce 8800 GTX",
        "GeForce GTX 480",
        "GeForce GTX 980",
        "Radeon R9 200 Series",
    ]
    .contains(&core)
        || i128::from(width) * i128::from(height) > 1024 * 600
}
fn dpr_steps(os: Os) -> &'static [Value] {
    constants()["plausibleDpr"][os.key()]
        .as_array()
        .expect("Compiled DPR steps are an array")
}
pub fn is_zoomed_ratio(dpr: &Value, os: Os) -> bool {
    dpr.as_f64()
        .is_some_and(|d| d > 0.0 && !dpr_steps(os).iter().any(|v| v.as_f64() == Some(d)))
}
pub fn repair_device_pixel_ratio(config: &mut Map<String, Value>, os: Os, target: Option<f64>) {
    let Some(dpr) = config.get("window.devicePixelRatio").and_then(float) else {
        return;
    };
    let Some(nearest) = dpr_steps(os).iter().min_by(|a, b| {
        (a.as_f64().unwrap_or(1.0) - dpr)
            .abs()
            .total_cmp(&(b.as_f64().unwrap_or(1.0) - dpr).abs())
    }) else {
        return;
    };
    let step = target
        .filter(|v| *v != 0.0)
        .map_or_else(|| nearest.clone(), |v| json!(v));
    let scale = dpr / step.as_f64().unwrap_or(1.0);
    config.insert("window.devicePixelRatio".into(), step);
    for key in CSS_GEOMETRY_KEYS {
        if let Some(value) = number(config, key) {
            config.insert(
                (*key).into(),
                json!((value * scale).round_ties_even() as i64),
            );
        }
    }
}
pub fn screen_is_implausible(config: &Map<String, Value>) -> bool {
    check(8, config, Os::Linux).is_some()
}
pub fn repair_screen_orientation(config: &mut Map<String, Value>) -> bool {
    let (Some(width), Some(height)) = (
        number(config, "screen.width"),
        number(config, "screen.height"),
    ) else {
        return false;
    };
    if width == 0.0 || height <= width {
        return false;
    }
    for (w, h) in [
        ("screen.width", "screen.height"),
        ("screen.availWidth", "screen.availHeight"),
    ] {
        if config.get(w).is_some_and(truth) && config.get(h).is_some_and(truth) {
            let a = config[w].clone();
            let b = config[h].clone();
            config.insert(w.into(), b);
            config.insert(h.into(), a);
        }
    }
    true
}
pub fn raise_screen_to_modern_floor(config: &mut Map<String, Value>) {
    let (Some(sw), Some(sh)) = (
        number(config, "screen.width"),
        number(config, "screen.height"),
    ) else {
        return;
    };
    if sw == 0.0 || sh == 0.0 || (sw >= 1366.0 && sh >= 768.0) {
        return;
    }
    for (key, avail, old, floor) in [
        ("screen.width", "screen.availWidth", sw, 1366.0),
        ("screen.height", "screen.availHeight", sh, 768.0),
    ] {
        let original = config[key].clone();
        let new = if old >= floor {
            original.clone()
        } else {
            json!(floor as i64)
        };
        let size = new.as_f64().unwrap_or(floor);
        if let Some(value) = number(config, avail).filter(|v| *v != 0.0) {
            let integral = original.as_i64().is_some() && config[avail].as_i64().is_some();
            config.insert(
                avail.into(),
                numeric((size - (old - value).max(0.0)).max(1.0), integral),
            );
        }
        config.insert(key.into(), new);
    }
}
const RULES: &[&str] = &[
    "apple-silicon-cores",
    "gpu-matches-os",
    "intel-mac-hardware",
    "color-depth",
    "pixel-depth",
    "touch-points",
    "device-pixel-ratio",
    "window-chrome",
    "screen-shape",
    "avail-bounds",
    "arch-agreement",
    "cpu-matches-hardware",
];
fn check(rule: usize, c: &Map<String, Value>, os: Os) -> Option<String> {
    let r = renderer(c);
    match rule {
        0 if r.contains("Apple M") => {
            let v = c.get("navigator.hardwareConcurrency")?;
            let cores = integer(v)?;
            (!allowed("appleSiliconCores", cores)).then(|| {
                format!(
                    "{} with hardwareConcurrency {}; Apple Silicon starts at 8",
                    repr_str(r),
                    display(Some(v))
                )
            })
        }
        1 if !gpu_fits_os(Some(r), os) => Some(match os {
            Os::Macos => format!("macOS identity with {}, which no Mac reports", repr_str(r)),
            Os::Windows => format!(
                "Windows identity with {}; Firefox on Windows renders through ANGLE",
                repr_str(r)
            ),
            Os::Linux => format!("Linux identity with {}", repr_str(r)),
        }),
        2 if os == Os::Macos => intel_mac_misfit(
            r,
            c.get("navigator.hardwareConcurrency").and_then(integer),
            c.get("screen.width").and_then(integer),
            c.get("screen.height").and_then(integer),
        ),
        3 => {
            let v = c.get("screen.colorDepth").filter(|v| !v.is_null())?;
            let d = v.as_f64();
            if d != Some(24.0) && d != Some(30.0) {
                Some(format!(
                    "screen.colorDepth {}; Firefox reports 24 or 30",
                    display(Some(v))
                ))
            } else if os == Os::Macos && d != Some(30.0) {
                Some(format!(
                    "macOS identity with colorDepth {}; deep colour is the macOS default",
                    display(Some(v))
                ))
            } else {
                None
            }
        }
        4 => {
            let p = c.get("screen.pixelDepth").filter(|v| !v.is_null())?;
            let color = c.get("screen.colorDepth").cloned().unwrap_or(json!(24));
            (p != &color && !(p.as_f64().is_some() && p.as_f64() == color.as_f64())).then(|| {
                format!(
                    "screen.pixelDepth {} differs from colorDepth {}",
                    display(Some(p)),
                    display(Some(&color))
                )
            })
        }
        5 => {
            let v = c.get("navigator.maxTouchPoints").filter(|v| !v.is_null())?;
            match integer(v) {
                Some(t) if (0..=10).contains(&t) => (os == Os::Macos && t != 0).then(|| {
                    format!(
                        "macOS identity with maxTouchPoints {}; no Mac has a touchscreen",
                        display(Some(v))
                    )
                }),
                _ => Some(format!(
                    "navigator.maxTouchPoints {}; a digitiser reports at most 10",
                    display(Some(v))
                )),
            }
        }
        6 => {
            let v = c.get("window.devicePixelRatio").filter(|v| !v.is_null())?;
            let ratio = float(v);
            (!dpr_steps(os).iter().any(|v| v.as_f64() == ratio)).then(|| {
                format!(
                    "window.devicePixelRatio {} is not a display mode {} offers",
                    display(Some(v)),
                    os.key()
                )
            })
        }
        7 => {
            let inner = number(c, "window.innerHeight").filter(|v| *v != 0.0)?;
            let outer = number(c, "window.outerHeight").filter(|v| *v != 0.0)?;
            (outer-inner < BROWSER_CHROME_HEIGHT as f64).then(|| format!("window.outerHeight {} - innerHeight {} = {}, less than the 86px of chrome the window actually has; the bottom {}px of the claimed viewport cannot receive input",display(c.get("window.outerHeight")),display(c.get("window.innerHeight")),fmt_num(outer-inner,c),fmt_num(86.0-(outer-inner),c)))
        }
        8 => {
            let width = number(c, "screen.width").filter(|v| *v != 0.0)?;
            let height = number(c, "screen.height").filter(|v| *v != 0.0)?;
            if height > width {
                Some(format!(
                    "portrait screen {}x{}; desktop panels are landscape",
                    display(c.get("screen.width")),
                    display(c.get("screen.height"))
                ))
            } else if width < 1024.0 {
                Some(format!(
                    "screen {}x{} is smaller than any current desktop panel",
                    display(c.get("screen.width")),
                    display(c.get("screen.height"))
                ))
            } else {
                None
            }
        }
        9 => {
            for axis in ["Width", "Height"] {
                let full = format!("screen.{}", axis.to_lowercase());
                let avail = format!("screen.avail{axis}");
                if let (Some(w), Some(a)) = (number(c, &full), number(c, &avail))
                    && w != 0.0
                    && a != 0.0
                    && a > w
                {
                    return Some(format!(
                        "{avail} {} exceeds {full} {}",
                        display(c.get(&avail)),
                        display(c.get(&full))
                    ));
                }
            }
            None
        }
        10 => {
            let ua = c
                .get("navigator.userAgent")
                .and_then(Value::as_str)
                .unwrap_or("");
            let p = c
                .get("navigator.platform")
                .and_then(Value::as_str)
                .unwrap_or("");
            let o = c
                .get("navigator.oscpu")
                .and_then(Value::as_str)
                .unwrap_or("");
            (ua.contains("x86_64") && (p.contains("armv") || o.contains("armv"))).then(|| {
                format!(
                    "user agent claims x86_64 while platform/oscpu say {}/{}",
                    repr_str(p),
                    repr_str(o)
                )
            })
        }
        11 => super::camoufox_cpu::conflict(c, os),
        _ => None,
    }
}
fn fmt_num(value: f64, config: &Map<String, Value>) -> String {
    numeric(
        value,
        config["window.innerHeight"].as_i64().is_some()
            && config["window.outerHeight"].as_i64().is_some(),
    )
    .to_string()
}
fn repair(rule: usize, c: &mut Map<String, Value>, os: Os) {
    match rule {
        0 => {
            if let Some(cores) = c.get("navigator.hardwareConcurrency").and_then(integer) {
                let rows = constants()["appleSiliconCores"]
                    .as_array()
                    .expect("Compiled core counts are an array");
                if let Some(value) = rows
                    .iter()
                    .find(|v| v.as_i64().is_some_and(|v| v >= cores))
                    .or_else(|| rows.last())
                {
                    c.insert("navigator.hardwareConcurrency".into(), value.clone());
                }
            }
        }
        3 => {
            if os == Os::Macos {
                c.insert("screen.colorDepth".into(), json!(30));
            } else if ![Some(24.0), Some(30.0)].contains(&number(c, "screen.colorDepth")) {
                c.insert("screen.colorDepth".into(), json!(24));
            }
            if !c.contains_key("screen.pixelDepth") {
                c.insert("screen.pixelDepth".into(), c["screen.colorDepth"].clone());
            }
        }
        4 => {
            c.insert(
                "screen.pixelDepth".into(),
                c.get("screen.colorDepth").cloned().unwrap_or(json!(24)),
            );
        }
        5 => {
            let v = c.get("navigator.maxTouchPoints");
            if os == Os::Macos
                || v.and_then(integer).is_some_and(|v| v > 10)
                || v.is_some_and(|v| !v.is_null() && integer(v).is_none())
            {
                c.insert("navigator.maxTouchPoints".into(), json!(0));
            }
        }
        6 => repair_device_pixel_ratio(c, os, None),
        7 => {
            if let (Some(inner), Some(outer)) = (
                number(c, "window.innerHeight"),
                number(c, "window.outerHeight"),
            ) {
                let avail = number(c, "screen.availHeight")
                    .filter(|v| *v != 0.0)
                    .or_else(|| number(c, "screen.height"));
                if avail.is_none_or(|v| v == 0.0 || inner + 86.0 <= v) {
                    let value = numeric(inner + 86.0, c["window.innerHeight"].as_i64().is_some());
                    c.insert("window.outerHeight".into(), value);
                } else {
                    let value = numeric(
                        (outer - 86.0).max(1.0),
                        c["window.outerHeight"].as_i64().is_some(),
                    );
                    c.insert("window.innerHeight".into(), value);
                }
            }
        }
        9 => {
            for axis in ["Width", "Height"] {
                let full = format!("screen.{}", axis.to_lowercase());
                let avail = format!("screen.avail{axis}");
                if let (Some(w), Some(a)) = (number(c, &full), number(c, &avail))
                    && w != 0.0
                    && a > w
                {
                    c.insert(avail, c[&full].clone());
                }
            }
        }
        11 => super::camoufox_cpu::repair(c, os),
        _ => (),
    }
}
fn changed(
    before: &Map<String, Value>,
    after: &Map<String, Value>,
    keep: &HashSet<String>,
) -> Vec<String> {
    let mut out = keep
        .iter()
        .filter(|k| before.get(*k) != after.get(*k))
        .cloned()
        .collect::<Vec<_>>();
    out.sort();
    out
}
pub fn validate(config: &Map<String, Value>, os: Os) -> Vec<Violation> {
    RULES
        .iter()
        .enumerate()
        .filter_map(|(i, name)| {
            check(i, config, os).map(|detail| Violation {
                rule: (*name).into(),
                detail,
                kept: Vec::new(),
            })
        })
        .collect()
}
pub fn apply(config: &mut Map<String, Value>, os: Os, keep: &HashSet<String>) -> Vec<Violation> {
    let mut blocked = std::collections::HashMap::new();
    for (i, name) in RULES.iter().enumerate() {
        if [1, 2, 8, 10].contains(&i) || check(i, config, os).is_none() {
            continue;
        }
        let mut repaired = config.clone();
        repair(i, &mut repaired, os);
        if i == 6 && CSS_GEOMETRY_KEYS.iter().any(|k| keep.contains(*k)) {
            for key in CSS_GEOMETRY_KEYS {
                if let Some(v) = config.get(*key) {
                    repaired.insert((*key).into(), v.clone());
                } else {
                    repaired.remove(*key);
                }
            }
        }
        let keys = changed(config, &repaired, keep);
        if keys.is_empty() {
            *config = repaired;
        } else {
            blocked.insert(*name, keys);
        }
    }
    validate(config, os)
        .into_iter()
        .map(|mut v| {
            v.kept = blocked.remove(v.rule.as_str()).unwrap_or_default();
            v
        })
        .collect()
}
pub fn drop_incoherent_source_values(
    config: &mut Map<String, Value>,
    os: Os,
    keep: &HashSet<String>,
) -> Vec<Violation> {
    let r = renderer(config).to_owned();
    if r.is_empty() {
        return Vec::new();
    }
    let rule = if !gpu_fits_os(Some(&r), os) {
        "gpu-matches-os"
    } else if check(2, config, os).is_some() {
        "intel-mac-hardware"
    } else {
        return Vec::new();
    };
    let mut kept = ["webGl:renderer", "webGl:vendor"]
        .into_iter()
        .filter(|k| keep.contains(*k) && config.contains_key(*k))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    kept.sort();
    let detail = if kept.is_empty() {
        config.remove("webGl:renderer");
        config.remove("webGl:vendor");
        format!("dropped {} for a {} identity", repr_str(&r), os.key())
    } else {
        format!("{} does not fit this {} identity", repr_str(&r), os.key())
    };
    vec![Violation {
        rule: rule.into(),
        detail,
        kept,
    }]
}
