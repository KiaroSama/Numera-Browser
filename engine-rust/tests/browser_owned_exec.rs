#![cfg(all(target_os = "linux", feature = "native-browser"))]

use numera_core as core;
use numera_core::{
    browser::adapter::Adapter,
    ownership::{claims, identity, recorded_tree, unit_report},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
mod common;
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;

struct CapturedGroup(Value);
impl CapturedGroup {
    fn signal(&self, signal: &str) -> io::Result<()> {
        let pid = self.0["pid"]
            .as_u64()
            .ok_or_else(|| io::Error::other("Missing fixture PID"))? as u32;
        if identity::started_at(pid)? != self.0["pidToken"].as_u64() {
            return Ok(());
        }
        let status = Command::new("kill")
            .args([signal, "--", &format!("-{pid}")])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("Cannot signal owned fixture group"))
        }
    }
}
impl Drop for CapturedGroup {
    fn drop(&mut self) {
        let _ = self.signal("-KILL");
    }
}

fn await_leaf(root: &std::path::Path) -> io::Result<CapturedGroup> {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(pid) = fs::read_to_string(root.join("driver/leaf-running")) {
            let pid: u32 = pid.parse().map_err(io::Error::other)?;
            let birth = identity::started_at(pid)?
                .ok_or_else(|| io::Error::other("Fixture leaf exited before capture"))?;
            return Ok(CapturedGroup(
                json!({"pid":pid,"pidToken":birth,"tree":{"kind":"pgid","group":pid}}),
            ));
        }
        if Instant::now() >= end {
            return Err(io::Error::other(
                "Owned browser wrapper did not execute its leaf",
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn detached_browser_groups_remain_owned_through_stop_drop_and_damaged_receipts()
-> Result<(), Box<dyn std::error::Error>> {
    for mode in ["stop", "drop", "damaged", "closed"] {
        let temporary = common::TempDirectory::new("browser-owned-exec")?;
        let root = temporary.path();
        let mut command = fixture::worker(root, "playwright", "normal")?;
        let mut adapter = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        adapter.open("playwright", json!({
            "user_data_dir":root.join("profile"), "headless":true, "channel":"chromium",
            "env":{"NUMERA_JOB_ARGUMENT_KEY":"private-test-key", "NUMERA_PASSWORD":"private-test-password"}
        }).as_object().cloned().ok_or("Fixture options missing")?, Vec::new(), Duration::from_secs(5))?;
        let browser = await_leaf(root)?;
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(root.join("driver/leaf-controls.json"))?)?,
            json!([false, false, false, false, false, false, false, false])
        );
        let claims::Claim::Record(claim) = claims::read(root, "a") else {
            return Err("Session claim missing".into());
        };
        assert_ne!(claim["tree"]["group"], browser.0["tree"]["group"]);
        let path = std::path::Path::new(
            claim["browserUnits"]
                .as_str()
                .ok_or("Browser receipt missing")?,
        );
        let report = unit_report::read(path)?;
        assert!(report.complete);
        assert_eq!(report.trees, vec![browser.0.clone()]);
        assert!(!unit_report::empty(path)?);
        let worker = CapturedGroup(
            json!({"pid":claim["pid"],"pidToken":claim["token"],"tree":claim["tree"]}),
        );
        if mode == "closed" {
            adapter.call("close", Default::default(), Duration::from_secs(5))?;
            let end = Instant::now() + Duration::from_secs(5);
            while adapter.settled_exit()?.is_none() {
                if Instant::now() >= end {
                    return Err("Closed SDK worker did not exit".into());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let completed = adapter.finish(Duration::ZERO);
            assert!(
                matches!(claims::read(root, "a"), claims::Claim::Absent),
                "Zero-budget observation retained a proved-empty receipt"
            );
            // Closing the writer queue can race its thread's final return;
            // that unrelated join may still need a bounded completion wait.
            if completed.is_err() {
                adapter.finish(Duration::from_secs(5))?;
            }
        } else {
            browser.signal("-STOP")?;
            worker.signal("-STOP")?;
            if mode == "drop" {
                drop(adapter);
            } else if mode == "damaged" {
                let intact = fs::read(path)?;
                fs::OpenOptions::new()
                    .append(true)
                    .open(path)?
                    .write_all(b"{\"event\":")?;
                assert!(adapter.finish(Duration::from_secs(5)).is_err());
                assert!(!matches!(claims::read(root, "a"), claims::Claim::Absent));
                assert_ne!(
                    claims::state(&claims::read(root, "a"), "fixture-host"),
                    claims::State::Free
                );
                // Restore only this test's interrupted write after checking the
                // retained claim; normal product recovery never repairs it.
                fs::write(path, intact)?;
                adapter.finish(Duration::from_secs(5))?;
            } else {
                adapter.finish(Duration::from_secs(5))?;
            }
        }
        assert!(
            recorded_tree::empty(&browser.0)?,
            "A live browser escaped {mode}"
        );
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        let admission =
            numera_core::ownership::update_admission::Admission::new(root, "fixture-host")?;
        let ticket = admission.quiesce(None, || Ok(false))?;
        admission.resume(&ticket)?;
    }
    Ok(())
}

struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn an_unreaped_worker_cannot_admit_a_late_browser_receipt() -> Result<(), Box<dyn std::error::Error>>
{
    const CHILD: &str = "NUMERA_TEST_BROWSER_RECEIPT_OWNER";
    if std::env::var_os(CHILD).is_some() {
        io::stdin().read_to_end(&mut Vec::new())?;
        return Ok(());
    }
    let temporary = common::TempDirectory::new("browser-late-receipt")?;
    let directory = temporary.path().join("sessions");
    fs::create_dir(&directory)?;
    let path = directory.join("late.units");
    unit_report::initialize(&path)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "an_unreaped_worker_cannot_admit_a_late_browser_receipt",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = OwnedChild(command.spawn()?);
    let pid = child.0.id();
    let birth = identity::started_at(pid)?.ok_or("Owner identity missing")?;
    drop(child.0.stdin.take());
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
        if stat
            .rsplit_once(") ")
            .and_then(|(_, fields)| fields.split_whitespace().next())
            == Some("Z")
        {
            break;
        }
        if Instant::now() >= end {
            return Err("Fixture worker did not become an unreaped zombie".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        identity::alive(pid, Some(birth))?,
        "The fixture must preserve the former birth token"
    );
    let record = json!({"pid":pid,"token":birth,"tree":{"kind":"pgid","group":pid}});
    assert!(unit_report::record_browser_for_owner(&path, &record, pid, birth, pid).is_err());
    let report = unit_report::read(&path)?;
    assert!(report.complete);
    assert!(report.trees.is_empty());
    child.0.wait()?;
    Ok(())
}

#[test]
fn cross_root_capture_keeps_browser_identity_after_temporary_profile_cleanup()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::{capture, capture_units::Units};
    const OUTPUT: &str = "NUMERA_TEST_CROSS_ROOT_BROWSER_OUTPUT";
    if let Some(output) = std::env::var_os(OUTPUT) {
        let temporary = common::TempDirectory::new("isolated-captured-browser")?;
        let root = temporary.path();
        let mut command = fixture::worker(root, "playwright", "normal")?;
        let mut adapter = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        adapter.open(
            "playwright",
            json!({"user_data_dir":root.join("profile"), "headless":true,"channel":"chromium"})
                .as_object()
                .cloned()
                .ok_or("Fixture launch missing")?,
            Vec::new(),
            Duration::from_secs(5),
        )?;
        let browser = await_leaf(root)?;
        let identity = browser.0.clone();
        adapter.finish(Duration::from_secs(5))?;
        drop(adapter);
        drop(browser);
        temporary.cleanup()?;
        fs::write(output, serde_json::to_vec(&identity)?)?;
        return Ok(());
    }
    let temporary = common::TempDirectory::new("cross-root-browser-capture")?;
    let observed = temporary.path().join("browser.json");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            "cross_root_capture_keeps_browser_identity_after_temporary_profile_cleanup",
            "--nocapture",
        ])
        .env(OUTPUT, &observed);
    let mut units = Units::new(temporary.path(), &mut command)?;
    let receipt = command
        .get_envs()
        .find_map(|(key, value)| (key == unit_report::ENV).then_some(value).flatten())
        .map(std::path::PathBuf::from)
        .ok_or("Capture receipt missing")?;
    let scope = command
        .get_envs()
        .find_map(|(key, value)| (key == unit_report::SCOPE_ENV).then_some(value).flatten())
        .and_then(|scope| scope.to_str())
        .ok_or("Capture scope missing")?
        .to_owned();
    let mut report = None;
    let output = units.run_observed(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(60),
            idle: Duration::from_secs(60),
            bytes: 65_536,
            cleanup: Duration::from_secs(5),
        },
        || false,
        |event, _| {
            if matches!(event, capture::Observation::BeforeCleanup { .. }) {
                report = Some(unit_report::read_scoped(&receipt, Some(&scope)));
            }
        },
    )?;
    assert_eq!(
        output.exit,
        0,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = report.ok_or("Capture cleanup was not observed")??;
    assert!(report.complete);
    let browser: Value = serde_json::from_slice(&fs::read(observed)?)?;
    assert!(
        report.trees.contains(&browser),
        "Scoped capture lost its separately grouped browser"
    );
    assert!(
        report
            .trees
            .iter()
            .all(|tree| tree.get("browserUnits").is_none()),
        "Capture retained a link into removed temporary data"
    );
    assert!(!units.pending());
    Ok(())
}
