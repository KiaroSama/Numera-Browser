#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::io::Result<()> {
    numera_dashboard::development::run(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("dist"),
    )
}
#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("dashboard-dev runs on the host; compile the dashboard library for WebAssembly");
}
