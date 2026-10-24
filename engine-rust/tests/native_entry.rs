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
    let adapter = root.join("sdk.py");
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
        fs::write(
            &adapter,
            format!(
                r#"
import json,sys
def available():
 raise RuntimeError('Redundant metadata preflight')
if __name__=='__main__':
 for line in sys.stdin:
  request=json.loads(line);op=request['operation']
  if op=='hello':result={{'backends':{{'playwright':{{'installed':{installed}}}}}}}
  elif op=='open':result={{'opened':True,'backend':'playwright'}}
  elif op=='cookies-read':result={{'cookies':[]}}
  elif op=='close':result={{'closing':True}}
  else:raise RuntimeError('Unexpected SDK operation')
  print(json.dumps({{'protocol':1,'id':request['id'],'ok':True,'result':result}}),flush=True)
  if op=='close':break
"#,
                installed = if installed { "True" } else { "False" }
            ),
        )?;
        let environment = Environment {
            app: app.clone(),
            home: root.into(),
            cwd: root.into(),
            host: "fixture-host".into(),
            browser: Runtime {
                python: std::env::var_os("NUMERA_ORACLE_PYTHON")
                    .ok_or("Fixture interpreter required")?
                    .into(),
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
