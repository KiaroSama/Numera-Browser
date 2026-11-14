// Shared, compiled Rust subprocess fixtures. These helpers add no test cases.
use super::core;
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
fn failed(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
fn library(deps: &Path, name: &str) -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let stem = executable
        .file_stem()
        .ok_or_else(|| failed("Test artifact stem missing"))?
        .to_string_lossy();
    let (_, suffix) = stem
        .rsplit_once('-')
        .ok_or_else(|| failed("Hashed test artifact required"))?;
    let fingerprints = deps
        .parent()
        .ok_or_else(|| failed("Target profile directory missing"))?
        .join(".fingerprint");
    let own_dir = fingerprints.join(format!("{}-{suffix}", env!("CARGO_PKG_NAME")));
    let own_file = fs::read_dir(own_dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("test-"))
                && p.extension().is_some_and(|e| e == "json")
        })
        .ok_or_else(|| failed("Test dependency metadata missing"))?;
    let own: Value = serde_json::from_slice(&fs::read(own_file)?).map_err(failed)?;
    let wanted = own["deps"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r[1] == name))
        .and_then(|r| r[3].as_u64())
        .ok_or_else(|| failed(format!("Test dependency {name} missing")))?;
    let package = if name == "numera_core" {
        "numera-core"
    } else {
        name
    };
    fs::read_dir(deps)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&format!("lib{name}-")))
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .find(|p| {
            let Some(stem) = p.file_stem() else {
                return false;
            };
            let stem = stem.to_string_lossy();
            let Some(suffix) = stem.strip_prefix(&format!("lib{name}-")) else {
                return false;
            };
            let encoded =
                fs::read_to_string(fingerprints.join(format!("{package}-{suffix}/lib-{name}")))
                    .unwrap_or_default();
            u64::from_str_radix(encoded.trim(), 16)
                .is_ok_and(|n| u64::from_le_bytes(n.to_be_bytes()) == wanted)
        })
        .ok_or_else(|| failed(format!("Exact compiled {name} dependency missing")))
}
fn build(sdk: bool) -> io::Result<PathBuf> {
    let deps = std::env::current_exe()?
        .parent()
        .ok_or_else(|| failed("Dependency directory missing"))?
        .to_owned();
    // This file can be included by multiple modules in one libtest process.
    // Each module has its own OnceLock, so a PID alone is not a build identity.
    let dir = loop {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(failed)?;
        let dir = deps.join(format!(
            "browser-fixture-{}-{}-{:032x}",
            std::process::id(),
            if sdk { "sdk" } else { "rpc" },
            u128::from_le_bytes(nonce)
        ));
        match fs::create_dir(&dir) {
            Ok(()) => break dir,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let binary = dir.join(if cfg!(windows) {
        "fixture.exe"
    } else {
        "fixture"
    });
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(if sdk {
        "tests/source_browser/fixture.rs"
    } else {
        "tests/common/browser_fixture_main.rs"
    });
    let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    command
        .arg("--edition=2024")
        .arg("--crate-name=numera_browser_fixture")
        .arg("-Cdebuginfo=0")
        .arg(source)
        .arg("--extern")
        .arg(format!(
            "serde_json={}",
            library(&deps, "serde_json")?.display()
        ));
    if sdk {
        for name in ["numera_core", "tokio"] {
            command
                .arg("--extern")
                .arg(format!("{name}={}", library(&deps, name)?.display()));
        }
        command.env(
            "NUMERA_TEST_WORKER",
            option_env!("CARGO_BIN_EXE_numera-browser-runtime").unwrap_or(""),
        );
    }
    command
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("-o")
        .arg(&binary);
    let result = core::ownership::capture::run(
        &mut command,
        core::ownership::capture::Limits {
            wall: Duration::from_secs(60),
            idle: Duration::from_secs(60),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )
    .map_err(failed)?;
    if result.exit != 0 {
        return Err(failed(format!(
            "Rust browser fixture compilation failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )));
    }
    Ok(binary)
}
fn cached(slot: &OnceLock<Result<PathBuf, String>>, sdk: bool) -> io::Result<PathBuf> {
    slot.get_or_init(|| build(sdk).map_err(|e| e.to_string()))
        .clone()
        .map_err(failed)
}
pub fn binary() -> io::Result<PathBuf> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    cached(&BINARY, false)
}
pub fn sdk_binary() -> io::Result<PathBuf> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    cached(&BINARY, true)
}
pub fn command(root: &Path, mode: &str, config: Value) -> io::Result<Command> {
    static NONCE: AtomicU64 = AtomicU64::new(0);
    fs::create_dir_all(root)?;
    let config_file = root.join(format!(
        "browser-fixture-{}.json",
        NONCE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&config_file, serde_json::to_vec(&config).map_err(failed)?)?;
    let mut command = Command::new(binary()?);
    command.arg(mode).arg(config_file);
    Ok(command)
}
pub fn executable(path: &Path, mode: &str, mut config: Value) -> io::Result<PathBuf> {
    let path = if cfg!(windows) && path.extension().is_none() {
        path.with_extension("exe")
    } else {
        path.to_owned()
    };
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| failed("Fixture executable parent missing"))?,
    )?;
    fs::copy(binary()?, &path)?;
    config["mode"] = json!(mode);
    config["base"] = json!(path);
    fs::write(
        path.with_extension("fixture.json"),
        serde_json::to_vec(&config).map_err(failed)?,
    )?;
    Ok(path)
}
pub fn oracle(name: &str) -> io::Result<Value> {
    use sha2::Digest;
    let text = match name {
        "cookie-write" => include_str!("../fixtures/browser-characterization/cookie-write.json"),
        "warmup" => include_str!("../fixtures/browser-characterization/warmup.json"),
        "launch-cli" => include_str!("../fixtures/browser-characterization/launch-cli.json"),
        "bundled" => include_str!("../fixtures/browser-characterization/bundled.json"),
        "extension-paths" => {
            include_str!("../fixtures/browser-characterization/extension-paths.json")
        }
        _ => return Err(failed("Unknown frozen browser observation")),
    };
    let manifest: Value =
        serde_json::from_str(include_str!("../fixtures/browser-oracle-manifest.json"))
            .map_err(failed)?;
    let suffix = format!("/{name}.json");
    let row = manifest["fixtures"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["file"]
                    .as_str()
                    .is_some_and(|path| path.ends_with(&suffix))
            })
        })
        .ok_or_else(|| failed("Frozen browser provenance missing"))?;
    let hash = sha2::Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    if row["sha256"] != hash {
        return Err(failed(
            "Frozen browser observation digest differs from reviewed manifest",
        ));
    }
    let mut value: Value = serde_json::from_str(text).map_err(failed)?;
    fn platform(value: &mut Value) {
        match value {
            Value::String(text) if cfg!(windows) && text.contains("$ROOT") => {
                *text = text.replace('/', "\\")
            }
            Value::Array(rows) => rows.iter_mut().for_each(platform),
            Value::Object(rows) => rows.values_mut().for_each(platform),
            _ => {}
        }
    }
    platform(&mut value);
    Ok(value)
}
pub fn worker(root: &Path, backend: &str, mode: &str) -> io::Result<Command> {
    let runtime = std::env::var_os("NUMERA_BROWSER_RUNTIME")
        .or_else(|| option_env!("CARGO_BIN_EXE_numera-browser-runtime").map(Into::into))
        .ok_or_else(|| failed("Native Rust SDK runtime required"))?;
    let driver = root.join("driver");
    fs::create_dir_all(driver.join("package"))?;
    fs::copy(
        sdk_binary()?,
        driver.join(if cfg!(windows) { "node.exe" } else { "node" }),
    )?;
    fs::write(
        driver.join("package/cli.js"),
        b"Rust vendor protocol fixture",
    )?;
    fs::write(
        driver.join("package/package.json"),
        json!({"version":if backend=="camoufox"{"1.62.0"}else{"1.64.0"}}).to_string(),
    )?;
    fs::write(driver.join("vendor-mode"), mode)?;
    let key = match backend {
        "playwright" => "NUMERA_PLAYWRIGHT_DRIVER",
        "patchright" => "NUMERA_PATCHRIGHT_DRIVER",
        "camoufox" => "NUMERA_CAMOUFOX_DRIVER",
        "cloak" => "NUMERA_CLOAK_DRIVER",
        _ => return Err(failed("Unknown fixture SDK backend")),
    };
    let mut command = Command::new(runtime);
    command
        .env(key, driver)
        .env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"));
    Ok(command)
}
pub fn requests(root: &Path) -> io::Result<Vec<Value>> {
    fs::read_to_string(root.join("driver/vendor-requests.jsonl"))?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(failed))
        .collect()
}
