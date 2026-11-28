#![cfg(unix)]

use numera_core::ownership::{identity, update_admission::Admission};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read},
    os::unix::{fs::symlink, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};
mod common;

const CHILD: &str = "NUMERA_TEST_UPDATE_RECEIPT_CHILD";
const HEADER: &str = "{\"version\":2,\"event\":\"header\"}\n";

struct Browser(Child, Value);
impl Browser {
    fn start() -> Result<Self, Box<dyn std::error::Error>> {
        let child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "updates_accept_completed_browser_receipts_and_managed_lock_directories",
            ])
            .env(CHILD, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?;
        let mut browser = Self(child, Value::Null);
        let pid = browser.0.id();
        let birth = identity::started_at(pid)?.ok_or("Fixture browser birth identity missing")?;
        browser.1 = json!({"event":"browser-unit","attempt":"fixture","pid":pid,
            "pidToken":birth,"tree":{"kind":"pgid","group":pid}});
        Ok(browser)
    }
    fn finish(&mut self) -> io::Result<()> {
        self.0.kill()?;
        self.0.wait()?;
        Ok(())
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn receipt(root: &Path, tree: Option<&Value>) -> io::Result<PathBuf> {
    let directory = root.join("sessions");
    fs::create_dir_all(&directory)?;
    let path = directory.join("browser-fixture.units");
    let mut content = HEADER.to_owned();
    if let Some(tree) = tree {
        content.push_str(&serde_json::to_string(tree)?);
        content.push('\n');
    }
    fs::write(&path, content)?;
    Ok(path)
}

#[test]
fn updates_accept_completed_browser_receipts_and_managed_lock_directories()
-> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os(CHILD).is_some() {
        io::stdin().read_to_end(&mut Vec::new())?;
        return Ok(());
    }
    let mut browser = Browser::start()?;
    browser.finish()?;
    for mode in [
        "session-locks",
        "job-locks",
        "empty-receipt",
        "completed-browser",
    ] {
        let temporary = common::TempDirectory::new("update-completed-receipt")?;
        let root = temporary.path();
        match mode {
            "session-locks" => fs::create_dir_all(root.join("sessions/.locks"))?,
            "job-locks" => fs::create_dir_all(root.join("jobs/.locks"))?,
            "empty-receipt" => {
                receipt(root, None)?;
            }
            "completed-browser" => {
                let path = receipt(root, Some(&browser.1))?;
                fs::create_dir_all(root.join("sessions/.locks"))?;
                fs::write(
                    root.join("sessions/old-profile.json"),
                    serde_json::to_vec(&json!({
                        "pid":browser.1["pid"],"token":browser.1["pidToken"],"tree":browser.1["tree"],
                        "session":"fixture-session","host":"fixture-host","browserUnits":path
                    }))?,
                )?;
            }
            _ => unreachable!(),
        }
        let admission = Admission::new(root, "fixture-host")?;
        let ticket = admission.quiesce(None, || Ok(false)).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("Settled {mode} blocked update: {error}"),
            )
        })?;
        admission.resume(&ticket)?;
    }
    Ok(())
}

#[test]
fn updates_retain_refusal_for_live_pending_damaged_linked_and_unknown_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    let browser = Browser::start()?;
    for mode in [
        "live",
        "pending",
        "damaged",
        "oversized",
        "linked-receipt",
        "linked-locks",
        "lock-file",
        "unknown-file",
        "unknown-directory",
    ] {
        let temporary = common::TempDirectory::new("update-uncertain-receipt")?;
        let root = temporary.path();
        let path = receipt(root, None)?;
        match mode {
            "live" => {
                receipt(root, Some(&browser.1))?;
            }
            "pending" => fs::write(
                &path,
                format!("{HEADER}{{\"event\":\"pending\",\"attempt\":\"not-transferred\"}}\n"),
            )?,
            "damaged" => fs::write(&path, format!("{HEADER}{{"))?,
            "oversized" => fs::write(&path, vec![b' '; 1_048_577])?,
            "linked-receipt" => {
                let outside = root.join("outside.units");
                fs::rename(&path, &outside)?;
                symlink(outside, &path)?;
            }
            "linked-locks" => {
                let outside = root.join("outside-locks");
                fs::create_dir(&outside)?;
                symlink(outside, root.join("sessions/.locks"))?;
            }
            "lock-file" => fs::write(root.join("sessions/.locks"), b"not a directory")?,
            "unknown-file" => fs::write(root.join("sessions/unknown"), b"{}")?,
            "unknown-directory" => fs::create_dir(root.join("sessions/unknown"))?,
            _ => unreachable!(),
        }
        let admission = Admission::new(root, "fixture-host")?;
        assert!(
            admission.quiesce(None, || Ok(false)).is_err(),
            "Unsafe {mode} permitted update"
        );
        assert!(
            !admission.gate().exists(),
            "Refused {mode} left a new update gate"
        );
        assert!(path.exists(), "Refused {mode} removed receipt evidence");
    }
    Ok(())
}

#[test]
fn updates_refuse_stale_claims_that_reference_foreign_browser_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    let mut browser = Browser::start()?;
    browser.finish()?;
    let temporary = common::TempDirectory::new("update-foreign-browser-claim")?;
    let foreign = common::TempDirectory::new("foreign-browser-receipt")?;
    let root = temporary.path();
    let foreign_receipt = receipt(foreign.path(), None)?;
    fs::create_dir_all(root.join("sessions"))?;
    let path = root.join("sessions/old-profile.json");
    let bytes = serde_json::to_vec(&json!({
        "pid":browser.1["pid"],"token":browser.1["pidToken"],"tree":browser.1["tree"],
        "session":"fixture-session","host":"fixture-host","browserUnits":foreign_receipt
    }))?;
    fs::write(&path, &bytes)?;
    let admission = Admission::new(root, "fixture-host")?;
    assert!(
        admission.quiesce(None, || Ok(false)).is_err(),
        "A foreign browser receipt made an unverifiable local claim idle"
    );
    assert!(!admission.gate().exists());
    assert_eq!(fs::read(&path)?, bytes);
    assert!(foreign_receipt.exists());
    Ok(())
}
