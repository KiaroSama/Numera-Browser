use super::*;
#[path = "../../tests/common/mod.rs"]
mod common;
use std::sync::{atomic::Ordering, mpsc};

#[test]
fn background_stop_timeout_retains_worker_then_settles_only_its_schedule()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("background-browser")?;
    let root = temporary.path();
    let repo = crate::storage::repo::ProfileRepo::new(root.into(), None);
    let fingerprint = crate::fingerprint::generate(crate::fingerprint::GenerateOptions {
        seed: 42,
        ..Default::default()
    })?;
    repo.save(
        Profile::from_record(&json!({"id":"a","name":"Fixture","fingerprint":fingerprint}))?,
        1000.0,
    )?;
    let schedules = ScheduleStore::new(root.into());
    let first = schedules.add("a", json!({"minutes":1}), Some(1000.0))?;
    let second = schedules.add("a", json!({"minutes":1}), Some(1000.0))?;
    let first_id = first["id"].as_str().ok_or("Schedule ID missing")?;
    let second_id = second["id"].as_str().ok_or("Schedule ID missing")?;
    assert!(schedules.claim_due(first_id, "fixture-occurrence", 1000.0)?);
    let claimed = schedules.get(first_id)?.ok_or("Claimed schedule missing")?;
    let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    assert!(registry.start_background_browser(
        "a",
        "Fixture",
        Kind::ScheduledWarmup,
        Some(first_id.into()),
        |session, _| {
            let mut command = Command::new(&python);
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
            Adapter::spawn_admitted(session, &mut command, Duration::from_secs(5)).map_err(
                |error| StartupFailed {
                    kind: "failed".into(),
                    message: error.to_string(),
                },
            )
        },
        move |_, cancelled| {
            entered_tx.send(()).map_err(io::Error::other)?;
            release_rx
                .recv_timeout(Duration::from_secs(10))
                .map_err(io::Error::other)?;
            assert!(cancelled.load(Ordering::Acquire));
            Ok(())
        }
    )?);
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        entered_rx.recv_timeout(Duration::from_secs(5))?;
        assert!(registry.owns("a")?);
        {
            let decision = registry.decision("a")?;
            let _guard = decision.lock().map_err(|_| poisoned())?;
            let failed = registry.background_result_locked(
                "a",
                Err(StartupFailed {
                    kind: "failed".into(),
                    message: "Controlled startup failure".into(),
                }),
                |owner| owner.stop(Duration::ZERO),
            );
            let failed = failed.expect_err("Cleanup failure reported startup success");
            assert_eq!(failed.kind, "unsettled");
            crate::http::scheduler::release_failed(
                &schedules,
                first_id,
                true,
                failed.kind == "unsettled",
            );
            assert_eq!(
                schedules
                    .get(first_id)?
                    .ok_or("Retained occurrence missing")?,
                claimed
            );
            assert!(registry.owns("a")?);
        }
        assert!(registry.stop("a", Duration::ZERO).is_err());
        assert!(registry.owns("a")?, "Timeout lost the worker owner");
        assert!(registry.running("a")?);
        assert!(claims::state(&claims::read(root, "a"), "fixture-host") != claims::State::Free);
        release_tx.send(())?;
        assert!(registry.stop("a", Duration::from_secs(5))?);
        assert!(!registry.owns("a")?);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        assert_eq!(
            schedules.get(first_id)?.ok_or("First schedule missing")?["lastOutcome"],
            "failed"
        );
        assert_eq!(
            schedules.get(second_id)?.ok_or("Second schedule missing")?,
            second
        );
        Ok(())
    })();
    let _ = release_tx.try_send(());
    let failed = registry.shutdown(Duration::from_secs(5));
    assert!(failed.is_empty(), "Background shutdown failed: {failed:?}");
    drop(registry);
    result?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn scheduled_commit_failure_settles_same_owner_under_its_admission_decision()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("background-commit")?;
    let root = temporary.path();
    let repo = crate::storage::repo::ProfileRepo::new(root.into(), None);
    let fingerprint = crate::fingerprint::generate(crate::fingerprint::GenerateOptions {
        seed: 42,
        ..Default::default()
    })?;
    repo.save(
        Profile::from_record(&json!({"id":"a","name":"Fixture","fingerprint":fingerprint}))?,
        1000.0,
    )?;
    let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let mut committed = false;
    let result = registry.start_background_checked(
        "a",
        "Fixture",
        Kind::ScheduledWarmup,
        Some("schedule".into()),
        |session, _| {
            let mut command = Command::new(&python);
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
            Adapter::spawn_admitted(session, &mut command, Duration::from_secs(5)).map_err(
                |error| StartupFailed {
                    kind: "failed".into(),
                    message: error.to_string(),
                },
            )
        },
        move |_, cancelled| {
            release_rx
                .recv_timeout(Duration::from_secs(5))
                .map_err(io::Error::other)?;
            let end = std::time::Instant::now() + Duration::from_secs(5);
            while !cancelled.load(Ordering::Acquire) {
                if std::time::Instant::now() >= end {
                    return Err(io::Error::other("Commit cleanup did not cancel worker"));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        },
        || {
            let decision = registry.decision("a")?;
            assert!(matches!(
                decision.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            committed = true;
            release_tx.send(()).map_err(io::Error::other)?;
            Err(io::Error::other("Controlled schedule write failure"))
        },
    );
    let _ = release_tx.try_send(());
    let failed = registry.shutdown(Duration::from_secs(5));
    assert!(
        failed.is_empty(),
        "Commit failure cleanup remained owned: {failed:?}"
    );
    assert!(committed);
    let error = result.expect_err("Failed schedule commit reported success");
    assert_eq!(error.kind, "failed");
    assert_eq!(error.message, "Controlled schedule write failure");
    assert!(!registry.owns("a")?);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    drop(registry);
    temporary.cleanup()?;
    Ok(())
}
