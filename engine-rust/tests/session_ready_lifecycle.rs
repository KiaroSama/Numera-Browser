use numera_core::ownership::{claims, session::Session};
use std::{
    fs,
    process::{Command, Stdio},
    time::Duration,
};
mod common;

#[test]
fn readiness_failure_settles_tree_and_success_retains_lease_until_close()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    for failure in [false, true] {
        let temporary = common::TempDirectory::new("session-ready-lifecycle")?;
        let root = temporary.path();
        let ready = root.join("ready.json");
        let script = "import json,os,pathlib,threading; p=pathlib.Path(os.environ['FIXTURE_READY']); tmp=p.with_suffix('.part'); tmp.write_text(json.dumps({'ok':os.environ['FIXTURE_FAILURE']=='false','kind':'locked','error':'fixture startup refusal'}),encoding='utf-8'); tmp.replace(p); threading.Event().wait()";
        let mut command = Command::new(&python);
        command
            .args(["-u", "-c", script])
            .env("FIXTURE_READY", &ready)
            .env("FIXTURE_FAILURE", failure.to_string())
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
