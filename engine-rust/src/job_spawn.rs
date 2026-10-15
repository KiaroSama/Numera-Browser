//! Detached startup ownership and child reaping for short and long-lived callers.
use crate::job_supervisor::{read, record_location, remove};
use crate::{
    jobs::{JobStore, child_argv},
    ownership::{identity, recorded_tree},
    storage::{atomic::write_json, paths::safe_id},
};
use serde_json::{Value, json};
use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const STARTUP: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);
const MAX_REQUEST: u64 = 1_048_576;
pub struct SpawnOptions<'a> {
    pub executable: &'a Path,
    pub argv: &'a [String],
    pub command: &'a str,
    pub profile: Option<&'a str>,
    pub password: Option<&'a str>,
}
struct Pending {
    child: Child,
    record: PathBuf,
    token: Option<u64>,
    admission: crate::ownership::update_admission::Reservation,
}
pub struct SpawnFailure {
    error: io::Error,
    pending: Option<Pending>,
}
impl std::fmt::Debug for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnFailure")
            .field("kind", &self.error.kind())
            .field("unsettled", &self.pending.is_some())
            .finish()
    }
}
impl std::fmt::Display for SpawnFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for SpawnFailure {}
impl From<io::Error> for SpawnFailure {
    fn from(error: io::Error) -> Self {
        Self {
            error,
            pending: None,
        }
    }
}
impl SpawnFailure {
    pub fn kind(&self) -> io::ErrorKind {
        self.error.kind()
    }
    pub(crate) fn into_io_error(self) -> io::Error {
        // Keep the actual child owner inside the propagated error; its Drop
        // retries cleanup and quarantines uncertainty rather than losing it.
        io::Error::new(self.kind(), self)
    }
    pub fn retained_record(&self) -> Option<Value> {
        self.pending.as_ref().map(|pending| {
            json!({
                "supervisorPid":pending.child.id(), "supervisorToken":pending.token,
                "record":pending.record,
            })
        })
    }
    pub fn settle(&mut self, budget: Duration) -> io::Result<()> {
        if let Some(pending) = self.pending.as_mut() {
            cancel(pending, budget)?;
            self.pending = None;
        }
        Ok(())
    }
}
fn retained() -> &'static Mutex<Vec<Pending>> {
    static RETAINED: OnceLock<Mutex<Vec<Pending>>> = OnceLock::new();
    RETAINED.get_or_init(|| Mutex::new(Vec::new()))
}
impl Drop for SpawnFailure {
    fn drop(&mut self) {
        if self.settle(Duration::from_secs(2)).is_err()
            && let Some(pending) = self.pending.take()
        {
            retained()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(pending);
            eprintln!("Detached startup cleanup remains owned; retry retained supervisors");
        }
    }
}
pub fn retry_retained(budget: Duration) -> Vec<String> {
    let mut pending = std::mem::take(
        &mut *retained()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let end = Instant::now()
        .checked_add(budget)
        .unwrap_or_else(Instant::now);
    let mut failed = Vec::new();
    pending.retain_mut(|owner| {
        match cancel(owner, end.saturating_duration_since(Instant::now())) {
            Ok(()) => false,
            Err(error) => {
                failed.push(error.to_string());
                true
            }
        }
    });
    retained()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .extend(pending);
    failed
}
fn now() -> io::Result<f64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_secs_f64())
}
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Native detached supervisor ownership is not implemented on this platform",
    )
}

fn cancel(pending: &mut Pending, budget: Duration) -> io::Result<()> {
    write_json(
        &pending.record.with_extension("cancel"),
        &json!({"cancel":true}),
    )?;
    let end = Instant::now()
        .checked_add(budget)
        .ok_or_else(|| io::Error::other("Invalid supervisor cleanup budget"))?;
    loop {
        if pending.child.try_wait()?.is_some() {
            if pending.record.exists() {
                let record = read(&pending.record)?;
                if (record["pid"].as_u64().is_some() && !recorded_tree::empty(&record)?)
                    || !crate::ownership::unit_report::settle(
                        &pending.record.with_extension("units"),
                        end.saturating_duration_since(Instant::now()),
                    )?
                {
                    return Err(io::Error::other(
                        "Detached command tree remains owned after supervisor exit",
                    ));
                }
            } else if !crate::ownership::unit_report::settle(
                &pending.record.with_extension("units"),
                end.saturating_duration_since(Instant::now()),
            )? {
                return Err(io::Error::other(
                    "Detached startup transfer remains pending",
                ));
            }
            pending.admission.release()?;
            return Ok(());
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Detached supervisor has not settled; retain its startup owner",
            ));
        }
        thread::sleep(end.saturating_duration_since(Instant::now()).min(POLL));
    }
}

fn completed_children() -> &'static Mutex<Vec<Child>> {
    static CHILDREN: OnceLock<Mutex<Vec<Child>>> = OnceLock::new();
    CHILDREN.get_or_init(|| Mutex::new(Vec::new()))
}

/// Nonblocking: durable jobs continue after MCP ends, but exited direct children
/// must be reaped while a long-lived caller is still waiting for its next input.
pub(crate) fn reap_completed() -> io::Result<()> {
    let mut failure = None;
    completed_children()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain_mut(|child| match child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) => true,
            Err(error) => {
                failure.get_or_insert(error);
                true
            }
        });
    failure.map_or(Ok(()), Err)
}

fn check_cancelled(cancelled: &mut impl FnMut() -> bool) -> io::Result<()> {
    if cancelled() {
        Err(io::Error::new(
            io::ErrorKind::ConnectionAborted,
            "Detached startup cancelled",
        ))
    } else {
        Ok(())
    }
}

impl JobStore {
    pub fn spawn_detached(&self, options: SpawnOptions<'_>) -> Result<Value, SpawnFailure> {
        self.spawn_detached_controlled(options, || false)
    }
    pub fn spawn_detached_controlled(
        &self,
        options: SpawnOptions<'_>,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Value, SpawnFailure> {
        reap_completed()?;
        check_cancelled(&mut cancelled)?;
        if !cfg!(any(windows, target_os = "linux")) {
            return Err(unsupported().into());
        }
        let executable = options.executable.canonicalize()?;
        let root = self.root.canonicalize()?;
        let admission =
            crate::ownership::update_admission::Reservation::begin(&root, "native-job-start")?;
        if let Some(profile) = options.profile {
            safe_id(profile).map_err(io::Error::other)?;
        }
        let mut nonce = [0u8; 6];
        getrandom::fill(&mut nonce)
            .map_err(|_| io::Error::other("Job identity generation failed"))?;
        let id = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let record = self.path(&id)?;
        let (_, _, record) = record_location(&record)?;
        if [
            "json", "launch", "ready", "cancel", "exit", "log", "units", "go", "admitted",
        ]
        .iter()
        .any(|extension| record.with_extension(extension).exists())
        {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "Job identity already exists",
            )
            .into());
        }
        let argv = child_argv(options.argv, &root);
        let request = json!({"id":id,"root":root,"executable":executable,"argv":argv,
            "command":options.command,"profile":options.profile,"started":now()?});
        if serde_json::to_vec(&request)
            .map_err(io::Error::other)?
            .len() as u64
            > MAX_REQUEST
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Job request exceeds its size limit",
            )
            .into());
        }
        write_json(&record.with_extension("launch"), &request)?;
        crate::ownership::unit_report::initialize(&record.with_extension("units"))?;
        write_json(
            &record,
            &json!({"id":id,"argv":argv,"command":options.command,
            "profile":options.profile,"log":record.with_extension("log"),"started":request["started"],
            "pid":Value::Null,"units":record.with_extension("units")}),
        )?;
        let parent_pid = std::process::id();
        let parent_token = identity::started_at(parent_pid)?
            .ok_or_else(|| io::Error::other("Detached parent has no birth identity"))?;
        let mut command = Command::new(&executable);
        command
            .env("NUMERA_NATIVE_SUPERVISE", &record)
            .env("NUMERA_JOB_PARENT_PID", parent_pid.to_string())
            .env("NUMERA_JOB_PARENT_TOKEN", parent_token.to_string())
            .env_remove("NUMERA_HELPER_UNITS")
            .env_remove("NUMERA_NATIVE_JOB_GATE")
            .env_remove("NUMERA_JOB_FILE")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(password) = options.password.filter(|value| !value.is_empty()) {
            command.env("NUMERA_PASSWORD", password);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Breakaway denial is an explicit refusal, never an unowned fallback.
            command.creation_flags(0x0100_0000 | 0x0000_0008);
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        if let Err(error) = check_cancelled(&mut cancelled) {
            remove(&record.with_extension("launch"))?;
            remove(&record.with_extension("units"))?;
            remove(&record)?;
            return Err(error.into());
        }
        let startup_lock = crate::storage::locks::DataLock::transaction(&root)?;
        let startup_guard = startup_lock.acquire(STARTUP)?;
        if let Err(error) = crate::ownership::update_admission::assert_open_locked(&root) {
            remove(&record.with_extension("launch"))?;
            remove(&record.with_extension("units"))?;
            remove(&record)?;
            return Err(error.into());
        }
        let child = match command.spawn() {
            Ok(child) => child,
            Err(_) => {
                remove(&record.with_extension("launch"))?;
                remove(&record.with_extension("units"))?;
                remove(&record)?;
                return Err(io::Error::other(
                    "Detached supervisor could not start; the host may deny job breakaway",
                )
                .into());
            }
        };
        drop(startup_guard);
        let token = identity::started_at(child.id()).ok().flatten();
        let mut pending = Pending {
            child,
            record,
            token,
            admission,
        };
        let result = (|| {
            if pending.token.is_none() {
                return Err(io::Error::other(
                    "Detached supervisor has no captured birth identity",
                ));
            }
            let end = Instant::now() + STARTUP;
            loop {
                check_cancelled(&mut cancelled)?;
                if pending.record.with_extension("ready").exists() {
                    let ready = read(&pending.record.with_extension("ready"))?;
                    if ready["supervisorPid"].as_u64() != Some(u64::from(pending.child.id()))
                        || ready["supervisorToken"].as_u64() != pending.token
                    {
                        return Err(io::Error::other(
                            "Detached readiness does not match its supervisor",
                        ));
                    }
                    let value = self
                        .get(&id, now()?)?
                        .ok_or_else(|| io::Error::other("Detached job identity missing"))?;
                    if !recorded_tree::verified(&value) {
                        return Err(io::Error::other(
                            "Detached command has no verified tree identity",
                        ));
                    }
                    check_cancelled(&mut cancelled)?;
                    write_json(
                        &pending.record.with_extension("go"),
                        &json!({
                        "parentPid":parent_pid,"parentToken":parent_token}),
                    )?;
                    remove(&pending.record.with_extension("ready"))?;
                    check_cancelled(&mut cancelled)?;
                    return Ok(value);
                }
                if pending.child.try_wait()?.is_some() {
                    return Err(io::Error::other(
                        "Detached supervisor exited before publishing readiness",
                    ));
                }
                if Instant::now() >= end {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "Detached supervisor readiness exceeded ten seconds",
                    ));
                }
                thread::sleep(POLL);
            }
        })();
        match result {
            Ok(value) => {
                // The durable verified job record now owns the full lifetime.
                if pending.admission.release().is_err() {
                    pending.admission.retain();
                }
                completed_children()
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(pending.child);
                Ok(value)
            }
            Err(error) => {
                let mut failure = SpawnFailure {
                    error,
                    pending: Some(pending),
                };
                let _ = failure.settle(STARTUP);
                Err(failure)
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    #[allow(unsafe_code)]
    fn cancelled_handoff_settles_actual_supervisor_and_command()
    -> Result<(), Box<dyn std::error::Error>> {
        let executable = std::env::var_os("NUMERA_NATIVE_CANDIDATE")
            .ok_or("Hosted native candidate fixture required")?;
        let executable = PathBuf::from(executable).canonicalize()?;
        // An unresolved process/assertion keeps its exact receipt directory;
        // the normal TempDirectory unwind cleanup must not erase its owner.
        let temporary =
            std::mem::ManuallyDrop::new(crate::common::TempDirectory::new("detached handoff")?);
        let root = temporary.path().canonicalize()?;
        let store = JobStore::new(root.clone());
        let argv = vec![
            "list".to_owned(),
            "--detach".to_owned(),
            "--json".to_owned(),
        ];
        let mut command = None;
        let mut probe_failure = None;
        let mut handoff = false;
        let result = store.spawn_detached_controlled(
            SpawnOptions {
                executable: &executable,
                argv: &argv,
                command: "list",
                profile: None,
                password: None,
            },
            || {
                let probe = (|| -> io::Result<bool> {
                    let directory = root.join("jobs");
                    if !directory.exists() {
                        return Ok(false);
                    }
                    for entry in std::fs::read_dir(&directory)? {
                        let record = entry?.path();
                        if record
                            .extension()
                            .is_none_or(|extension| extension != "json")
                        {
                            continue;
                        }
                        if command.is_none() && record.with_extension("ready").exists() {
                            let owner = read(&record)?;
                            let pid = owner["pid"]
                                .as_u64()
                                .and_then(|value| i32::try_from(value).ok())
                                .filter(|pid| *pid > 0)
                                .ok_or_else(|| io::Error::other("Gated fixture PID missing"))?;
                            let token = Some(owner["pidToken"].as_u64().ok_or_else(|| {
                                io::Error::other("Gated fixture birth token missing")
                            })?);
                            if !identity::alive(pid as u32, token)? {
                                return Err(io::Error::other("Gated fixture identity changed"));
                            }
                            // SAFETY: the private readiness record names the gated,
                            // birth-verified child; no user command has been admitted.
                            if unsafe { libc::kill(pid, libc::SIGSTOP) } != 0 {
                                return Err(io::Error::last_os_error());
                            }
                            command = Some((pid, token));
                            let stopped_by = Instant::now() + Duration::from_secs(1);
                            loop {
                                let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
                                if stat
                                    .rsplit_once(") ")
                                    .is_some_and(|(_, fields)| fields.starts_with('T'))
                                {
                                    break;
                                }
                                if Instant::now() >= stopped_by {
                                    return Err(io::Error::new(
                                        io::ErrorKind::TimedOut,
                                        "Gated fixture did not stop",
                                    ));
                                }
                                thread::sleep(Duration::from_millis(5));
                            }
                        }
                        if record.with_extension("go").exists() {
                            let (pid, token) = command
                                .ok_or_else(|| io::Error::other("Handoff fixture not gated"))?;
                            if identity::alive(pid as u32, token)? {
                                // SAFETY: same captured fixture identity, never a guessed PID.
                                if unsafe { libc::kill(pid, libc::SIGCONT) } != 0 {
                                    return Err(io::Error::last_os_error());
                                }
                            }
                            handoff = true;
                            return Ok(true);
                        }
                    }
                    Ok(false)
                })();
                match probe {
                    Ok(cancelled) => cancelled,
                    Err(error) => {
                        probe_failure = Some(error);
                        true
                    }
                }
            },
        );
        // Resume on an assertion/probe failure too, so cleanup cannot be held
        // hostage by the fixture's own stop signal.
        if let Some((pid, token)) = command
            && identity::alive(pid as u32, token)?
        {
            // SAFETY: retained birth-verified fixture command, as above.
            if unsafe { libc::kill(pid, libc::SIGCONT) } != 0 {
                return Err(io::Error::last_os_error().into());
            }
        }
        let mut failure = match result {
            Ok(record) => {
                if let Some(id) = record["id"].as_str() {
                    store.stop(id, "fixture", now()?)?;
                }
                let supervisor = u32::try_from(
                    record["supervisorPid"]
                        .as_u64()
                        .ok_or("Unexpected successful supervisor PID missing")?,
                )?;
                let end = Instant::now() + STARTUP;
                while identity::alive(supervisor, record["supervisorToken"].as_u64())? {
                    reap_completed()?;
                    if Instant::now() >= end {
                        return Err("Unexpected successful handoff did not settle".into());
                    }
                    thread::sleep(POLL);
                }
                return Err("Cancellation after .go publication returned a successful job".into());
            }
            Err(failure) => failure,
        };
        failure.settle(STARTUP)?;
        if let Some(error) = probe_failure {
            return Err(error.into());
        }
        assert!(handoff, "Cancellation did not reach actual .go handoff");
        assert_eq!(failure.kind(), io::ErrorKind::ConnectionAborted);
        assert!(
            failure.retained_record().is_none(),
            "Cancelled handoff retained live ownership"
        );
        let jobs = store.listing(now()?)?;
        assert_eq!(jobs.len(), 1);
        let record = &jobs[0];
        assert!(recorded_tree::empty(record)?);
        let report = crate::ownership::unit_report::read(&PathBuf::from(
            record["units"]
                .as_str()
                .ok_or("Cancelled receipt missing")?,
        ))?;
        assert!(report.complete);
        for unit in report.trees {
            assert!(recorded_tree::empty(&unit)?);
        }
        let supervisor_pid = record["supervisorPid"]
            .as_u64()
            .ok_or("Supervisor PID missing")?;
        assert!(!identity::alive(
            u32::try_from(supervisor_pid)?,
            record["supervisorToken"].as_u64()
        )?);
        assert!(
            !PathBuf::from(format!("/proc/{supervisor_pid}")).exists(),
            "Cancelled supervisor was not reaped"
        );
        std::mem::ManuallyDrop::into_inner(temporary).cleanup()?;
        Ok(())
    }
}
