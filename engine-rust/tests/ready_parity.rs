use numera_core::ownership::ready::await_ready;
use serde_json::{Value, json};
use std::{fs, time::Duration};
mod common;

#[test]
fn readiness_preserves_original_startup_failure_kinds_and_exit_precedence()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/ready.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Readiness cases missing")?;
    assert_eq!(cases.len(), 9);
    for case in cases {
        let temporary = common::TempDirectory::new("session-ready")?;
        let path = temporary.path().join("ready.json");
        if !case["payload"].is_null() {
            let bytes = match case["payload"].as_str() {
                Some(text) => text.as_bytes().to_vec(),
                None => serde_json::to_vec(&case["payload"])?,
            };
            fs::write(&path, bytes)?;
        }
        let code = case["exit"].as_i64().map(i32::try_from).transpose()?;
        let actual = match await_ready(&path, Duration::ZERO, || Ok(code)) {
            Ok(()) => json!({"ok":true}),
            Err(error) => json!({"ok":false,"kind":error.kind,"error":error.message}),
        };
        assert_eq!(actual, case["expected"], "{}", case["name"]);
        temporary.cleanup()?;
    }
    Ok(())
}
