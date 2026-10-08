use numera_core::storage::schedules::{ScheduleStore, due_schedules, next_run_at};
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn schedule_claim_is_single_occurrence_and_record_clears_only_its_claim()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/schedule-store.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Schedule cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let temporary = common::TempDirectory::new("schedule-store")?;
        let root = temporary.path().to_path_buf();
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let path = root.join("schedules.json");
            let before = serde_json::to_vec(&case["initial"])?;
            fs::write(&path, &before)?;
            let store = ScheduleStore::new(root.clone());
            let now = case["now"].as_f64().ok_or("Now missing")?;
            let rows = store.list()?;
            assert_eq!(
                serde_json::to_value(due_schedules(&rows, now)?)?,
                case["due"]
            );
            assert_eq!(serde_json::to_value(next_run_at(&rows[0])?)?, case["next"]);
            assert_eq!(
                Value::Bool(fs::read(&path)? == before),
                case["read_unchanged"]
            );
            assert_eq!(
                Value::Bool(store.claim_due("fixture-schedule", "fixture-worker", now)?),
                case["claimed"]
            );
            assert_eq!(serde_json::to_value(store.list()?)?, case["after_claim"]);
            assert_eq!(
                Value::Bool(store.claim_due("fixture-schedule", "second-worker", now + 1.0)?),
                case["again"]
            );
            store.release("fixture-schedule")?;
            assert_eq!(
                store.get("fixture-schedule")?,
                Some(case["released"].clone())
            );
            let outcome = serde_json::json!({"lastRun":now,"lastOutcome":"done","lastError":null});
            assert_eq!(
                store.record(
                    "fixture-schedule",
                    outcome.as_object().ok_or("Outcome missing")?.clone()
                )?,
                Some(case["outcome"].clone())
            );
            assert_eq!(serde_json::to_value(store.list()?)?, case["after_record"]);
            let error = store
                .record(
                    "fixture-schedule",
                    serde_json::json!({"minutes":1})
                        .as_object()
                        .ok_or("Patch missing")?
                        .clone(),
                )
                .err()
                .ok_or("Invalid outcome accepted")?;
            assert_eq!(
                Some(error.to_string().as_str()),
                case["owned_error"].as_str()
            );
            assert!(store.get("missing")?.is_none());
            assert_eq!(
                Value::Bool(store.delete("fixture-schedule")?),
                case["deleted"]
            );
            assert_eq!(
                Value::Bool(store.delete("fixture-schedule")?),
                case["deleted_again"]
            );
            assert_eq!(serde_json::to_value(store.list()?)?, case["final"]);
            Ok(())
        })();
        temporary.cleanup()?;
        result?;
    }
    Ok(())
}
