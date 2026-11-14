//! Actual compiler profile and source receipts for comparative native binaries.
use super::{Result, fail};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
pub fn release_artifact(
    messages: &[u8],
    manifest: &Path,
    target_root: &Path,
    name: &str,
    feature: &str,
) -> Result<PathBuf> {
    let mut found = Vec::new();
    let mut finished = None;
    for line in messages.split(|v| *v == b'\n').filter(|v| !v.is_empty()) {
        let event = super::decode(line)?;
        if event["reason"] == "build-finished" {
            if finished.is_some() {
                return fail("Duplicate release compiler completion");
            }
            finished = event["success"].as_bool();
        }
        if event["reason"] != "compiler-artifact" || event["target"]["name"] != name {
            continue;
        }
        let executable = PathBuf::from(
            event["executable"]
                .as_str()
                .ok_or("Release compiler executable missing")?,
        );
        if !event["manifest_path"]
            .as_str()
            .is_some_and(|path| super::candidate::same_path(Path::new(path), manifest))
            || event["target"]["kind"] != json!(["bin"])
            || event["profile"]["test"] != false
            || event["profile"]["opt_level"] != "3"
            || event["profile"]["debug_assertions"] != false
            || !event["features"]
                .as_array()
                .is_some_and(|features| features.iter().any(|value| value == feature))
            || !super::candidate::within_path(&executable, target_root)
        {
            return fail("Comparative artifact must be the exact enabled release-opt3 binary");
        }
        super::regular(&executable, false)?;
        found.push(executable);
    }
    if finished != Some(true) || found.len() != 1 {
        return fail("Release compiler did not produce exactly one successful enabled binary");
    }
    Ok(found.remove(0))
}
pub fn receipt_path(binary: &Path) -> PathBuf {
    binary.with_extension("build.json")
}
pub fn write(
    binary: &Path,
    worker: &Path,
    messages: &[u8],
    source: &str,
    reference: bool,
    context: Value,
) -> Result<Value> {
    let generation = std::env::var("GITHUB_RUN_ID")?.parse::<u64>()?;
    if !super::canonical_sha(source) || generation == 0 {
        return fail("Comparative build identity is invalid");
    }
    let output = super::capture(
        Command::new(binary).arg("--build-info"),
        Duration::from_secs(15),
        Duration::from_secs(15),
        65536,
    )?;
    let identity = super::decode(&output.stdout)?;
    if output.exit != 0
        || !output.stderr.is_empty()
        || identity["source_sha"] != source
        || identity["generation"] != generation
        || identity["acceptance"] != reference
        || identity["application_runtime"]
            != if reference {
                "native-candidate"
            } else {
                "native"
            }
    {
        return fail("Compiled comparative binary source/runtime identity differs");
    }
    let receipt = json!({"schema":1,"source_sha":source,"generation":generation,"profile":"release-opt3","opt_level":"3","debug_assertions":false,"binary_sha256":super::digest(binary)?,"sdk_binary_sha256":super::digest(worker)?,"compiler_messages_sha256":super::sha(messages),"identity":identity,"context":context});
    let path = receipt_path(binary);
    numera_core::storage::atomic::write_bytes(&path, &serde_json::to_vec_pretty(&receipt)?)?;
    Ok(receipt)
}
pub fn verify(binary: &Path, worker: &Path, source: &str, reference: bool) -> Result<Value> {
    let path = receipt_path(binary);
    super::regular(&path, false)?;
    let receipt = super::decode(&fs::read(path)?)?;
    if receipt["schema"] != 1
        || receipt["source_sha"] != source
        || receipt["profile"] != "release-opt3"
        || receipt["opt_level"] != "3"
        || receipt["debug_assertions"] != false
        || receipt["binary_sha256"] != super::digest(binary)?
        || receipt["sdk_binary_sha256"] != super::digest(worker)?
        || receipt["identity"]["acceptance"] != reference
        || receipt["identity"]["application_runtime"]
            != if reference {
                "native-candidate"
            } else {
                "native"
            }
    {
        return fail("Comparative compiler receipt no longer binds the selected binary and SDK");
    }
    Ok(receipt)
}
pub fn product(root: &Path) -> Result<PathBuf> {
    let mut command = Command::new("cargo");
    command.current_dir(root).args([
        "build",
        "--manifest-path",
        "engine-rust/Cargo.toml",
        "--locked",
        "--release",
        "--no-default-features",
        "--features",
        "native-product,native-browser",
        "--bin",
        "numera-engine",
        "--bin",
        "numera-browser-runtime",
        "--message-format=json",
    ]);
    let output = super::owned::run(
        root,
        &mut command,
        super::owned::limits(
            Duration::from_secs(900),
            Duration::from_secs(300),
            32 * 1024 * 1024,
        ),
    )?
    .command;
    if output.exit != 0 {
        return fail("Native product comparison compilation failed");
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|p| if p.is_absolute() { p } else { root.join(p) })
        .unwrap_or_else(|| root.join("engine-rust/target"));
    let manifest = root.join("engine-rust/Cargo.toml");
    let engine = release_artifact(
        &output.stdout,
        &manifest,
        &target,
        "numera-engine",
        "native-product",
    )?;
    let worker = release_artifact(
        &output.stdout,
        &manifest,
        &target,
        "numera-browser-runtime",
        "native-browser",
    )?;
    let source = std::env::var("GITHUB_SHA")?;
    write(
        &engine,
        &worker,
        &output.stdout,
        &source,
        false,
        json!({"manifest_sha256":super::digest(&manifest)?}),
    )?;
    write(
        &worker,
        &worker,
        &output.stdout,
        &source,
        false,
        json!({"component":"browser-runtime","manifest_sha256":super::digest(&manifest)?}),
    )?;
    Ok(engine)
}
