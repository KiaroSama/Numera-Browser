use numera_core::{identity_script, models::Fingerprint};
use serde_json::Value;
mod common;
#[path = "../../browser-page-rust/src/artifact.rs"]
mod compiler_artifact;
#[path = "init_script_parity/contracts.rs"]
mod contracts;
#[path = "../../browser-page-rust/src/source_paths.rs"]
mod source_paths;

#[test]
fn native_script_and_config_match_original_generator() -> Result<(), Box<dyn std::error::Error>> {
    let original = include_bytes!("fixtures/init-script.json");
    assert_eq!(
        compiler_artifact::digest(original),
        "ed0383fd02663f98ee4778ae51e4aeb1f3461be2a9cc6287f68262eb36adbb9d"
    );
    let document: Value = serde_json::from_slice(original)?;
    assert_eq!(
        document["source_commit"],
        "13b6d9c6973281fe123353eaec05b29d65781a2d"
    );
    contracts::artifact()?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Init script cases missing")?;
    assert_eq!(cases.len(), 10);
    for case in cases {
        let fingerprint = Fingerprint::from_record(&case["input"])?;
        assert_eq!(
            identity_script::config(&fingerprint)?,
            case["config"],
            "Config {}",
            case["name"]
        );
        let script = identity_script::build(&fingerprint)?;
        let legacy = case["script"].as_str().ok_or("Original script missing")?;
        let expected = legacy
            .strip_prefix("(() => {\n  const cfg = ")
            .and_then(|script| script.split_once(';').map(|(encoded, _)| encoded))
            .ok_or("Original configuration boundary missing")?;
        let actual = script
            .strip_prefix("(() => {\nconst CFG = ")
            .and_then(|script| script.split_once(";\n(").map(|(encoded, _)| encoded))
            .ok_or("Native configuration boundary missing")?;
        assert_eq!(actual, expected, "Canonical SourceJson {}", case["name"]);
        // Original script bytes remain pinned above. The generated Rust page
        // implementation has its own source/tool/output receipt and independent
        // frozen browser observables in the existing actual-browser live case.
        contracts::frames(&fingerprint, &script)?;
    }
    Ok(())
}
