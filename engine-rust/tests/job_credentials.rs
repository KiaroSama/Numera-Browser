#![cfg(feature = "native-product")]

use numera_core::{
    job_supervisor::SpawnOptions,
    jobs::JobStore,
    ownership::{
        capture::{self, Limits},
        identity,
    },
    storage::{atomic::write_json, repo::ProfileRepo},
    vault,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::Command,
    thread,
    time::{Duration, Instant},
};
mod common;

const OLD: &str = "synthetic-detached-old-master";
const NEW: &str = "synthetic-detached-new-master";

fn now() -> std::io::Result<f64> {
    numera_core::native_entry::now()
}

fn finish(store: &JobStore, id: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let current = store.get(id, now()?)?.ok_or("Detached job disappeared")?;
        if current["running"] == false {
            return Ok(current);
        }
        if Instant::now() >= deadline {
            store.stop(id, "fixture-host", now()?)?;
            return Err("Detached job did not finish within its deadline".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn detached_vault_rotation_keeps_master_keys_out_of_job_artifacts()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("job-vault-credentials")?;
    ProfileRepo::open(temporary.path().into(), None, None)?;
    vault::transaction::enable(temporary.path(), OLD)?;
    let store = JobStore::new(temporary.path().into());
    let argv = [
        "vault",
        "change",
        "--new-password",
        NEW,
        "--detach",
        "--json",
    ]
    .map(String::from);
    let handle = store.spawn_detached(SpawnOptions {
        executable: Path::new(env!("CARGO_BIN_EXE_numera-engine")),
        argv: &argv,
        command: "vault",
        profile: None,
        password: Some(OLD),
    })?;
    let id = handle["id"].as_str().ok_or("Detached job ID missing")?;
    let outcome = finish(&store, id)?;
    assert_eq!(
        outcome["exit"], 0,
        "Detached vault rotation did not execute"
    );
    let repository = ProfileRepo::new(temporary.path().into(), Some(NEW.into()));
    assert!(
        repository.unlocked()?,
        "Detached rotation did not retain the new master key"
    );
    for entry in fs::read_dir(temporary.path().join("jobs"))? {
        let path = entry?.path();
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(path)?;
        for secret in [OLD, NEW] {
            assert!(
                !bytes
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes()),
                "A detached job artifact retained a master password"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

fn rejected_handoff(key: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("job-untrusted-handoff")?;
    ProfileRepo::open(temporary.path().into(), None, None)?;
    fs::create_dir_all(temporary.path().join("jobs"))?;
    let record = temporary.path().join("jobs/rejected.json");
    let executable = Path::new(env!("CARGO_BIN_EXE_numera-engine")).canonicalize()?;
    write_json(
        &record,
        &json!({"id":"rejected", "started":now()?, "pid":null}),
    )?;
    write_json(
        &record.with_extension("launch"),
        &json!({
            "id":"rejected", "root":temporary.path(), "executable":executable,
            "command":"list", "profile":null, "started":now()?,
            "argv":["--data-dir",temporary.path(),"list","--json"],
            "privateArgv":{"version":1,"ciphertext":"tampered-private-arguments"}
        }),
    )?;
    let mut command = Command::new(&executable);
    command
        .env("NUMERA_NATIVE_SUPERVISE", &record)
        .env("NUMERA_JOB_PARENT_PID", std::process::id().to_string())
        .env(
            "NUMERA_JOB_PARENT_TOKEN",
            identity::started_at(std::process::id())?
                .ok_or("Parent birth token missing")?
                .to_string(),
        )
        .env_remove("NUMERA_NATIVE_JOB_GATE")
        .env_remove("NUMERA_PASSWORD")
        .env_remove("NUMERA_JOB_ARGUMENT_KEY");
    if let Some(key) = key {
        command.env("NUMERA_JOB_ARGUMENT_KEY", key);
    }
    let output = capture::run(
        &mut command,
        Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(30),
            bytes: 1024 * 1024,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_ne!(output.exit, 0, "Untrusted detached request was accepted");
    assert!(
        !record.with_extension("log").exists(),
        "Untrusted detached request started a command"
    );
    assert!(
        !record.with_extension("ready").exists(),
        "Untrusted detached request published readiness"
    );
    let retained: Value = serde_json::from_slice(&fs::read(&record)?)?;
    assert!(
        retained["pid"].is_null(),
        "Untrusted detached request published a child identity"
    );
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn detached_request_without_ephemeral_key_refuses_before_child_spawn()
-> Result<(), Box<dyn std::error::Error>> {
    rejected_handoff(None)
}

#[test]
fn detached_request_with_tampered_arguments_refuses_before_child_spawn()
-> Result<(), Box<dyn std::error::Error>> {
    rejected_handoff(Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="))
}
