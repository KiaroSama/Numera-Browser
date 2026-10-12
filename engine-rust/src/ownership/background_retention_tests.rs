use super::*;
use crate::ownership::{claims, session::Session};
#[path = "../../tests/common/mod.rs"]
mod common;

#[test]
fn quarantine_keeps_actual_worker_and_adapter_until_explicit_retry()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("background-retention")?;
    let root = temporary.path();
    let mut command = std::process::Command::new(python);
    command
        .args([
            "-u",
            "-c",
            r#"
import json,sys
for line in sys.stdin:
    request=json.loads(line)
    result={'closing':True} if request['operation']=='close' else {}
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if request['operation']=='close': break
"#,
        ])
        .env("PYTHONUTF8", "1");
    let mut adapter = Some(Adapter::spawn_admitted(
        Session::admit(root, "retained", "fixture-host")?,
        &mut command,
        Duration::from_secs(5),
    )?);
    let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
    let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
    let mut owner = Background::start(&mut adapter, move |_, _| {
        entered_tx.send(()).map_err(io::Error::other)?;
        release_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(io::Error::other)?;
        Ok(())
    })?;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), Box<dyn std::error::Error>> {
            entered_rx.recv_timeout(Duration::from_secs(5))?;
            assert!(owner.finish(Duration::ZERO).is_err());
            owner.retain();
            assert!(owner.worker.is_none());
            assert!(owner.pending.is_none());
            assert!(
                claims::state(&claims::read(root, "retained"), "fixture-host")
                    != claims::State::Free
            );
            assert!(!retry_retained(Duration::ZERO).is_empty());
            assert!(
                claims::state(&claims::read(root, "retained"), "fixture-host")
                    != claims::State::Free
            );
            release_tx.send(())?;
            assert!(retry_retained(Duration::from_secs(5)).is_empty());
            assert!(matches!(
                claims::read(root, "retained"),
                claims::Claim::Absent
            ));
            Ok(())
        },
    ));
    let _ = release_tx.try_send(());
    let remaining = retry_retained(Duration::from_secs(5));
    assert!(
        remaining.is_empty(),
        "Retained owner cleanup failed: {remaining:?}"
    );
    owner.finish(Duration::from_secs(5))?;
    drop(owner);
    match result {
        Ok(result) => result?,
        Err(panic) => std::panic::resume_unwind(panic),
    };
    temporary.cleanup()?;
    Ok(())
}
