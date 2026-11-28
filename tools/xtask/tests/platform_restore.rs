//! Platform restoration contracts supplement the frozen historical case inventory.
use numera_xtask::{
    artifact_identity::BuildIdentity,
    ci::{self, release, workflow},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn document(name: &str) -> ci::Result<Value> {
    workflow::parse(&fs::read_to_string(
        root().join(format!(".github/workflows/{name}.yml")),
    )?)
}

fn documents() -> ci::Result<Value> {
    let mut values = serde_json::Map::new();
    for name in [
        "ci",
        "desktop-contracts",
        "build-desktop",
        "container",
        "release-review",
    ] {
        values.insert(name.into(), document(name)?);
    }
    Ok(Value::Object(values))
}

#[test]
fn native_host_admission_restores_only_the_supported_macos_architecture() {
    for (os, arch) in [
        ("windows", "x86_64"),
        ("windows", "aarch64"),
        ("linux", "x86_64"),
        ("linux", "aarch64"),
        ("macos", "aarch64"),
    ] {
        assert!(BuildIdentity::supported_host(os, arch), "{os}/{arch}");
    }
    for (os, arch) in [
        ("macos", "x86_64"),
        ("linux", "x86"),
        ("windows", "arm"),
        ("freebsd", "aarch64"),
    ] {
        assert!(!BuildIdentity::supported_host(os, arch), "{os}/{arch}");
    }
}

#[test]
fn native_builds_and_functional_tests_cannot_drop_or_mislabel_macos() -> ci::Result<()> {
    for name in ["native-ci", "rust-candidate"] {
        let original = document(name)?;
        workflow::native_automatic(&original)?;
        for remove in [false, true] {
            let mut damaged = original.clone();
            let rows = damaged["jobs"]["build"]["strategy"]["matrix"]["include"]
                .as_array_mut()
                .expect("native matrix");
            if remove {
                rows.retain(|row| row["platform"] != "macos-arm64");
            } else {
                rows.iter_mut()
                    .find(|row| row["platform"] == "macos-arm64")
                    .expect("macOS target")["arch"] = json!("X64");
            }
            assert!(
                workflow::native_automatic(&damaged).is_err(),
                "{name}: remove={remove}"
            );
        }
    }
    let mut functional = document("native-ci")?;
    workflow::native_functional_fixture(&functional)?;
    functional["jobs"]["functional"]["strategy"]["matrix"]["os"] =
        json!(["ubuntu-24.04", "windows-latest"]);
    assert!(workflow::native_functional_fixture(&functional).is_err());
    let mut live = document("native-ci")?;
    workflow::macos_live_sdk(&live)?;
    live["jobs"]["macos-live"]["strategy"]["matrix"]["engine"] = json!(["playwright", "cloak"]);
    assert!(workflow::macos_live_sdk(&live).is_err());
    let mut live = document("native-ci")?;
    live["jobs"]["macos-live"]["steps"]
        .as_array_mut()
        .expect("live steps")
        .iter_mut()
        .find(|step| {
            step["run"]
                .as_str()
                .is_some_and(|run| run.contains("ci counted-tests 1 build/selected-sdk.log"))
        })
        .expect("SDK lifecycle test")["if"] = json!("false");
    assert!(workflow::macos_live_sdk(&live).is_err());
    Ok(())
}

#[test]
fn desktop_contracts_and_manual_review_require_macos_and_its_ownership_proof() -> ci::Result<()> {
    let original = documents()?;
    workflow::desktop_contracts(&original)?;
    release::release_workflows(&original)?;
    let mut damaged = original.clone();
    damaged["desktop-contracts"]["jobs"]["contracts"]["strategy"]["matrix"]["os"] =
        json!(["ubuntu-latest", "windows-latest"]);
    assert!(workflow::desktop_contracts(&damaged).is_err());
    let mut damaged = original.clone();
    damaged["desktop-contracts"]["jobs"]["contracts"]["steps"]
        .as_array_mut()
        .expect("desktop steps")
        .retain(|step| {
            !step["run"]
                .as_str()
                .is_some_and(|run| run.ends_with("ci macos-ownership test"))
        });
    assert!(workflow::desktop_contracts(&damaged).is_err());
    let mut damaged = original;
    damaged["build-desktop"]["jobs"]["desktop"]["strategy"]["matrix"]["include"]
        .as_array_mut()
        .expect("manual matrix")
        .retain(|row| row["platform"] != "macos-arm64");
    assert!(release::release_workflows(&damaged).is_err());
    assert!(release::RELEASE_PLATFORMS.contains(&"macos-arm64"));
    Ok(())
}

#[test]
fn signed_release_and_update_feed_preserve_all_five_native_targets() -> ci::Result<()> {
    let original = document("release")?;
    workflow::versioned_release(&original)?;
    for platform in release::NATIVE_PLATFORMS {
        let mut damaged = original.clone();
        damaged["jobs"]["signing"]["strategy"]["matrix"]["include"]
            .as_array_mut()
            .expect("release matrix")
            .retain(|row| row["platform"] != platform);
        assert!(workflow::versioned_release(&damaged).is_err(), "{platform}");
    }
    assert_eq!(
        BTreeSet::from(release::NATIVE_PLATFORMS),
        BTreeSet::from([
            "windows-x64",
            "windows-arm64",
            "linux-x64",
            "linux-arm64",
            "macos-arm64",
        ])
    );
    Ok(())
}

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
    let signed = temp.0.join("signed");
    ci::browser_benchmark::copy_tree(&original, &signed)?;
    fs::write(
        signed.join("engine/numera-engine"),
        b"signed binary fixture",
    )?;
    ci::product_signing::refresh_inventory(&signed)?;
    let inventory: Value = serde_json::from_slice(&fs::read(signed.join("engine/FILES.json"))?)?;
    assert_eq!(inventory["files"][0]["path"], "numera-engine");
    assert_eq!(
        inventory["files"][0]["sha256"],
        ci::sha(b"signed binary fixture")
    );
    assert!(!signed.join("FILES.json").exists());
    assert_eq!(fs::read(original.join("engine/FILES.json"))?, before);
    assert!(original.join("FILES.json").is_file());
    Ok(())
}
