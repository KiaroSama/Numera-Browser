#![cfg(target_os = "linux")]

use numera_core::{
    browser::adapter::Adapter,
    ownership::{claims, identity, recorded_tree},
};
use serde_json::{Map, json};
use std::{fs, io, path::Path, process::Command, time::Duration};

use super::common;

struct BrowserProcess {
    pid: u32,
    token: u64,
    group: u32,
}
impl BrowserProcess {
    fn find(profile: &Path) -> io::Result<Self> {
        let argument = format!("--user-data-dir={}", profile.display());
        for entry in fs::read_dir("/proc")? {
            let entry = entry?;
            let Some(pid) = entry.file_name().to_str().and_then(|s| s.parse().ok()) else {
                continue;
            };
            let Ok(cmdline) = fs::read(entry.path().join("cmdline")) else {
                continue;
            };
            let fields = cmdline.split(|byte| *byte == 0).collect::<Vec<_>>();
            if !fields.contains(&argument.as_bytes())
                || fields.iter().any(|arg| arg.starts_with(b"--type="))
            {
                continue;
            }
            let token = identity::started_at(pid)?
                .ok_or_else(|| io::Error::other("Test browser birth identity missing"))?;
            let stat = fs::read_to_string(entry.path().join("stat"))?;
            let (_, fields) = stat
                .rsplit_once(") ")
                .ok_or_else(|| io::Error::other("Test browser stat is malformed"))?;
            let group = fields
                .split_whitespace()
                .nth(2)
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| io::Error::other("Test browser process group missing"))?;
            return Ok(Self { pid, token, group });
        }
        Err(io::Error::other("Own launched test browser was not found"))
    }
    fn signal(&self, signal: &str, group: bool) -> io::Result<()> {
        if identity::started_at(self.pid)? != Some(self.token) {
            return Ok(());
        }
        let target = if group {
            format!("-{}", self.group)
        } else {
            self.pid.to_string()
        };
        let status = Command::new("kill")
            .args([signal, "--", &target])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other("Cannot signal own test browser"))
        }
    }
}
impl Drop for BrowserProcess {
    fn drop(&mut self) {
        // The group was captured from this test's exclusive profile argument.
        // A birth-token check prevents signaling a recycled process identity.
        let _ = self.signal("-KILL", true);
    }
}

pub fn forced_shutdown_does_not_release_claim_while_browser_survives(
    backend: &str,
    runtime: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    assert!(matches!(backend, "playwright" | "patchright"));
    let root = common::TempDirectory::new("browser-owned-stop")?;
    let profile = root.path().join("exclusive-browser-profile");
    let mut command = Command::new(runtime);
    let mut adapter = Adapter::spawn(
        root.path(),
        "owned",
        "fixture-host",
        &mut command,
        Duration::from_secs(10),
    )?;
    adapter.open(
        backend,
        json!({
            "user_data_dir":profile,
            "headless":true,
            "channel":"chromium",
            "args":["--no-sandbox"],
        })
        .as_object()
        .cloned()
        .unwrap_or_else(Map::new),
        Vec::new(),
        Duration::from_secs(45),
    )?;
    let browser = BrowserProcess::find(&profile)?;
    let claim = claims::read(root.path(), "owned");
    let claims::Claim::Record(record) = claim else {
        return Err("Live browser claim missing".into());
    };
    eprintln!(
        "Owned worker group={}, actual browser pid={}, group={}",
        record["tree"]["group"], browser.pid, browser.group
    );
    browser.signal("-STOP", false)?;
    // Neither the browser nor its SDK may cooperate with forced settlement.
    let worker_group = record["tree"]["group"]
        .as_u64()
        .ok_or("Native worker group missing")?;
    assert!(
        Command::new("kill")
            .args(["-STOP", "--", &format!("-{worker_group}")])
            .status()?
            .success()
    );
    let shutdown = adapter.finish(Duration::from_secs(5));
    let survives = !recorded_tree::empty(&json!({
        "pid":browser.pid,"pidToken":browser.token,
        "tree":{"kind":"pgid","group":browser.group}
    }))?;
    let state = claims::state(&claims::read(root.path(), "owned"), "fixture-host");
    eprintln!("Forced shutdown={shutdown:?}, browser survives={survives}, claim={state:?}");
    shutdown?;
    assert!(!survives, "A live browser escaped native forced settlement");
    assert_eq!(state, claims::State::Free);
    browser.signal("-CONT", false)?;
    adapter.finish(Duration::from_secs(10))?;
    Ok(())
}
