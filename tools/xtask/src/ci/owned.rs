//! The shipped finite CI command boundary. Kernel observations precede cleanup
//! and nested receipts remain owned until every reported unit settles.
use super::{Result, fail, memory};
use numera_core::ownership::{
    capture::{self, Observation},
    capture_units::Units,
    resources,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

pub struct Output {
    pub command: capture::Output,
    pub memory: Value,
}
pub fn run(root: &Path, command: &mut Command, limits: capture::Limits) -> Result<Output> {
    run_streamed(root, command, limits, |_, _| {})
}
pub fn run_streamed(
    root: &Path,
    command: &mut Command,
    limits: capture::Limits,
    mut on_chunk: impl FnMut(usize, &[u8]),
) -> Result<Output> {
    command.env_remove("NUMERA_UPDATE_ACTIVATION");
    let temporary = super::migration::Temporary::new(
        &super::root(),
        &std::env::vars().collect(),
        if cfg!(target_os = "linux") {
            "linux"
        } else {
            "win32"
        },
    )?;
    let child_temporary = super::owned_paths::temporary_environment_path(temporary.path())?;
    for key in ["TMPDIR", "TMP", "TEMP"] {
        command.env(key, &child_temporary);
    }
    let log = numera_core::runtime_log::RuntimeLog::new(&root.join(".ci-work/logs"), "ci-owned");
    log.event("INFO", "ci-owned", "phase=spawn");
    let cleanup = limits.cleanup;
    let mut units = Units::new(temporary.path(), command)?;
    let receipt = command.get_envs().find_map(|(key, value)| {
        (key == numera_core::ownership::unit_report::ENV)
            .then(|| value.map(PathBuf::from))
            .flatten()
    });
    let started = Instant::now();
    let mut recorder = None;
    let mut verdict = None;
    let mut observed = false;
    let mut diagnostic_failure = None;
    #[cfg(windows)]
    let mut accounting = None;
    let output = units.run_observed_streamed(
        command,
        limits,
        || false,
        |event, tree| match event {
            Observation::Spawned { record } => {
                log.event("INFO", "ci-owned", "phase=monitor");
                match memory::Owner::from_record(&record) {
                    Ok(owner) => {
                        let mut started_recorder =
                            memory::Recorder::new(owner, Default::default(), cfg!(windows));
                        // A short command may end before the first periodic
                        // callback. Attempt a real retained-owner observation
                        // at birth; an already ended proc is recorded missing.
                        if started_recorder
                            .sample_native(0.0, tree, receipt.as_deref())
                            .is_err()
                        {
                            diagnostic_failure =
                                Some("Native memory observation lost an owned handle");
                        }
                        recorder = Some(started_recorder);
                    }
                    Err(_) => {
                        diagnostic_failure = Some("Native command owner could not be measured")
                    }
                };
                #[cfg(windows)]
                {
                    match super::accounting::Native::new(tree) {
                        Ok(native) => {
                            accounting = Some(super::accounting::Scope::begin(0, native, 0))
                        }
                        Err(_) => eprintln!("Historical resources index=0 status=unavailable"),
                    }
                }
            }
            Observation::Polling => {
                if let Some(recorder) = recorder.as_mut()
                    && recorder
                        .sample_native(started.elapsed().as_secs_f64(), tree, receipt.as_deref())
                        .is_err()
                {
                    diagnostic_failure = Some("Native memory observation lost an owned handle");
                }
            }
            Observation::BeforeCleanupSample {
                leader_exited,
                natural_settlement,
                natural_reason,
                ..
            } => {
                observed = true;
                if resources::require_natural_settlement(leader_exited, natural_settlement).is_err() {
                    #[cfg(windows)]
                    retain_member_diagnostic(
                        root,
                        tree,
                        leader_exited,
                        natural_settlement,
                        natural_reason,
                    );
                }
                let _ = natural_reason;
                log.event("INFO", "ci-owned", "phase=initial-pre-cleanup-sample");
                #[cfg(windows)]
                if let Some(scope) = accounting.take() {
                    let (result, events) = scope.finish(
                        Ok(()),
                        u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX),
                    );
                    for event in events {
                        eprintln!("{event}");
                    }
                    if result.is_err() {
                        diagnostic_failure = Some("Historical resource query handle did not close");
                    }
                }
            }
            Observation::BeforeCleanup {
                leader_exited,
                natural_settlement,
                natural_reason,
                ..
            } => {
                if let Err(error) = resources::require_natural_settlement(leader_exited, natural_settlement) {
                    verdict = Some(error);
                }
                log.message("INFO", "ci-owned", &format!(
                    "phase=final-pre-cleanup-proof leader_exited={leader_exited} natural={natural_settlement:?} reason={natural_reason}"
                ));
                log.event("INFO", "ci-owned", "phase=settle-tree");
            }
        },
        |index, bytes| {
            crate::progress::completed(
                if index == 0 {
                    "owned-stdout"
                } else {
                    "owned-stderr"
                },
                bytes.len(),
            );
            super::compiler::observe_current(index, bytes);
            on_chunk(index, bytes);
        },
    );
    // Cleanup remains mandatory even when the original command failed or the
    // pre-cleanup observation already proves a leaked subprocess.
    let output: Result<capture::Output> = match output {
        Ok(output) => Ok(output),
        Err(mut error) => {
            let settled = error.settle(cleanup);
            match settled {
                Ok(()) => Err(error.into()),
                Err(cleanup) => Err(cleanup.into()),
            }
        }
    };
    let nested = units.settle(cleanup);
    if let Err(error) = nested {
        retain_units_diagnostic(root, &units);
        temporary.retain();
        return Err(error.into());
    }
    log.event("INFO", "ci-owned", "phase=reap-leader");
    log.event("INFO", "ci-owned", "phase=drain-output");
    log.event("INFO", "ci-owned", "phase=close-output");
    temporary.finish()?;
    log.event("INFO", "ci-owned", "phase=remove-temp");
    let output = output?;
    log.message("INFO", "ci-owned", &format!("exit={}", output.exit));
    if !observed {
        return fail("Native worker has no cleanup observation");
    }
    if let Some(error) = verdict {
        retain_verdict_diagnostics(root, &output);
        return Err(error.into());
    }
    if let Some(message) = diagnostic_failure {
        return fail(message);
    }
    let memory = recorder
        .map(|recorder| recorder.result())
        .unwrap_or(Value::Null);
    Ok(Output {
        command: output,
        memory,
    })
}
fn retain_verdict_diagnostics(root: &Path, output: &capture::Output) {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = root.join(format!(
        "build/owned-command-failure-{}-{stamp}-{}.log",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let reserved = fs::create_dir_all(root.join("build")).and_then(|_| {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
    });
    if reserved.is_err()
        || super::evidence::retain_test_diagnostics(&path, &output.stdout, &output.stderr).is_err()
    {
        // The command's original ownership refusal remains authoritative even
        // if the bounded diagnostic cannot be persisted.
        eprintln!("Owned command verdict transcript could not be retained");
    }
}
fn retain_units_diagnostic(root: &Path, units: &Units) {
    use std::{
        fs,
        io::Write,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // This is an opt-in projection from the real receipt parser. Unknown
    // record fields, argv, paths and arbitrary strings never enter the report.
    // A failed cleanup must not acquire a fresh lock-wait budget for logging.
    let diagnostic = units.diagnostic(Duration::ZERO).unwrap_or_else(
        |error| serde_json::json!({"available":false,"query_error_code":error.raw_os_error()}),
    );
    let evidence = serde_json::json!({"schema":1,"kind":"nested-receipt","diagnostic":diagnostic});
    let Ok(bytes) = serde_json::to_vec(&evidence) else {
        return;
    };
    if bytes.len() > 65536 {
        return;
    }
    eprintln!(
        "Owned nested receipt evidence: {}",
        String::from_utf8_lossy(&bytes)
    );
    // Reuse the retained member-evidence glob in every owning workflow.
    let path = root.join(format!(
        "build/owned-members-{}-units-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = fs::create_dir_all(root.join("build")).and_then(|_| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()
    });
    if written.is_ok() {
        eprintln!("Retained nested receipt evidence: {}", path.display());
    }
}
pub fn limits(wall: Duration, idle: Duration, bytes: usize) -> capture::Limits {
    capture::Limits {
        wall,
        idle,
        bytes,
        cleanup: Duration::from_secs(10),
    }
}

#[cfg(windows)]
fn retain_member_diagnostic(
    root: &Path,
    tree: &numera_core::ownership::tree::OwnedTree,
    leader_exited: bool,
    natural: Option<bool>,
    reason: &str,
) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    // The core API returns numeric identities/states and static executable
    // classes. It never returns argv, full paths or arbitrary error strings.
    let mut members = tree.job_member_diagnostics().unwrap_or_else(|error| {
        serde_json::json!({
            "complete":false,"query_error_code":error.raw_os_error()
        })
    });
    if let Some(rows) = members["members"].as_array_mut() {
        let omitted = rows.len().saturating_sub(32);
        rows.truncate(32);
        members["omitted_members"] = omitted.into();
    }
    let evidence = serde_json::json!({"schema":1,"phase":"initial-pre-cleanup-sample","leader_pid":tree.pid(),"leader_exited":leader_exited,"natural_settlement":natural,"natural_reason":reason,"job":members});
    let Ok(bytes) = serde_json::to_vec(&evidence) else {
        return;
    };
    if bytes.len() > 16 * 1024 {
        eprintln!("Owned Windows member evidence exceeded its diagnostic bound");
        return;
    }
    eprintln!(
        "Owned Windows settlement evidence: {}",
        String::from_utf8_lossy(&bytes)
    );
    let relative = format!(
        "build/owned-members-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let path = root.join(&relative);
    let retained = std::fs::create_dir_all(root.join("build")).and_then(|_| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(&bytes)
    });
    if retained.is_ok() {
        eprintln!("Retained bounded Windows member evidence: {relative}");
    } else {
        eprintln!(
            "Owned Windows member evidence could not be retained; original refusal remains fatal"
        );
    }
}
