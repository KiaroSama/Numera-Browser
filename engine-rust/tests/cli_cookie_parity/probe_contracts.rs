use super::*;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
pub(super) fn original(value: &Value, expected_hash: &str) -> Result<()> {
    assert_eq!(
        page_artifact::digest(value.as_str().ok_or("Original probe missing")?.as_bytes()),
        expected_hash
    );
    Ok(())
}
pub(super) fn artifact() -> Result<()> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    page_artifact::check_generated(repo)?;
    let receipt: Value = serde_json::from_slice(&std::fs::read(
        repo.join("browser-page-rust/generated/receipt.json"),
    )?)?;
    page_artifact::validate(
        repo,
        numera_core::page_runtime::artifact().as_bytes(),
        &receipt,
    )?;
    let mut changed = receipt.clone();
    changed["wasm2js_binary_sha256"] = "0".repeat(64).into();
    assert!(
        page_artifact::validate(
            repo,
            numera_core::page_runtime::artifact().as_bytes(),
            &changed
        )
        .is_err()
    );
    Ok(())
}
pub(super) fn expression(value: &Value, export: &str) -> Result<()> {
    let expression = value.as_str().ok_or("Actual probe expression missing")?;
    let body = expression
        .strip_prefix("(argument) => (")
        .and_then(|expression| expression.strip_suffix(&format!(")().{export}(argument)")))
        .ok_or("Actual probe selected a different export or call contract")?;
    assert_eq!(
        body,
        numera_core::page_runtime::artifact(),
        "Actual probe must use the validated complete generated artifact"
    );
    Ok(())
}
