use numera_core::{
    browser::{adapter::Adapter, admission, options},
    identity_script,
    models::{Fingerprint, Profile},
    ownership::claims,
};
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
    let fingerprint_document: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let fingerprint = Fingerprint::from_record(&fingerprint_document["cases"][0]["input"])?;
    let profile = Profile::from_record(
        &json!({"id":"a", "name":"fixture", "engine":backend, "created_at":1000.0, "updated_at":1000.0, "fingerprint":fingerprint_document["cases"][0]["input"].clone()}),
    )?;
    std::fs::create_dir_all(root.join("profiles"))?;
    std::fs::write(root.join("profiles/a.json"), serde_json::to_vec(&profile)?)?;
    let (session, profile) =
        admission::current(root, "a", "fixture", "fixture-host", None, 1000.0)?;
    let mut adapter = Adapter::spawn_admitted(session, &mut command, Duration::from_secs(10))?;
    let options = options::build(
        &profile,
        &backend,
        root.join("user-data/a")
            .to_str()
            .ok_or("Profile path not Unicode")?,
        true,
        &[],
        &[],
        None,
    )?;
    let uses_page_script = matches!(backend.as_str(), "playwright" | "patchright");
    let scripts = if uses_page_script {
        vec![identity_script::build(&fingerprint)?]
    } else {
        Vec::new()
    };
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        adapter.open(
            &backend,
            options.as_object().cloned().ok_or("Options missing")?,
            scripts,
            Duration::from_secs(90),
        )?;
        assert_eq!(
            claims::state(&claims::read(root, "a"), "fixture-host"),
            claims::State::Busy
        );
        assert_eq!(
            adapter.call(
                "navigate",
                Map::from_iter([("url".into(), "about:blank".into())]),
                Duration::from_secs(35)
            )?["navigated"],
            true
        );
        let view = adapter.call("inspect", Map::new(), Duration::from_secs(10))?;
        if uses_page_script {
            assert_eq!(view["platform"], fingerprint.platform);
            assert_eq!(view["cores"], fingerprint.hardware_concurrency);
            assert_eq!(view["languages"], json!(fingerprint.languages));
            assert_eq!(view["userAgent"], fingerprint.user_agent);
            assert_eq!(view["width"], fingerprint.viewport_width);
            assert_eq!(view["height"], fingerprint.viewport_height);
        }
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
