// SPDX-License-Identifier: MPL-2.0
// Camoufox CPU identity preparation from daijro/camoufox
// df9ca1156abf94bf5e8e7764effd35d3a7b298f9 (coherence/fingerprints/utils).
// Browser emulation is available only when the selected package declares these
// properties. The official pinned Firefox158 package does not contain it.
use super::{camoufox_config, camoufox_identity::Os};
use crate::python_random::PythonRandom;
use serde_json::{Map, Value, json};
use std::{io, sync::OnceLock};
pub const SOURCE_COMMIT: &str = "df9ca1156abf94bf5e8e7764effd35d3a7b298f9";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86,
    Arm,
}
impl Arch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::Arm => "arm",
        }
    }
    fn parse(value: &Value) -> io::Result<Self> {
        match value.as_str() {
            Some("x86") => Ok(Self::X86),
            Some("arm") => Ok(Self::Arm),
            _ => Err(bad("Camoufox cpu arch must be x86 or arm")),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Cpu {
    pub arch: Option<Arch>,
    pub manufacturer: Option<String>,
}
impl Cpu {
    pub fn config(&self) -> Map<String, Value> {
        let mut out = Map::new();
        if let Some(arch) = self.arch {
            out.insert("cpu:arch".into(), arch.as_str().into());
        }
        if let Some(maker) = &self.manufacturer {
            out.insert("cpu:manufacturer".into(), maker.clone().into());
        }
        out
    }
}
fn bad(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn constants() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("camoufox-data/launcher-constants.json"))
            .expect("Compiled Camoufox CPU constants")
    })
}
fn os_key(os: Os) -> &'static str {
    match os {
        Os::Windows => "windows",
        Os::Macos => "mac",
        Os::Linux => "linux",
    }
}
pub fn manufacturers(os: Os, arch: Arch) -> Vec<&'static str> {
    constants()["cpuManufacturers"][os_key(os)][arch.as_str()]
        .as_array()
        .map_or_else(Vec::new, |rows| {
            rows.iter().filter_map(Value::as_str).collect()
        })
}
pub fn resolve(value: &Value, os: Os) -> io::Result<Cpu> {
    let value = value
        .as_object()
        .ok_or_else(|| bad("Camoufox cpu must be an object"))?;
    if value
        .keys()
        .any(|key| !["arch", "manufacturer"].contains(&key.as_str()))
    {
        return Err(bad("Camoufox cpu accepts only arch and manufacturer"));
    }
    let mut arch = value
        .get("arch")
        .filter(|v| !v.is_null())
        .map(Arch::parse)
        .transpose()?;
    let maker = value
        .get("manufacturer")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_str()
                .ok_or_else(|| bad("Camoufox cpu manufacturer must be text"))
        })
        .transpose()?;
    if let Some(maker) = maker {
        let allowed = [Arch::X86, Arch::Arm]
            .into_iter()
            .filter(|a| manufacturers(os, *a).contains(&maker))
            .collect::<Vec<_>>();
        if allowed.is_empty() {
            return Err(bad(
                "Camoufox cpu manufacturer is unavailable on the selected OS",
            ));
        }
        if let Some(arch) = arch {
            if !allowed.contains(&arch) {
                return Err(bad(
                    "Camoufox cpu manufacturer contradicts its architecture",
                ));
            }
        } else {
            arch = allowed.first().copied();
        }
    } else if arch.is_some_and(|a| manufacturers(os, a).is_empty()) {
        return Err(bad(
            "Camoufox cpu architecture is unavailable on the selected OS",
        ));
    }
    Ok(Cpu {
        arch,
        manufacturer: maker.map(str::to_owned),
    })
}
pub fn from_config(config: &Map<String, Value>, os: Os) -> io::Result<Cpu> {
    let mut input = Map::new();
    for (key, target) in [("cpu:arch", "arch"), ("cpu:manufacturer", "manufacturer")] {
        if let Some(value) = config.get(key) {
            input.insert(target.into(), value.clone());
        }
    }
    resolve(&Value::Object(input), os)
}
pub fn supported(schema: &Value) -> bool {
    schema.as_array().is_some_and(|entries| {
        ["cpu:arch", "cpu:manufacturer"].iter().all(|key| {
            entries.iter().any(|entry| {
                entry["property"] == *key
                    && entry["type"] == "str"
                    && entry.get("removed").is_none()
            })
        })
    })
}
/// Keep old official browser packages launchable and avoid claiming emulation
/// merely because a preference name is accepted by stock Firefox.
pub fn prepare(
    config: &mut Map<String, Value>,
    requested: Option<&Value>,
    os: Os,
    available: bool,
) -> io::Result<Option<Cpu>> {
    let requested = requested.filter(|v| !v.is_null());
    if !available {
        if requested.is_some()
            || ["cpu:arch", "cpu:manufacturer"]
                .iter()
                .any(|k| config.contains_key(*k))
        {
            return Err(bad("Selected Camoufox build does not support CPU identity"));
        }
        return Ok(None);
    }
    if let Some(requested) = requested {
        config.extend(resolve(requested, os)?.config());
    }
    from_config(config, os).map(Some)
}
pub fn gpu_cpu(renderer: Option<&str>, os: Os) -> (Option<Arch>, Vec<&'static str>) {
    let renderer = renderer.unwrap_or("");
    if renderer.is_empty() {
        return (None, Vec::new());
    }
    for (marker, makers) in [
        ("Apple M", &["apple"][..]),
        ("Apple GPU", &["apple"][..]),
        ("Adreno", &["qualcomm"][..]),
        ("V3D", &["broadcom"][..]),
        ("VideoCore", &["broadcom"][..]),
        (
            "Mali",
            &["rockchip", "mediatek", "amlogic", "allwinner"][..],
        ),
        ("Immortalis", &["mediatek"][..]),
        ("Tegra", &["nvidia"][..]),
    ] {
        if renderer.contains(marker) {
            return (Some(Arch::Arm), makers.to_vec());
        }
    }
    if os == Os::Macos
        || [
            "Intel(R) HD Graphics, or similar",
            "Intel(R) HD Graphics 400, or similar",
            "Intel(R) HD Graphics Direct3D11",
            "Intel(R) HD Graphics 400 Direct3D11",
        ]
        .iter()
        .any(|igpu| renderer.contains(igpu))
    {
        return (Some(Arch::X86), vec!["intel"]);
    }
    if renderer.contains("Radeon HD 3200 Graphics") {
        return (Some(Arch::X86), vec!["amd"]);
    }
    (None, Vec::new())
}
pub fn gpu_fits(renderer: Option<&str>, os: Os, cpu: &Cpu) -> bool {
    let (by_gpu, makers) = gpu_cpu(renderer, os);
    let arch = cpu.arch.or_else(|| {
        cpu.manufacturer.as_deref().and_then(|maker| {
            [Arch::X86, Arch::Arm]
                .into_iter()
                .find(|a| manufacturers(os, *a).contains(&maker))
        })
    });
    let Some(arch) = arch else { return true };
    if renderer.unwrap_or("").contains("llvmpipe") {
        return true;
    }
    let Some(by_gpu) = by_gpu else {
        return arch == Arch::X86 && os != Os::Macos;
    };
    if by_gpu != arch {
        return false;
    }
    cpu.manufacturer
        .as_deref()
        .is_none_or(|maker| makers.is_empty() || makers.contains(&maker))
}
pub fn cores_vendor(value: Option<&Value>) -> Option<&'static str> {
    value
        .and_then(Value::as_f64)
        .filter(|v| {
            constants()["intelOnlyCoreCounts"]
                .as_array()
                .is_some_and(|counts| counts.iter().any(|n| n.as_f64() == Some(*v)))
        })
        .map(|_| "intel")
}
pub fn hardware(config: &Map<String, Value>, os: Os) -> (Option<Arch>, Option<&'static str>) {
    let (arch, makers) = gpu_cpu(config.get("webGl:renderer").and_then(Value::as_str), os);
    if arch == Some(Arch::Arm) {
        return (arch, (makers.len() == 1).then(|| makers[0]));
    }
    let maker = makers
        .first()
        .copied()
        .or_else(|| cores_vendor(config.get("navigator.hardwareConcurrency")));
    (
        (maker.is_some() || arch.is_some()).then_some(Arch::X86),
        maker,
    )
}
pub fn conflict(config: &Map<String, Value>, os: Os) -> Option<String> {
    if ["cpu:arch", "cpu:manufacturer"]
        .iter()
        .all(|key| config.get(*key).is_none_or(Value::is_null))
    {
        return None;
    }
    let cpu = match from_config(config, os) {
        Ok(cpu) => cpu,
        Err(error) => return Some(error.to_string()),
    };
    let renderer = config
        .get("webGl:renderer")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !renderer.is_empty() && !gpu_fits(Some(renderer), os, &cpu) {
        return Some("CPU architecture or manufacturer contradicts the GPU".into());
    }
    if cpu.arch.unwrap_or(Arch::X86) == Arch::X86
        && cpu.manufacturer.as_deref().is_some_and(|maker| {
            cores_vendor(config.get("navigator.hardwareConcurrency"))
                .is_some_and(|required| required != maker)
        })
    {
        return Some("CPU manufacturer contradicts the Intel-only core count".into());
    }
    None
}
pub fn repair(config: &mut Map<String, Value>, os: Os) {
    let (arch, maker) = hardware(config, os);
    let arch = arch.unwrap_or(Arch::X86);
    config.insert("cpu:arch".into(), arch.as_str().into());
    if let Some(maker) = maker {
        config.insert("cpu:manufacturer".into(), maker.into());
    } else if config
        .get("cpu:manufacturer")
        .and_then(Value::as_str)
        .is_none_or(|m| !manufacturers(os, arch).contains(&m))
        || conflict(config, os).is_some()
    {
        config.remove("cpu:manufacturer");
    }
}
pub fn set_default(config: &mut Map<String, Value>, os: Os, salt: u64) {
    let (derived, maker) = hardware(config, os);
    let arch = config
        .get("cpu:arch")
        .filter(|v| crate::conversion::truth(v))
        .cloned()
        .unwrap_or_else(|| derived.unwrap_or(Arch::X86).as_str().into());
    config.entry("cpu:arch").or_insert(arch.clone());
    if config.contains_key("cpu:manufacturer") || config.get("cpu:arch") != Some(&arch) {
        return;
    }
    let maker = maker.or_else(|| {
        if arch != "x86" {
            return None;
        }
        let mut rng = PythonRandom::new(i64::from(
            camoufox_config::identity_seed(config, salt) ^ 0xC9D0,
        ));
        Some(
            if rng.random()
                < constants()["x86IntelShare"]
                    .as_f64()
                    .expect("Compiled CPU distribution")
            {
                "intel"
            } else {
                "amd"
            },
        )
    });
    if let Some(maker) = maker {
        config.insert("cpu:manufacturer".into(), maker.into());
    }
}
pub fn identity_pref(config: &Map<String, Value>) -> i64 {
    let arch = config.get("cpu:arch").and_then(Value::as_str);
    let key = if arch == Some("arm") {
        Some("arm".into())
    } else if arch.is_none() || arch == Some("x86") {
        config
            .get("cpu:manufacturer")
            .and_then(Value::as_str)
            .map(|m| format!("x86/{m}"))
    } else {
        None
    };
    key.and_then(|k| constants()["cpuIdentityPref"][k].as_i64())
        .unwrap_or(0)
}
pub fn realtime_only(config: &Map<String, Value>, os: Os) -> bool {
    config.get("cpu:arch").is_some_and(|v| v == "arm") && matches!(os, Os::Windows | Os::Linux)
}
pub fn preferences(config: &Map<String, Value>, os: Os, prefs: &mut Map<String, Value>) {
    let cpu = identity_pref(config);
    if cpu != 0 {
        prefs.entry("general.cpu_identity").or_insert(json!(cpu));
    }
    if realtime_only(config, os) {
        prefs
            .entry("media.libvpx.realtime_only")
            .or_insert(true.into());
    }
}
