//! Detached command ownership stays in a supervisor, never in an exited CLI.
pub use crate::job_spawn::{SpawnFailure, SpawnOptions, retry_retained};
use crate::{
    jobs::{JobStore, child_argv},
    native_entry::now,
    ownership::{identity, recorded_tree, tree::OwnedTree},
    runtime_log::RuntimeLog,
    storage::{atomic::write_json, locks::DataLock, paths::safe_id},
};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const STARTUP: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);
const MAX_REQUEST: u64 = 1_048_576;

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "Native detached supervisor ownership is not implemented on this platform",
    )
}

pub(crate) fn read(path: &Path) -> io::Result<Value> {
    let mut file = fs::File::open(path)?;
    if file.metadata()?.len() > MAX_REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Job request exceeds its size limit",
        ));
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_REQUEST + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_REQUEST {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Job request exceeds its size limit",
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Job request is not valid JSON"))
}
fn regular(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_symlink() || !metadata.is_file() => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Job artifact is not a regular file",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
pub(crate) fn record_location(path: &Path) -> io::Result<(JobStore, String, PathBuf)> {
    if path.extension().is_none_or(|extension| extension != "json") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Supervisor record must be a job JSON file",
        ));
    }
    let id = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Job ID is not UTF-8"))?;
    safe_id(id).map_err(io::Error::other)?;
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("Job directory missing"))?;
    if directory.file_name().is_none_or(|name| name != "jobs")
        || fs::symlink_metadata(directory)?.is_symlink()
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Supervisor record is outside its jobs directory",
        ));
    }
    let root = directory
        .parent()
        .ok_or_else(|| io::Error::other("Job root missing"))?
        .canonicalize()?;
    let store = JobStore::new(root);
    let record = store.path(id)?;
    if directory.canonicalize()?
        != record
            .parent()
            .ok_or_else(|| io::Error::other("Job directory missing"))?
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Supervisor job directory is linked outside its root",
        ));
    }
    for extension in [
        "json", "launch", "ready", "cancel", "exit", "log", "units", "go", "admitted",
    ] {
        regular(&record.with_extension(extension))?;
    }
    Ok((store, id.into(), record))
}
pub(crate) fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
/// Native command children wait here before parsing or application side effects.
pub fn await_gate(path: &Path) -> io::Result<()> {
    let (_, _, record) = record_location(path)?;
    let end = Instant::now() + STARTUP;
    loop {
        if record.with_extension("cancel").exists() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Detached startup was cancelled",
            ));
        }
        if record.with_extension("go").exists() {
            let owner = read(&record)?;
            let token = identity::started_at(std::process::id())?
                .ok_or_else(|| io::Error::other("Gated command birth identity missing"))?;
            if owner["pid"].as_u64() != Some(u64::from(std::process::id()))
                || owner["pidToken"].as_u64() != Some(token)
                || !recorded_tree::verified(&owner)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Detached gate belongs to another command",
                ));
            }
            let units = crate::ownership::unit_report::validate(&record.with_extension("units"))?;
            if std::env::var_os("NUMERA_HELPER_UNITS")
                .map(PathBuf::from)
                .and_then(|path| path.canonicalize().ok())
                .as_ref()
                != Some(&units)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Detached ownership report does not match its gate",
                ));
            }
            write_json(
                &record.with_extension("admitted"),
                &json!({"admitted":true}),
            )?;
            return Ok(());
        }
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Detached command gate exceeded ten seconds",
            ));
        }
        thread::sleep(POLL);
    }
}

/// Internal entrypoint is selected before CLI parsing; it accepts no shell command.
pub fn run_internal(path: &Path) -> io::Result<i32> {
    if !cfg!(any(windows, target_os = "linux")) {
        return Err(unsupported());
    }
    let (store, id, record) = record_location(path)?;
    let mut admission =
        crate::ownership::update_admission::Reservation::begin(&store.root, "native-job")?;
    let log = RuntimeLog::new(&store.root.join("logs"), "native-job-supervisor");
    log.event("INFO", "supervisor", "Startup requested");
    let request = read(&record.with_extension("launch"))?;
    if request["id"] != id || Path::new(request["root"].as_str().unwrap_or("")) != store.root {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Supervisor request does not match its root and job",
        ));
    }
    let executable = PathBuf::from(
        request["executable"]
            .as_str()
            .ok_or_else(|| io::Error::other("Job executable missing"))?,
    );
    if executable.canonicalize()? != std::env::current_exe()?.canonicalize()? {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Supervisor executable does not match the native entrypoint",
        ));
    }
    let argv = request["argv"]
        .as_array()
        .ok_or_else(|| io::Error::other("Job arguments missing"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(String::from)
                .ok_or_else(|| io::Error::other("Job argument is not text"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    if child_argv(&argv, &store.root) != argv {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Job arguments contain detach or password options",
        ));
    }
    let mut pinned = None;
    for (index, argument) in argv.iter().enumerate() {
        if argument == "--data-dir" {
            pinned = argv.get(index + 1).map(String::as_str);
        } else if let Some(value) = argument.strip_prefix("--data-dir=") {
            pinned = Some(value);
        }
    }
    if pinned.is_none_or(|value| Path::new(value).canonicalize().ok().as_ref() != Some(&store.root))
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Detached command data root does not match its supervisor",
        ));
    }
    if request["command"].as_str().is_none()
        || !request["started"].as_f64().is_some_and(f64::is_finite)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Job metadata is invalid",
        ));
    }
    if let Some(profile) = request["profile"].as_str() {
        safe_id(profile).map_err(io::Error::other)?;
    } else if !request["profile"].is_null() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Job profile identity is invalid",
        ));
    }
    if record.with_extension("cancel").exists() {
        return Ok(1);
    }
    let output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(record.with_extension("log"))?;
    let mut command = Command::new(executable);
    command
        .args(&argv)
        .env_remove("NUMERA_NATIVE_SUPERVISE")
        .env_remove("NUMERA_JOB_FILE")
        .stdin(Stdio::null())
        .stderr(Stdio::from(output.try_clone()?))
        .stdout(Stdio::from(output));
    command
        .env("NUMERA_NATIVE_JOB_GATE", &record)
        .env("NUMERA_HELPER_UNITS", record.with_extension("units"))
        .env_remove("NUMERA_JOB_PARENT_PID")
        .env_remove("NUMERA_JOB_PARENT_TOKEN");
    let units = record.with_extension("units");
    let attempt = crate::ownership::unit_report::begin_at(&units)?;
    let startup_lock = DataLock::transaction(&store.root)?;
    let startup_guard = startup_lock.acquire(STARTUP)?;
    crate::ownership::update_admission::assert_open_locked(&store.root)?;
    let mut tree = match OwnedTree::spawn(&mut command) {
        Ok(tree) => tree,
        Err(error) => {
            admission.retain();
            return Err(error);
        }
    };
    drop(startup_guard);
    let result = (|| -> io::Result<i32> {
        let actual = tree.record();
        crate::ownership::unit_report::record_at(&units, &actual, &attempt)?;
        let supervisor_pid = std::process::id();
        let supervisor_token = identity::started_at(supervisor_pid)?
            .ok_or_else(|| io::Error::other("Supervisor birth identity missing"))?;
        let identity = json!({"id":id,"argv":argv,"command":request["command"],"profile":request["profile"],
        "pid":actual["pid"],"pidToken":actual["token"],"tree":actual["tree"],"verified":true,
        "log":record.with_extension("log"),"started":request["started"],"units":units,
        "supervisorPid":supervisor_pid,"supervisorToken":supervisor_token});
        write_json(&record, &identity)?;
        remove(&record.with_extension("launch"))?;
        let parent_pid = std::env::var("NUMERA_JOB_PARENT_PID")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|pid| *pid > 0)
            .ok_or_else(|| io::Error::other("Detached parent identity missing"))?;
        let parent_token = std::env::var("NUMERA_JOB_PARENT_TOKEN")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| io::Error::other("Detached parent birth token missing"))?;
        if record.with_extension("cancel").exists() {
            tree.settle(STARTUP)?;
        } else {
            write_json(
                &record.with_extension("ready"),
                &json!({
            "supervisorPid":supervisor_pid,"supervisorToken":supervisor_token}),
            )?;
            log.event("INFO", "supervisor", "Command ownership published");
        }
        let handoff_deadline = Instant::now() + STARTUP;
        while !record.with_extension("admitted").exists() {
            if record.with_extension("cancel").exists()
                || (!record.with_extension("go").exists()
                    && !identity::alive(parent_pid, Some(parent_token))?)
                || Instant::now() >= handoff_deadline
            {
                tree.settle(STARTUP)?;
                return Err(io::Error::other(
                    "Detached parent did not acknowledge ownership",
                ));
            }
            thread::sleep(POLL);
        }
        remove(&record.with_extension("go"))?;
        remove(&record.with_extension("admitted"))?;
        let code = loop {
            if record.with_extension("cancel").exists() {
                tree.settle(STARTUP)?;
                break 1;
            }
            if let Some(status) = tree.child_mut().try_wait()? {
                tree.settle(STARTUP)?;
                break status.code().unwrap_or(-1);
            }
            thread::sleep(POLL);
        };
        if !crate::ownership::unit_report::settle(&units, STARTUP)? {
            return Err(io::Error::other(
                "Detached nested ownership transfers remain incomplete",
            ));
        }
        let lock = DataLock::for_file(&record)?;
        let _guard = lock.acquire(STARTUP)?;
        let current = store.record(&id)?.ok_or_else(|| {
            io::Error::other("Job identity disappeared before outcome publication")
        })?;
        if current.get("state").and_then(Value::as_str) != Some("stopped") {
            store.record_exit(&id, code, now()?)?;
        }
        log.event(
            "INFO",
            "supervisor",
            "Command tree settled and outcome recorded",
        );
        remove(&record.with_extension("cancel"))?;
        Ok(code)
    })();
    if result.is_err() {
        if tree.settle(STARTUP).is_err() {
            admission.retain();
            log.event(
                "ERROR",
                "supervisor",
                "Command cleanup remains owned in quarantine",
            );
            crate::ownership::capture::retain_tree(tree);
            return result;
        }
        if !crate::ownership::unit_report::settle(&units, STARTUP).unwrap_or(false) {
            admission.retain();
        }
        log.event(
            "ERROR",
            "supervisor",
            "Operation failed after owned cleanup",
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(any(windows, target_os = "linux")))]
    #[test]
    fn unsupported_detach_refuses_before_files_or_processes() {
        let store = JobStore::new(PathBuf::from("never-created-native-detach-root"));
        let result = store.spawn_detached(SpawnOptions {
            executable: Path::new("never-start-native-command"),
            argv: &[],
            command: "launch",
            profile: None,
            password: None,
        });
        assert_eq!(
            result.expect_err("Unsupported detach started").kind(),
            io::ErrorKind::Unsupported
        );
    }

    #[test]
    fn internal_record_refuses_wrong_extension_before_files_or_processes() {
        assert_eq!(
            record_location(Path::new("root/jobs/fixture.launch"))
                .err()
                .expect("Non-record path accepted")
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
