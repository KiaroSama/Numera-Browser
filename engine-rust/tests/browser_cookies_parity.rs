use numera_core as core;
use numera_core::{
    browser::{adapter::Adapter, cookies},
    ownership::claims,
};
use serde_json::Value;
use std::time::Duration;
mod common;
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;

#[test]
fn cookie_write_matches_original_noop_clear_add_and_failure_order()
-> Result<(), Box<dyn std::error::Error>> {
    let document = fixture::oracle("cookie-write")?;
    assert!(
        cookies::write(&[Value::Null], true, || panic!(
            "Invalid cookie activated SDK"
        ))
        .is_err()
    );
    for clear in [false, true] {
        let oversized = serde_json::json!({"name":"fixture", "value":"x".repeat(1_048_576), "domain":".fixture.test"});
        let error = cookies::write(&[oversized], clear, || {
            panic!("Oversized cookie activated or cleared SDK")
        })
        .err()
        .ok_or("Oversized replacement accepted")?;
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
    let cases = document["cases"]
        .as_array()
        .ok_or("Original cases missing")?;
    assert_eq!(cases.len(), 6);
    for case in cases {
        eprintln!("Cookie stage: {}", case["name"]);
        let temporary = common::TempDirectory::new("cookie-write-policy")?;
        let root = temporary.path();
        let capture = root.join("seen.json");
        let jar = case["cookies"].as_array().ok_or("Cookie input missing")?;
        let clear = case["clear"].as_bool().ok_or("Clear input missing")?;
        let mut command = fixture::command(
            root,
            "cookies",
            serde_json::json!({"seen":capture,"failure":case["failure"]}),
        )?;
        let result = cookies::write(jar, clear, || {
            assert!(!jar.is_empty() || clear, "No-op activated a browser");
            Adapter::spawn(
                root,
                "a",
                "fixture-host",
                &mut command,
                Duration::from_secs(5),
            )
        });
        if case["outcome"]["failed"] == true {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(
                result?,
                case["outcome"]["count"].as_u64().ok_or("Count missing")? as usize
            );
        }
        let seen: Value = if capture.exists() {
            serde_json::from_slice(&std::fs::read(capture)?)?
        } else {
            Value::Array(Vec::new())
        };
        assert_eq!(seen, case["expected"], "{}", case["name"]);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        temporary.cleanup()?;
    }
    Ok(())
}
