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
    for key in ["TMPDIR", "TMP", "TEMP"] {
        command.env(key, temporary.path());
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
            Observation::BeforeCleanup {
                leader_exited,
                natural_settlement,
                ..
            } => {
                log.event("INFO", "ci-owned", "phase=settle-tree");
                observed = true;
                if let Err(error) =
                    resources::require_natural_settlement(leader_exited, natural_settlement)
                {
                    verdict = Some(error);
                }
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
pub fn limits(wall: Duration, idle: Duration, bytes: usize) -> capture::Limits {
    capture::Limits {
        wall,
        idle,
        bytes,
        cleanup: Duration::from_secs(10),
    }
}
