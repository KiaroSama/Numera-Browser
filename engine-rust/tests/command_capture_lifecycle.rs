use numera_core::ownership::capture::{self, Limits};
#[cfg(windows)]
use numera_core::ownership::identity;
#[cfg(unix)]
use numera_core::ownership::recorded_tree;
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};
mod common;
fn limits(bytes: usize) -> Limits {
    Limits {
        wall: Duration::from_secs(3),
        idle: Duration::from_secs(2),
        bytes,
        cleanup: Duration::from_secs(5),
    }
}
fn command(python: &std::ffi::OsStr, script: &str, root: &Path) -> Command {
    let mut command = Command::new(python);
    command
        .args(["-I", "-S", "-u", "-c", script])
        .env("PYTHONUTF8", "1")
        .env("FIXTURE_ROOT", root);
    command
}
#[test]
fn bounded_capture_drains_both_pipes_and_settles_descendants_on_every_exit()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("command-capture")?;
    let root = temporary.path();
    let mut normal = command(
        &python,
        "import sys; sys.stdout.buffer.write('α 😀'.encode('utf-8')); sys.stderr.write('diagnostic'); raise SystemExit(1)",
        root,
    );
    let output = capture::run(&mut normal, limits(65536))
        .map_err(|error| format!("Unicode/verdict capture: {error}"))?;
    assert_eq!(output.exit, 1);
    assert_eq!(String::from_utf8(output.stdout)?, "α 😀");
    assert_eq!(output.stderr, b"diagnostic");
    let mut pressure = command(
        &python,
        r#"
import os,threading
ready=threading.Barrier(3,timeout=2)
def write(fd,byte):
    ready.wait()
    data=byte*32768
    while data:
        data=data[os.write(fd,data):]
threads=[threading.Thread(target=write,args=(1,b'o')),threading.Thread(target=write,args=(2,b'e'))]
for thread in threads:
    thread.start()
ready.wait()
for thread in threads:
    thread.join(timeout=2)
if any(thread.is_alive() for thread in threads):
    os._exit(2)
"#,
        root,
    );
    let output = capture::run(&mut pressure, limits(65536))
        .map_err(|error| format!("Concurrent stdout/stderr pressure capture: {error}"))?;
    assert_eq!(output.stdout.len(), 32768);
    assert_eq!(output.stderr.len(), 32768);
    assert_eq!(output.exit, 0);
    for (index, script) in [
        "import sys; sys.stdout.buffer.write(b'x'*65536); sys.stdout.flush()",
        r#"
import json,os,pathlib,subprocess,sys,threading
child=subprocess.Popen([sys.executable,'-I','-S','-c','import threading; threading.Event().wait(10)'])
pathlib.Path(os.environ['FIXTURE_ROOT'],'child.json').write_text(json.dumps({'pid':child.pid,'leader':os.getpid()}),encoding='utf-8')
print('ready',flush=True)
threading.Event().wait(10)
"#,
    ].into_iter().enumerate() {
        let mut child = command(&python, script, root);
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
            assert!(path.is_file(), "Timed-out child did not publish its identity");
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
        let mut signalled = command(
            &python,
            "import os,signal,sys; print('{\"ok\":false}',flush=True); os.kill(os.getpid(),signal.SIGTERM)",
            root,
        );
        let output = capture::run(&mut signalled, limits(1024))?;
        assert_eq!(output.exit, -libc::SIGTERM);
        assert_eq!(output.stdout, b"{\"ok\":false}\n");
    }
    let ready = root.join("cancel-ready");
    let mut cancelled = command(
        &python,
        "import os,pathlib,threading; pathlib.Path(os.environ['FIXTURE_ROOT'],'cancel-ready').write_text('ready',encoding='utf-8'); threading.Event().wait(10)",
        root,
    );
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
    escaped_writer(&python, root)?;
    let mut invalid = command(&python, "raise AssertionError('must not spawn')", root);
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
fn escaped_writer(python: &std::ffi::OsStr, root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Instant;
    let mut command = command(
        python,
        r#"
import json,os,pathlib,subprocess,sys,time
code="import json,os,pathlib,threading; fields=pathlib.Path('/proc/self/stat').read_text(encoding='utf-8').rsplit(') ',1)[1].split(); pathlib.Path(os.environ['FIXTURE_ROOT'],'escaped.json').write_text(json.dumps({'pid':os.getpid(),'pidToken':int(fields[19]),'tree':{'kind':'pgid','group':os.getpgrp()}}),encoding='utf-8'); threading.Event().wait(8)"
child=subprocess.Popen([sys.executable,'-I','-S','-c',code],start_new_session=True)
end=time.monotonic()+2
while not pathlib.Path(os.environ['FIXTURE_ROOT'],'escaped.json').exists() and time.monotonic()<end:
    time.sleep(0.01)
print('ready',flush=True)
"#,
        root,
    );
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
