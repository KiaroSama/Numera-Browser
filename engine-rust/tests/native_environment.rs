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
    let adapter = root.join("adapter.py");
    let mut environment = Environment {
        app: root.clone(),
        home: root.clone(),
        cwd: root.clone(),
        host: "fixture-host".into(),
        browser: Runtime {
            python: std::env::var_os("NUMERA_ORACLE_PYTHON")
                .ok_or("Fixture interpreter missing")?
                .into(),
            adapter: adapter.clone(),
            cwd: root.clone(),
            home: root.clone(),
            logs: root.join("logs"),
            country: None,
            relay: None,
            binary_options: Default::default(),
            deployment: Some(Deployment {
                app: root.clone(),
                environment: BTreeMap::new(),
                default_addons: Vec::new(),
            }),
        },
    };
    let mut host = std::process::Command::new(&environment.browser.python);
    host.args([
        "-I",
        "-c",
        "import platform,sys; sys.stdout.buffer.write(platform.node().encode('utf-8'))",
    ])
    .env_remove("COMPUTERNAME")
    .env_remove("HOSTNAME");
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
    assert_eq!(numera_core::native_host::name()?.as_bytes(), source.stdout);
    let absent = environment.installed()?;
    assert_eq!(absent.len(), 4);
    assert!(absent.values().all(|present| !present));
    fs::write(
        &adapter,
        r#"
import os
assert 'NUMERA_PASSWORD' not in os.environ
assert 'NUMERA_API_TOKEN' not in os.environ
def available():
    return {name: {'installed': name == 'playwright'} for name in ('playwright','patchright','camoufox','cloak')}
def main():
    raise AssertionError('Application or browser was launched by metadata')
"#,
    )?;
    let installed = environment.installed()?;
    assert!(installed["playwright"]);
    assert!(!installed["camoufox"]);
    assert!(
        environment
            .browser
            .deployment
            .as_ref()
            .ok_or("Deployment missing")?
            .default_addons
            .is_empty()
    );
    fs::write(
        &adapter,
        r#"
import enum,sys,types
addons=types.ModuleType('camoufox.addons')
addons.DefaultAddons=enum.Enum('DefaultAddons', ['UBO','FIXTURE'])
sys.modules['camoufox.addons']=addons
def available():
    return {name: {'installed': name == 'camoufox'} for name in ('playwright','patchright','camoufox','cloak')}
"#,
    )?;
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
        &adapter,
        "def available():\n return {'playwright': {'installed': 'not-a-boolean'}}\n",
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
