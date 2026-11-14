use numera_core::ownership::capture::{self, Limits};
#[cfg(windows)]
use numera_core::ownership::identity;
#[cfg(unix)]
use numera_core::ownership::recorded_tree;
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};
mod common;
use numera_core as core;
#[allow(dead_code)]
#[path = "common/ownership_process.rs"]
mod fixture;
fn limits(bytes: usize) -> Limits {
    Limits {
        wall: Duration::from_secs(3),
        idle: Duration::from_secs(2),
        bytes,
        cleanup: Duration::from_secs(5),
    }
}
fn command(mode: &str, root: &Path) -> std::io::Result<Command> {
    let mut command = fixture::command("capture")?;
    command.arg(mode).arg(root);
    Ok(command)
}
#[test]
fn bounded_capture_drains_both_pipes_and_settles_descendants_on_every_exit()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("command-capture")?;
    let root = temporary.path();
    let mut normal = command("unicode", root)?;
    let output = capture::run(&mut normal, limits(65536))
        .map_err(|error| format!("Unicode/verdict capture: {error}"))?;
    assert_eq!(output.exit, 1);
    assert_eq!(String::from_utf8(output.stdout)?, "α 😀");
    assert_eq!(output.stderr, b"diagnostic");
    let mut pressure = command("pressure", root)?;
    let output = capture::run(&mut pressure, limits(65536))
        .map_err(|error| format!("Concurrent stdout/stderr pressure capture: {error}"))?;
    assert_eq!(output.stdout.len(), 32768);
    assert_eq!(output.stderr.len(), 32768);
    assert_eq!(output.exit, 0);
    for (index, mode) in ["overflow", "timeout"].into_iter().enumerate() {
        let mut child = command(mode, root)?;
        let mut error = capture::run(&mut child, limits(1024))
            .err()
            .ok_or("Capture ignored its bound")?;
        assert!(
            error.retained_record().is_none(),
            "Owned capture tree did not settle"
        );
        error.settle(Duration::from_secs(5))?;
        let path = root.join("child.json");
        if index == 1 {
            assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
            assert!(
                path.is_file(),
                "Timed-out child did not publish its identity"
            );
            let child: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            let pid = u32::try_from(child["pid"].as_u64().ok_or("Child identity missing")?)?;
            // Linux may retain an adopted zombie briefly; tree emptiness rejects
            // only live descendants and is the same proof the capture uses.
            #[cfg(windows)]
            assert!(
                !identity::alive(pid, None)?,
                "Timed-out capture left its child"
            );
            #[cfg(unix)]
            {
                let leader = child["leader"].as_u64().ok_or("Owned leader missing")?;
                assert!(recorded_tree::empty(
                    &serde_json::json!({"pid":pid,"tree":{"kind":"pgid","group":leader}})
                )?);
            }
            let diagnostic = error.to_string();
            assert!(
                diagnostic.contains("deadline=idle ") || diagnostic.contains("deadline=wall "),
                "Missing timeout observation: deadline"
            );
            for field in [
                "leader_exited=false",
                "stdout_eof=false",
                "stderr_eof=false",
                "elapsed_ms=",
                "idle_ms=",
                "stdout_bytes=",
                "stderr_bytes=",
            ] {
                assert!(
                    diagnostic.contains(field),
                    "Missing timeout observation: {field}"
                );
            }
            assert!(
                !diagnostic.contains("ready"),
                "Timeout exposed captured content"
            );
            std::fs::remove_file(path)?;
        }
    }
    #[cfg(unix)]
    {
        let mut signalled = command("signal", root)?;
        let output = capture::run(&mut signalled, limits(1024))?;
        assert_eq!(output.exit, -libc::SIGTERM);
        assert_eq!(output.stdout, b"{\"ok\":false}\n");
    }
    let ready = root.join("cancel-ready");
    let mut cancelled = command("cancel", root)?;
    let started = std::time::Instant::now();
    let error = capture::run_controlled(&mut cancelled, limits(1024), || ready.is_file())
        .err()
        .ok_or("Cancellation did not interrupt capture")?;
    assert_eq!(error.kind(), std::io::ErrorKind::ConnectionAborted);
    assert!(
        error.retained_record().is_none(),
        "Cancelled tree remained live"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    #[cfg(target_os = "linux")]
    escaped_writer(root)?;
    let mut invalid = command("reject", root)?;
    assert!(
        capture::run(
            &mut invalid,
            Limits {
                wall: Duration::ZERO,
                ..limits(1024)
            }
        )
        .is_err()
    );
    temporary.cleanup()?;
    Ok(())
}
#[cfg(target_os = "linux")]
fn escaped_writer(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;
    let mut command = command("escape", root)?;
    let started = Instant::now();
    let result = capture::run(
        &mut command,
        Limits {
            wall: Duration::from_secs(3),
            idle: Duration::from_secs(2),
            ..limits(1024)
        },
    );
    let elapsed = started.elapsed();
    // The escaped fixture publishes its own birth/group identity before the
    // captured leader exits. Cleanup uses that identity, never a borrowed PID.
    let path = root.join("escaped.json");
    let cleanup = (|| -> Result<(), Box<dyn std::error::Error>> {
        let record: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        assert!(
            recorded_tree::settle(&record, Duration::from_secs(3))?,
            "Escaped fixture cleanup failed"
        );
        assert!(recorded_tree::empty(&record)?);
        Ok(())
    })();
    cleanup?;
    let error = result
        .err()
        .ok_or("Open escaped pipe reported completed output")?;
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    let diagnostic = error.to_string();
    assert!(diagnostic.contains("deadline=idle ") || diagnostic.contains("deadline=wall "));
    for field in [
        "leader_exited=true",
        "stdout_eof=false",
        "stderr_eof=false",
        "elapsed_ms=",
        "idle_ms=",
        "stdout_bytes=",
        "stderr_bytes=",
    ] {
        assert!(
            diagnostic.contains(field),
            "Missing escaped-writer observation: {field}"
        );
    }
    assert!(
        !diagnostic.contains("ready"),
        "Timeout exposed captured content"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "Escaped pipe defeated capture deadline"
    );
    Ok(())
}
