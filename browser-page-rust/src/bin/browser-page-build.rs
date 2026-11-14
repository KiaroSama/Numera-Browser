//! Compile authored Rust page behavior into a CSP-compatible generated factory.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const BINDGEN: &str = "wasm-bindgen 0.2.129";
const BINARYEN: &str = "wasm2js version 133 (version_133)";
const BINARYEN_ARCHIVE: &str = "https://github.com/WebAssembly/binaryen/releases/download/version_133/binaryen-version_133-x86_64-linux.tar.gz";
const BINARYEN_ARCHIVE_SHA: &str =
    "2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489";
const BINARYEN_LINUX_SHA: &str = "52597efec0ad2416f8b5c245864960e775b8ffc94c751cc0b016c4b4986d3bcb";
const MAX_SOURCE: usize = 1024 * 1024;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Options {
    check: bool,
    wasm2js: PathBuf,
    bindgen: PathBuf,
    target: PathBuf,
    output: PathBuf,
}
impl Options {
    fn read(root: &Path) -> Result<Self> {
        let mut options = Self {
            check: false,
            wasm2js: "wasm2js".into(),
            bindgen: "wasm-bindgen".into(),
            target: env::var_os("CARGO_TARGET_DIR")
                .map_or_else(|| root.join("target"), PathBuf::from),
            output: root.join("generated"),
        };
        let mut args = env::args_os().skip(1);
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--check") => options.check = true,
                Some("--release") => {}
                Some("--wasm2js") => {
                    options.wasm2js = args.next().ok_or("--wasm2js needs a path")?.into();
                }
                Some("--bindgen") => {
                    options.bindgen = args.next().ok_or("--bindgen needs a path")?.into();
                }
                Some("--target-dir") => {
                    options.target = args.next().ok_or("--target-dir needs a path")?.into();
                }
                Some("--output-dir") => {
                    options.output = args.next().ok_or("--output-dir needs a path")?.into();
                }
                _ => return Err(format!("Unknown browser-page build option: {arg:?}").into()),
            }
        }
        Ok(options)
    }
}

fn digest(bytes: &[u8]) -> String {
    let hash = Sha256::digest(bytes);
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn executable(path: &Path) -> Result<PathBuf> {
    if path.components().count() > 1 {
        return Ok(path.canonicalize()?);
    }
    for directory in env::split_paths(&env::var_os("PATH").ok_or("PATH missing")?) {
        let candidate = directory.join(path);
        if candidate.is_file() {
            return Ok(candidate.canonicalize()?);
        }
    }
    Err(format!("Compiler tool not found: {}", path.display()).into())
}

fn version(command: &Path) -> Result<String> {
    let output = Command::new(command).arg("--version").output()?;
    if !output.status.success() {
        return Err(format!("Compiler version check failed: {}", command.display()).into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn run(command: &mut Command) -> Result<()> {
    let status = command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if !status.success() {
        return Err(format!("Browser compiler step failed: {status}").into());
    }
    Ok(())
}

fn replace_once(text: &mut String, before: &str, after: &str) -> Result<()> {
    let start = text
        .find(before)
        .ok_or("Generated compiler shape changed")?;
    if text[start + before.len()..].contains(before) {
        return Err("Generated compiler shape is ambiguous".into());
    }
    text.replace_range(start..start + before.len(), after);
    Ok(())
}

/// The input bodies are untouched third-party compiler output. This bridge only
/// joins their module imports/exports and adapts the generated memory-view caches.
fn factory(mut glue: String, mut compiled: String) -> Result<String> {
    replace_once(
        &mut compiled,
        "import * as $_numera_browser_page_bg_js from './numera_browser_page_bg.js';\n",
        "",
    )?;
    let suffix = compiled
        .find("\nvar retasmFunc = asmFunc({")
        .ok_or("Generated wasm2js export wrapper missing")?;
    let trailing = &compiled[suffix..];
    let prefix = "\nvar retasmFunc = asmFunc({\n  \"./numera_browser_page_bg.js\": $_numera_browser_page_bg_js,\n});\n";
    let exports = trailing
        .strip_prefix(prefix)
        .ok_or("Generated wasm2js import wrapper changed")?;
    let mut names = Vec::new();
    for line in exports.lines() {
        let assignment = line
            .strip_prefix("export var ")
            .and_then(|line| line.strip_suffix(';'))
            .ok_or("Generated wasm2js export changed")?;
        let (name, value) = assignment
            .split_once(" = retasmFunc.")
            .ok_or("Generated wasm2js export assignment changed")?;
        if name != value
            || name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
        {
            return Err("Generated wasm2js export identifier changed".into());
        }
        names.push(name.to_owned());
    }
    if !names.iter().any(|name| name == "memory") {
        return Err("Generated wasm2js memory export missing".into());
    }
    compiled.truncate(suffix);
    let start = glue
        .find("    async function __wbg_load(")
        .ok_or("Generated wasm-bindgen loader missing")?;
    let end = glue
        .find("    return Object.assign(__wbg_init, { initSync }, exports);")
        .filter(|end| *end > start)
        .ok_or("Generated wasm-bindgen initializer end missing")?;
    let initialize = "\n    function initSync() {\n        if (wasm !== undefined) return wasm;\n        return __wbg_finalize_init({exports:asmFunc(__wbg_get_imports())}, undefined);\n    }\n    async function __wbg_init() { return initSync(); }\n\n";
    glue.replace_range(start..end, &(compiled + initialize));
    // Native WebAssembly detaches old buffers on growth. wasm2js copies to a
    // new ArrayBuffer, so both generated views must notice the changed buffer.
    replace_once(
        &mut glue,
        "cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0",
        "cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0 || cachedUint8ArrayMemory0.buffer !== wasm.memory.buffer",
    )?;
    replace_once(
        &mut glue,
        "cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer",
        "cachedDataViewMemory0.buffer !== wasm.memory.buffer",
    )?;
    for prohibited in ["WebAssembly", "eval(", "eval (", "new Function"] {
        if glue.contains(prohibited) {
            return Err(
                format!("Runtime compiler remained in generated page code: {prohibited}").into(),
            );
        }
    }
    let output = format!(
        "// GENERATED from browser-page-rust by locked wasm-bindgen and Binaryen133.\n// Regenerate with browser-page-build; authored browser behavior lives in Rust.\n(function numera_browser_page() {{\n{glue}\nwasm_bindgen.initSync();\nreturn wasm_bindgen;\n}})\n"
    );
    if output.len() >= MAX_SOURCE {
        return Err(format!(
            "Generated page factory exceeds the1MiB request ceiling: {}",
            output.len()
        )
        .into());
    }
    Ok(output)
}

fn source_files(root: &Path, path: &Path, found: &mut BTreeMap<String, Value>) -> Result<()> {
    let mut entries = fs::read_dir(path)?.collect::<io::Result<Vec<_>>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            source_files(root, &path, found)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("Source path is not UTF-8")?;
            found.insert(relative.replace('\\', "/"), digest(&fs::read(path)?).into());
        }
    }
    Ok(())
}

fn source_snapshot(repo: &Path, crate_root: &Path) -> Result<BTreeMap<String, Value>> {
    let mut sources = BTreeMap::new();
    source_files(repo, &crate_root.join("src"), &mut sources)?;
    for path in [
        crate_root.join("Cargo.toml"),
        crate_root.join("Cargo.lock"),
        repo.join("rust-toolchain.toml"),
    ] {
        let relative = path
            .strip_prefix(repo)?
            .to_str()
            .ok_or("Source path is not UTF-8")?;
        sources.insert(relative.replace('\\', "/"), digest(&fs::read(path)?).into());
    }
    Ok(sources)
}

fn home_path(variable: &str, fallback: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
}

fn main() -> Result<()> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).canonicalize()?;
    let repo = crate_root
        .parent()
        .ok_or("Browser crate repository missing")?;
    let options = Options::read(&crate_root)?;
    let wasm2js = executable(&options.wasm2js)?;
    let bindgen = executable(&options.bindgen)?;
    if version(&bindgen)? != BINDGEN {
        return Err(format!("Expected {BINDGEN}").into());
    }
    if version(&wasm2js)? != BINARYEN || digest(&fs::read(&wasm2js)?) != BINARYEN_LINUX_SHA {
        return Err("Expected the reviewed Binaryen133 x86_64-linux wasm2js binary".into());
    }
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let rustc_version = version(Path::new(&rustc))?;
    if !rustc_version.starts_with("rustc 1.99.1 ") {
        return Err("Browser artifact regeneration requires Rust1.99.1".into());
    }
    let mut flags = vec![format!("--remap-path-prefix={}=numera", repo.display())];
    for (variable, fallback, mapped) in [
        ("CARGO_HOME", ".cargo", "cargo"),
        ("RUSTUP_HOME", ".rustup", "rustup"),
    ] {
        if let Some(path) = home_path(variable, fallback).filter(|path| path.exists()) {
            flags.push(format!(
                "--remap-path-prefix={}={mapped}",
                path.canonicalize()?.display()
            ));
        }
    }
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let metadata = Command::new(&cargo)
        .current_dir(&crate_root)
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "wasm32-unknown-unknown",
        ])
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()?;
    if !metadata.status.success() || metadata.stdout.len() > 4 * MAX_SOURCE {
        return Err("Locked browser dependency metadata failed or exceeded its bound".into());
    }
    // These more specific mappings must follow Cargo-home/repository mappings.
    // Otherwise a registry cache hash survives inside compiled panic locations.
    flags.extend(numera_browser_page::source_paths::dependency_remaps(
        &serde_json::from_slice(&metadata.stdout)?,
    )?);
    let target = if options.target.is_absolute() {
        options.target.clone()
    } else {
        env::current_dir()?.join(&options.target)
    };
    let build = target.join("browser-page-generated");
    fs::create_dir_all(&build)?;
    let source_before = source_snapshot(repo, &crate_root)?;
    run(Command::new(&cargo)
        .current_dir(&crate_root)
        .args([
            "build",
            "--locked",
            "--release",
            "--lib",
            "--target",
            "wasm32-unknown-unknown",
            "--target-dir",
        ])
        .arg(&target)
        .env_remove("RUSTFLAGS")
        .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\u{1f}")))?;
    let input = target.join("wasm32-unknown-unknown/release/numera_browser_page.wasm");
    run(Command::new(&bindgen)
        .arg(&input)
        .args(["--target", "no-modules", "--no-typescript", "--out-dir"])
        .arg(&build))?;
    let wasm = build.join("numera_browser_page_bg.wasm");
    let js = build.join("numera_browser_page.js");
    let compiled = build.join("compiled.js");
    let wasm2js_flags = [
        "--enable-bulk-memory",
        "--enable-bulk-memory-opt",
        "--enable-nontrapping-float-to-int",
        "--pedantic",
        "-O",
    ];
    run(Command::new(&wasm2js)
        .arg(&wasm)
        .args(wasm2js_flags)
        .arg("-o")
        .arg(&compiled))?;
    let glue = fs::read_to_string(&js)?;
    let code = fs::read_to_string(&compiled)?;
    let output = factory(glue.clone(), code.clone())?;
    let sources = source_snapshot(repo, &crate_root)?;
    if sources != source_before {
        return Err("Rust browser sources changed while the compiler was running".into());
    }
    let receipt = json!({
        "schema":1,"artifact":"browser-page-rust/generated/runtime.js",
        "authored_language":"Rust","generated_languages":["JavaScript"],
        "factory":"numera_browser_page() returns wasm-bindgen exports synchronously",
        "source_sha256":sources,"rustc":rustc_version,"target":"wasm32-unknown-unknown",
        "path_remapping":numera_browser_page::artifact::PATH_REMAPPING,
        "wasm_bindgen":BINDGEN,"binaryen":BINARYEN,
        "binaryen_archive":BINARYEN_ARCHIVE,"binaryen_archive_sha256":BINARYEN_ARCHIVE_SHA,
        "wasm2js_binary_sha256":BINARYEN_LINUX_SHA,"wasm2js_flags":wasm2js_flags,
        "input_wasm_sha256":digest(&fs::read(input)?),"binding_wasm_sha256":digest(&fs::read(wasm)?),
        "bindgen_glue_sha256":digest(glue.as_bytes()),"compiler_javascript_sha256":digest(code.as_bytes()),
        "compiler_javascript_bytes":code.len(),"artifact_sha256":digest(output.as_bytes()),
        "artifact_bytes":output.len(),"minified":false,"runtime_compilation":false,
        "bridge":["generated module imports/exports become a synchronous classic factory","generated Uint8Array and DataView caches observe non-detaching ArrayBuffer growth"],
        "regeneration_host":"Linux x86_64; output uses portable relative source paths"
    });
    let mut receipt_bytes = serde_json::to_vec_pretty(&receipt)?;
    receipt_bytes.push(b'\n');
    numera_browser_page::artifact::validate(repo, output.as_bytes(), &receipt)?;
    let generated = options.output;
    if options.check {
        let artifact_matches = fs::read(generated.join("runtime.js"))? == output.as_bytes();
        let committed_receipt = fs::read(generated.join("receipt.json"))?;
        let receipt_matches = committed_receipt == receipt_bytes;
        if !artifact_matches || !receipt_matches {
            eprintln!(
                "Browser compiler comparison artifact_matches={artifact_matches} receipt_matches={receipt_matches}"
            );
            if let Ok(committed) = serde_json::from_slice::<Value>(&committed_receipt) {
                for field in [
                    "artifact_sha256",
                    "input_wasm_sha256",
                    "binding_wasm_sha256",
                    "bindgen_glue_sha256",
                    "compiler_javascript_sha256",
                    "source_sha256",
                    "rustc",
                    "path_remapping",
                ] {
                    if committed[field] != receipt[field] {
                        eprintln!("Browser compiler receipt differs field={field}");
                    }
                }
            }
            return Err("Generated browser artifact or source/tool receipt is stale; regenerate with browser-page-build".into());
        }
        println!(
            "Verified exact Rust browser artifact and receipt ({}bytes)",
            output.len()
        );
    } else {
        fs::create_dir_all(&generated)?;
        fs::write(generated.join("runtime.js"), &output)?;
        fs::write(generated.join("receipt.json"), receipt_bytes)?;
        println!(
            "Generated Rust browser artifact and receipt ({}bytes)",
            output.len()
        );
    }
    Ok(())
}
