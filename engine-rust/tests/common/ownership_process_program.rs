//! Actual native processes and wire fixtures; no additional test cases.
mod ownership_capture_program;
use std::{
    fs,
    io::{self, BufRead, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn wait(path: &Path) -> io::Result<()> {
    let end = Instant::now() + Duration::from_secs(10);
    while !path.is_file() {
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "fixture rendezvous",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Native ownership fixture failed: {error}");
        std::process::exit(111)
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("capture") => ownership_capture_program::run(&args[1..])?,
        Some("identity") => {
            let pid: u32 = args[1].parse()?;
            println!("{}", observe_identity(pid)?);
        }
        Some("wait") => {
            println!("ready");
            io::stdout().flush()?;
            thread::sleep(Duration::from_secs(30));
        }
        Some("sleep") => {
            thread::sleep(Duration::from_secs(30));
        }
        Some("ready") => {
            let path = Path::new(&args[1]);
            let failure = args[2] == "locked";
            let staging = path.with_extension("part");
            fs::write(
                &staging,
                serde_json::to_vec(
                    &serde_json::json!({"ok":!failure,"kind":"locked","error":"fixture startup refusal"}),
                )?,
            )?;
            fs::rename(staging, path)?;
            if let Some(release) = args.get(3) {
                wait(Path::new(release))?;
            } else {
                thread::sleep(Duration::from_secs(30));
            }
        }
        Some("tree") => {
            let root = Path::new(&args[1]);
            fs::create_dir_all(root)?;
            let mut leaf = Command::new(std::env::current_exe()?)
                .arg("sleep")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            fs::write(
                root.join("child.json"),
                serde_json::to_vec(
                    &serde_json::json!({"pid":leaf.id(),"leader":std::process::id()}),
                )?,
            )?;
            let _ = leaf.try_wait()?;
            fs::write(root.join("ready"), b"ready")?;
            println!("ready");
            io::stdout().flush()?;
            thread::sleep(Duration::from_secs(30));
        }
        Some("rpc") => {
            for line in io::stdin().lock().lines() {
                let request: serde_json::Value = serde_json::from_str(&line?)?;
                let close = request["operation"] == "close";
                println!(
                    "{}",
                    serde_json::json!({"protocol":1,"id":request["id"],"ok":true,"result":if close{serde_json::json!({"closing":true})}else{serde_json::json!({})}})
                );
                io::stdout().flush()?;
                if close {
                    break;
                }
            }
        }
        _ => return Err(io::Error::other("unknown fixture mode").into()),
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn observe_identity(pid: u32) -> io::Result<serde_json::Value> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let suffix = text
        .rsplit_once(')')
        .ok_or_else(|| io::Error::other("stat command boundary absent"))?
        .1;
    let token = suffix
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| io::Error::other("birth token absent"))?
        .parse::<u64>()
        .map_err(io::Error::other)?;
    Ok(serde_json::json!({"token":token}))
}
#[cfg(target_os = "macos")]
fn observe_identity(pid: u32) -> io::Result<serde_json::Value> {
    let output = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("process birth observer refused"));
    }
    let text = String::from_utf8(output.stdout).map_err(io::Error::other)?;
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(io::Error::other("process birth observer empty"));
    }
    Ok(serde_json::json!({"lstart":normalized}))
}
#[cfg(windows)]
fn observe_identity(pid: u32) -> io::Result<serde_json::Value> {
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn GetProcessTimes(
            process: *mut std::ffi::c_void,
            created: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    struct Process(*mut std::ffi::c_void);
    impl Drop for Process {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }
    // SAFETY: query-only access; pointer-sized owned handle and four valid distinct output records.
    let handle = Process(unsafe { OpenProcess(0x1000, 0, pid) });
    if handle.0.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut created = FileTime { low: 0, high: 0 };
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe { GetProcessTimes(handle.0, &mut created, &mut exit, &mut kernel, &mut user) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(serde_json::json!({"token":(u64::from(created.high)<<32)|u64::from(created.low)}))
}
