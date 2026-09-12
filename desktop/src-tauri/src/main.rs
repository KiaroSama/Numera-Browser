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
/// `None` in two cases, and both of them are the engine being better informed
/// than the shell:
///
/// * `NUMERA_DATA_DIR` is set - the user has already answered this question,
///   and an explicit `--data-dir` would quietly overrule them;
/// * the portable location cannot be written - an install under Program Files,
///   a read-only mount - where the engine falls back to per-user storage and
///   the shell forcing an unwritable path would just fail to start.
///
/// Nothing is migrated or removed here; this only chooses what to pass.
fn portable_data_dir(dir: &std::path::Path) -> Option<PathBuf> {
    if std::env::var_os("NUMERA_DATA_DIR").is_some() {
        return None;
    }
    let data = dir.join("data");
    // Creating it is the writability test: `metadata().permissions()` does not
    // answer this on Windows, where the ACL and not the read-only bit decides.
    match std::fs::create_dir_all(&data) {
        Ok(()) => Some(data),
        Err(_) => None,
    }
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

    /// The shell used to pass `--data-dir` unconditionally, and an explicit
    /// argument outranks `NUMERA_DATA_DIR` in the engine's resolution order -
    /// so a user who set it from the desktop app was quietly ignored.
    #[test]
    fn an_environment_override_is_left_to_the_engine() {
        let tmp = std::env::temp_dir().join("numera-n15-env");
        let _ = std::fs::create_dir_all(&tmp);
        // SAFETY: single-threaded test process; restored immediately below.
        unsafe { std::env::set_var("NUMERA_DATA_DIR", tmp.join("elsewhere")) };
        let chosen = portable_data_dir(&tmp);
        unsafe { std::env::remove_var("NUMERA_DATA_DIR") };

        assert!(
            chosen.is_none(),
            "the shell overruled the user's own NUMERA_DATA_DIR"
        );
    }

    /// The ordinary portable case: data beside the shell, never inside
    /// engine/, which an update replaces wholesale.
    #[test]
    fn a_writable_portable_root_keeps_data_beside_the_shell() {
        let tmp = std::env::temp_dir().join("numera-n15-writable");
        let _ = std::fs::create_dir_all(&tmp);
        unsafe { std::env::remove_var("NUMERA_DATA_DIR") };

        let chosen = portable_data_dir(&tmp).expect("a writable root got no data dir");

        assert_eq!(chosen, tmp.join("data"));
        assert!(chosen.is_dir(), "the chosen directory was not created");
        assert!(
            !chosen.starts_with(tmp.join("engine")),
            "data was placed inside the replaceable engine directory"
        );
    }

    /// An install the user cannot write - Program Files, a read-only mount.
    /// Forcing the path there is what turned "fall back to per-user storage"
    /// into "fail to start with nowhere to go".
    #[test]
    fn an_unwritable_root_defers_to_the_engines_own_fallback() {
        unsafe { std::env::remove_var("NUMERA_DATA_DIR") };
        // A path under a FILE cannot be created as a directory on any platform.
        let tmp = std::env::temp_dir().join("numera-n15-file");
        let _ = std::fs::write(&tmp, b"not a directory");

        let chosen = portable_data_dir(&tmp.join("under-a-file"));

        assert!(
            chosen.is_none(),
            "the shell forced a data dir it could not create"
        );
    }

    #[test]
    fn free_port_hands_out_a_bindable_port() {
        let port = free_port().unwrap();
        assert!(port > 0);
        TcpListener::bind(("127.0.0.1", port)).expect("the port we handed out was not free");
    }
}
