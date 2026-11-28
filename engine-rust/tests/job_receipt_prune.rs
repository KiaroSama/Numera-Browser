#![cfg(unix)]

use numera_core::{
    jobs::JobStore,
    ownership::{identity, unit_report},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read},
    os::unix::{fs::symlink, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
mod common;

const CHILD: &str = "NUMERA_TEST_RECEIPT_PRUNE_CHILD";

struct Owner(Child, Value);
impl Owner {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "prune_removes_only_the_completed_jobs_unreferenced_empty_receipt",
            ])
            .env(CHILD, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?;
        let mut owner = Self(child, Value::Null);
        let pid = owner.0.id();
        let token = identity::started_at(pid)?.ok_or("Fixture birth missing")?;
        owner.1 = json!({"pid":pid,"token":token,"pidToken":token,
            "tree":{"kind":"pgid","group":pid}});
        Ok(owner)
    }
    fn finish(&mut self) -> io::Result<()> {
        drop(self.0.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.0.try_wait()?.is_none() {
            if Instant::now() >= deadline {
                return Err(io::Error::other("Receipt fixture did not exit"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn receipt(root: &Path, name: &str, tree: &Value) -> io::Result<PathBuf> {
    let path = root.join("sessions").join(format!("{name}.units"));
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("Receipt parent missing"))?,
    )?;
    unit_report::initialize(&path)?;
    let attempt = unit_report::begin_at(&path)?;
    unit_report::record_at(&path, tree, &attempt)?;
    Ok(path)
}

fn job(root: &Path, id: &str, receipt: &Path, tree: &Value, starting: bool) -> io::Result<()> {
    let directory = root.join("jobs");
    fs::create_dir_all(&directory)?;
    let units = directory.join(format!("{id}.units"));
    unit_report::initialize(&units)?;
    let mut linked = tree.clone();
    linked["browserUnits"] = receipt.to_string_lossy().into_owned().into();
    let attempt = unit_report::begin_at(&units)?;
    unit_report::record_at(&units, &linked, &attempt)?;
    let mut record = tree.clone();
    record["id"] = id.into();
    record["units"] = units.to_string_lossy().into_owned().into();
    record["started"] = 1000.0.into();
    if starting {
        record["pid"] = Value::Null;
    } else {
        record["state"] = "done".into();
    }
    fs::write(
        directory.join(format!("{id}.json")),
        serde_json::to_vec(&record)?,
    )
}

#[test]
fn prune_removes_only_the_completed_jobs_unreferenced_empty_receipt()
-> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os(CHILD).is_some() {
        io::stdin().read_to_end(&mut Vec::new())?;
        return Ok(());
    }
    let temporary = common::TempDirectory::new("job-receipt-prune")?;
    let root = temporary.path().canonicalize()?;
    let mut owner = Owner::start()?;
    owner.finish()?;
    let nominated = receipt(&root, "browser-owned", &owner.1)?;
    let unrelated = receipt(&root, "browser-unrelated", &owner.1)?;
    job(&root, "complete", &nominated, &owner.1, false)?;
    assert_eq!(JobStore::new(root.clone()).prune(1001.0)?, 1);
    assert!(!root.join("jobs/complete.json").exists());
    assert!(!root.join("jobs/complete.units").exists());
    assert!(
        !nominated.exists(),
        "Completed job left its unreferenced empty receipt"
    );
    assert!(
        unrelated.is_file(),
        "A receipt was inferred from its name alone"
    );
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn prune_keeps_a_shared_receipt_until_the_last_job_reference_retires()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("job-shared-receipt")?;
    let root = temporary.path().canonicalize()?;
    let mut owner = Owner::start()?;
    owner.finish()?;
    let nominated = receipt(&root, "browser-shared", &owner.1)?;
    job(&root, "first", &nominated, &owner.1, false)?;
    job(&root, "second", &nominated, &owner.1, true)?;
    let store = JobStore::new(root.clone());
    assert_eq!(store.prune(1001.0)?, 1);
    assert!(
        nominated.is_file(),
        "Another durable job still refers to this receipt"
    );
    assert!(unit_report::empty(&root.join("jobs/second.units"))?);
    store.record_exit("second", 0, 1002.0)?;
    assert_eq!(store.prune(1002.0)?, 1);
    assert!(!nominated.exists());
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn prune_retains_receipts_when_another_durable_owner_is_present_or_uncertain()
-> Result<(), Box<dyn std::error::Error>> {
    let mut owner = Owner::start()?;
    owner.finish()?;
    for mode in [
        "claim",
        "damaged-claim",
        "linked-claim",
        "damaged-job",
        "pending-report",
    ] {
        let temporary = common::TempDirectory::new("job-receipt-retain")?;
        let root = temporary.path().canonicalize()?;
        let nominated = receipt(&root, "browser-retained", &owner.1)?;
        job(&root, "complete", &nominated, &owner.1, false)?;
        match mode {
            "claim" => fs::write(
                root.join("sessions/profile.json"),
                serde_json::to_vec(&json!({"browserUnits":nominated}))?,
            )?,
            "damaged-claim" => fs::write(root.join("sessions/profile.json"), b"{")?,
            "linked-claim" => symlink(&nominated, root.join("sessions/profile.json"))?,
            "damaged-job" => fs::write(root.join("jobs/other.json"), b"{")?,
            "pending-report" => {
                let units = root.join("jobs/other.units");
                unit_report::initialize(&units)?;
                unit_report::begin_at(&units)?;
            }
            _ => unreachable!(),
        }
        assert_eq!(JobStore::new(root.clone()).prune(1001.0)?, 1, "{mode}");
        assert!(
            nominated.is_file(),
            "Uncertain/reference-bearing {mode} permitted receipt deletion"
        );
        temporary.cleanup()?;
    }
    Ok(())
}

#[test]
fn prune_never_retires_live_damaged_linked_or_foreign_browser_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    let live = Owner::start()?;
    let mut exited = Owner::start()?;
    exited.finish()?;
    for mode in ["live", "damaged", "linked", "foreign"] {
        let temporary = common::TempDirectory::new("job-receipt-refuse")?;
        let root = temporary.path().canonicalize()?;
        let tree = if mode == "live" { &live.1 } else { &exited.1 };
        let local = receipt(&root, "browser-local", tree)?;
        let nominated = match mode {
            "damaged" => {
                fs::write(&local, b"{\"version\":2,\"event\":\"header\"}\n{")?;
                local.clone()
            }
            "linked" => {
                let linked = root.join("sessions/browser-linked.units");
                symlink(&local, &linked)?;
                linked
            }
            "foreign" => receipt(&root.join("foreign"), "browser-foreign", &exited.1)?,
            _ => local.clone(),
        };
        job(&root, "retained", &nominated, &exited.1, false)?;
        assert_eq!(JobStore::new(root.clone()).prune(1001.0)?, 0, "{mode}");
        assert!(root.join("jobs/retained.json").is_file());
        assert!(nominated.exists(), "Unsafe {mode} receipt was deleted");
        temporary.cleanup()?;
    }
    Ok(())
}
