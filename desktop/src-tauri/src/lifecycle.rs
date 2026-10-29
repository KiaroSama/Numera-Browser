//! Engine startup, data-root selection and native lifetime ownership.
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::Manager;
#[cfg(windows)]
const FROZEN_ENGINE: &str = "numera-engine.exe";
#[cfg(not(windows))]
const FROZEN_ENGINE: &str = "numera-engine";
#[cfg(windows)]
const VENV_PYTHON: &str = "Scripts/python.exe";
#[cfg(not(windows))]
const VENV_PYTHON: &str = "bin/python";
pub(crate) struct Engine(pub Mutex<Option<Child>>);
pub(crate) struct Reader(pub Mutex<Option<std::thread::JoinHandle<()>>>);
pub(crate) type Log = Arc<Mutex<VecDeque<String>>>;

pub(crate) fn start_engine(
    log: Log,
    update_token: &str,
) -> Result<(Child, u16, Option<std::thread::JoinHandle<()>>), String> {
    let port = free_port().map_err(|e| format!("could not reserve a local port: {e}"))?;
    let mut cmd = engine_command(port)?;
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    no_console(&mut cmd);

    die_with_parent(&mut cmd);
    // The portable half of the same guarantee: the engine watches this pid and
    // exits when it goes away. Linux has PDEATHSIG above and Windows has the
    // job object, but macOS has neither, and a watcher is the one mechanism
    // that works everywhere.
    cmd.env("NUMERA_PARENT_PID", std::process::id().to_string());
    cmd.env("NUMERA_UPDATE_TOKEN", update_token);
    cmd.env_remove("NUMERA_UPDATE_TICKET");
    if let Some(path) = std::env::var_os("NUMERA_UPDATE_RECOVERY") {
        let journal =
            numera_desktop::updates::transaction::Journal::load(std::path::Path::new(&path))
                .map_err(|_| "Update recovery ownership could not be verified".to_string())?;
        let executable = std::env::current_exe().map_err(|_| "Desktop identity unavailable")?;
        let root = executable
            .parent()
            .ok_or("Desktop directory missing")?
            .canonicalize()
            .map_err(|_| "Desktop directory unavailable")?;
        if journal.root != root
            || std::env::var("NUMERA_UPDATE_ACTIVATION").as_deref() != Ok(journal.nonce.as_str())
        {
            return Err("Update recovery belongs to another installation".into());
        }
        cmd.env("NUMERA_UPDATE_TICKET", &journal.ticket);
    }
    cmd.env_remove("NUMERA_UPDATE_WORKER");
    let mut child = cmd
        .spawn()
        .map_err(|error| format!("Could not start the application engine: {:?}", error.kind()))?;

    let reader = child.stderr.take().map(|err| drain(err, log));
    Ok((child, port, reader))
}

/// A port nothing else is using. Asking the OS for port 0 and handing on what it
/// gave us means two copies of Numera, or anything already sitting on 8787,
/// cannot collide.
///
/// ponytail: there is a gap between dropping this listener and the engine
/// binding the same port. Losing that race shows up as the health poll timing
/// out, which is already handled. Pass the socket to the engine if it ever bites.
fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

/// Where the portable build should keep its data, or `None` to let the engine
/// decide for itself.
///
/// `None` means the user has already answered this question with
/// `NUMERA_DATA_DIR`, and an explicit `--data-dir` would quietly overrule them.
/// (Or that nothing on the machine can be written at all, which the engine
/// reports better than the shell can.)
///
/// Every other case names a path, because *not* naming one is not neutral: an
/// explicit `--data-dir` outranks everything in the engine's resolution order,
/// and omitting it sends a frozen engine to its own portable default - `data/`
/// beside its binary, and the binary lives in `engine/`, the directory an
/// update replaces wholesale. So an unwritable portable root falls back to the
/// per-user location by name rather than by silence.
///
/// Nothing is migrated, deleted or re-permissioned here; this only chooses.
fn portable_data_dir(dir: &std::path::Path) -> Option<PathBuf> {
    if std::env::var_os("NUMERA_DATA_DIR").is_some() {
        return None;
    }
    let portable = dir.join("data");
    if accepts_a_file(&portable) {
        return Some(portable);
    }
    let fallback = per_user_data_dir()?;
    accepts_a_file(&fallback).then_some(fallback)
}

/// Probe sequence, so two probes in one process cannot pick the same name.
static PROBE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Can this directory actually take a new file?
///
/// `create_dir_all` cannot answer that. It returns `Ok(())` for a directory
/// that already exists, without touching it, so a read-only mount and a
/// Windows ACL that denies file creation both pass it - and the engine takes
/// an explicit `--data-dir` verbatim, with no writability check of its own to
/// catch what this one missed. On Windows the read-only *attribute* is not the
/// question either: NTFS still creates files in a directory carrying it, and a
/// DENY ACE is what actually refuses. Only a write answers a question about
/// writing.
///
/// The probe name carries the pid and a counter, so two shells starting at the
/// same moment cannot delete each other's, and it is removed whether or not
/// the write succeeded.
fn accepts_a_file(dir: &std::path::Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let seq = PROBE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let probe = dir.join(format!(".numera-write-probe-{}-{seq}", std::process::id()));
    let written = std::fs::write(&probe, b"numera").is_ok();
    let _ = std::fs::remove_file(&probe);
    written
}

/// The per-user location - the same one the engine would have picked.
///
/// Mirrored rather than delegated, because the engine only chooses for itself
/// when no `--data-dir` is passed, and that is the case this exists to avoid.
/// The rename is part of the mirror: Numera was Persona, and the engine still
/// reads `~/.persona` for as long as `~/.numera` does not exist - so a shell
/// naming `~/.numera` unconditionally would hand a Persona user an empty
/// profile list beside a full directory.
fn per_user_data_dir() -> Option<PathBuf> {
    let home = std::env::home_dir()?;
    let numera = home.join(".numera");
    if !numera.exists() && home.join(".persona").is_dir() {
        return Some(home.join(".persona"));
    }
    Some(numera)
}

fn engine_command(port: u16) -> Result<Command, String> {
    let port = port.to_string();
    let serve = ["serve", "--host", "127.0.0.1", "--port"];

    if let Some(exe) = std::env::var_os("NUMERA_ENGINE") {
        let mut cmd = Command::new(exe);
        cmd.args(serve).arg(&port);
        return Ok(cmd);
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("the shell has no parent directory")?;

    // Portable layout: the frozen engine ships next to the shell.
    let frozen = dir.join("engine").join(FROZEN_ENGINE);
    if frozen.is_file() {
        let mut cmd = Command::new(frozen);
        // Data goes beside the shell, not inside engine/. Left to its own
        // default it lands next to the engine binary, so replacing engine/ on
        // an update would take the user's profiles with it.
        //
        // But this used to be passed UNCONDITIONALLY, and an explicit argument
        // outranks everything in the engine's own resolution order. That made
        // NUMERA_DATA_DIR silently ineffective from the desktop app, and it
        // removed the engine's fallback for an install location it cannot
        // write - so a copy under Program Files could fail to start with
        // nowhere else to go. `portable_data_dir` decides instead: nothing is
        // passed when the user has already said where data lives, or when the
        // portable spot is not writable and the engine's per-user fallback is
        // the better answer.
        // --data-dir is a global flag, so it has to precede the subcommand.
        if let Some(data) = portable_data_dir(dir) {
            cmd.arg("--data-dir").arg(data);
        }
        cmd.args(serve).arg(&port);
        return Ok(cmd);
    }

    // Source checkout: the project venv, somewhere above the build output.
    for root in dir.ancestors() {
        let python: PathBuf = root.join(".venv").join(VENV_PYTHON);
        if python.is_file() {
            let mut cmd = Command::new(python);
            cmd.args(["-m", "numera"]).args(serve).arg(&port);
            return Ok(cmd);
        }
    }

    Err(format!(
        "No engine to run.\nLooked for a frozen engine at:\n  {}\nand for a .venv in every \
         directory above:\n  {}",
        frozen.display(),
        dir.display()
    ))
}

#[cfg(windows)]
fn no_console(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
fn no_console(_cmd: &mut Command) {}

/// Linux: ask the kernel to SIGKILL the engine when this process dies, however
/// it dies. The job object does this on Windows; Linux had nothing, so a
/// SIGKILLed shell left the engine running on its port and the next launch
/// started a second one against the same portable data root.
///
/// The race matters. `PR_SET_PDEATHSIG` is set *after* the fork, and the parent
/// can already be gone by then - in which case the signal has been delivered
/// (to nobody) and will never fire again. Comparing the parent pid we recorded
/// before forking with `getppid()` afterwards catches exactly that window, and
/// the child exits immediately rather than becoming the orphan this is meant to
/// prevent.
#[cfg(target_os = "linux")]
fn die_with_parent(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    let parent = std::process::id();
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() as u32 != parent {
                std::process::exit(0); // the parent died inside the race window
            }
            Ok(())
        });
    }
}

#[cfg(not(target_os = "linux"))]
fn die_with_parent(_cmd: &mut Command) {}

/// Killing the child at exit covers the user closing the window. It does not
/// cover the shell being killed outright -- measured: force-kill the shell and
/// the engine keeps running and keeps its port. A job object does, because
/// Windows closes our handles however this process dies, and closing the last
/// handle to the job takes everything in it with it.
#[cfg(windows)]
pub(crate) mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    pub struct Job(HANDLE);

    // A job handle is process-wide and safe to use from any thread; the raw
    // pointer is the only reason this is not derived.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    impl Job {
        pub fn new() -> Option<Self> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of!(limits).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    CloseHandle(handle);
                    return None;
                }
                Some(Job(handle))
            }
        }

        pub fn adopt(&self, child: &Child) -> bool {
            unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) != 0 }
        }
    }
}

// --- readiness --------------------------------------------------------------

/// Poll until the engine answers its own health endpoint, or give up. Never a
/// fixed sleep: a slow machine deserves the extra second and a dead engine
/// should not cost the user the full deadline.
pub(crate) fn wait_healthy(handle: &tauri::AppHandle, port: u16) -> Result<(), String> {
    let budget = std::env::var("NUMERA_ENGINE_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(45);
    let deadline = Instant::now() + Duration::from_secs(budget);

    while Instant::now() < deadline {
        if healthy(port) {
            return Ok(());
        }
        // If it already died there is nothing to wait for.
        if let Some(engine) = handle.try_state::<Engine>() {
            let mut slot = engine.0.lock().unwrap();
            if let Some(child) = slot.as_mut() {
                if let Ok(Some(status)) = child.try_wait() {
                    *slot = None;
                    return Err(format!("The engine exited early ({status})."));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err(format!(
        "The engine did not answer http://127.0.0.1:{port}/api/health within {budget}s."
    ))
}

/// One HTTP/1.0 GET, by hand. A 200 alone is not proof -- something else could
/// own the port -- so the body has to be Numera's own health payload.
fn healthy(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut sock) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(2000)));
    // `/api/ready`, not `/api/health`: health is behind the token, so an engine
    // with one configured answered 401 and the shell decided it had failed to
    // start - leaving no way to reach the screen that asks for the token.
    let req =
        format!("GET /api/ready HTTP/1.0\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if sock.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut body = Vec::new();
    let _ = sock.take(64 * 1024).read_to_end(&mut body);
    let text = String::from_utf8_lossy(&body);
    let status_ok = text
        .lines()
        .next()
        .is_some_and(|line| line.starts_with("HTTP/1.") && line.contains(" 200 "));
    // Identify the service too: something else answering 200 on this port is
    // not our engine, and treating it as ready would point the window at it.
    status_ok && text.contains("\"service\":\"numera-browser\"")
}
/// Drain the engine's stderr on its own thread. Piping without reading would
/// wedge the engine once the pipe buffer filled; keeping every line would grow
/// without bound over a long session. Keep the last few, which is all an error
/// message needs.
fn drain(stream: impl Read + Send + 'static, log: Log) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let mut lines = log.lock().unwrap();
            lines.push_back(line);
            if lines.len() > 40 {
                lines.pop_front();
            }
        }
    })
}

pub(crate) fn tail(log: &Log) -> String {
    log.lock()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
