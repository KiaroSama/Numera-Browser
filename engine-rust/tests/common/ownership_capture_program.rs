//! Real stdout/stderr pressure, descendant, cancellation and deliberately
//! escaped-writer controls for the retained capture integration case.
#[cfg(target_os = "linux")]
use std::time::Instant;
use std::{
    fs,
    io::{self, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, Barrier},
    thread,
    time::Duration,
};
fn pause() {
    thread::sleep(Duration::from_secs(10));
}
fn ready() -> io::Result<()> {
    println!("ready");
    io::stdout().flush()
}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn setsid() -> i32;
    fn getpgrp() -> i32;
}

pub fn run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let mode = args.first().ok_or("capture mode missing")?;
    let root = Path::new(args.get(1).ok_or("capture root missing")?);
    match mode.as_str() {
        "unicode" => {
            io::stdout().write_all("α 😀".as_bytes())?;
            io::stderr().write_all(b"diagnostic")?;
            io::stdout().flush()?;
            io::stderr().flush()?;
            std::process::exit(1);
        }
        "pressure" => {
            let barrier = Arc::new(Barrier::new(3));
            let first = Arc::clone(&barrier);
            let stdout = thread::spawn(move || -> io::Result<()> {
                first.wait();
                let mut output = io::stdout().lock();
                output.write_all(&[b'o'; 32768])?;
                output.flush()
            });
            let second = Arc::clone(&barrier);
            let stderr = thread::spawn(move || -> io::Result<()> {
                second.wait();
                let mut output = io::stderr().lock();
                output.write_all(&[b'e'; 32768])?;
                output.flush()
            });
            barrier.wait();
            stdout.join().map_err(|_| "stdout writer panicked")??;
            stderr.join().map_err(|_| "stderr writer panicked")??;
        }
        "overflow" => {
            io::stdout().write_all(&[b'x'; 65536])?;
            io::stdout().flush()?;
        }
        "timeout" => {
            let mut child = Command::new(std::env::current_exe()?)
                .args(["capture", "leaf"])
                .arg(root)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;
            fs::write(
                root.join("child.json"),
                serde_json::to_vec(
                    &serde_json::json!({"pid":child.id(),"leader":std::process::id()}),
                )?,
            )?;
            ready()?;
            pause();
            let _ = child.try_wait()?;
        }
        "leaf" => pause(),
        "cancel" => {
            fs::write(root.join("cancel-ready"), b"ready")?;
            pause();
        }
        #[cfg(unix)]
        "signal" => {
            println!("{{\"ok\":false}}");
            io::stdout().flush()?;
            // This standalone control intentionally delivers SIGTERM to itself.
            if unsafe { kill(i32::try_from(std::process::id())?, 15) } != 0 {
                return Err(io::Error::last_os_error().into());
            }
            return Err("SIGTERM did not terminate its control process".into());
        }
        #[cfg(target_os = "linux")]
        "escape" => {
            let mut child = Command::new(std::env::current_exe()?)
                .args(["capture", "escape-child"])
                .arg(root)
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()?;
            let end = Instant::now() + Duration::from_secs(2);
            while !root.join("escaped.json").is_file() {
                if Instant::now() >= end {
                    return Err("Escaped writer did not publish identity".into());
                }
                thread::sleep(Duration::from_millis(5));
            }
            ready()?;
            let _ = child.try_wait()?;
        }
        #[cfg(target_os = "linux")]
        "escape-child" => {
            // Match the planted original control's start_new_session=True.
            // Only this standalone fixture escapes; its birth/group identity is
            // published for the test's explicit independent cleanup authority.
            if unsafe { setsid() } < 0 {
                return Err(io::Error::last_os_error().into());
            }
            let stat = fs::read_to_string("/proc/self/stat")?;
            let fields = stat
                .rsplit_once(") ")
                .ok_or("Malformed control stat")?
                .1
                .split_whitespace()
                .collect::<Vec<_>>();
            let token = fields
                .get(19)
                .ok_or("Control birth token missing")?
                .parse::<u64>()?;
            let group = unsafe { getpgrp() };
            if group != i32::try_from(std::process::id())? {
                return Err("Control did not establish its own native session".into());
            }
            let staging = root.join("escaped.part");
            fs::write(
                &staging,
                serde_json::to_vec(
                    &serde_json::json!({"pid":std::process::id(),"pidToken":token,"tree":{"kind":"pgid","group":group}}),
                )?,
            )?;
            fs::rename(staging, root.join("escaped.json"))?;
            thread::sleep(Duration::from_secs(8));
        }
        "reject" => return Err("Capture with invalid bounds must not spawn".into()),
        _ => return Err("Unknown capture process control".into()),
    }
    Ok(())
}
