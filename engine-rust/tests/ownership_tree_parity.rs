use numera_core::ownership::{claims, session::Session};
use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
    time::Instant,
};

fn source_state(
    python: &std::ffi::OsStr,
    root: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/migration/claim_fixture.py");
    let mut child = Command::new(python)
        .arg(fixture)
        .arg(root)
        .arg("fixture-host")
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        loop {
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("Source claim query timed out".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
    })();
    if let Err(error) = result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let output = child.wait_with_output()?;
    assert!(output.status.success(), "Source claim query failed");
    let record: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    Ok(record["state"]
        .as_str()
        .ok_or("Source claim verdict missing")?
        .into())
}

#[test]
fn native_owned_tree_contains_descendants_before_their_first_capability()
-> Result<(), Box<dyn std::error::Error>> {
    let python =
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Source fixture interpreter required")?;
    let script = "import subprocess,sys,threading; p=subprocess.Popen([sys.executable,'-c','import threading; threading.Event().wait()']); print('ready',flush=True); threading.Event().wait()";
    let mut command = Command::new(&python);
    command
        .args(["-u", "-c", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!("claim-{}", std::process::id()));
    std::fs::create_dir_all(&root)?;
    let mut session_owner = Session::admit(&root, "fixture-profile", "fixture-host")?;
    assert!(
        Session::admit(&root, "fixture-profile", "fixture-host").is_err(),
        "Second opener ignored the lease before helper birth"
    );
    let tree = session_owner.spawn(&mut command, "test session")?;
    let output = tree.child_mut().stdout.take().ok_or("No startup output")?;
    let (send, recv) = mpsc::channel();
    let reader = thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(output).read_line(&mut line).map(|_| line);
        let _ = send.send(result);
    });
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(recv.recv_timeout(Duration::from_secs(10))??.trim(), "ready");
        assert!(!tree.empty()?, "A running tree was called empty");
        let claims::Claim::Record(record) = claims::read(&root, "fixture-profile") else {
            return Err("No durable session claim".into());
        };
        let session = record["session"]
            .as_str()
            .ok_or("No claim session identity")?;
        assert_eq!(
            claims::state(&claims::read(&root, "fixture-profile"), "fixture-host"),
            claims::State::Busy
        );
        assert_eq!(
            source_state(&python, &root)?,
            "busy",
            "Python ignored the native named tree"
        );
        assert!(
            !claims::release(&root, "fixture-profile", "another-session", tree)?,
            "Unrelated session released its claim"
        );
        assert!(
            !claims::clear(&root, "fixture-profile", "fixture-host")?,
            "Live tree cleared its claim"
        );
        assert!(
            !claims::release(&root, "fixture-profile", session, tree)?,
            "Live tree released its claim"
        );
        tree.settle(Duration::from_secs(5))?;
        assert!(tree.empty()?, "Descendant survived the native tree cleanup");
        assert_eq!(
            source_state(&python, &root)?,
            "free",
            "Python could not recover the settled native claim"
        );
        Ok(())
    })();
    let cleanup = session_owner.finish(Duration::from_secs(5));
    reader.join().map_err(|_| "Output reader panicked")?;
    cleanup?;
    assert!(matches!(
        claims::read(&root, "fixture-profile"),
        claims::Claim::Absent
    ));
    drop(session_owner);
    let reopened = Session::admit(&root, "fixture-profile", "fixture-host")?;
    drop(reopened);
    std::fs::remove_dir_all(root)?;
    result
}
