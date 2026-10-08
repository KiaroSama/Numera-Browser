use numera_core::{browser::adapter::Adapter, ownership::claims};
use serde_json::Map;
use std::{path::Path, process::Command, time::Duration};
mod common;

fn adapter(python: &std::ffi::OsStr, logs: &Path) -> Command {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("Repository exists")
        .join("scripts/browser-adapter/adapter.py");
    let mut command = Command::new(python);
    command
        .arg("-u")
        .arg(script)
        .env("NUMERA_ADAPTER_LOG_DIR", logs)
        .env("PYTHONUTF8", "1");
    command
}
#[test]
fn actual_sdk_adapter_stays_inert_before_open_and_cleans_protocol_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    for invalid in [false, true] {
        let temporary = common::TempDirectory::new("browser-adapter-protocol")?;
        let root = temporary.path();
        let mut command = adapter(&python, &root.join("logs"));
        let mut sdk = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        assert_eq!(
            claims::state(&claims::read(root, "a"), "fixture-host"),
            claims::State::Busy
        );
        assert_eq!(
            sdk.metadata()["backends"]
                .as_object()
                .ok_or("Backend metadata missing")?
                .len(),
            4
        );
        assert!(
            !root.join("user-data").exists(),
            "Hello performed browser work"
        );
        if invalid {
            let failure = sdk.call("unsupported", Map::new(), Duration::from_secs(5));
            assert!(failure.is_err());
        } else {
            assert_eq!(
                sdk.call("close", Map::new(), Duration::from_secs(5))?["closing"],
                true
            );
        }
        sdk.finish(Duration::from_secs(5))?;
        drop(sdk);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        let logs = std::fs::read_dir(root.join("logs"))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(logs.len(), 1);
        let log = std::fs::read_to_string(logs[0].path())?;
        assert!(log.contains("[INFO] [numera.browser-adapter] Starting"));
        assert!(log.contains("Finished exit="));
        temporary.cleanup()?;
    }
    Ok(())
}
#[test]
fn native_transport_refuses_wrong_envelope_partial_frame_and_unresponsive_writer()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    for script in [
        "import sys,json; sys.stdin.readline(); print(json.dumps({'protocol':2,'id':1,'ok':True,'result':{}}),flush=True)",
        "import sys; sys.stdin.readline(); sys.stdout.write('{'); sys.stdout.flush()",
        "import threading; threading.Event().wait()",
    ] {
        let temporary = common::TempDirectory::new("browser-adapter-refusal")?;
        let mut command = Command::new(&python);
        command.args(["-u", "-c", script]);
        let result = Adapter::spawn(
            temporary.path(),
            "a",
            "fixture-host",
            &mut command,
            Duration::from_millis(250),
        );
        assert!(result.is_err());
        assert!(matches!(
            claims::read(temporary.path(), "a"),
            claims::Claim::Absent
        ));
        temporary.cleanup()?;
    }
    Ok(())
}
