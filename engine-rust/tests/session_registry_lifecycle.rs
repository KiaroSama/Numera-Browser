use numera_core::{
    ownership::{
        claims,
        registry::{Kind, Registry},
    },
    storage::dashboard::DashboardStore,
};
use serde_json::{Value, json};
use std::{
    fs,
    process::{Command, Stdio},
    time::Duration,
};
mod common;

fn helper(python: &std::ffi::OsStr, ready: &std::path::Path, release: &std::path::Path) -> Command {
    let script = "import json,os,pathlib,time; p=pathlib.Path(os.environ['FIXTURE_READY']); t=p.with_suffix('.part'); t.write_text(json.dumps({'ok':True}),encoding='utf-8'); t.replace(p); stop=pathlib.Path(os.environ['FIXTURE_RELEASE']); deadline=time.monotonic()+10; exec('while not stop.exists() and time.monotonic()<deadline:\\n time.sleep(0.01)')";
    let mut command = Command::new(python);
    command
        .args(["-u", "-c", script])
        .env("FIXTURE_READY", ready)
        .env("FIXTURE_RELEASE", release)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}
#[test]
fn registry_reaps_only_owned_children_and_records_exact_schedule_occurrence()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("session-registry")?;
    let root = temporary.path();
    let store = DashboardStore::new(root.to_path_buf(), None);
    store.save(
        json!({"id":"a","name":"A","seed":42}),
        Default::default(),
        None,
        None,
        1700000000.0,
    )?;
    fs::write(
        root.join("schedules.json"),
        serde_json::to_vec(&json!([
        {"id":"one","profileId":"a","lastOutcome":null},
        {"id":"two","profileId":"a","lastOutcome":null}]))?,
    )?;
    let registry = Registry::new(root.to_path_buf(), "fixture-host".into(), None)?;
    let foreign = Registry::new(root.to_path_buf(), "fixture-host".into(), None)?;
    let ready = root.join("ready.json");
    let release = root.join("release");
    let mut command = helper(&python, &ready, &release);
    assert!(registry.spawn(
        "a",
        &mut command,
        &ready,
        Kind::ScheduledWarmup,
        Some("one".into()),
        Duration::from_secs(5)
    )?);
    assert!(registry.owns("a")?);
    assert!(registry.running("a")?);
    assert!(foreign.running("a")?);
    assert!(!foreign.stop("a", Duration::from_secs(1))?);
    assert!(
        registry.running("a")?,
        "Foreign stop terminated another registry's tree"
    );
    assert!(
        !registry.spawn(
            "a",
            &mut command,
            &ready,
            Kind::Launch,
            None,
            Duration::from_secs(5)
        )?,
        "Duplicate launch replaced ownership"
    );
    fs::write(&release, b"release")?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while registry.running("a")? {
        if std::time::Instant::now() >= deadline {
            return Err("Registry did not reap exited helper".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!registry.owns("a")?);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    let profile = store.get("a")?.ok_or("Profile missing")?;
    assert!(
        profile["_last_active"]
            .as_f64()
            .is_some_and(|time| time > 1700000000.0)
    );
    let schedules: Value = serde_json::from_slice(&fs::read(root.join("schedules.json"))?)?;
    assert_eq!(schedules[0]["lastOutcome"], "ok");
    assert_eq!(schedules[1]["lastOutcome"], Value::Null);
    let revision = profile["_rev"].clone();
    registry.reap()?;
    assert_eq!(
        store.get("a")?.ok_or("Profile missing")?["_rev"],
        revision,
        "Reaper finalized twice"
    );
    assert!(registry.shutdown(Duration::from_secs(5)).is_empty());
    assert!(
        registry
            .spawn(
                "a",
                &mut command,
                &ready,
                Kind::Launch,
                None,
                Duration::from_secs(5)
            )
            .is_err()
    );
    drop(registry);
    drop(foreign);
    temporary.cleanup()?;
    Ok(())
}
