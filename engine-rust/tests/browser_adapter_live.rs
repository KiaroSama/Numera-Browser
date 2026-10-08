use numera_core::{browser::adapter::Adapter, ownership::claims};
use serde_json::{Map, json};
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
#[ignore = "requires explicitly selected SDK and installed browser in the hosted live tier"]
fn selected_sdk_opens_real_persistent_context_in_native_owned_tree()
-> Result<(), Box<dyn std::error::Error>> {
    let backend = std::env::var("NUMERA_NATIVE_LIVE_ENGINE")
        .map_err(|_| "Selected live backend is required")?;
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let temporary = common::TempDirectory::new("native-browser-live")?;
    let root = temporary.path();
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?
        .join("scripts/browser-adapter/adapter.py");
    let mut command = Command::new(python);
    command
        .arg("-u")
        .arg(script)
        .env("NUMERA_ADAPTER_LOG_DIR", root.join("logs"))
        .env("PYTHONUTF8", "1");
    let mut adapter = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(10),
    )?;
    let options = if backend == "camoufox" {
        json!({"user_data_dir":root.join("user-data/a").to_str().ok_or("Profile path not Unicode")?,"headless":true,"os":"windows","locale":"en-US"})
    } else if backend == "cloak" {
        json!({"user_data_dir":root.join("user-data/a").to_str().ok_or("Profile path not Unicode")?,"headless":true,"viewport":{"width":900,"height":650},"locale":"en-US","timezone":"America/New_York"})
    } else {
        json!({"user_data_dir":root.join("user-data/a").to_str().ok_or("Profile path not Unicode")?,"headless":true,"channel":"chromium","viewport":{"width":900,"height":650},"locale":"en-US","timezone_id":"America/New_York"})
    };
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        adapter.open(
            &backend,
            options.as_object().cloned().ok_or("Options missing")?,
            Vec::new(),
            Duration::from_secs(90),
        )?;
        assert_eq!(
            claims::state(&claims::read(root, "a"), "fixture-host"),
            claims::State::Busy
        );
        let view = adapter.call("inspect", Map::new(), Duration::from_secs(10))?;
        assert!(
            view["userAgent"]
                .as_str()
                .is_some_and(|agent| agent.starts_with("Mozilla/5.0"))
        );
        assert!(view["cores"].as_u64().is_some_and(|cores| cores > 0));
        assert!(root.join("user-data/a").is_dir());
        assert_eq!(
            adapter.call("close", Map::new(), Duration::from_secs(10))?["closing"],
            true
        );
        Ok(())
    })();
    let cleanup = adapter.finish(Duration::from_secs(10));
    drop(adapter);
    if result.is_err() || cleanup.is_err() {
        for entry in std::fs::read_dir(root.join("logs"))? {
            let path = entry?.path();
            if std::fs::metadata(&path)?.len() <= 65_536 {
                eprintln!(
                    "Sanitized adapter diagnostics:\n{}",
                    std::fs::read_to_string(path)?
                );
            }
        }
    }
    cleanup?;
    result?;
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    let log = std::fs::read_dir(root.join("logs"))?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(log.len(), 1);
    let content = std::fs::read_to_string(log[0].path())?;
    assert!(content.contains("Persistent context opened"));
    assert!(content.contains("Finished exit=0"));
    temporary.cleanup()?;
    Ok(())
}
