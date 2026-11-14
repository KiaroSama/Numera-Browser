//! Individually reported native ports of retained source ownership scenarios.
//! Exact source IDs and architecture adaptations are recorded in the test map.
use numera_core::{
    ownership::{
        capture, claims, identity, ready, recorded_tree, session::Session, tree::OwnedTree,
        unit_report,
    },
    storage::{
        atomic::write_json,
        config::ConfigStore,
        locks::{DataLock, SessionLease, configured_timeout},
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Barrier, OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};
mod common;
type TestResult = Result<(), Box<dyn std::error::Error>>;
macro_rules! source_test {
    ($name:ident,$source:literal,$body:block) => {
        #[test]
        #[doc=$source]
        fn $name() -> TestResult $body
    };
}
fn temp(name: &str) -> io::Result<common::TempDirectory> {
    common::TempDirectory::new(name)
}
fn limit(wall: Duration) -> capture::Limits {
    capture::Limits {
        wall,
        idle: wall,
        bytes: 1_048_576,
        cleanup: Duration::from_secs(2),
    }
}
fn receipt(root: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(root.join("jobs"))?;
    let path = root.join("jobs/source.units");
    unit_report::initialize(&path)?;
    Ok(path)
}
fn append(path: &Path, bytes: &[u8]) -> io::Result<()> {
    fs::OpenOptions::new()
        .append(true)
        .open(path)?
        .write_all(bytes)
}
fn unit(tree: Value) -> Vec<u8> {
    let mut value = tree.as_object().cloned().expect("unit object");
    value.insert("event".into(), "unit".into());
    value.insert("attempt".into(), "a".into());
    let mut bytes = b"{\"event\":\"pending\",\"attempt\":\"a\"}\n".to_vec();
    bytes.extend(serde_json::to_vec(&value).expect("unit encoding"));
    bytes.push(b'\n');
    bytes
}
fn wait_file(path: &Path) -> io::Result<()> {
    let end = Instant::now() + Duration::from_secs(10);
    while !path.is_file() {
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Rust fixture did not reach rendezvous",
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn library(deps: &Path, name: &str) -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    let stem = executable
        .file_stem()
        .ok_or_else(|| io::Error::other("Test artifact stem missing"))?
        .to_string_lossy();
    let (_, suffix) = stem
        .rsplit_once('-')
        .ok_or_else(|| io::Error::other("Hashed test artifact required"))?;
    let fingerprints = deps
        .parent()
        .ok_or_else(|| io::Error::other("Target profile directory missing"))?
        .join(".fingerprint");
    let own_dir = fingerprints.join(format!("{}-{suffix}", env!("CARGO_PKG_NAME")));
    let own_file = fs::read_dir(own_dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("test-"))
                && p.extension().is_some_and(|e| e == "json")
        })
        .ok_or_else(|| io::Error::other("Test dependency metadata missing"))?;
    let own: Value = serde_json::from_slice(&fs::read(own_file)?).map_err(io::Error::other)?;
    let wanted = own["deps"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r[1] == name))
        .and_then(|r| r[3].as_u64())
        .ok_or_else(|| io::Error::other(format!("Test dependency {name} missing")))?;
    let package = if name == "numera_core" {
        "numera-core"
    } else {
        name
    };
    fs::read_dir(deps)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(&format!("lib{name}-")))
                && p.extension().is_some_and(|e| e == "rlib")
        })
        .find(|p| {
            let Some(stem) = p.file_stem() else {
                return false;
            };
            let stem = stem.to_string_lossy();
            let Some(suffix) = stem.strip_prefix(&format!("lib{name}-")) else {
                return false;
            };
            let encoded =
                fs::read_to_string(fingerprints.join(format!("{package}-{suffix}/lib-{name}")))
                    .unwrap_or_default();
            u64::from_str_radix(encoded.trim(), 16)
                .is_ok_and(|n| u64::from_le_bytes(n.to_be_bytes()) == wanted)
        })
        .ok_or_else(|| io::Error::other(format!("Exact compiled {name} dependency missing")))
}
fn fixture() -> io::Result<PathBuf> {
    static BINARY: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let deps = std::env::current_exe()
                .map_err(|error| error.to_string())?
                .parent()
                .ok_or("Test dependency directory missing")?
                .to_owned();
            let directory = deps.join(format!("source-ownership-fixture-{}", std::process::id()));
            fs::create_dir(&directory).map_err(|error| error.to_string())?;
            let binary = directory.join(if cfg!(windows) {
                "helper.exe"
            } else {
                "helper"
            });
            let mut command =
                Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
            command
                .arg("--edition=2024")
                .arg("--crate-name=source_ownership_helper")
                .arg("-Cdebuginfo=0")
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/source_ownership/helper.rs"))
                .arg("--extern")
                .arg(format!(
                    "numera_core={}",
                    library(&deps, "numera_core")
                        .map_err(|error| error.to_string())?
                        .display()
                ))
                .arg("--extern")
                .arg(format!(
                    "serde_json={}",
                    library(&deps, "serde_json")
                        .map_err(|error| error.to_string())?
                        .display()
                ))
                .arg("-L")
                .arg(format!("dependency={}", deps.display()))
                .arg("-o")
                .arg(&binary);
            #[cfg(windows)]
            command.arg("--extern").arg(format!(
                "getrandom={}",
                library(&deps, "getrandom")
                    .map_err(|error| error.to_string())?
                    .display()
            ));
            let output = capture::run(&mut command, limit(Duration::from_secs(60)))
                .map_err(|error| error.to_string())?;
            if output.exit != 0 {
                return Err(format!(
                    "Rust fixture compile failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(binary)
        })
        .clone()
        .map_err(io::Error::other)
}
fn command(mode: &str) -> io::Result<Command> {
    let mut command = Command::new(fixture()?);
    command.arg(mode);
    Ok(command)
}
fn capture_initialization_evidence(report: &Value) {
    #[cfg(windows)]
    {
        let phase = &report["initialization"];
        eprintln!(
            "Windows capture initialization cold={:?} rng_first={:?} rng_second={:?} stdio_first={:?} stdio_second={:?}",
            phase["cold"].as_u64(),
            phase["rng_first"].as_u64(),
            phase["rng_second"].as_u64(),
            phase["stdio_closed"][0].as_u64(),
            phase["stdio_closed"][1].as_u64()
        );
    }
    #[cfg(not(windows))]
    let _ = report;
}
fn emit(payload: &str, code: i32) -> Result<capture::Output, capture::Failure> {
    let mut child = command("emit").map_err(capture::Failure::from)?;
    child.arg(payload).arg(code.to_string());
    capture::run(&mut child, limit(Duration::from_secs(5)))
}
fn profile(
    root: &Path,
    id: &str,
) -> Result<numera_core::models::Profile, Box<dyn std::error::Error>> {
    let profile = numera_core::conversion::to_engine(&json!({"id":id,"name":id}), 1000.0)
        .map_err(io::Error::other)?;
    Ok(numera_core::storage::repo::ProfileRepo::new(root.into(), None).save(profile, 1000.0)?)
}
fn spawn_tree(root: &Path, linger: bool) -> io::Result<OwnedTree> {
    let mut child = command("tree")?;
    child
        .arg(root)
        .arg(if linger { "hold" } else { "exit" })
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let tree = OwnedTree::spawn(&mut child)?;
    wait_file(&root.join("leader"))?;
    Ok(tree)
}
fn as_record(tree: &OwnedTree) -> Value {
    let mut record = tree.record();
    record["pidToken"] = record["token"].take();
    record
}
fn process_running(pid: u32, token: Option<u64>) -> io::Result<bool> {
    if !identity::alive(pid, token)? {
        return Ok(false);
    }
    #[cfg(target_os = "linux")]
    {
        // Signal-zero answers for an orphaned zombie. The original source
        // process oracle excludes those while preserving PID/birth checks.
        let stat = match fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => stat,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let (_, fields) = stat
            .rsplit_once(") ")
            .ok_or_else(|| io::Error::other("Process stat command was incomplete"))?;
        Ok(!matches!(fields.split_whitespace().next(), Some("Z" | "X")))
    }
    #[cfg(not(target_os = "linux"))]
    Ok(true)
}
#[cfg(unix)]
fn fixture_row_empty(root: &Path, group: i32, complete: bool) -> io::Result<bool> {
    use numera_core::ownership::observation::{self, Enumeration, GroupSource, Row};
    struct Rows<'a> {
        root: &'a Path,
        complete: bool,
    }
    impl GroupSource for Rows<'_> {
        fn enumerate(&mut self, _: i32) -> io::Result<Enumeration> {
            let pids = fs::read_dir(self.root)?
                .filter_map(|entry| {
                    entry.ok().and_then(|entry| {
                        entry
                            .file_name()
                            .to_str()
                            .and_then(|name| name.parse().ok())
                    })
                })
                .collect();
            Ok(Enumeration {
                pids,
                complete: self.complete,
                filtered: false,
            })
        }
        fn process(&mut self, pid: u32) -> io::Result<Option<Row>> {
            let text = fs::read_to_string(self.root.join(pid.to_string()).join("stat"))?;
            observation::procfs_row(pid, &text).map(Some)
        }
    }
    observation::observe_group(group, None, &mut Rows { root, complete })
}
#[path = "source_ownership/adapter_io.rs"]
mod adapter_io;
#[path = "source_ownership/admission.rs"]
mod admission;
#[path = "source_ownership/completion.rs"]
mod completion;
#[path = "source_ownership/crossprocess.rs"]
mod crossprocess;
#[path = "source_ownership/lifecycle.rs"]
mod lifecycle;
#[path = "source_ownership/lock_phases.rs"]
mod lock_phases;
#[path = "source_ownership/locks.rs"]
mod locks;
#[path = "source_ownership/managers.rs"]
mod managers;
#[path = "source_ownership/observability.rs"]
mod observability;
#[path = "source_ownership/orphans.rs"]
mod orphans;
#[path = "source_ownership/pipe_errors.rs"]
mod pipe_errors;
#[path = "source_ownership/races.rs"]
mod races;
#[path = "source_ownership/readiness.rs"]
mod readiness;
#[path = "source_ownership/receipt_failures.rs"]
mod receipt_failures;
#[path = "source_ownership/receipts.rs"]
mod receipts;
#[path = "source_ownership/sessions.rs"]
mod sessions;
#[path = "source_ownership/status.rs"]
mod status;
#[path = "source_ownership/trees.rs"]
mod trees;

#[path = "source_ownership/kernel.rs"]
mod kernel;

#[path = "source_ownership/integration.rs"]
mod integration;
#[path = "common/socket_fixture.rs"]
mod socket_fixture;
