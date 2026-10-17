use numera_core::{
    browser::runtime::Runtime, cli, native_entry, native_environment::Environment,
    runtime_log::RuntimeLog,
};
use serde_json::Value;
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};
mod common;

#[test]
fn entry_dispatch_reads_real_store_and_preserves_structured_refusals()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("native entry α")?;
    let root = temporary.path().to_owned();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let profile = numera_core::models::Profile::from_record(&serde_json::json!({
        "id":"fixture","name":"Fixture","engine":"playwright","fingerprint":fixture["cases"][0]["input"]
    }))?;
    numera_core::storage::repo::ProfileRepo::new(root.clone(), None).save(profile, 1000.0)?;
    fs::write(
        root.join("broken-adapter.py"),
        "raise AssertionError('Store command must not discover browser SDK')\n",
    )?;
    let log = RuntimeLog::new(&root.join("logs"), "native-entry-test");
    let environment = || Environment {
        app: root.clone(),
        home: root.clone(),
        cwd: root.clone(),
        host: "fixture-host".into(),
        browser: Runtime {
            python: std::env::var_os("NUMERA_ORACLE_PYTHON")
                .expect("Fixture interpreter required")
                .into(),
            adapter: root.join("broken-adapter.py"),
            cwd: root.clone(),
            home: root.clone(),
            logs: root.join("logs"),
            country: None,
            relay: None,
            binary_options: Default::default(),
            deployment: None,
        },
    };
    for (argv, expected_exit) in [
        (vec!["list", "--json"], 0),
        (vec!["show", "missing", "--json"], 1),
    ] {
        let mut registry = cli::registry();
        registry.build();
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(argv.iter().copied()))?;
        let args = cli::reflection::values(&registry, &matches);
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), true, false);
        let code = native_entry::dispatch(
            args.as_object().ok_or("Parsed args missing")?,
            &argv.into_iter().map(String::from).collect::<Vec<_>>(),
            &mut output,
            environment(),
            &root,
            Arc::new(AtomicBool::new(false)),
            &log,
        )?;
        assert_eq!(code, expected_exit);
        let (stdout, stderr) = output.into_parts();
        assert!(stderr.is_empty());
        let observed: Value = serde_json::from_slice(&stdout)?;
        if code == 0 {
            assert_eq!(observed["profiles"][0]["id"], "fixture");
            assert_eq!(observed["profiles"][0]["name"], "Fixture");
        } else {
            assert_eq!(
                observed,
                serde_json::json!({"ok":false,"error":"no profile matching 'missing'"})
            );
        }
        assert!(matches!(
            numera_core::ownership::claims::read(&root, "fixture"),
            numera_core::ownership::claims::Claim::Absent
        ));
    }
    let mut registry = cli::registry();
    registry.build();
    let argv = ["pool", "add", "-", "--json"];
    let matches = registry
        .clone()
        .try_get_matches_from(std::iter::once("numera").chain(argv))?;
    let args = cli::reflection::values(&registry, &matches);
    let mut output = cli::output::Output::new(Vec::new(), Vec::new(), true, false);
    assert_eq!(
        native_entry::dispatch(
            args.as_object().ok_or("Args missing")?,
            &argv.into_iter().map(String::from).collect::<Vec<_>>(),
            &mut output,
            environment(),
            &root,
            Arc::new(AtomicBool::new(true)),
            &log
        )?,
        130
    );
    let (stdout, stderr) = output.into_parts();
    assert!(stdout.is_empty());
    assert!(stderr.is_empty());
    assert!(!root.join("proxies.json").exists());
    drop(log);
    assert!(
        !fs::read_dir(root.join("logs"))?
            .collect::<Result<Vec<_>, _>>()?
            .is_empty()
    );
    temporary.cleanup()?;
    Ok(())
}
