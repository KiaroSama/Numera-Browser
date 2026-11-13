use super::{adapter::Adapter, runtime::Runtime, warmup, warmup_policy};
use crate as core;
use crate::common;
#[allow(dead_code)]
#[path = "../../tests/common/browser_fixture.rs"]
mod fixture;
use crate::{
    cli::warmup_commands::{Options, Warmup},
    ownership::registry::Registry,
    storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{process::Command, time::Duration};

struct Clock(f64, std::path::PathBuf);
impl warmup::Control for Clock {
    fn elapsed(&self) -> f64 {
        self.0
            + std::fs::read_to_string(&self.1)
                .ok()
                .and_then(|value| value.parse::<u32>().ok())
                .map_or(0.0, |value| f64::from(value) * 0.01)
    }
    fn pause(&mut self, seconds: f64) -> std::io::Result<()> {
        self.0 += seconds;
        Ok(())
    }
    fn random(&mut self) -> f64 {
        0.0
    }
    fn below(&mut self, _: usize) -> usize {
        0
    }
    fn cancelled(&mut self) -> bool {
        false
    }
}
#[test]
fn original_warmup_validation_presets_and_native_control_match_without_real_waits()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("warmup-control")?;
    let root = temporary.path();
    let document = fixture::oracle("warmup")?;
    for case in document["policy"]["validation"]
        .as_array()
        .ok_or("Validation cases missing")?
    {
        match warmup_policy::validate(
            &case["minutes"],
            Some(&case["preset"]),
            Some(&case["sites"]),
        ) {
            Ok(value) => {
                assert!(case["error"].is_null());
                assert_eq!(json!(value), case["value"]);
            }
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"].as_str().ok_or("Source error missing")?
            ),
        }
    }
    for (preset, urls) in document["policy"]["presets"]
        .as_object()
        .ok_or("Presets missing")?
    {
        assert_eq!(json!(warmup_policy::sites(Some(preset))), *urls);
    }
    assert_eq!(
        json!(warmup_policy::sites(Some("missing"))),
        document["policy"]["fallback"]
    );
    for (program, suffix) in [
        (warmup::HOST, ")().location_host(argument)"),
        (warmup::LINKS, ")().internal_links(argument)"),
    ] {
        let expression = crate::page_runtime::expression(program);
        assert_eq!(
            expression
                .strip_prefix("(argument) => (")
                .and_then(|body| body.strip_suffix(suffix)),
            Some(crate::page_runtime::artifact()),
            "Warmup must invoke the exact compiled export using the canonical artifact"
        );
    }
    let runs = document["runs"]
        .as_array()
        .ok_or("Warmup run cases missing")?;
    assert_eq!(runs.len(), 2);
    let script = fixture::executable(&root.join("sdk-fixture"), "warmup", json!({}))?;
    for case in runs {
        let ticks = script.with_extension("ticks");
        if ticks.exists() {
            std::fs::remove_file(&ticks)?;
        }
        fixture::executable(&script, "warmup", json!({"failed":case["failed"]}))?;
        let mut command = Command::new(&script);
        let mut sdk = Adapter::spawn(
            root,
            "native",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        let mut clock = Clock(0.0, ticks);
        let result = warmup::run(
            &mut sdk,
            0.001,
            vec!["https://fixture.test/".into()],
            "custom",
            0.0,
            &mut clock,
            |_| Ok(()),
        );
        sdk.finish(Duration::from_secs(5))?;
        drop(sdk);
        assert_eq!(result?, case["summary"]);
        let operations: Value =
            serde_json::from_slice(&std::fs::read(script.with_extension("seen"))?)?;
        assert_eq!(operations, case["operations"]);
        assert_eq!(case["closed"], json!([true]));
        assert!(matches!(
            crate::ownership::claims::read(root, "native"),
            crate::ownership::claims::Claim::Absent
        ));
    }
    let native = root.join("service");
    let repo = ProfileRepo::new(native.clone(), None);
    let fingerprint = crate::fingerprint::generate(crate::fingerprint::GenerateOptions {
        seed: 42,
        ..Default::default()
    })?;
    repo.save(
        crate::models::Profile::from_record(
            &json!({"id":"fixture","name":"Fixture","fingerprint":fingerprint,
        "startup_urls":["https://must-not-navigate.fixture.test/"]}),
        )?,
        1000.0,
    )?;
    let sessions = Registry::new(native.clone(), "fixture-host".into(), None)?;
    fixture::executable(&script, "warmup", json!({"failed":false}))?;
    let browser = Runtime {
        adapter: script.clone(),
        cwd: root.into(),
        home: root.into(),
        logs: root.join("logs"),
        country: None,
        relay: None,
        binary_options: Default::default(),
        registry: None,
        deployment: None,
    };
    let service = Warmup {
        repo: &repo,
        sessions: &sessions,
        runtime: &browser,
    };
    let mut output = crate::cli::output::Output::new(Vec::new(), Vec::new(), false, false);
    let mut clock = Clock(0.0, root.join("unused-ticks"));
    let minutes = json!(0.001);
    let urls = json!(["https://fixture.test/"]);
    let result = service.run_output_controlled(
        Options {
            reference: "Fixture",
            minutes: &minutes,
            preset: None,
            sites: Some(&urls),
            engine: None,
            headless: true,
            time: 1000.0,
        },
        &mut clock,
        &mut output,
    )?;
    assert_eq!(result.exit, 0);
    let mut expected = document["runs"][0]["summary"].clone();
    expected["ok"] = true.into();
    expected["id"] = "fixture".into();
    expected["name"] = "Fixture".into();
    assert_eq!(result.payload, expected);
    let (stdout, stderr) = output.into_parts();
    assert_eq!(
        String::from_utf8(stdout)?,
        document["runs"][0]["human"][0]["stdout"]
            .as_str()
            .ok_or("Source warmup output missing")?
    );
    assert!(stderr.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&std::fs::read(script.with_extension("seen"))?)?,
        document["runs"][0]["operations"]
    );
    assert!(!sessions.owns("fixture")?);
    assert!(matches!(
        crate::ownership::claims::read(&native, "fixture"),
        crate::ownership::claims::Claim::Absent
    ));
    assert_eq!(
        repo.get("fixture")?
            .ok_or("Stored profile missing")?
            .startup_urls,
        vec!["https://must-not-navigate.fixture.test/"]
    );
    let schedules = crate::storage::schedules::ScheduleStore::new(native.clone());
    for case in document["schedules"]
        .as_array()
        .ok_or("Schedule source cases missing")?
    {
        std::fs::write(
            native.join("schedules.json"),
            serde_json::to_vec(&case["rows"])?,
        )?;
        let mut calls = Vec::new();
        let mut output = crate::cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let outcome = service.run_due_with_output(1000.0,||Ok(1000.0),|options| {
            calls.push(json!({"id":options.reference,"minutes":options.minutes,"preset":options.preset,"headless":options.headless}));
            if case["mode"] == "failure" {return Err(crate::storage::repo::RepoError::Message("Controlled warmup failure".into()));}
            Ok(())
        }, &mut output);
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["human"][0]["stdout"]
                .as_str()
                .ok_or("Run-due source output missing")?
        );
        assert!(stderr.is_empty());
        match outcome {
            Ok(outcome) => {
                assert!(case["error"].is_null());
                assert_eq!(outcome.payload, case["payload"]);
                assert_eq!(
                    i64::from(outcome.exit),
                    case["exit"].as_i64().ok_or("Schedule exit missing")?
                );
            }
            Err(error) => assert_eq!(
                error.to_string(),
                case["error"].as_str().ok_or("Schedule error missing")?
            ),
        }
        assert_eq!(json!(calls), case["calls"]);
        assert_eq!(json!(schedules.list()?), case["stored"]);
    }
    assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
    drop(sessions);
    temporary.cleanup()?;
    Ok(())
}
