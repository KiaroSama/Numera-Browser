use numera_core::{
    browser::runtime::Runtime,
    cli::{self, application::Application},
    fingerprint::{GenerateOptions, generate},
    models::Profile,
    ownership::{claims, registry::Registry},
    storage::repo::ProfileRepo,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
#[path = "support/attach_output.rs"]
mod attach_output;
mod common;
use numera_core as core;
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;

#[test]
fn foreground_launch_reuses_admission_and_native_backend_lifetime_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("cli-launch")?;
    let document = fixture::oracle("launch-cli")?;
    let root = temporary.path().join("native");
    let repo = ProfileRepo::open(root.clone(), None, None)?;
    repo.save(Profile::from_record(&json!({"id":"fixture","name":"Fixture","fingerprint":generate(GenerateOptions{seed:42,..Default::default()})?,"created_at":1000.0,"updated_at":1000.0}))?,1000.0)?;
    let script = fixture::executable(&root.join("sdk"), "cli", json!({}))?;
    let sessions = Registry::new(root.clone(), "fixture-host".into(), None)?;
    let browser = Runtime {
        adapter: script,
        cwd: root.clone(),
        home: root.clone(),
        logs: root.join("logs"),
        country: None,
        relay: None,
        binary_options: Default::default(),
        registry: None,
        deployment: None,
    };
    let installed = BTreeMap::new();
    let application = Application {
        root: &root,
        home: &root,
        repo: &repo,
        installed: &installed,
        host: "fixture-host",
        password: None,
        environment_token: false,
        sessions: &sessions,
        browser: Some(&browser),
    };
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in document["cases"]
        .as_array()
        .ok_or("Launch cases missing")?
        .iter()
        .enumerate()
    {
        let matches = registry.clone().try_get_matches_from(
            std::iter::once("numera").chain(
                case["argv"]
                    .as_array()
                    .ok_or("Launch argv missing")?
                    .iter()
                    .map(|v| v.as_str().expect("Source argument text")),
            ),
        )?;
        let values = cli::reflection::values(&registry, &matches);
        let seen = root.join("sdk.seen");
        if seen.exists() {
            std::fs::remove_file(&seen)?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let outcome = application.execute_with_output(
            values.as_object().ok_or("Launch args missing")?,
            1000.0,
            |_| panic!("Launch fixture prompted"),
            || {
                // Cancellation follows observed SDK readiness, never a blind delay.
                seen.exists() || Instant::now() >= deadline
            },
            || panic!("Launch fixture read stdin"),
            &mut output,
        )?;
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            document["human"][index]["stdout"]
                .as_str()
                .ok_or("Launch source bytes missing")?
        );
        assert!(stderr.is_empty());
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Launch source exit missing")? as i32
        );
        assert_eq!(outcome.payload, case["payload"]);
        if outcome.exit == 0 {
            let policy: Value = serde_json::from_slice(&std::fs::read(root.join("sdk.policy"))?)?;
            assert_eq!(policy["backend"], outcome.payload["engine"]);
            assert_eq!(
                policy["first"],
                if outcome.payload["engine"] == "camoufox" {
                    "0"
                } else {
                    "1"
                }
            );
            let calls: Value = serde_json::from_slice(&std::fs::read(&seen)?)?;
            assert_eq!(calls, json!(["hello", "open", "close"]));
        } else {
            assert!(!seen.exists(), "Missing profile activated SDK");
        }
        assert!(matches!(
            claims::read(&root, "fixture"),
            claims::Claim::Absent
        ));
    }
    assert_eq!(
        numera_core::network::devtools::snippets(
            17777,
            "ws://127.0.0.1:17777/devtools/browser/fixture"
        ),
        document["attach"]["snippets"]
            .as_str()
            .ok_or("Source snippets missing")?
    );
    for case in document["attach"]["headings"]
        .as_array()
        .ok_or("Attach heading observations missing")?
    {
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        cli::attach_commands::ready(&mut output, &case["info"], 17777)?;
        output.say("\nClosing.", None)?;
        let (stdout, stderr) = output.into_parts();
        assert_eq!(
            String::from_utf8(stdout)?,
            case["stdout"]
                .as_str()
                .ok_or("Attach heading source bytes missing")?
        );
        assert!(stderr.is_empty());
    }
    let attach_script = fixture::executable(&root.join("attach-sdk"), "attach", json!({}))?;
    let browser = Runtime {
        adapter: attach_script.clone(),
        ..browser.clone()
    };
    let application = Application {
        browser: Some(&browser),
        ..application
    };
    let attach = document["attach"]["cases"]
        .as_array()
        .ok_or("Attach cases missing")?;
    assert_eq!(attach.len(), 2);
    for case in attach {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        drop(listener);
        let mut argv = case["argv"]
            .as_array()
            .ok_or("Attach argv missing")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .ok_or("Attach argument not text")
                    .map(str::to_owned)
            })
            .collect::<Result<Vec<_>, _>>()?;
        argv[3] = port.to_string();
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera".to_owned()).chain(argv))?;
        let args = cli::reflection::values(&registry, &matches);
        let published = std::cell::RefCell::new(None);
        let outcome = application.execute_streaming(
            args.as_object().ok_or("Attach arguments missing")?,
            1000.0,
            |_| panic!("Attach confirmation"),
            || published.borrow().is_some(),
            (
                || panic!("Attach stdin"),
                |value: &Value| {
                    assert!(
                        claims::state(&claims::read(&root, "fixture"), "fixture-host")
                            != claims::State::Free
                    );
                    assert!(attach_script.with_extension("open").exists());
                    *published.borrow_mut() = Some(value.clone());
                    Ok(())
                },
            ),
        )?;
        let mut expected = case["payload"].clone();
        if expected["ok"] == true {
            expected["port"] = port.into();
            expected["wsUrl"] = format!("ws://127.0.0.1:{port}/devtools/browser/fixture").into();
            assert_eq!(published.into_inner(), Some(expected.clone()));
            let open: Value =
                serde_json::from_slice(&std::fs::read(attach_script.with_extension("open"))?)?;
            assert!(
                open["options"]["args"]
                    .as_array()
                    .ok_or("Attach options missing")?
                    .contains(&json!(format!("--remote-debugging-port={port}")))
            );
            assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
        } else {
            assert!(published.into_inner().is_none());
        }
        assert_eq!(outcome.payload, expected);
        assert_eq!(
            outcome.exit,
            case["exit"].as_i64().ok_or("Attach exit missing")? as i32
        );
        assert!(matches!(
            claims::read(&root, "fixture"),
            claims::Claim::Absent
        ));
    }
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let natural_script =
        fixture::executable(&root.join("natural-sdk"), "attach", json!({"natural":true}))?;
    let browser = Runtime {
        adapter: natural_script.clone(),
        ..browser.clone()
    };
    let application = Application {
        browser: Some(&browser),
        ..application
    };
    let matches = registry.clone().try_get_matches_from([
        "numera",
        "attach",
        "Fixture",
        "--headless",
        "--port",
        &port.to_string(),
        "--json",
    ])?;
    let args = cli::reflection::values(&registry, &matches);
    let bytes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = attach_output::Published {
        bytes: bytes.clone(),
        root: root.clone(),
        release: natural_script.with_extension("release"),
        ready: natural_script.with_extension("open"),
    };
    let mut output = cli::output::Output::new(writer, Vec::new(), true, false);
    let outcome = application.execute_with_output(
        args.as_object().ok_or("Natural attach arguments missing")?,
        1000.0,
        |_| panic!("Natural attach confirmation"),
        || false,
        || panic!("Natural attach stdin"),
        &mut output,
    )?;
    assert!(output.emitted());
    let published: Value = serde_json::from_slice(
        bytes
            .lock()
            .map_err(|_| "Attach output lock poisoned")?
            .as_slice(),
    )?;
    assert_eq!(published, outcome.payload);
    assert!(output.emit(&outcome.payload, outcome.exit).is_err());
    drop(output);
    assert_eq!(
        outcome.exit, 0,
        "Natural attachment close became a false failure"
    );
    assert!(matches!(
        claims::read(&root, "fixture"),
        claims::Claim::Absent
    ));
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_err());
    drop(sessions);
    temporary.cleanup()?;
    Ok(())
}
