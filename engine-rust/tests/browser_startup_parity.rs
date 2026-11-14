use numera_core as core;
use numera_core::{
    browser::{adapter::Adapter, startup},
    ownership::claims,
};
use serde_json::Value;
use std::time::Duration;
mod common;
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;

#[test]
fn startup_navigation_preserves_original_url_order_tabs_and_per_url_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-startup.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Startup cases missing")?;
    assert_eq!(cases.len(), 4);
    for case in cases {
        let temporary = common::TempDirectory::new("startup-navigation")?;
        let root = temporary.path();
        let capture = root.join("seen.json");
        std::fs::write(&capture, b"[]")?;
        let mut command = fixture::command(
            root,
            "startup",
            serde_json::json!({"seen":capture,"failed":case["failed"]}),
        )?;
        let mut adapter = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        let urls: Vec<String> = serde_json::from_value(case["urls"].clone())?;
        startup::navigate(&mut adapter, &urls)?;
        adapter.finish(Duration::from_secs(5))?;
        drop(adapter);
        let seen: Value = serde_json::from_slice(&std::fs::read(capture)?)?;
        assert_eq!(seen, case["expected"], "{}", case["name"]);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        temporary.cleanup()?;
    }
    Ok(())
}
