use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};
pub fn limit(wall: Duration) -> numera_core::ownership::capture::Limits {
    numera_core::ownership::capture::Limits {
        wall,
        idle: wall,
        bytes: 8 * 1024 * 1024,
        cleanup: Duration::from_secs(3),
    }
}
pub fn fixture() -> PathBuf {
    super::browser_fixture::sdk_binary().expect("compiled Rust SDK fixture")
}
pub fn vendor(root: &Path, mode: &str) -> PathBuf {
    fs::create_dir_all(root.join("package")).expect("browser contract");
    fs::copy(
        fixture(),
        root.join(if cfg!(windows) { "node.exe" } else { "node" }),
    )
    .expect("browser contract");
    fs::write(
        root.join("package/cli.js"),
        b"third-party transport fixture",
    )
    .expect("browser contract");
    fs::write(
        root.join("package/package.json"),
        br#"{"version":"1.64.0"}"#,
    )
    .expect("browser contract");
    fs::write(root.join("vendor-mode"), mode.as_bytes()).expect("browser contract");
    root.into()
}
pub fn worker(root: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_numera-browser-runtime"));
    c.env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"))
        .env("NUMERA_BROWSER_DRIVERS", root.join("missing-drivers"))
        .env_remove("CLOAKBROWSER_LICENSE_KEY")
        .env_remove("CLOAKBROWSER_BINARY_PATH")
        .env_remove("CLOAKBROWSER_CACHE_DIR")
        .env_remove("CLOAKBROWSER_LICENSE_STATUS_FILE")
        .env_remove("NUMERA_LOG_DIR");
    c
}
pub fn logs(root: &Path) -> String {
    fs::read_dir(root.join("logs"))
        .expect("browser contract")
        .filter_map(Result::ok)
        .map(|e| fs::read_to_string(e.path()).expect("browser contract"))
        .collect()
}
pub fn wait(path: &Path) {
    let end = std::time::Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < end,
            "fixture marker missing: {}",
            path.display()
        );
        std::thread::park_timeout(Duration::from_millis(2));
    }
}
pub fn requests(root: &Path) -> Vec<Value> {
    fs::read_to_string(root.join("vendor-requests.jsonl"))
        .expect("browser contract")
        .lines()
        .map(|line| serde_json::from_str(line).expect("browser contract"))
        .collect()
}
pub fn native_open(
    root: &Path,
    backend: &str,
    mode: &str,
    p: &Profile,
) -> Result<browser::adapter::Adapter, numera_core::ownership::ready::StartupFailed> {
    let driver = vendor(&root.join("driver"), mode);
    let mut c = worker(root);
    c.env(
        if backend == "camoufox" {
            "NUMERA_CAMOUFOX_DRIVER"
        } else if backend == "cloak" {
            "NUMERA_CLOAK_DRIVER"
        } else {
            "NUMERA_PLAYWRIGHT_DRIVER"
        },
        driver,
    );
    let binary = root.join("chrome");
    fs::write(&binary, b"selected native fixture").expect("browser contract");
    c.env("CLOAKBROWSER_BINARY_PATH", binary);
    let session =
        numera_core::ownership::session::Session::admit(root, &p.id, "source-browser-host")
            .expect("browser contract");
    browser::launch::open(
        session,
        p.clone(),
        &mut c,
        browser::launch::PreparedLaunch {
            backend,
            headless: true,
            extra_args: &[],
            cwd: root,
            home: root,
            country: None,
            relay: None,
            binary_options: serde_json::Map::new(),
        },
        1000.0,
    )
}

pub fn runtime(root: &Path) -> browser::runtime::Runtime {
    vendor(&root.join("driver"), "normal");
    browser::runtime::Runtime {
        adapter: fixture(),
        cwd: root.into(),
        home: root.into(),
        logs: root.join("logs"),
        country: None,
        relay: None,
        binary_options: serde_json::Map::new(),
        registry: None,
        deployment: None,
    }
}
