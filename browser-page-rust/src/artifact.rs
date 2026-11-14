//! Host-side checks shared by the real compiler and native contract tests.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::Path,
};
pub const MAX_ARTIFACT: usize = 1024 * 1024;
pub const PATH_REMAPPING: [&str; 4] = [
    "repository=numera",
    "cargo-home=cargo",
    "rustup-home=rustup",
    "dependency-crates=crates/name-version",
];
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn refuse() -> io::Error {
    io::Error::other("Browser artifact or source/tool receipt is stale or malformed")
}
fn read(path: &Path) -> io::Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_ARTIFACT as u64
    {
        return Err(refuse());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_ARTIFACT as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_ARTIFACT {
        return Err(refuse());
    }
    Ok(bytes)
}
fn sources(repo: &Path) -> io::Result<BTreeMap<String, Value>> {
    fn visit(
        repo: &Path,
        directory: &Path,
        result: &mut BTreeMap<String, Value>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(refuse());
            }
            if kind.is_dir() {
                visit(repo, &entry.path(), result)?;
            } else if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "rs")
            {
                add(repo, &entry.path(), result)?;
            }
        }
        Ok(())
    }
    fn add(repo: &Path, path: &Path, result: &mut BTreeMap<String, Value>) -> io::Result<()> {
        let relative = path
            .strip_prefix(repo)
            .ok()
            .and_then(Path::to_str)
            .ok_or_else(refuse)?;
        result.insert(relative.replace('\\', "/"), digest(&read(path)?).into());
        if result.len() > 512 {
            return Err(refuse());
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(repo, &repo.join("browser-page-rust/src"), &mut result)?;
    for name in [
        "browser-page-rust/Cargo.toml",
        "browser-page-rust/Cargo.lock",
        "rust-toolchain.toml",
    ] {
        add(repo, &repo.join(name), &mut result)?;
    }
    Ok(result)
}
/// This validates source attribution and fixed compiler policy. The compiler's
/// --check still performs a full rebuild and exact output/receipt comparison.
pub fn validate(repo: &Path, artifact: &[u8], receipt: &Value) -> io::Result<()> {
    let fixed = json!({
        "schema":1,"artifact":"browser-page-rust/generated/runtime.js",
        "authored_language":"Rust","generated_languages":["JavaScript"],
        "factory":"numera_browser_page() returns wasm-bindgen exports synchronously",
        "target":"wasm32-unknown-unknown","rustc":"rustc 1.99.1 (58cf8f9c3 2026-10-13)",
        "wasm_bindgen":"wasm-bindgen 0.2.129","binaryen":"wasm2js version 133 (version_133)",
        "binaryen_archive":"https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-linux.tar.gz",
        "binaryen_archive_sha256":"2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489",
        "wasm2js_binary_sha256":"52597efec0ad2416f8b5c245864960e775b8ffc94c751cc0b016c4b4986d3bcb",
        "path_remapping":PATH_REMAPPING,
        "wasm2js_flags":["--enable-bulk-memory","--enable-bulk-memory-opt","--enable-nontrapping-float-to-int","--pedantic","-O"],
        "minified":false,"runtime_compilation":false,
        "bridge":["generated module imports/exports become a synchronous classic factory","generated Uint8Array and DataView caches observe non-detaching ArrayBuffer growth"],
        "regeneration_host":"Linux x86_64; output uses portable relative source paths"
    });
    let source_map = serde_json::to_value(sources(repo)?).map_err(io::Error::other)?;
    let object = receipt.as_object().ok_or_else(refuse)?;
    if artifact.is_empty()
        || artifact.len() > MAX_ARTIFACT
        || object.len() != fixed.as_object().ok_or_else(refuse)?.len() + 8
        || fixed
            .as_object()
            .ok_or_else(refuse)?
            .iter()
            .any(|(key, expected)| receipt[key] != *expected)
        || receipt["artifact_sha256"] != digest(artifact)
        || receipt["artifact_bytes"].as_u64() != Some(artifact.len() as u64)
        || receipt["source_sha256"] != source_map
        || receipt["compiler_javascript_bytes"]
            .as_u64()
            .is_none_or(|n| n == 0 || n > MAX_ARTIFACT as u64)
    {
        return Err(refuse());
    }
    for key in [
        "artifact_sha256",
        "input_wasm_sha256",
        "binding_wasm_sha256",
        "bindgen_glue_sha256",
        "compiler_javascript_sha256",
    ] {
        if !receipt[key].as_str().is_some_and(|hash| {
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err(refuse());
        }
    }
    Ok(())
}
pub fn check_generated(repo: &Path) -> io::Result<()> {
    let artifact = read(&repo.join("browser-page-rust/generated/runtime.js"))?;
    let receipt: Value = serde_json::from_slice(&read(
        &repo.join("browser-page-rust/generated/receipt.json"),
    )?)
    .map_err(io::Error::other)?;
    validate(repo, &artifact, &receipt)
}
