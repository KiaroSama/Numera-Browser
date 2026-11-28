//! Signing inventory regression supplements the frozen historical case inventory.
use numera_xtask::ci;
use serde_json::Value;
use std::{fs, path::PathBuf};

#[test]
fn manual_signing_refreshes_only_the_copied_inventory_after_binary_changes() -> ci::Result<()> {
    struct Temporary(PathBuf);
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let path = std::env::temp_dir().join(format!(
        "numera-inventory-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&path)?;
    let temp = Temporary(path);
    let original = temp.0.join("unsigned");
    fs::create_dir_all(original.join("engine"))?;
    fs::write(
        original.join("engine/numera-engine"),
        b"unsigned binary fixture",
    )?;
    for dir in [original.join("engine"), original.clone()] {
        numera_xtask::product::inventory(&dir)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
    }
    let before = fs::read(original.join("engine/FILES.json"))?;
    let root_before = fs::read(original.join("FILES.json"))?;
    let signed = temp.0.join("signed");
    ci::browser_benchmark::copy_tree(&original, &signed)?;
    fs::write(
        signed.join("engine/numera-engine"),
        b"signed binary fixture",
    )?;
    let collision = numera_xtask::product::inventory(&signed.join("engine"))
        .expect_err("The original signing call must refuse its copied inventory");
    assert_eq!(
        collision
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind),
        Some(std::io::ErrorKind::AlreadyExists)
    );
    assert_eq!(fs::read(signed.join("engine/FILES.json"))?, before);
    ci::product_signing::refresh_inventory(&signed)?;
    let inventory: Value = serde_json::from_slice(&fs::read(signed.join("engine/FILES.json"))?)?;
    assert_eq!(inventory["files"][0]["path"], "numera-engine");
    assert_eq!(
        inventory["files"][0]["sha256"],
        ci::sha(b"signed binary fixture")
    );
    assert!(!signed.join("FILES.json").exists());
    assert_eq!(fs::read(original.join("engine/FILES.json"))?, before);
    assert_eq!(fs::read(original.join("FILES.json"))?, root_before);
    assert_eq!(
        fs::read(original.join("engine/numera-engine"))?,
        b"unsigned binary fixture"
    );
    Ok(())
}
