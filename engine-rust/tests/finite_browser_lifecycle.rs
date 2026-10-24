use numera_core::{
    browser::{adapter::Adapter, cookies},
    ownership::{claims, ready::StartupFailed, registry::Registry},
    storage::repo::ProfileRepo,
};
use serde_json::json;
use std::{
    io,
    process::Command,
    sync::{Arc, mpsc},
    time::Duration,
};
mod common;

#[test]
fn shutdown_joins_finite_operations_and_refuses_later_activation()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("finite-browser-lifecycle")?;
    let root = temporary.path();
    let repo = ProfileRepo::new(root.into(), None);
    let fingerprint =
        numera_core::fingerprint::generate(numera_core::fingerprint::GenerateOptions {
            seed: 42,
            ..Default::default()
        })?;
    let profile = numera_core::models::Profile::from_record(
        &json!({"id":"a","name":"Fixture","fingerprint":fingerprint}),
    )?;
    repo.save(profile, 1000.0)?;
    let registry = Arc::new(Registry::new(root.into(), "fixture-host".into(), None)?);
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let (ended_tx, ended_rx) = mpsc::sync_channel(1);
    let active = registry.clone();
    let worker = std::thread::spawn(move || {
        let result = active.with_finite_browser(
            "a",
            "Fixture",
            |session, _| {
                let mut command = Command::new(python);
                command
                    .args([
                        "-u",
                        "-c",
                        r#"
import json,sys
for line in sys.stdin:
    request=json.loads(line)
    op=request['operation']
    result={'cookies':[]} if op=='cookies-read' else {'closing':True} if op=='close' else {}
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close': break
"#,
                    ])
                    .env("PYTHONUTF8", "1");
                Adapter::spawn_admitted(session, &mut command, Duration::from_secs(5)).map_err(
                    |error| StartupFailed {
                        kind: "failed".into(),
                        message: error.to_string(),
                    },
                )
            },
            |adapter| {
                entered_tx.send(()).map_err(io::Error::other)?;
                release_rx
                    .recv_timeout(Duration::from_secs(10))
                    .map_err(io::Error::other)?;
                cookies::read(adapter)
            },
        );
        let _ = ended_tx.send(());
        result
    });
    let mut shutdown = None;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        entered_rx.recv_timeout(Duration::from_secs(5))?;
        assert!(
            !registry.owns("a")?,
            "Finite operation became a launch owner"
        );
        assert!(!matches!(claims::read(root, "a"), claims::Claim::Absent));
        let closing = registry.clone();
        let (closed_tx, closed_rx) = mpsc::sync_channel(1);
        shutdown = Some(std::thread::spawn(move || {
            let failed = closing.shutdown(Duration::from_secs(5));
            let _ = closed_tx.send(failed);
        }));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let probe: io::Result<()> = registry.with_finite_browser(
                "b",
                "Fixture",
                |_, _| {
                    Err(StartupFailed {
                        kind: "failed".into(),
                        message: "probe".into(),
                    })
                },
                |_| Ok(()),
            );
            if probe.is_err_and(|error| error.to_string() == "Session registry is shutting down") {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err("Shutdown did not close finite admission".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            matches!(closed_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "Shutdown did not join the blocked finite operation"
        );
        release_tx.send(())?;
        assert!(closed_rx.recv_timeout(Duration::from_secs(5))?.is_empty());
        ended_rx.recv_timeout(Duration::from_secs(5))?;
        assert!(
            matches!(claims::read(root, "a"), claims::Claim::Absent),
            "Shutdown returned before finite owner settlement"
        );
        let result: io::Result<()> = registry.with_finite_browser(
            "a",
            "Fixture",
            |_, _| panic!("SDK activated after shutdown"),
            |_| Ok(()),
        );
        assert_eq!(
            result
                .expect_err("Closed registry admitted finite operation")
                .to_string(),
            "Session registry is shutting down"
        );
        Ok(())
    })();
    drop(release_tx);
    let operation = worker.join().map_err(|_| "Finite worker panicked")?;
    if let Some(shutdown) = shutdown {
        shutdown.join().map_err(|_| "Shutdown worker panicked")?;
    }
    assert!(registry.shutdown(Duration::from_secs(5)).is_empty());
    operation?;
    result?;
    drop(registry);
    temporary.cleanup()?;
    Ok(())
}
