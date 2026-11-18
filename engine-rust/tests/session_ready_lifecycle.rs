use numera_core::ownership::{claims, session::Session};
use std::{fs, process::Stdio, time::Duration};
mod common;
use numera_core as core;
#[allow(dead_code)]
#[path = "common/ownership_process.rs"]
mod fixture;

#[test]
fn readiness_failure_settles_tree_and_success_retains_lease_until_close()
-> Result<(), Box<dyn std::error::Error>> {
    for failure in [false, true] {
        let temporary = common::TempDirectory::new("session-ready-lifecycle")?;
        let root = temporary.path();
        let ready = root.join("ready.json");
        let mut command = fixture::command("ready")?;
        command
            .arg(&ready)
            .arg(if failure { "locked" } else { "ok" })
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut session = Session::admit(root, "fixture-profile", "fixture-host")?;
        session.spawn(&mut command, "readiness fixture")?;
        let result = session.await_ready(&ready, Duration::from_secs(5));
        if failure {
            let error = result.expect_err("Failure was reported ready");
            assert_eq!(error.kind, "locked");
            assert_eq!(error.message, "fixture startup refusal");
            assert!(matches!(
                claims::read(root, "fixture-profile"),
                claims::Claim::Absent
            ));
        } else {
            result?;
            assert_eq!(
                claims::state(&claims::read(root, "fixture-profile"), "fixture-host"),
                claims::State::Busy
            );
            assert!(Session::admit(root, "fixture-profile", "fixture-host").is_err());
        }
        session.finish(Duration::from_secs(5))?;
        drop(session);
        let reopened = Session::admit(root, "fixture-profile", "fixture-host")?;
        drop(reopened);
        assert!(ready.exists());
        fs::remove_file(ready)?;
        temporary.cleanup()?;
    }
    Ok(())
}
