//! Compiled native process fixture shared by retained ownership tests.
use super::core;
use crate::common::cargo_artifacts::Artifacts;
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
pub fn binary() -> io::Result<PathBuf> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BINARY
        .get_or_init(|| build().map_err(|error| error.to_string()))
        .clone()
        .map_err(failed)
}
fn build() -> io::Result<PathBuf> {
    let artifacts = Artifacts::current()?;
    let directory = (0..64)
        .find_map(|nonce| {
            let path = artifacts.output_directory().join(format!(
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
            artifacts.library("serde_json")?.display()
        ))
        .arg("-o")
        .arg(&binary);
    artifacts.add_dependency_search_paths(&mut compiler, &directory.join("rustc-search.args"))?;
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
