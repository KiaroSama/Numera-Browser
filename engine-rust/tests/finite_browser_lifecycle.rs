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
fn adapter_lifecycle_timings_cover_the_same_pipe_and_settlement_without_public_output()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("browser-timing")?;
    for mode in ["logged", "unavailable", "disabled", "invalid-frame"] {
        let root = temporary.path().join(mode);
        std::fs::create_dir(&root)?;
        let logs = root.join("logs with spaces");
        if mode == "unavailable" {
            std::fs::write(&logs, b"retained")?;
        }
        let mut command = Command::new(&python);
        command
            .args([
                "-I",
                "-S",
                "-u",
                "-c",
                r#"
import json,sys
for line in sys.stdin:
    request=json.loads(line)
    op=request['operation']
    if sys.argv[1]=='invalid-frame' and op=='open':
        print('not-json',flush=True)
        break
    result=({'backends':{'playwright':{'installed':True}}} if op=='hello' else
            {'opened':True,'backend':'playwright'} if op=='open' else
            {'cookies':[]} if op=='cookies-read' else {'closing':True})
    print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':result}),flush=True)
    if op=='close': break
"#,
            ])
            .arg(mode);
        if mode == "disabled" {
            command.env_remove("NUMERA_ADAPTER_LOG_DIR");
        } else {
            command.env("NUMERA_ADAPTER_LOG_DIR", &logs);
        }
        let mut adapter = Adapter::spawn(
            &root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        let opened = adapter.open(
            "playwright",
            serde_json::Map::new(),
            vec![],
            Duration::from_secs(5),
        );
        if mode == "invalid-frame" {
            assert_eq!(
                opened.expect_err("Malformed frame accepted").kind(),
                io::ErrorKind::InvalidData
            );
            adapter.finish(Duration::from_secs(5))?;
            assert!(matches!(claims::read(&root, "a"), claims::Claim::Absent));
            let path = std::fs::read_dir(&logs)?
                .next()
                .ok_or("Failed lifecycle log missing")??
                .path();
            let text = std::fs::read_to_string(path)?;
            for phase in [
                "browser_spawn",
                "browser_hello",
                "browser_open",
                "browser_settle",
            ] {
                assert!(text.contains(&format!("phase={phase} ")));
            }
            assert!(!text.contains("phase=browser_close "));
            continue;
        }
        opened?;
        assert!(cookies::read(&mut adapter)?.is_empty());
        assert_eq!(
            adapter.call("close", serde_json::Map::new(), Duration::from_secs(5))?,
            json!({"closing":true})
        );
        assert!(!matches!(claims::read(&root, "a"), claims::Claim::Absent));
        if mode == "logged" {
            let path = std::fs::read_dir(&logs)?
                .next()
                .ok_or("Lifecycle log missing")??
                .path();
            let text = std::fs::read_to_string(path)?;
            assert!(text.contains("phase=browser_close "));
            assert!(
                !text.contains("phase=browser_settle "),
                "Cleanup timing ended before settlement"
            );
        }
        adapter.finish(Duration::from_secs(5))?;
        assert_eq!(adapter.settled_exit()?, None);
        assert!(matches!(claims::read(&root, "a"), claims::Claim::Absent));
        drop(adapter);
        if mode == "logged" {
            let paths = std::fs::read_dir(&logs)?.collect::<Result<Vec<_>, _>>()?;
            assert_eq!(paths.len(), 1, "Lifecycle logger duplicated or missing");
            let text = std::fs::read_to_string(paths[0].path())?;
            let phases = text
                .lines()
                .map(|line| {
                    let event = line
                        .split_once("[INFO] [entry] Native timing phase=")
                        .ok_or("Unsafe lifecycle event")?
                        .1;
                    let (phase, elapsed) =
                        event.split_once(" elapsed_ns=").ok_or("Invalid timing")?;
                    elapsed.parse::<u64>()?;
                    Ok(phase.to_owned())
                })
                .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
            assert_eq!(
                phases,
                [
                    "browser_spawn",
                    "browser_hello",
                    "browser_open",
                    "browser_cookies_read",
                    "browser_close",
                    "browser_settle"
                ]
            );
            assert!(!text.contains("fixture-host") && !text.contains("playwright"));
        } else if mode == "unavailable" {
            assert_eq!(std::fs::read(&logs)?, b"retained");
        } else {
            assert!(!logs.exists());
        }
    }
    temporary.cleanup()?;
    Ok(())
}

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
