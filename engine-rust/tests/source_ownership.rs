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
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};
mod common;
#[path = "common/ownership_fixture.rs"]
mod ownership_fixture;
use crate::common::cargo_artifacts::Artifacts;
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
fn fixture() -> io::Result<PathBuf> {
    ownership_fixture::binary()
}
fn command(mode: &str) -> io::Result<Command> {
    let mut command = Command::new(fixture()?);
    command.arg(mode);
    Ok(command)
}
fn capture_initialization_evidence(report: &Value) {
    handle_attribution_evidence(report);
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
fn handle_attribution_evidence(report: &Value) {
    fn array(value: &Value, length: usize, signed: bool) -> bool {
        value.as_array().is_some_and(|values| {
            values.len() == length
                && values.iter().all(|n| {
                    if signed {
                        n.as_i64().is_some_and(|n| (-4096..=4096).contains(&n))
                    } else {
                        n.as_u64().is_some_and(|n| n <= 4096)
                    }
                })
        })
    }
    fn sample(value: &Value) -> bool {
        value.as_object().is_some_and(|fields| {
            fields.len() == 7
                && fields
                    .get("status")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n <= 8)
                && fields
                    .get("total")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n <= 4096)
                && fields
                    .get("duplicates_failed")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n <= 4224)
                && fields
                    .get("queries_failed")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n <= 128)
                && array(&value["types"], 28, false)
                && array(&value["files"], 4, false)
                && array(&value["standard"], 3, false)
        })
    }
    fn valid(value: &Value) -> bool {
        value.as_object().is_some_and(|fields| fields.len() == 4)
            && value["schema"] == 1
            && sample(&value["before"])
            && sample(&value["after"])
            && array(&value["type_delta"], 28, true)
    }
    if let Some(control) = report.get("handle_attribution_control") {
        // These pure formatting controls execute under the existing capture
        // cases; they neither query Windows nor publish invented live samples.
        assert!(valid(control), "Portable attribution JSON is invalid");
        let mut malformed = control.clone();
        malformed["unexpected_name"] = "must never be forwarded".into();
        assert!(
            !valid(&malformed),
            "Arbitrary attribution fields were admitted"
        );
        let mut malformed = control.clone();
        malformed["before"]["types"][0] = u64::MAX.into();
        assert!(
            !valid(&malformed),
            "Unbounded attribution values were admitted"
        );
    }
    for field in ["handle_attribution_before_cleanup", "handle_attribution"] {
        let Some(text) = report[field].as_str() else {
            continue;
        };
        let decoded = (text.len() <= 16384)
            .then(|| serde_json::from_str::<Value>(text).ok())
            .flatten();
        if let Some(value) = decoded.filter(valid) {
            eprintln!(
                "Windows own-handle attribution stage={field} type_order=File,Thread,Process,Event,Job,IoCompletion,Mutant,Section,Semaphore,Timer,Key,Token,Directory,SymbolicLink,ALPC_Port,WaitCompletionPacket,TpWorkerFactory,IRTimer,Desktop,WindowStation,EtwRegistration,EtwConsumer,Partition,Session,DebugObject,Device,Driver,Other files=Pipe,Disk,Char,Unknown standard=stdin,stdout,stderr status=complete,deadline,unsupported,symbols,query,bounds,type_cap,unresolved_type,duplicate sampling=non_atomic complete=bounded_query_procedure_only report={value}"
            );
        } else {
            eprintln!("Windows own-handle attribution unavailable: invalid fixed schema");
        }
    }
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
#[cfg(unix)]
#[path = "source_ownership/signal_errors.rs"]
mod signal_errors;

#[path = "source_ownership/integration.rs"]
mod integration;
#[path = "common/socket_fixture.rs"]
mod socket_fixture;
