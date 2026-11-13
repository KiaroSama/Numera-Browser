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
#[allow(dead_code)]
#[path = "common/http_fixture.rs"]
mod http_fixture;
use numera_core as core;
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;

#[test]
fn entry_logs_follow_original_data_root_and_preserve_explicit_override()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::{path::Path, process::Command, time::Duration};

    if let Some(case) = std::env::var_os("NUMERA_LOG_ROOT_CASE") {
        let case: Value = serde_json::from_slice(&fs::read(case)?)?;
        let argv: Vec<String> = serde_json::from_value(case["argv"].clone())?;
        assert_eq!(native_entry::run(argv), 0, "Log destination entry failed");
        let logs = Path::new(case["logs"].as_str().ok_or("Log directory missing")?);
        if case["name"] == "unavailable" {
            assert_eq!(fs::read(logs)?, b"retained log blocker");
        } else {
            let files = fs::read_dir(logs)?.collect::<Result<Vec<_>, _>>()?;
            assert_eq!(files.len(), 1, "Entry log destination differs");
            let text = fs::read_to_string(files[0].path())?;
            assert!(text.contains("[INFO] [entry] Startup\n"));
            assert!(text.contains("[INFO] [entry] Shutdown\n"));
            assert!(!text.contains("private-fixture-sentinel"));
        }
        assert!(
            !Path::new(case["app"].as_str().ok_or("App directory missing")?)
                .join("logs")
                .exists(),
            "Entry log destination differs: application directory was used"
        );
        return Ok(());
    }
    let temporary = common::TempDirectory::new("entry log roots α")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/entry_logs_follow_original_data_root_and_preserve_explicit_override.json"
    ))?;
    assert_eq!(cases.len(), 9, "Log directory observations missing");
    for mut case in cases {
        let observed_root = Path::new(
            case["app"]
                .as_str()
                .ok_or("Observed application root missing")?,
        )
        .parent()
        .and_then(Path::parent)
        .ok_or("Observed temporary root missing")?
        .to_string_lossy()
        .into_owned();
        http_fixture::rebase(&mut case, &observed_root, temporary.path())?;
        for key in ["app", "home", "cwd"] {
            fs::create_dir_all(case[key].as_str().ok_or("Fixture directory missing")?)?;
        }
        if matches!(case["name"].as_str(), Some("home" | "legacy")) {
            fs::write(
                Path::new(case["app"].as_str().ok_or("App missing")?).join("data"),
                b"read-only layout stand-in",
            )?;
        }
        if case["name"] == "legacy" {
            fs::create_dir(
                Path::new(case["home"].as_str().ok_or("Home missing")?).join(".persona"),
            )?;
        }
        if case["name"] == "unavailable" {
            let path = Path::new(case["logs"].as_str().ok_or("Log path missing")?);
            fs::create_dir_all(path.parent().ok_or("Log parent missing")?)?;
            fs::write(path, b"retained log blocker")?;
        }
        fs::write(
            case["record"].as_str().ok_or("Record path missing")?,
            serde_json::to_vec(&case)?,
        )?;
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args([
                "--exact",
                "entry_logs_follow_original_data_root_and_preserve_explicit_override",
                "--nocapture",
                "--test-threads=1",
            ])
            .current_dir(case["cwd"].as_str().ok_or("CWD missing")?)
            .env(
                "NUMERA_LOG_ROOT_CASE",
                case["record"].as_str().ok_or("Record missing")?,
            )
            .env("NUMERA_APP_DIR", case["app"].as_str().ok_or("App missing")?)
            .env("HOME", case["home"].as_str().ok_or("Home missing")?)
            .env("USERPROFILE", case["home"].as_str().ok_or("Home missing")?)
            .env(
                "NUMERA_BROWSER_ADAPTER",
                temporary.path().join("absent-sdk"),
            )
            .env("NUMERA_PASSWORD", "private-fixture-sentinel")
            .env_remove("NUMERA_NATIVE_SUPERVISE")
            .env_remove("NUMERA_NATIVE_JOB_GATE")
            .env_remove("NUMERA_PARENT_PID")
            .env_remove("NUMERA_LOG_DIR")
            .env_remove("NUMERA_DATA_DIR");
        if let Some(value) = case["override"].as_str() {
            child.env("NUMERA_LOG_DIR", value);
        }
        if let Some(value) = case["inherited"].as_str() {
            child.env("NUMERA_DATA_DIR", value);
        }
        let observed = capture::run(
            &mut child,
            capture::Limits {
                wall: Duration::from_secs(5),
                idle: Duration::from_secs(5),
                bytes: 65536,
                cleanup: Duration::from_secs(5),
            },
        )?;
        assert_eq!(
            observed.exit, 0,
            "Entry log destination differs: {}",
            case["name"]
        );
        let text = String::from_utf8(observed.stdout)?;
        assert!(text.lines().any(|line| line == "running 1 test"));
        assert!(text.contains("\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "));
        let stderr = String::from_utf8(observed.stderr)?;
        assert_eq!(
            stderr.contains("Runtime file logging unavailable"),
            case["name"] == "unavailable"
        );
        assert!(!stderr.contains("private-fixture-sentinel"));
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn browser_dispatch_uses_actual_handshake_without_redundant_metadata()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        browser::runtime::Deployment, ownership::claims, storage::repo::ProfileRepo,
    };
    use serde_json::json;
    let temporary = common::TempDirectory::new("entry-sdk-handshake")?;
    let root = temporary.path();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let profile = numera_core::models::Profile::from_record(&json!({
        "id":"fixture","name":"Fixture","engine":"playwright","fingerprint":fixture["cases"][0]["input"]
    }))?;
    ProfileRepo::new(root.into(), None).save(profile, 1000.0)?;
    let adapter_path = root.join("sdk");
    let log = RuntimeLog::new(&root.join("logs"), "native-entry-test");
    for (installed, packaged, command) in [
        (true, false, "cookies"),
        (false, false, "cookies"),
        (true, true, "cookies"),
        (true, false, "engines"),
    ] {
        let app = root.join(if packaged { "packaged" } else { "unbundled" });
        fs::create_dir_all(&app)?;
        if packaged {
            fs::create_dir_all(app.join("browsers"))?;
            fs::write(app.join("browsers/marker"), b"packaged resource discovery")?;
        }
        let adapter = fixture::executable(
            &adapter_path,
            "rpc",
            json!({"metadata":{"backends":{"playwright":{"installed":installed}}},"metadata_failure":true}),
        )?;
        let environment = Environment {
            app: app.clone(),
            home: root.into(),
            cwd: root.into(),
            host: "fixture-host".into(),
            browser: Runtime {
                adapter: adapter.clone(),
                cwd: root.into(),
                home: root.into(),
                logs: root.join("logs"),
                country: None,
                relay: None,
                binary_options: Default::default(),
                deployment: Some(Deployment {
                    app,
                    environment: Default::default(),
                    default_addons: Vec::new(),
                }),
            },
        };
        let argv = if command == "cookies" {
            vec!["cookies", "list", "fixture", "--json"]
        } else {
            vec!["engines", "--json"]
        };
        let mut registry = cli::registry();
        registry.build();
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(argv.iter().copied()))?;
        let values = cli::reflection::values(&registry, &matches);
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), true, false);
        let result = native_entry::dispatch(
            values.as_object().ok_or("Args missing")?,
            &argv.into_iter().map(String::from).collect::<Vec<_>>(),
            &mut output,
            environment,
            root,
            Arc::new(AtomicBool::new(false)),
            &log,
        );
        if packaged || command == "engines" {
            assert!(
                result
                    .expect_err("Required metadata was skipped")
                    .to_string()
                    .contains("metadata discovery failed")
            );
        } else if !installed {
            assert!(
                result
                    .expect_err("Missing SDK accepted")
                    .to_string()
                    .contains("SDK is not installed")
            );
        } else {
            assert_eq!(
                result.expect("Browser dispatch repeated SDK metadata preflight"),
                0
            );
            let (stdout, stderr) = output.into_parts();
            assert!(stderr.is_empty());
            assert_eq!(
                serde_json::from_slice::<Value>(&stdout)?,
                json!({"total":0,"domains":{}})
            );
        }
        assert!(matches!(
            claims::read(root, "fixture"),
            claims::Claim::Absent
        ));
    }
    let timings = fs::read_to_string(log.path().ok_or("Entry log missing")?)?;
    assert_eq!(timings.matches("Native timing phase=prepare ").count(), 4);
    assert_eq!(timings.matches("Native timing phase=metadata ").count(), 4);
    assert_eq!(timings.matches("Native timing phase=execute ").count(), 2);
    assert_eq!(timings.matches("Native timing phase=shutdown ").count(), 2);
    drop(log);
    temporary.cleanup()?;
    Ok(())
}

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
    let broken_adapter = fixture::executable(
        &root.join("broken-adapter"),
        "rpc",
        serde_json::json!({"reject":true}),
    )?;
    let log = RuntimeLog::new(&root.join("logs"), "native-entry-test");
    let environment = || Environment {
        app: root.clone(),
        home: root.clone(),
        cwd: root.clone(),
        host: "fixture-host".into(),
        browser: Runtime {
            adapter: broken_adapter.clone(),
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
    let timings = fs::read_to_string(log.path().ok_or("Entry log missing")?)?;
    for phase in ["prepare", "metadata", "execute", "shutdown"] {
        assert_eq!(
            timings
                .matches(&format!("Native timing phase={phase} "))
                .count(),
            3
        );
    }
    drop(log);
    assert!(
        !fs::read_dir(root.join("logs"))?
            .collect::<Result<Vec<_>, _>>()?
            .is_empty()
    );
    temporary.cleanup()?;
    Ok(())
}
