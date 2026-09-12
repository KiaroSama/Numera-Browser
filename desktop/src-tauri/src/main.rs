// Numera Browser desktop shell.
//
// The whole job of this binary: pick a free port, start the Python engine on it
// with no console window, wait until it really answers /api/health, then point
// the window at it. Everything the user sees after that is served by the engine
// itself -- this shell owns no UI beyond the splash in ../ui/index.html.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{Manager, RunEvent};

#[cfg(windows)]
const FROZEN_ENGINE: &str = "numera-engine.exe";
#[cfg(not(windows))]
const FROZEN_ENGINE: &str = "numera-engine";

#[cfg(windows)]
const VENV_PYTHON: &str = "Scripts/python.exe";
#[cfg(not(windows))]
const VENV_PYTHON: &str = "bin/python";

/// The running engine, so the exit handler can stop it. A leaked engine still
/// holding the port after the window closes is a defect, not a nuisance.
struct Engine(Mutex<Option<Child>>);

/// Last few lines the engine wrote to stderr, for the error splash.
type Log = Arc<Mutex<VecDeque<String>>>;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let log: Log = Arc::new(Mutex::new(VecDeque::new()));

            match start_engine(log.clone()) {
                Ok((child, port)) => {
                    #[cfg(windows)]
                    if let Some(guard) = job::Job::new() {
                        guard.adopt(&child);
                        // Held for the life of the app: the job dies with us,
                        // and the engine dies with the job.
                        app.manage(guard);
                    }
                    app.manage(Engine(Mutex::new(Some(child))));
                    std::thread::spawn(move || match wait_healthy(&handle, port) {
                        Ok(()) => show_dashboard(&handle, port),
                        Err(why) => show_error(&handle, &format!("{why}\n\n{}", tail(&log))),
                    });
                }
                // No window is a worse failure than a window with a reason in it.
                Err(why) => show_error(&handle, &why),
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build the Numera Browser window")
        .run(|handle, event| {
            if matches!(event, RunEvent::Exit) {
                if let Some(engine) = handle.try_state::<Engine>() {
                    if let Some(mut child) = engine.0.lock().unwrap().take() {
                        let _ = child.kill();
                        let _ = child.wait(); // reap, so the port is released before we go
                    }
                }
            }
        });
}

// --- engine lifecycle -------------------------------------------------------

fn start_engine(log: Log) -> Result<(Child, u16), String> {
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
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("could not start the engine ({cmd:?}): {e}"))?;

    if let Some(err) = child.stderr.take() {
        drain(err, log);
    }
    Ok((child, port))
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
mod job {
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
fn wait_healthy(handle: &tauri::AppHandle, port: u16) -> Result<(), String> {
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

// --- window -----------------------------------------------------------------

fn show_dashboard(handle: &tauri::AppHandle, port: u16) {
    let url = format!("http://127.0.0.1:{port}/");
    match (handle.get_webview_window("main"), tauri::Url::parse(&url)) {
        (Some(window), Ok(parsed)) => {
            if let Err(e) = window.navigate(parsed) {
                show_error(handle, &format!("Could not open {url}: {e}"));
            }
        }
        _ => show_error(handle, &format!("Could not open {url}.")),
    }
}

fn show_error(handle: &tauri::AppHandle, message: &str) {
    if let Some(window) = handle.get_webview_window("main") {
        let json = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".into());
        let _ = window.eval(format!("window.numeraError({json})"));
    }
}

// --- engine stderr ----------------------------------------------------------

/// Drain the engine's stderr on its own thread. Piping without reading would
/// wedge the engine once the pipe buffer filled; keeping every line would grow
/// without bound over a long session. Keep the last few, which is all an error
/// message needs.
fn drain(stream: impl Read + Send + 'static, log: Log) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let mut lines = log.lock().unwrap();
            lines.push_back(line);
            if lines.len() > 40 {
                lines.pop_front();
            }
        }
    });
}

fn tail(log: &Log) -> String {
    log.lock()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answer one request with `response`, then close. Returns the port.
    fn stub(response: &'static str) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut sock, _)) = listener.accept() {
                let mut scratch = [0u8; 1024];
                let _ = sock.read(&mut scratch);
                let _ = sock.write_all(response.as_bytes());
            }
        });
        port
    }

    #[test]
    fn accepts_the_engines_own_health_payload() {
        // Exactly what `numera serve` returns from `/api/ready`, verified
        // against a live server. This stub still carried the old
        // `/api/health` payload, which has no `service` field - so it
        // asserted a contract `healthy()` had stopped accepting, and
        // nothing ever ran it to notice.
        let port = stub("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"ok\":true,\"service\":\"numera-browser\",\"version\":\"0.11.0\",\"authRequired\":false}");
        assert!(healthy(port));
    }

    #[test]
    fn rejects_a_200_from_something_that_is_not_numera() {
        // The whole reason the body is checked: another app can own the port.
        let port = stub("HTTP/1.1 200 OK\r\n\r\n<html>some other server</html>");
        assert!(!healthy(port));
    }

    #[test]
    fn rejects_a_non_200() {
        let port = stub("HTTP/1.1 503 Service Unavailable\r\n\r\n{\"ok\":true}");
        assert!(!healthy(port));
    }

    #[test]
    fn rejects_a_closed_port() {
        let port = free_port().unwrap(); // reserved then released; nothing listening
        assert!(!healthy(port));
    }

    /// The whole point of the job object: the child must die when the last
    /// handle to the job closes, which is what happens if the shell is killed.
    #[cfg(windows)]
    #[test]
    fn a_job_kills_its_child_when_the_last_handle_closes() {
        let guard = job::Job::new().expect("could not create a job object");
        let mut child = Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("could not start the stand-in child");
        assert!(
            guard.adopt(&child),
            "the child was not adopted into the job"
        );
        assert!(
            child.try_wait().unwrap().is_none(),
            "the child died on its own"
        );

        drop(guard);

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        panic!("the child outlived the job");
    }

    // --- where the portable build keeps its data ----------------------------

    /// A scratch directory of our own: empty at the start, gone at the end,
    /// however the test ended. `Drop` also puts back anything a case did to
    /// its permissions, so a panicked assertion cannot leave an undeletable
    /// directory behind in the system temp folder.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("numera-n15-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("could not create the scratch directory");
            Scratch(dir)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            #[cfg(windows)]
            {
                let _ = Command::new("attrib")
                    .arg("-R")
                    .arg(self.0.join("*"))
                    .args(["/S", "/D"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                if let Some(account) = account() {
                    let _ = icacls(&self.0, &["/remove:d", &account, "/T", "/C", "/Q"]);
                }
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                for dir in [self.0.clone(), self.0.join("data")] {
                    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755));
                }
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(windows)]
    const HOME_VAR: &str = "USERPROFILE";
    #[cfg(not(windows))]
    const HOME_VAR: &str = "HOME";

    /// The environment these cases need: `NUMERA_DATA_DIR` cleared, and a home
    /// of our own so the per-user fallback can be asserted without ever
    /// touching the real one. Under a lock, because environment variables are
    /// process-wide and `cargo test` runs these on parallel threads.
    struct Env {
        home: Scratch,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl Env {
        fn new(name: &str) -> Self {
            // A failing case poisons the lock; the next one still needs it.
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let home = Scratch::new(&format!("{name}-home"));
            // SAFETY: that lock is what makes this the only thread touching
            // the environment. `home` is declared first so it is removed
            // before the lock is released.
            unsafe {
                std::env::remove_var("NUMERA_DATA_DIR");
                std::env::set_var(HOME_VAR, home.path());
            }
            Env { home, _lock: lock }
        }

        fn home(&self) -> &std::path::Path {
            self.home.path()
        }
    }

    /// The account this test runs as, in a form `icacls` accepts.
    #[cfg(windows)]
    fn account() -> Option<String> {
        let user = std::env::var("USERNAME").ok().filter(|u| !u.is_empty())?;
        match std::env::var("USERDOMAIN") {
            Ok(domain) if !domain.is_empty() => Some(format!("{domain}\\{user}")),
            _ => Some(user),
        }
    }

    #[cfg(windows)]
    fn icacls(dir: &std::path::Path, args: &[&str]) -> bool {
        Command::new("icacls")
            .arg(dir)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }

    /// The shell used to pass `--data-dir` unconditionally, and an explicit
    /// argument outranks `NUMERA_DATA_DIR` in the engine's resolution order -
    /// so a user who set it from the desktop app was quietly ignored. It is
    /// answered first, before the portable location is so much as probed.
    #[test]
    fn an_environment_override_is_left_to_the_engine() {
        let env = Env::new("env-override");
        let root = Scratch::new("env-override-root");
        // SAFETY: `Env` holds the lock that serialises these cases.
        unsafe { std::env::set_var("NUMERA_DATA_DIR", env.home().join("elsewhere")) };

        // A perfectly writable portable root: the override still wins.
        assert!(
            portable_data_dir(root.path()).is_none(),
            "the shell overruled the user's own NUMERA_DATA_DIR"
        );
        assert!(
            !root.path().join("data").exists(),
            "the portable location was probed despite an explicit override"
        );
    }

    /// The ordinary portable case: data beside the shell, never inside
    /// engine/, which an update replaces wholesale.
    #[test]
    fn a_writable_portable_root_keeps_data_beside_the_shell() {
        let _env = Env::new("writable");
        let root = Scratch::new("writable-root");

        let chosen = portable_data_dir(root.path()).expect("a writable root got no data dir");

        assert_eq!(chosen, root.path().join("data"));
        assert!(chosen.is_dir(), "the chosen directory was not created");
        assert!(
            !chosen.starts_with(root.path().join("engine")),
            "data was placed inside the replaceable engine directory"
        );
        // The probe clears up after itself. A fresh data directory that starts
        // life with a stray dotfile in it ends up in the user's next backup.
        assert!(
            std::fs::read_dir(&chosen).unwrap().next().is_none(),
            "the write probe left a file behind"
        );
        // And the answer is load-bearing rather than inferred: it takes a file.
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the chosen directory refused a real write");
    }

    #[test]
    fn an_absent_portable_root_is_created_and_chosen() {
        let _env = Env::new("absent");
        let root = Scratch::new("absent-root");
        let install = root.path().join("not").join("there").join("yet");

        let chosen = portable_data_dir(&install).expect("a creatable root got no data dir");

        assert_eq!(chosen, install.join("data"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the chosen directory refused a real write");
    }

    /// A portable install normally lives somewhere with spaces in the path,
    /// like `C:\Program Files\Numera Browser`. Nothing here quotes or splits a
    /// string, and this is what keeps it that way.
    #[test]
    fn a_path_with_spaces_is_chosen_and_written_like_any_other() {
        let _env = Env::new("spaced");
        let root = Scratch::new("with spaces");
        let install = root.path().join("Program Files").join("Numera Browser");

        let chosen = portable_data_dir(&install).expect("a path with spaces got no data dir");

        assert_eq!(chosen, install.join("data"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("a directory whose path contains spaces refused a real write");
    }

    /// Windows: the read-only *attribute* on a directory is not an answer
    /// about writability. NTFS creates files inside one perfectly happily, so
    /// rejecting such a directory would move a working portable install to the
    /// user's home for no reason. The decision has to agree with what the
    /// filesystem actually does, which is what this asks it directly.
    #[cfg(windows)]
    #[test]
    fn a_read_only_attribute_is_not_an_answer_about_writability() {
        let _env = Env::new("read-only-attr");
        let root = Scratch::new("read-only-attr-root");
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let marked = Command::new("attrib")
            .arg("+R")
            .arg(&data)
            .arg("/D")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if !marked || !std::fs::metadata(&data).unwrap().permissions().readonly() {
            eprintln!(
                "SKIPPED a_read_only_attribute_is_not_an_answer_about_writability: the \
                 read-only attribute would not stay on {}",
                data.display()
            );
            return;
        }

        // What the filesystem says, asked independently of the code under test.
        let witness = data.join("independent-witness");
        let creatable = std::fs::File::create(&witness).is_ok();
        let _ = std::fs::remove_file(&witness);

        let chosen = portable_data_dir(root.path());
        if creatable {
            assert_eq!(
                chosen.as_deref(),
                Some(data.as_path()),
                "a directory that accepts files was rejected over an attribute flag"
            );
        } else {
            assert_ne!(
                chosen.as_deref(),
                Some(data.as_path()),
                "a directory that refuses files was chosen anyway"
            );
        }
    }

    /// The same case where the permission bits really do decide it.
    #[cfg(unix)]
    #[test]
    fn a_read_only_portable_root_falls_back_to_the_per_user_location() {
        use std::os::unix::fs::PermissionsExt;

        let env = Env::new("read-only");
        let root = Scratch::new("read-only-root");
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o555)).unwrap();

        let witness = data.join("independent-witness");
        if std::fs::File::create(&witness).is_ok() {
            let _ = std::fs::remove_file(&witness);
            eprintln!(
                "SKIPPED a_read_only_portable_root_falls_back_to_the_per_user_location: mode \
                 0o555 did not deny file creation - running as root?"
            );
            return;
        }

        let chosen = portable_data_dir(root.path()).expect("an unwritable root got no data dir");

        assert_eq!(chosen, env.home().join(".numera"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the location it fell back to refused a real write");
    }

    /// Windows: the case the read-only attribute cannot express, and the
    /// finding itself. A DENY ACE refuses file creation while `create_dir_all`
    /// still answers `Ok(())`, because the directory is already there - so the
    /// shell handed the engine a `--data-dir` it cannot write, and an explicit
    /// one takes away the fallback the engine would otherwise have found.
    #[cfg(windows)]
    #[test]
    fn an_acl_that_denies_file_creation_falls_back_to_the_per_user_location() {
        let env = Env::new("acl-denied");
        let root = Scratch::new("acl-denied-root");
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let Some(account) = account() else {
            eprintln!(
                "SKIPPED an_acl_that_denies_file_creation_falls_back_to_the_per_user_location: \
                 no USERNAME in the environment to write a DENY ACE for"
            );
            return;
        };
        // WD is "create a file in this directory", AD "create a subdirectory".
        if !icacls(&data, &["/deny", &format!("{account}:(WD,AD)")]) {
            eprintln!(
                "SKIPPED an_acl_that_denies_file_creation_falls_back_to_the_per_user_location: \
                 icacls could not write a DENY ACE on {}",
                data.display()
            );
            return;
        }
        // Prove the condition exists before trusting what the code says of it.
        let witness = data.join("independent-witness");
        if std::fs::File::create(&witness).is_ok() {
            let _ = std::fs::remove_file(&witness);
            eprintln!(
                "SKIPPED an_acl_that_denies_file_creation_falls_back_to_the_per_user_location: \
                 the DENY ACE did not deny - a privileged or specially granted account"
            );
            return;
        }

        assert!(
            std::fs::create_dir_all(&data).is_ok(),
            "the trap this exists for is gone: create_dir_all now reports the denial"
        );

        let chosen = portable_data_dir(root.path()).expect("an unwritable root got no data dir");

        assert_eq!(chosen, env.home().join(".numera"));
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the location it fell back to refused a real write");
    }

    /// An install the shell cannot write - Program Files, a read-only mount.
    ///
    /// This asserted `None` and called it deferring to the engine, which reads
    /// well and is wrong: with no `--data-dir`, a frozen engine's own portable
    /// default is `data/` beside its *binary*, and the binary sits in
    /// `engine/` - the directory an update replaces wholesale. Silence put the
    /// user's profiles exactly where the next update would delete them.
    #[test]
    fn an_unwritable_root_names_the_per_user_location_instead() {
        let env = Env::new("unwritable");
        let root = Scratch::new("unwritable-root");
        // A path under a FILE cannot be created as a directory on any platform.
        let file = root.path().join("not a directory");
        std::fs::write(&file, b"a file, not a directory").unwrap();

        let chosen =
            portable_data_dir(&file.join("under-a-file")).expect("an unwritable root got nothing");

        assert_eq!(chosen, env.home().join(".numera"));
        assert!(
            !chosen.starts_with(root.path()),
            "data was left inside the install the shell cannot write"
        );
        std::fs::write(chosen.join("profiles.json"), b"{}")
            .expect("the location it fell back to refused a real write");
    }

    /// Numera was renamed from Persona, and the engine still reads
    /// `~/.persona` for as long as `~/.numera` does not exist. Naming the
    /// fallback in the shell means naming the same one, or an upgrade from
    /// Persona comes up with an empty profile list beside a full directory.
    #[test]
    fn the_per_user_fallback_keeps_an_existing_persona_install() {
        let env = Env::new("persona");
        let root = Scratch::new("persona-root");
        let file = root.path().join("not a directory");
        std::fs::write(&file, b"a file, not a directory").unwrap();
        std::fs::create_dir_all(env.home().join(".persona")).unwrap();

        let chosen =
            portable_data_dir(&file.join("under-a-file")).expect("an unwritable root got nothing");

        assert_eq!(chosen, env.home().join(".persona"));
    }

    /// Neither location works: no path is invented. The engine has a better
    /// error for this than the shell does, and an explicit `--data-dir` would
    /// only take its remaining options away.
    #[test]
    fn nothing_writable_anywhere_leaves_the_choice_to_the_engine() {
        let env = Env::new("nowhere");
        let root = Scratch::new("nowhere-root");
        let file = root.path().join("not a directory");
        std::fs::write(&file, b"a file, not a directory").unwrap();
        let fake_home = env.home().join("not a directory either");
        std::fs::write(&fake_home, b"nor is this").unwrap();
        // SAFETY: `Env` holds the lock that serialises these cases.
        unsafe { std::env::set_var(HOME_VAR, &fake_home) };

        assert!(
            portable_data_dir(&file.join("under-a-file")).is_none(),
            "the shell invented a data directory that nothing can write"
        );
    }

    #[test]
    fn free_port_hands_out_a_bindable_port() {
        let port = free_port().unwrap();
        assert!(port > 0);
        TcpListener::bind(("127.0.0.1", port)).expect("the port we handed out was not free");
    }
}
