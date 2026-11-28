//! Shared native fixture compilation; this module declares no test cases.
use crate::common::cargo_artifacts::Artifacts;
use numera_core::ownership::capture;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::Duration,
};

pub fn binary() -> io::Result<PathBuf> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let artifacts = Artifacts::current().map_err(|error| error.to_string())?;
            let directory = artifacts
                .output_directory()
                .join(format!("source-ownership-fixture-{}", std::process::id()));
            fs::create_dir(&directory).map_err(|error| error.to_string())?;
            let binary = directory.join(if cfg!(windows) {
                "helper.exe"
            } else {
                "helper"
            });
            let mut command =
                Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
            command
                .arg("--edition=2024")
                .arg("--crate-name=source_ownership_helper")
                .arg("-Cdebuginfo=0")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/source_ownership/helper.rs"))
                .arg("--extern")
                .arg(format!(
                    "numera_core={}",
                    artifacts
                        .library("numera_core")
                        .map_err(|error| error.to_string())?
                        .display()
                ))
                .arg("--extern")
                .arg(format!(
                    "serde_json={}",
                    artifacts
                        .library("serde_json")
                        .map_err(|error| error.to_string())?
                        .display()
                ))
                .arg("-o")
                .arg(&binary);
            artifacts
                .add_dependency_search_paths(&mut command, &directory.join("rustc-search.args"))
                .map_err(|error| error.to_string())?;
            #[cfg(windows)]
            command.arg("--extern").arg(format!(
                "getrandom={}",
                artifacts
                    .library("getrandom")
                    .map_err(|error| error.to_string())?
                    .display()
            ));
            #[cfg(unix)]
            command.arg("--extern").arg(format!(
                "libc={}",
                artifacts
                    .library("libc")
                    .map_err(|error| error.to_string())?
                    .display()
            ));
            let output = capture::run(
                &mut command,
                capture::Limits {
                    wall: Duration::from_secs(60),
                    idle: Duration::from_secs(60),
                    bytes: 1_048_576,
                    cleanup: Duration::from_secs(2),
                },
            )
            .map_err(|error| error.to_string())?;
            if output.exit != 0 {
                return Err(format!(
                    "Rust fixture compile failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(binary)
        })
        .clone()
        .map_err(io::Error::other)
}
