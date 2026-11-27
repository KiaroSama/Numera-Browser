//! Host-side checks shared by the real compiler and native contract tests.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::Path,
};
pub const RUSTC_VERSION: &str = "rustc 1.100.0 (0ef1bc386 2026-11-09)";
pub const MAX_ARTIFACT: usize = 1024 * 1024;
pub const GENERATED_BANNER: &str = "// GENERATED from browser-page-rust by locked wasm-bindgen and Binaryen133.\n// Regenerate with browser-page-build; authored browser behavior lives in Rust.\n";
pub const FACTORY_PREFIX: &str = "(function numera_browser_page()";
pub const PATH_REMAPPING: [&str; 5] = [
    "repository=numera",
    "cargo-home=cargo",
    "rustup-home=rustup",
    "rust-src=rustc-virtual",
    "dependency-crates=crates/name-version",
];
/// The lockfile/source receipt also pins every transitive host dependency.
pub fn printer_policy() -> Value {
    json!({
        "name":"oxc","version":"0.160.0",
        "mode":"parse_expression+CodegenOptions::minify+print_expression",
        "source_type":"cjs","preserve_parentheses":true,
        "regex_pattern_validation":true,"semantic_transforms":false,
        "identifier_mangling":false,"ascii_only":false,
        "crate_sha256":{
            "oxc_allocator":"fff8d79a61ccac3b777cc79317a3f1b8afdaa86489532d427c7eb42e1370790a",
            "oxc_codegen":"bfd23bfa766e8f74bdd02783a09bc6bc8ad22bb11cb69b44969473cc40606576",
            "oxc_parser":"2bba04afea1b900540dd4e2cd75d86ffff57f9e4c1dcfe68702314c9a81841da",
            "oxc_span":"109cc73d34598dd5f93f5cfcbca2c4ab139b78aec59cece5ff65212ece878f92"
        }
    })
}
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
        "target":"wasm32-unknown-unknown","rustc":RUSTC_VERSION,
        "wasm_bindgen":"wasm-bindgen 0.2.129","binaryen":"wasm2js version 133 (version_133)",
        "binaryen_archive":"https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-linux.tar.gz",
        "binaryen_archive_sha256":"2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489",
        "wasm2js_binary_sha256":"52597efec0ad2416f8b5c245864960e775b8ffc94c751cc0b016c4b4986d3bcb",
        "path_remapping":PATH_REMAPPING,
        "wasm2js_flags":["--enable-bulk-memory","--enable-bulk-memory-opt","--enable-nontrapping-float-to-int","--pedantic","-O"],
        "minified":true,"runtime_compilation":false,
        "source_printer":printer_policy(),
        "bridge":["generated module imports/exports become a synchronous classic factory","generated Uint8Array and DataView caches observe non-detaching ArrayBuffer growth"],
        "regeneration_host":"Linux x86_64; output uses portable relative source paths"
    });
    let source_map = serde_json::to_value(sources(repo)?).map_err(io::Error::other)?;
    let object = receipt.as_object().ok_or_else(refuse)?;
    if artifact.is_empty()
        || artifact.len() > MAX_ARTIFACT
        || artifact
            .strip_prefix(GENERATED_BANNER.as_bytes())
            .is_none_or(|body| {
                !body.starts_with(FACTORY_PREFIX.as_bytes()) || !body.ends_with(b"})\n")
            })
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
