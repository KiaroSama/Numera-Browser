//! Build the Rust application and generated third-party browser bindings.
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitCode},
};
fn run(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("command failed: {command:?}"))
    }
}
fn build() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));
    let target = if target.is_absolute() {
        target
    } else {
        env::current_dir().map_err(|e| e.to_string())?.join(target)
    };
    run(
        Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args([
                "build",
                "--locked",
                "--lib",
                "--release",
                "--target",
                "wasm32-unknown-unknown",
                "--manifest-path",
            ])
            .arg(root.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(&target),
    )?;
    let out = root.join(".dist-rust-pending");
    if out.exists() {
        fs::remove_dir_all(&out).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(out.join("assets")).map_err(|e| e.to_string())?;
    run(Command::new("wasm-bindgen")
        .arg(target.join("wasm32-unknown-unknown/release/numera_dashboard.wasm"))
        .args([
            "--target",
            "web",
            "--no-typescript",
            "--out-name",
            "numera_dashboard",
            "--out-dir",
        ])
        .arg(out.join("assets")))?;
    for name in ["logo-512.png", "favicon.ico", "apple-touch-icon.png"] {
        fs::copy(root.join("public").join(name), out.join(name)).map_err(|e| e.to_string())?;
    }
    fs::copy(root.join("assets/app.css"), out.join("assets/app.css")).map_err(|e| e.to_string())?;
    let source = fs::read_to_string(root.join("index.html")).map_err(|e| e.to_string())?;
    fs::write(out.join("index.html"), source).map_err(|e| e.to_string())?;
    let source_sha = env::var("GITHUB_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&root)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".into());
    let dirty = Command::new("git")
        .args(["diff", "--quiet"])
        .current_dir(&root)
        .status()
        .map(|s| !s.success())
        .unwrap_or(true);
    fs::write(out.join("ci-source.json"),serde_json::to_vec_pretty(&serde_json::json!({"source_sha":source_sha,"version":env!("CARGO_PKG_VERSION"),"runtime":"rust-wasm32","source_dirty":dirty})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    let dist = root.join("dist");
    if dist.exists() {
        fs::remove_dir_all(&dist).map_err(|e| e.to_string())?;
    }
    fs::rename(&out, &dist).map_err(|e| e.to_string())?;
    println!("Rust dashboard built at {}", dist.display());
    Ok(())
}
fn main() -> ExitCode {
    match build() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
