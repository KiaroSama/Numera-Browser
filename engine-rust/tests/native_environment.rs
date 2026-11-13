use numera_core::{
    browser::runtime::{Deployment, Runtime},
    native_environment::Environment,
};
use std::{collections::BTreeMap, fs, io};
mod common;

#[test]
fn metadata_discovery_uses_sdk_only_adapter_and_rejects_malformed_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("native metadata α")?;
    let root = temporary.path().to_owned();
    let adapter = root.join(if cfg!(windows) {
        "metadata-fixture.exe"
    } else {
        "metadata-fixture"
    });
    let mut environment = Environment {
        app: root.clone(),
        home: root.clone(),
        cwd: root.clone(),
        host: "fixture-host".into(),
        browser: Runtime {
            adapter: adapter.clone(),
            cwd: root.clone(),
            home: root.clone(),
            logs: root.join("logs"),
            country: None,
            relay: None,
            binary_options: Default::default(),
            registry: None,
            deployment: Some(Deployment {
                app: root.clone(),
                environment: BTreeMap::new(),
                default_addons: Vec::new(),
            }),
        },
    };
    let mut host = std::process::Command::new(if cfg!(windows) {
        "hostname.exe"
    } else {
        "hostname"
    });
    host.env_remove("COMPUTERNAME").env_remove("HOSTNAME");
    let source = numera_core::ownership::capture::run(
        &mut host,
        numera_core::ownership::capture::Limits {
            wall: std::time::Duration::from_secs(5),
            idle: std::time::Duration::from_secs(5),
            bytes: 65536,
            cleanup: std::time::Duration::from_secs(5),
        },
    )?;
    assert_eq!(source.exit, 0);
    assert_eq!(
        numera_core::native_host::name()?,
        String::from_utf8(source.stdout)?.trim()
    );
    let absent = environment.installed()?;
    assert_eq!(absent.len(), 4);
    assert!(absent.values().all(|present| !present));
    let fixture_source = root.join("metadata_fixture.rs");
    fs::write(
        &fixture_source,
        r#"
use std::io::{self, Write, Read};
fn main() -> io::Result<()> {
    assert_eq!(std::env::args().skip(1).collect::<Vec<_>>(), ["--metadata"]);
    assert!(std::env::var_os("NUMERA_PASSWORD").is_none());
    assert!(std::env::var_os("NUMERA_API_TOKEN").is_none());
    let config = std::env::current_exe()?.with_extension("json");
    let mut bytes = Vec::new();
    std::fs::File::open(config)?.take(65537).read_to_end(&mut bytes)?;
    assert!(bytes.len() <= 65536);
    io::stdout().write_all(&bytes)
}
"#,
    )?;
    let mut compile =
        std::process::Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
    compile
        .args([
            "--edition=2024",
            "-Cdebuginfo=0",
            "--crate-name=numera_metadata_fixture",
        ])
        .arg(&fixture_source)
        .arg("-o")
        .arg(&adapter);
    let compiled = numera_core::ownership::capture::run(
        &mut compile,
        numera_core::ownership::capture::Limits {
            wall: std::time::Duration::from_secs(30),
            idle: std::time::Duration::from_secs(20),
            bytes: 65536,
            cleanup: std::time::Duration::from_secs(5),
        },
    )?;
    assert_eq!(
        compiled.exit,
        0,
        "Metadata fixture compilation failed: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let receipt = adapter.with_extension("json");
    fs::write(&receipt, br#"{"backends":{"playwright":{"installed":true},"patchright":{"installed":false},"camoufox":{"installed":false},"cloak":{"installed":false}},"default_addons":["UBO"]}"#)?;
    let installed = environment.installed()?;
    assert!(installed["playwright"]);
    assert!(!installed["camoufox"]);
    assert!(!environment.metadata(true)?["camoufox"]);
    assert!(
        !root.join("__pycache__").exists(),
        "SDK metadata wrote bytecode into runtime inputs"
    );
    assert!(
        environment
            .browser
            .deployment
            .as_ref()
            .ok_or("Deployment missing")?
            .default_addons
            .is_empty()
    );
    fs::write(&receipt, br#"{"backends":{"playwright":{"installed":false},"patchright":{"installed":false},"camoufox":{"installed":true},"cloak":{"installed":false}},"default_addons":["UBO","FIXTURE"]}"#)?;
    assert!(environment.installed()?["camoufox"]);
    assert!(
        environment
            .browser
            .deployment
            .as_ref()
            .ok_or("Deployment missing")?
            .default_addons
            .is_empty()
    );
    assert!(environment.metadata(true)?["camoufox"]);
    assert_eq!(
        environment
            .browser
            .deployment
            .as_ref()
            .ok_or("Deployment missing")?
            .default_addons,
        ["UBO", "FIXTURE"]
    );
    fs::write(
        &receipt,
        br#"{"backends":{"playwright":{"installed":"not-a-boolean"}}}"#,
    )?;
    let failure = environment
        .installed()
        .expect_err("Malformed SDK metadata accepted");
    assert_eq!(failure.kind(), io::ErrorKind::Other);
    assert!(failure.retained_record().is_none());
    drop(failure);
    temporary.cleanup()?;
    Ok(())
}
