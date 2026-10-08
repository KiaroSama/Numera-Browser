use numera_core::storage::schedules::{ScheduleStore, validate_schedule};
use serde_json::Value;
use std::fs;
mod common;

#[test]
fn schedule_user_writes_validate_and_cannot_forge_scheduler_outcomes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/schedule-write.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Write cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        if case["operation"] == "validate" {
            match validate_schedule(&case["input"], case["partial"] == true) {
                Ok(value) => {
                    assert!(case["error"].is_null(), "{}", case["name"]);
                    assert_eq!(Value::Object(value), case["result"], "{}", case["name"]);
                }
                Err(error) => assert_eq!(
                    Some(error.to_string().as_str()),
                    case["error"].as_str(),
                    "{}",
                    case["name"]
                ),
            }
        } else {
            let temporary = common::TempDirectory::new("schedule-write")?;
            let root = temporary.path().to_path_buf();
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                let store = ScheduleStore::new(root.clone());
                let mut created = store.add("fixture-profile", serde_json::json!({"everyHours":0.0001,"minutes":7,"preset":"ADS","enabled":false}), Some(1000.0))?;
                let id = created["id"].as_str().ok_or("Id missing")?.to_owned();
                assert_eq!(id.len(), 12);
                assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
                created["id"] = case["created"]["id"].clone();
                assert_eq!(created, case["created"]);
                let normalized = |mut rows: Vec<Value>| {
                    for row in &mut rows {
                        row["id"] = case["created"]["id"].clone();
                    }
                    Value::Array(rows)
                };
                assert_eq!(normalized(store.list()?), case["before"]);
                let mut updated = store.update(&id, &serde_json::json!({"everyHours":"2","enabled":true,"preset":null,"future":7}))?.ok_or("Updated missing")?;
                updated["id"] = case["created"]["id"].clone();
                assert_eq!(updated, case["updated"]);
                assert_eq!(normalized(store.list()?), case["after"]);
                let before = fs::read(root.join("schedules.json"))?;
                let error = store
                    .update(&id, &serde_json::json!({"lastRun":99}))
                    .err()
                    .ok_or("Forged outcome accepted")?;
                assert_eq!(Some(error.to_string().as_str()), case["error"].as_str());
                assert!(
                    store
                        .update("missing", &serde_json::json!({"minutes":1}))?
                        .is_none()
                );
                assert_eq!(
                    Value::Bool(before == fs::read(root.join("schedules.json"))?),
                    case["unchanged"]
                );
                Ok(())
            })();
            temporary.cleanup()?;
            result?;
        }
    }
    Ok(())
}
