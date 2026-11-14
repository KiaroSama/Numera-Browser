//! Compiled native process fixture shared by retained ownership tests.
use super::core;
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Duration,
};
fn failed(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}
pub fn library(deps: &Path, name: &str) -> io::Result<PathBuf> {
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

pub fn binary() -> io::Result<PathBuf> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BINARY
        .get_or_init(|| build().map_err(|error| error.to_string()))
        .clone()
        .map_err(failed)
}
fn build() -> io::Result<PathBuf> {
    let deps = std::env::current_exe()?
        .parent()
        .ok_or_else(|| failed("deps"))?
        .to_owned();
    let directory = (0..64)
        .find_map(|nonce| {
            let path = deps.join(format!(
                "ownership-native-fixture-{}-{nonce}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => Some(Ok(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .ok_or_else(|| failed("No exclusive fixture directory"))??;
    let binary = directory.join(if cfg!(windows) {
        "helper.exe"
    } else {
        "helper"
    });
    let mut compiler = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    compiler
        .args([
            "--edition=2024",
            "--crate-name=ownership_native_fixture",
            "-Cdebuginfo=0",
        ])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/common/ownership_process_program.rs"),
        )
        .arg("--extern")
        .arg(format!(
            "serde_json={}",
            library(&deps, "serde_json")?.display()
        ))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("-o")
        .arg(&binary);
    let output = core::ownership::capture::run(
        &mut compiler,
        core::ownership::capture::Limits {
            wall: Duration::from_secs(60),
            idle: Duration::from_secs(60),
            bytes: 65536,
            cleanup: Duration::from_secs(5),
        },
    )
    .map_err(failed)?;
    if output.exit != 0 {
        return Err(failed(format!(
            "Native ownership fixture compile failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(binary)
}
pub fn command(mode: &str) -> io::Result<Command> {
    let mut command = Command::new(binary()?);
    command.arg(mode);
    Ok(command)
}
