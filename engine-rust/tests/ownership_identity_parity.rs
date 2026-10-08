use numera_core::ownership::identity::{alive, started_at};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn native_birth_token_matches_original_runtime_for_the_same_live_process()
-> Result<(), Box<dyn std::error::Error>> {
    let pid = std::process::id();
    let token = started_at(pid)?.ok_or("Current process has no native birth identity")?;
    assert!(alive(pid, Some(token))?);
    assert!(
        !alive(pid, Some(token.saturating_add(1)))?,
        "Reused PID identity accepted"
    );
    assert!(!alive(0, None)?);
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Source interpreter required")?;
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/migration/identity_fixture.py");
    let mut child = Command::new(python)
        .arg(fixture)
        .arg(pid.to_string())
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Err("Source birth query timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "Source process identity query failed"
    );
    let source = String::from_utf8(output.stdout)?.trim().parse::<u64>()?;
    assert_eq!(
        token, source,
        "A reused PID must be distinguished by the same birth token"
    );
    Ok(())
}
