#![cfg(feature = "native-product")]

use numera_core::ownership::capture::{self, Limits};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};
mod common;

fn run(root: &Path, args: &[&str]) -> Result<(i32, String), Box<dyn std::error::Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_numera-engine"));
    command
        .arg("--data-dir")
        .arg(root)
        .args(args)
        .env("NUMERA_LOG_DIR", root.join("logs"))
        .env_remove("NUMERA_PASSWORD")
        .env_remove("NUMERA_NATIVE_SUPERVISE")
        .env_remove("NUMERA_NATIVE_JOB_GATE")
        .env_remove("NUMERA_UPDATE_BRIDGE_PORT")
        .env_remove("NUMERA_UPDATE_TOKEN");
    let result = capture::run(
        &mut command,
        Limits {
            wall: Duration::from_secs(15),
            idle: Duration::from_secs(15),
            bytes: 1024 * 1024,
            cleanup: Duration::from_secs(5),
        },
    )?;
    Ok((result.exit, String::from_utf8(result.stdout)?))
}

#[test]
fn update_preferences_execute_in_json_and_human_modes_and_persist()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("update-cli-config")?;
    let root = temporary.path();
    for mode in [true, false] {
        let mut args = vec!["update", "config"];
        if mode {
            args.push("--json");
        }
        let (exit, output) = run(root, &args)?;
        assert_eq!(exit, 0, "Registered update config failed: {output}");
        let value: Value = serde_json::from_str(&output)?;
        assert_eq!(value["app_auto_update"], false);
        assert_eq!(value["clean_old_versions_on_update"], true);
    }
    let (exit, output) = run(
        root,
        &[
            "update",
            "config",
            "--clean-old-versions",
            "false",
            "--json",
        ],
    )?;
    assert_eq!(exit, 0, "Preference write failed: {output}");
    let (_, output) = run(root, &["update", "config", "--json"])?;
    assert_eq!(
        serde_json::from_str::<Value>(&output)?["clean_old_versions_on_update"],
        false
    );
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn standalone_update_check_never_claims_a_remote_check_or_latest_version()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("update-cli-check")?;
    for channel in ["app", "browsers", "all"] {
        let (exit, output) = run(
            temporary.path(),
            &["update", "check", "--channel", channel, "--json"],
        )?;
        assert_eq!(
            exit, 1,
            "Unavailable update discovery must not report success"
        );
        let value: Value = serde_json::from_str(&output)?;
        assert_eq!(value["state"], "unavailable");
        assert_eq!(value["checked"], false);
        assert!(value["app"]["latest"].is_null());
        assert!(
            value["message"]
                .as_str()
                .is_some_and(|text| text.contains("desktop"))
        );
        assert!(!output.contains("original application"));
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn update_activation_reaches_the_explicit_desktop_consent_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("update-cli-apply")?;
    let (exit, output) = run(
        temporary.path(),
        &["update", "apply", "--channel", "all", "--json"],
    )?;
    assert_eq!(exit, 1);
    let value: Value = serde_json::from_str(&output)?;
    assert!(
        value["error"]
            .as_str()
            .is_some_and(|error| error.contains("desktop updater bridge"))
    );
    assert!(!output.contains("original application"));
    assert!(!temporary.path().join("update-preferences.json").exists());
    temporary.cleanup()?;
    Ok(())
}
