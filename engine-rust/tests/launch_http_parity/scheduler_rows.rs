use numera_core::{
    browser::runtime::Runtime,
    http::scheduler::Scheduler,
    ownership::{claims, registry::Registry},
    storage::{atomic::write_json, schedules::ScheduleStore},
};
use serde_json::Value;
use std::{fs, sync::Arc, time::Duration};

#[test]
fn skipped_malformed_schedule_ids_do_not_abort_later_occurrences()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = super::common::TempDirectory::new("scheduler-row-isolation")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/skipped_malformed_schedule_ids_do_not_abort_later_occurrences.json"
    ))?;
    assert_eq!(cases.len(), 7, "Scheduler row observations missing");
    for (index, case) in cases.iter().enumerate() {
        assert!(case["error"].is_null(), "Original row failure escaped");
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("dashboard"))?;
        let profile = serde_json::to_vec(&case["profile"])?;
        fs::write(root.join("dashboard/bad.json"), &profile)?;
        write_json(&root.join("schedules.json"), &case["rows"])?;
        let sessions = Arc::new(Registry::new(root.clone(), "fixture-host".into(), None)?);
        let scheduler = Scheduler::new(
            root.clone(),
            None,
            "fixture-host".into(),
            sessions.clone(),
            Runtime {
                adapter: root.join("must-not-start-sdk"),
                cwd: root.clone(),
                home: root.clone(),
                logs: root.join("sdk-logs"),
                country: None,
                relay: None,
                binary_options: Default::default(),
                deployment: None,
            },
        )?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let verdict = scheduler.tick();
            assert!(
                verdict.is_ok(),
                "Scheduler row isolation differs: case={index} verdict={verdict:?}"
            );
            let mut actual = ScheduleStore::new(root.clone()).list()?;
            assert_eq!(actual.len(), 2);
            assert_eq!(actual[0], case["rows"][0], "Skipped schedule changed");
            let timestamp = actual[1]["lastFailedAt"]
                .as_f64()
                .ok_or("Later row was not processed")?;
            assert!(timestamp.is_finite() && timestamp > 1700000000.0);
            assert_eq!(case["stored"][1]["lastFailedAt"], 1000.0);
            // Only the real tick's wall-clock failure timestamp is canonicalized.
            actual[1]["lastFailedAt"] = case["stored"][1]["lastFailedAt"].clone();
            assert_eq!(
                Value::Array(actual),
                case["stored"],
                "Later row bookkeeping differs"
            );
            assert_eq!(fs::read(root.join("dashboard/bad.json"))?, profile);
            assert!(!root.join("profiles/bad.json").exists());
            assert!(
                !root.join("sdk-logs").exists(),
                "Failed conversion activated SDK"
            );
            assert!(!sessions.owns("bad")?);
            assert!(matches!(claims::read(&root, "bad"), claims::Claim::Absent));
            Ok(())
        })();
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        drop(scheduler);
        drop(sessions);
        result?;
    }
    temporary.cleanup()?;
    Ok(())
}
