use numera_core::browser::sdk::{
    camoufox_coherence as coherence, camoufox_identity::Os, camoufox_webgl as webgl,
    fingerprint::Model,
};
use serde_json::Value;
use std::{collections::HashSet, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn os(name: &Value) -> Result<Os> {
    match name.as_str() {
        Some("win") => Ok(Os::Windows),
        Some("mac") => Ok(Os::Macos),
        Some("lin") => Ok(Os::Linux),
        _ => Err("Invalid fixture OS".into()),
    }
}
pub fn verify() -> Result<()> {
    let data: Value = serde_json::from_str(include_str!("../fixtures/camoufox-webgl.json"))?;
    assert_eq!(
        data["source_commit"],
        "df9ca1156abf94bf5e8e7764effd35d3a7b298f9"
    );
    let model = Model::open(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/browser/sdk/fingerprint-data/model-2-2026.zip"),
    )?;
    let rows = data["cases"].as_array().ok_or("WebGL cases missing")?;
    assert_eq!(rows.len(), 24);
    for (i, row) in rows.iter().enumerate() {
        let host = row["host_gpu"]
            .as_array()
            .map(|v| {
                Ok::<_, Box<dyn std::error::Error>>((
                    v[0].as_str().ok_or("Host vendor missing")?,
                    v[1].as_str().ok_or("Host renderer missing")?,
                ))
            })
            .transpose()?;
        let actual = webgl::sample(
            &model,
            os(&row["os"])?,
            row["width"].as_i64(),
            row["height"].as_i64(),
            row["cores"].as_i64(),
            row["seed"].as_u64().ok_or("Seed missing")? as u32,
            row["ff_major"].as_u64().ok_or("Version missing")? as u32,
            host,
        )?;
        assert_eq!(actual, row["value"], "WebGL draw {i}");
        if row["ff_major"] == 158 {
            for key in ["webGl:supportedExtensions", "webGl2:supportedExtensions"] {
                if let Some(ext) = actual[key].as_array() {
                    assert!(
                        ext.iter().all(|v| ![
                            "EXT_clip_control",
                            "EXT_texture_norm16",
                            "EXT_texture_mirror_clamp_to_edge"
                        ]
                        .iter()
                        .any(|e| v == e)),
                        "Firefox 158 advertised a Firefox 159 extension"
                    );
                }
            }
        }
    }
    verify_conversions()?;
    assert!(
        webgl::sample(
            &model,
            Os::Linux,
            Some(1920),
            Some(1080),
            Some(8),
            0,
            158,
            Some(("Unrecorded", "Unrecorded"))
        )
        .is_err()
    );
    verify_coherence()
}
pub fn verify_coherence() -> Result<()> {
    let data: Value = serde_json::from_str(include_str!("../fixtures/camoufox-coherence.json"))?;
    let rows = data["cases"].as_array().ok_or("Coherence cases missing")?;
    assert_eq!(rows.len(), 165);
    for (i, row) in rows.iter().enumerate() {
        let os = os(&row["os"])?;
        let original = row["input"].as_object().ok_or("Config missing")?;
        let keep = row["keep"]
            .as_array()
            .ok_or("Keep missing")?
            .iter()
            .map(|v| v.as_str().map(str::to_owned).ok_or("Keep key missing"))
            .collect::<std::result::Result<HashSet<_>, _>>()?;
        let mut actual = original.clone();
        let violations = coherence::apply(&mut actual, os, &keep);
        assert_eq!(Value::Object(actual), row["value"], "Coherence repair {i}");
        assert_eq!(
            serde_json::to_value(violations)?,
            row["violations"],
            "Coherence violations {i}"
        );
        let mut dropped = original.clone();
        let violations = coherence::drop_incoherent_source_values(&mut dropped, os, &keep);
        assert_eq!(
            Value::Object(dropped),
            row["dropped_value"],
            "Dropped GPU {i}"
        );
        assert_eq!(
            serde_json::to_value(violations)?,
            row["dropped"],
            "Dropped GPU violations {i}"
        );
        let mut orientation = original.clone();
        assert_eq!(
            coherence::repair_screen_orientation(&mut orientation),
            row["orientation"].as_bool().ok_or("Orientation missing")?
        );
        assert_eq!(
            Value::Object(orientation),
            row["orientation_value"],
            "Screen orientation {i}"
        );
        let mut floor = original.clone();
        coherence::raise_screen_to_modern_floor(&mut floor);
        assert_eq!(Value::Object(floor), row["floor"], "Screen floor {i}");
        assert_eq!(
            coherence::screen_is_implausible(original),
            row["implausible"]
                .as_bool()
                .ok_or("Screen plausibility missing")?
        );
    }
    let rows = data["filters"]
        .as_array()
        .ok_or("GPU filter cases missing")?;
    assert_eq!(rows.len(), 480);
    for (i, row) in rows.iter().enumerate() {
        let os = os(&row["os"])?;
        let renderer = row["renderer"].as_str();
        assert_eq!(
            coherence::gpu_fits_os(renderer, os),
            row["fits_os"].as_bool().ok_or("OS fit missing")?,
            "GPU OS filter {i}"
        );
        assert_eq!(
            coherence::gpu_fits_machine(
                renderer,
                os,
                row["cores"].as_i64(),
                row["width"].as_i64(),
                row["height"].as_i64()
            ),
            row["fits_machine"].as_bool().ok_or("Machine fit missing")?,
            "GPU machine filter {i}"
        );
        assert_eq!(
            coherence::gpu_screen_is_plausible(
                renderer,
                row["width"].as_i64(),
                row["height"].as_i64()
            ),
            row["plausible"].as_bool().ok_or("Screen fit missing")?,
            "GPU screen filter {i}"
        );
        assert_eq!(
            coherence::is_software_renderer(renderer),
            row["software"].as_bool().ok_or("Software flag missing")?
        );
    }
    Ok(())
}

pub fn verify_conversions() -> Result<()> {
    let data: Value = serde_json::from_str(include_str!("../fixtures/camoufox-webgl.json"))?;
    let rows = data["conversions"]
        .as_array()
        .ok_or("WebGL conversion cases missing")?;
    assert_eq!(rows.len(), 32);
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(
            webgl::to_config(
                &row["webgl"],
                &row["webgl2"],
                os(&row["os"])?,
                row["ff_major"].as_u64().ok_or("Version missing")? as u32
            )?,
            row["value"],
            "WebGL conversion {i}"
        );
    }
    Ok(())
}
