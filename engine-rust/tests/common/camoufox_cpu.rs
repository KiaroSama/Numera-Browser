//! Executable source-AST characterization for the new Camoufox preparation
//! rules. This verifies prep/filtering, not unreleased browser CPU emulation.
use numera_core::browser::sdk::{
    camoufox_coherence, camoufox_cpu as cpu, camoufox_identity::Os, camoufox_webgl,
    fingerprint::Model,
};
use serde_json::{Map, Value, json};
use std::{collections::HashSet, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn os(value: &Value) -> Result<Os> {
    match value.as_str() {
        Some("win") => Ok(Os::Windows),
        Some("mac") => Ok(Os::Macos),
        Some("lin") => Ok(Os::Linux),
        _ => Err("Invalid CPU fixture OS".into()),
    }
}
fn arch(value: &Value) -> Result<Option<cpu::Arch>> {
    match value.as_str() {
        None if value.is_null() => Ok(None),
        Some("x86") => Ok(Some(cpu::Arch::X86)),
        Some("arm") => Ok(Some(cpu::Arch::Arm)),
        _ => Err("Invalid CPU fixture architecture".into()),
    }
}
fn object(value: &Value) -> Result<Map<String, Value>> {
    value
        .as_object()
        .cloned()
        .ok_or("CPU fixture config missing".into())
}
fn manufacturer(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}
pub fn verify() -> Result<()> {
    let data: Value = serde_json::from_str(include_str!("../fixtures/camoufox-cpu.json"))?;
    assert_eq!(data["source_commit"], cpu::SOURCE_COMMIT);
    // Real released/current-source property inventories establish capability;
    // Firefox accepting an arbitrary preference name does not establish support.
    let released = &data["properties"]["released158"];
    let current = &data["properties"]["current_source"];
    assert!(!cpu::supported(released));
    assert!(cpu::supported(current));
    for schema in [
        json!([]),
        json!([{"property":"cpu:arch","type":"str"}]),
        json!([{"property":"cpu:arch","type":"str"},{"property":"cpu:manufacturer","type":"str","removed":"future"}]),
        json!([{"property":"cpu:arch","type":"bool"},{"property":"cpu:manufacturer","type":"str"}]),
    ] {
        assert!(!cpu::supported(&schema));
    }
    let mut config = Map::new();
    assert!(cpu::prepare(&mut config, None, Os::Windows, cpu::supported(released))?.is_none());
    assert!(config.is_empty());
    assert!(
        cpu::prepare(
            &mut config,
            Some(&json!({"arch":"x86"})),
            Os::Windows,
            cpu::supported(released)
        )
        .is_err()
    );
    assert!(config.is_empty());
    let mut config = Map::from_iter([("cpu:arch".into(), json!("x86"))]);
    let before = config.clone();
    assert!(cpu::prepare(&mut config, None, Os::Windows, cpu::supported(released)).is_err());
    assert_eq!(config, before);
    let mut config = Map::new();
    assert!(
        cpu::prepare(
            &mut config,
            Some(&json!({"manufacturer":"intel"})),
            Os::Windows,
            cpu::supported(current)
        )?
        .is_some()
    );
    assert_eq!(
        config,
        json!({"cpu:arch":"x86","cpu:manufacturer":"intel"})
            .as_object()
            .ok_or("Expected CPU config missing")?
            .clone()
    );
    for (index, row) in data["resolve"]
        .as_array()
        .ok_or("CPU resolution fixtures missing")?
        .iter()
        .enumerate()
    {
        let result = cpu::resolve(&row["input"], os(&row["os"])?);
        if row["error"] == true {
            assert!(result.is_err(), "CPU resolution {index}");
        } else {
            assert_eq!(
                json!(result?.config()),
                row["value"],
                "CPU resolution {index}"
            );
        }
    }
    for (index, row) in data["gpu"]
        .as_array()
        .ok_or("CPU GPU fixtures missing")?
        .iter()
        .enumerate()
    {
        let os = os(&row["os"])?;
        let resolved = cpu::Cpu {
            arch: arch(&row["arch"])?,
            manufacturer: manufacturer(&row["manufacturer"]),
        };
        let renderer = row["renderer"].as_str();
        assert_eq!(
            cpu::gpu_fits(renderer, os, &resolved),
            row["value"].as_bool().ok_or("CPU GPU result missing")?,
            "CPU GPU fit {index}"
        );
        let (arch, makers) = cpu::gpu_cpu(renderer, os);
        assert_eq!(
            json!([arch.map(cpu::Arch::as_str), makers]),
            row["inferred"],
            "GPU CPU inference {index}"
        );
    }
    for (index, row) in data["config"]
        .as_array()
        .ok_or("CPU configuration fixtures missing")?
        .iter()
        .enumerate()
    {
        let os = os(&row["os"])?;
        let original = object(&row["input"])?;
        let (arch, maker) = cpu::hardware(&original, os);
        assert_eq!(
            json!([arch.map(cpu::Arch::as_str), maker]),
            row["hardware"],
            "CPU hardware {index}"
        );
        assert_eq!(
            cpu::conflict(&original, os).is_some(),
            row["conflict"].as_bool().ok_or("CPU conflict missing")?,
            "CPU conflict {index}"
        );
        let mut actual = original.clone();
        cpu::repair(&mut actual, os);
        assert_eq!(json!(actual), row["repair"], "CPU repair {index}");
        let mut actual = original;
        cpu::set_default(
            &mut actual,
            os,
            row["salt"].as_u64().ok_or("CPU salt missing")?,
        );
        assert_eq!(json!(actual), row["default"], "CPU default {index}");
        assert_eq!(
            json!(cpu::identity_pref(&actual)),
            row["pref"],
            "CPU preference {index}"
        );
        assert_eq!(
            json!(cpu::realtime_only(&actual, os)),
            row["realtime_only"],
            "CPU realtime encoder {index}"
        );
        let mut prefs = Map::new();
        cpu::preferences(&actual, os, &mut prefs);
        if row["pref"] != 0 {
            assert_eq!(prefs["general.cpu_identity"], row["pref"]);
        } else {
            assert!(!prefs.contains_key("general.cpu_identity"));
        }
        assert_eq!(
            prefs.get("media.libvpx.realtime_only").is_some(),
            row["realtime_only"] == true
        );
    }
    let mut arm = json!({"cpu:arch":"arm","cpu:manufacturer":"qualcomm"})
        .as_object()
        .ok_or("Arm fixture missing")?
        .clone();
    let mut prefs = Map::from_iter([
        ("general.cpu_identity".into(), json!(0)),
        ("media.libvpx.realtime_only".into(), json!(false)),
    ]);
    cpu::preferences(&arm, Os::Windows, &mut prefs);
    assert_eq!(prefs["general.cpu_identity"], 0);
    assert_eq!(prefs["media.libvpx.realtime_only"], false);
    arm.insert(
        "webGl:renderer".into(),
        json!("Intel(R) HD Graphics 400, or similar"),
    );
    let kept = HashSet::from(["cpu:arch".into(), "cpu:manufacturer".into()]);
    let before = arm.clone();
    let violations = camoufox_coherence::apply(&mut arm, Os::Windows, &kept);
    assert_eq!(arm["cpu:arch"], before["cpu:arch"]);
    assert_eq!(arm["cpu:manufacturer"], before["cpu:manufacturer"]);
    assert!(
        violations
            .iter()
            .any(|v| v.rule == "cpu-matches-hardware" && !v.kept.is_empty())
    );
    let model = Model::open(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/browser/sdk/fingerprint-data/model-2-2026.zip"),
    )?;
    for (index, row) in data["sampling"]
        .as_array()
        .ok_or("CPU conditioned sampling fixtures missing")?
        .iter()
        .enumerate()
    {
        let os = os(&row["os"])?;
        let resolved = cpu::resolve(&row["cpu"], os)?;
        let actual = camoufox_webgl::sample_for_cpu(
            &model,
            os,
            row["width"].as_i64(),
            row["height"].as_i64(),
            row["cores"].as_i64(),
            row["seed"].as_u64().ok_or("GPU seed missing")? as u32,
            row["ff_major"].as_u64().ok_or("GPU version missing")? as u32,
            None,
            Some(&resolved),
        );
        if row["error"] == true {
            assert!(actual.is_err(), "CPU conditioned GPU {index}");
        } else {
            assert_eq!(actual?, row["value"], "CPU conditioned GPU {index}");
        }
    }
    Ok(())
}
