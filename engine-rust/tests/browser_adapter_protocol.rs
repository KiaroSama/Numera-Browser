use numera_core as core;
use numera_core::{
    browser::adapter::Adapter,
    models::Profile,
    ownership::{claims, ready::StartupFailed, registry::Registry},
};
use serde_json::{Map, Value, json};
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
#[path = "common/browser_logs.rs"]
mod browser_logs;
#[cfg(feature = "native-browser")]
#[path = "common/cloak.rs"]
mod cloak;
mod common;

fn adapter_logs(root: &Path) -> std::io::Result<String> {
    let mut text = String::new();
    for entry in std::fs::read_dir(root.join("logs"))? {
        text.push_str(&std::fs::read_to_string(entry?.path())?);
    }
    Ok(text)
}

fn adapter(logs: &Path) -> std::io::Result<Command> {
    let native = std::env::var_os("NUMERA_BROWSER_RUNTIME")
        .or_else(|| option_env!("CARGO_BIN_EXE_numera-browser-runtime").map(Into::into))
        .ok_or_else(|| std::io::Error::other("Native Rust SDK runtime required"))?;
    let mut command = Command::new(native);
    command.env("NUMERA_ADAPTER_LOG_DIR", logs);
    Ok(command)
}
#[test]
fn actual_sdk_adapter_stays_inert_before_open_and_cleans_protocol_failure()
-> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "native-browser")]
    {
        cloak::verify()?;
        let document: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/sdk-evaluation-values.json"))?;
        let cases = document["cases"]
            .as_array()
            .ok_or("SDK value cases missing")?;
        assert_eq!(cases.len(), 38);
        for case in cases {
            let result = numera_core::browser::sdk::values::decode(&case["raw"]);
            if case["ok"] == true {
                let result = result?;
                assert_eq!(result, case["value"], "SDK value {}", case["name"]);
                let envelope = serde_json::json!({"protocol":1,"id":1,"ok":true,"result":{"evaluated":true,"value":result}});
                let wire = numera_core::browser::sdk::values::encode(&envelope)?;
                assert!(wire.is_ascii());
                assert_eq!(
                    String::from_utf8(wire)?,
                    case["wire"].as_str().ok_or("Source wire missing")?,
                    "SDK encoding {}",
                    case["name"]
                );
            } else {
                assert!(
                    result.is_err(),
                    "Unrepresentable SDK value {} was accepted",
                    case["name"]
                );
            }
        }
        let valid = serde_json::Value::String("x".repeat(1_048_576 - 3));
        assert_eq!(
            numera_core::browser::sdk::values::encode(&valid)?.len(),
            1_048_575
        );
        let excessive = serde_json::Value::String("x".repeat(1_048_576 - 2));
        assert!(numera_core::browser::sdk::values::encode(&excessive).is_err());
        let mut shared = serde_json::json!({"a":[{"s":"x"}],"id":1});
        for id in 2..25 {
            shared = serde_json::json!({"a":[shared,{"ref":id-1}],"id":id});
        }
        assert!(
            numera_core::browser::sdk::values::decode(&shared).is_err(),
            "Reference expansion exceeded the frame budget"
        );
    }
    for invalid in [false, true] {
        let temporary = common::TempDirectory::new("browser-adapter-protocol")?;
        let root = temporary.path();
        let mut command = adapter(&root.join("logs"))?;
        let mut sdk = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        assert_eq!(
            claims::state(&claims::read(root, "a"), "fixture-host"),
            claims::State::Busy
        );
        assert_eq!(
            sdk.metadata()["backends"]
                .as_object()
                .ok_or("Backend metadata missing")?
                .len(),
            4
        );
        assert!(
            !root.join("user-data").exists(),
            "Hello performed browser work"
        );
        if invalid {
            let failure = sdk.call("unsupported", Map::new(), Duration::from_secs(5));
            assert!(failure.is_err());
        } else {
            assert_eq!(
                sdk.call("close", Map::new(), Duration::from_secs(5))?["closing"],
                true
            );
        }
        sdk.finish(Duration::from_secs(5))?;
        drop(sdk);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        let phases: &[&str] = if invalid {
            &["browser_spawn", "browser_hello", "browser_settle"]
        } else {
            &[
                "browser_spawn",
                "browser_hello",
                "browser_close",
                "browser_settle",
            ]
        };
        let log = browser_logs::inspect(root, phases)?;
        assert!(log.contains("[INFO] [numera.browser-adapter] Starting"));
        assert!(log.contains(if invalid {
            "Finished exit=1"
        } else {
            "Finished exit=0"
        }));
        assert!(!log.contains("[WARNING]"));
        temporary.cleanup()?;
    }
    Ok(())
}
#[test]
fn fixed_adapter_observes_first_page_close_without_inventing_a_new_page()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("adapter-first-page")?;
    let root = temporary.path();
    let mut command = fixture::worker(root, "playwright", "first-page")?;
    command.env("NUMERA_ADAPTER_FIRST_PAGE", "1");
    let mut sdk = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        sdk.open(
            "playwright",
            json!({"user_data_dir":root.join("profile"),"headless":true})
                .as_object()
                .cloned()
                .ok_or("Options missing")?,
            vec![],
            Duration::from_secs(5),
        )?;
        std::fs::write(root.join("driver/trigger-close"), b"close")?;
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(code) = sdk.settled_exit()? {
                assert_eq!(
                    code,
                    0,
                    "First-page close became an SDK failure\n{}",
                    adapter_logs(root)?
                );
                break;
            }
            if Instant::now() >= end {
                return Err("First-page close did not finish the adapter".into());
            }
            std::thread::park_timeout(Duration::from_millis(2));
        }
        assert_eq!(
            std::fs::read_to_string(root.join("driver/context-closed"))?,
            "closed"
        );
        assert!(
            !fixture::requests(root)?
                .iter()
                .any(|r| r["method"] == "newPage"),
            "Closed page was replaced"
        );
        Ok(())
    })();
    let cleanup = sdk.finish(Duration::from_secs(5));
    cleanup?;
    result?;
    drop(sdk);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn registry_retains_sdk_transport_and_closes_only_its_admitted_session()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("browser-registry-protocol")?;
    let root = temporary.path();
    std::fs::create_dir_all(root.join("profiles"))?;
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/init-script.json"))?;
    let profile = Profile::from_record(&serde_json::json!({
        "id":"a", "name":"fixture", "fingerprint":fixture["cases"][0]["input"]
    }))?;
    std::fs::write(root.join("profiles/a.json"), serde_json::to_vec(&profile)?)?;
    let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
    let foreign = Registry::new(root.into(), "fixture-host".into(), None)?;
    let failure = |error: std::io::Error| StartupFailed {
        kind: "failed".into(),
        message: error.to_string(),
    };
    assert!(registry.open_browser("a", "fixture", |session, current| {
        assert_eq!(current.id, "a");
        let mut command = adapter(&root.join("logs")).map_err(failure)?;
        Adapter::spawn_admitted(session, &mut command, Duration::from_secs(5)).map_err(failure)
    })?);
    assert!(registry.owns("a")?);
    assert!(registry.running("a")?);
    assert!(!registry.open_browser("a", "fixture", |_, _| panic!(
        "Duplicate activated another SDK"
    ))?);
    assert!(!foreign.stop("a", Duration::from_secs(5))?);
    assert!(registry.running("a")?);
    assert!(registry.stop("a", Duration::from_secs(5))?);
    assert!(!registry.owns("a")?);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    assert!(registry.shutdown(Duration::from_secs(5)).is_empty());
    assert!(
        registry
            .open_browser("a", "fixture", |_, _| panic!("Shutdown activated SDK"))
            .is_err()
    );
    drop(registry);
    drop(foreign);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn acknowledged_close_waits_for_normal_exit_before_releasing_the_owner()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("browser-close-exit")?;
    let root = temporary.path();
    let registry = Registry::new(root.into(), "fixture-host".into(), None)?;
    let release = root.join("release");
    let completed = root.join("completed");
    let ack = root.join("ack");
    let mut command = fixture::command(
        root,
        "rpc",
        json!({"ack":ack,"release":release,"completed":completed}),
    )?;
    let adapter = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    registry.retain_browser("a", adapter)?;
    let stopping = std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let stop = scope.spawn(|| registry.stop("a", Duration::from_secs(5)));
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ack.exists() {
            if std::time::Instant::now() >= deadline {
                return Err("Close acknowledgement missing".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        std::fs::write(&release, b"release")?;
        assert!(stop.join().map_err(|_| "Stop thread failed")??);
        Ok(())
    });
    stopping?;
    assert_eq!(std::fs::read_to_string(completed)?, "finished");
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    assert!(!registry.owns("a")?);
    drop(registry);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fixed_adapter_maps_packaged_camoufox_enum_and_geoip_only_at_sdk_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("adapter-package-enum")?;
    let root = temporary.path();
    let resource = root.join("camoufox");
    std::fs::create_dir_all(resource.join("fonts"))?;
    std::fs::write(
        resource.join("fonts/fixture.ttf"),
        b"mock font resource; vendor transport does not render",
    )?;
    std::fs::write(resource.join("properties.json"), b"[]")?;
    let binary = resource.join(if cfg!(windows) {
        "camoufox.exe"
    } else {
        "camoufox"
    });
    std::fs::write(
        &binary,
        b"selected mock browser; never executed by vendor transport",
    )?;
    for os in ["linux", "macos", "windows"] {
        let folder = resource.join("fontconfig").join(os);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(
            folder.join("fonts.conf"),
            b"<fontconfig><dir prefix=\"cwd\">fonts</dir></fontconfig>",
        )?;
    }
    let geoip = root.join("geoip");
    let mut command = fixture::worker(root, "camoufox", "normal")?;
    command.env("NUMERA_ADAPTER_GEOIP_DIR", &geoip);
    let mut sdk = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let options = json!({"user_data_dir":root.join("user-data/a"),"executable_path":binary,"headless":true,"exclude_addons":["UBO"],"ff_version":"152"});
        sdk.open(
            "camoufox",
            options.as_object().cloned().ok_or("Options missing")?,
            vec![],
            Duration::from_secs(15),
        )?;
        let requests = fixture::requests(root)?;
        let launched = requests
            .iter()
            .rev()
            .find(|r| r["method"] == "launchPersistentContext")
            .ok_or("Camoufox launch missing")?;
        assert_eq!(launched["guid"], "firefox");
        let env = launched["params"]["env"]
            .as_array()
            .ok_or("Camoufox launch environment missing")?;
        let config_text = env
            .iter()
            .filter(|e| {
                e["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("CAMOU_CONFIG_"))
            })
            .map(|e| e["value"].as_str().ok_or("Config chunk not text"))
            .collect::<Result<Vec<_>, _>>()?
            .join("");
        let config: Value = serde_json::from_str(&config_text)?;
        assert!(
            config.get("addons").is_none(),
            "UBO exclusion token was not consumed by native SDK boundary"
        );
        assert!(
            config["navigator.userAgent"]
                .as_str()
                .is_some_and(|ua| ua.contains("Firefox/152.")),
            "Packaged major version was lost"
        );
        assert!(
            env.iter()
                .any(|e| e["name"] == "NUMERA_ADAPTER_GEOIP_DIR" && e["value"] == json!(geoip))
        );
        let paths = numera_core::browser::sdk::camoufox_geo::resource_paths(&geoip);
        assert_eq!(paths.root, geoip);
        assert_eq!(paths.databases, geoip.join("mmdb"));
        assert_eq!(paths.config, geoip.join("config.yml"));
        assert_eq!(
            sdk.call("close", Map::new(), Duration::from_secs(5))?["closing"],
            true
        );
        Ok(())
    })();
    sdk.finish(Duration::from_secs(5))?;
    result.map_err(|e| format!("{e}\n{}", adapter_logs(root).unwrap_or_default()))?;
    drop(sdk);
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fixed_adapter_probe_calls_preserve_sdk_args_and_refuse_invalid_deadlines()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("adapter-probes")?;
    let root = temporary.path();
    for (index, invalid) in [
        serde_json::json!(0),
        serde_json::json!(true),
        serde_json::json!(45001),
        serde_json::json!("20"),
        serde_json::Value::Null,
    ]
    .into_iter()
    .enumerate()
    {
        let observed = root.join("driver/seen.json");
        let mut command = fixture::worker(root, "playwright", "probe")?;
        let mut sdk = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(5),
        )?;
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            sdk.open(
                "playwright",
                Map::from_iter([(
                    "user_data_dir".into(),
                    serde_json::json!(root.join("profile")),
                )]),
                Vec::new(),
                Duration::from_secs(5),
            )?;
            if index == 0 {
                assert_eq!(sdk.call("route", serde_json::json!({"pattern":"**/*","status":200,"content_type":"text/html","body":"<title>fixture</title>"}).as_object().cloned().ok_or("Route missing")?,Duration::from_secs(5))?["routed"],true);
                for (url, timeout, expected) in [
                    ("https://ok.fixture.test/", 20000, true),
                    ("https://failure.fixture.test/", 30000, false),
                    ("https://warmup.fixture.test/", 45000, true),
                ] {
                    let result = sdk.call(
                        "navigate",
                        serde_json::json!({"url":url,"timeout_ms":timeout,"tolerate_failure":true})
                            .as_object()
                            .cloned()
                            .ok_or("Navigation missing")?,
                        Duration::from_secs(5),
                    )?;
                    assert_eq!(result["navigated"], expected);
                }
                for (fields, expected) in [
                    (serde_json::json!({"action":"wheel","x":0,"y":200}), true),
                    (
                        serde_json::json!({"action":"move","x":50,"y":80,"steps":5}),
                        true,
                    ),
                    (serde_json::json!({"action":"wheel","x":0,"y":-1}), false),
                ] {
                    assert_eq!(
                        sdk.call(
                            "mouse",
                            fields.as_object().cloned().ok_or("Mouse fields missing")?,
                            Duration::from_secs(5)
                        )?["performed"],
                        expected
                    );
                }
                for (expression, expected) in [("success", true), ("fail", false)] {
                    let result = sdk.call("evaluate", serde_json::json!({"expression":expression,"argument":{"n":8},"tolerate_failure":true}).as_object().cloned().ok_or("Evaluation missing")?, Duration::from_secs(5))?;
                    assert_eq!(result["evaluated"], expected);
                    assert_eq!(
                        result["value"],
                        if expected {
                            serde_json::json!({"argument":{"n":8}})
                        } else {
                            serde_json::Value::Null
                        }
                    );
                }
            }
            assert!(sdk.call("navigate", serde_json::json!({"url":"https://ok.fixture.test/","new_page":true,"timeout_ms":invalid,"tolerate_failure":true}).as_object().cloned().ok_or("Invalid navigation missing")?, Duration::from_secs(5)).is_err());
            Ok(())
        })();
        sdk.finish(Duration::from_secs(5))?;
        result?;
        let seen: serde_json::Value = serde_json::from_slice(&std::fs::read(observed)?)?;
        let seen = seen.as_array().ok_or("SDK observations missing")?;
        assert!(!seen.iter().any(|row| row["operation"] == "new_page"));
        assert_eq!(
            seen.last().ok_or("SDK close missing")?["operation"],
            "close"
        );
        if index == 0 {
            assert_eq!(seen[0]["pattern"], "**/*");
            assert_eq!(
                seen[1],
                serde_json::json!({"operation":"fulfill","status":200,"content_type":"text/html","body":"<title>fixture</title>"})
            );
            assert_eq!(seen[2]["timeout"], 20000, "Unexpected SDK calls: {seen:?}");
            assert_eq!(seen[2]["wait_until"], "domcontentloaded");
            assert_eq!(seen[4]["timeout"], 45000);
            assert_eq!(
                seen[5],
                serde_json::json!({"operation":"wheel","x":0,"y":200})
            );
            assert_eq!(
                seen[6],
                serde_json::json!({"operation":"move","x":50,"y":80,"steps":5})
            );
            assert_eq!(seen.len(), 11);
        } else {
            assert_eq!(seen.len(), 1, "Invalid deadline reached the SDK");
        }
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    }
    let mut command = fixture::worker(root, "playwright", "probe")?;
    let mut sdk = Adapter::spawn(
        root,
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        sdk.open(
            "playwright",
            Map::from_iter([(
                "user_data_dir".into(),
                serde_json::json!(root.join("profile")),
            )]),
            Vec::new(),
            Duration::from_secs(5),
        )?;
        assert!(
            sdk.call(
                "evaluate",
                serde_json::json!({"expression":"nonfinite","tolerate_failure":true})
                    .as_object()
                    .cloned()
                    .ok_or("Evaluation missing")?,
                Duration::from_secs(5)
            )
            .is_err(),
            "Serialization failure was swallowed as an SDK failure"
        );
        Ok(())
    })();
    sdk.finish(Duration::from_secs(5))?;
    result?;
    assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
    for entry in std::fs::read_dir(root.join("logs"))? {
        assert!(!std::fs::read_to_string(entry?.path())?.contains("Sensitive SDK detail"));
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn fixed_input_preserves_empty_partial_coalesced_and_bounded_frames()
-> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    struct Cancel(Arc<AtomicBool>);
    impl Drop for Cancel {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    for (case_index, (chunks, invalid)) in [
        (vec![], false),
        (vec![b"{".to_vec()], true),
        (vec![b"{}".to_vec()], true),
        (vec![b"{".to_vec(), b"}\n{}\n".to_vec()], false),
        (vec![b"{}\n".to_vec()], false),
        (vec![vec![b'x'; 17]], true),
        (vec![b"[]\n".to_vec()], true),
        (vec![b"\xff\n".to_vec()], true),
    ]
    .into_iter()
    .enumerate()
    {
        let temporary = common::TempDirectory::new("fixed-native-input")?;
        let root = temporary.path();
        let ready = root.join("ready");
        let partial = root.join("partial");
        let mut command = fixture::command(
            root,
            "frames",
            json!({"chunks":chunks,"ready":ready,"partial":partial}),
        )?;
        command
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped());
        let mut tree = numera_core::ownership::tree::OwnedTree::spawn(&mut command)?;
        let mut input = tree.child_mut().stdin.take().ok_or("Input pipe missing")?;
        let output = tree
            .child_mut()
            .stdout
            .take()
            .ok_or("Output pipe missing")?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let (send, recv) = mpsc::sync_channel(1);
        let result = std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
            let _cancel = Cancel(cancelled.clone());
            let worker = scope.spawn(move || {
                let stream = numera_core::native_input::Input::new(output, cancelled);
                let mut reader = std::io::BufReader::new(stream);
                let result = (|| -> std::io::Result<Vec<Value>> {
                    let mut values = Vec::new();
                    for _ in 0..8 {
                        match numera_core::browser::adapter::frame_with_limit(&mut reader, 16)? {
                            Some(value) => values.push(value),
                            None => return Ok(values),
                        }
                    }
                    Err(std::io::Error::other("Frame reader did not reach EOF"))
                })();
                let _ = send.send(result);
            });
            wait_fixture(&ready)?;
            assert!(
                matches!(recv.try_recv(), Err(mpsc::TryRecvError::Empty)),
                "Empty input was treated as EOF"
            );
            input.write_all(b"x")?;
            input.flush()?;
            if chunks.len() == 2 {
                wait_fixture(&partial)?;
                assert!(
                    matches!(recv.try_recv(), Err(mpsc::TryRecvError::Empty)),
                    "Partial frame was returned early"
                );
                input.write_all(b"y")?;
                input.flush()?;
            }
            let values = recv.recv_timeout(Duration::from_secs(5))?;
            worker.join().map_err(|_| "Input reader panicked")?;
            match values {
                Err(_) => assert!(invalid),
                Ok(values) => {
                    assert!(!invalid);
                    assert_eq!(
                        values,
                        if chunks.len() == 2 {
                            vec![json!({}), json!({})]
                        } else if chunks.is_empty() {
                            vec![]
                        } else {
                            vec![json!({})]
                        }
                    );
                }
            }
            Ok(())
        });
        drop(input);
        settle_input_fixture(&mut tree, case_index)?;
        result?;
        temporary.cleanup()?;
    }
    Ok(())
}

fn settle_input_fixture(
    tree: &mut numera_core::ownership::tree::OwnedTree,
    case_index: usize,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use numera_core::ownership::{observation, test_support};
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        #[derive(Default)]
        struct Probe {
            births: AtomicUsize,
            signals: AtomicUsize,
            groups: AtomicUsize,
            last: AtomicUsize,
        }
        impl test_support::Backend for Probe {
            fn birth(&self, _pid: u32) -> Option<std::io::Result<Option<u64>>> {
                self.births.fetch_add(1, Ordering::Relaxed);
                self.last.store(1, Ordering::Relaxed);
                None
            }
            fn signal(&self, _target: i32, _force: bool) -> Option<std::io::Result<()>> {
                self.signals.fetch_add(1, Ordering::Relaxed);
                self.last.store(2, Ordering::Relaxed);
                None
            }
            fn group_empty(&self, _group: i32) -> Option<std::io::Result<bool>> {
                self.groups.fetch_add(1, Ordering::Relaxed);
                self.last.store(3, Ordering::Relaxed);
                None
            }
        }
        let probe = Arc::new(Probe::default());
        let start = Instant::now();
        let result =
            test_support::with_backend(probe.clone(), || tree.settle(Duration::from_secs(5)));
        if let Err(error) = &result {
            let signals = probe.signals.load(Ordering::Relaxed);
            eprintln!(
                "Input fixture teardown case={case_index} elapsed_ms={} birth_calls={} signal_calls={signals} group_calls={} last_boundary_code={} errno={:?} kind={:?} {}",
                start.elapsed().as_millis(),
                probe.births.load(Ordering::Relaxed),
                probe.groups.load(Ordering::Relaxed),
                probe.last.load(Ordering::Relaxed),
                error.raw_os_error(),
                error.kind(),
                observation::diagnostic(
                    i32::try_from(tree.pid()).unwrap_or(0),
                    false,
                    signals > 0,
                    false
                )
            );
        }
        result
    }
    #[cfg(not(unix))]
    {
        let _ = case_index;
        tree.settle(Duration::from_secs(5))
    }
}

#[test]
fn native_transport_refuses_wrong_envelope_partial_frame_and_unresponsive_writer()
-> Result<(), Box<dyn std::error::Error>> {
    for mode in ["wrong-envelope", "partial-frame", "wait"] {
        let temporary = common::TempDirectory::new("browser-adapter-refusal")?;
        let mut command = fixture::command(temporary.path(), mode, json!({}))?;
        let result = Adapter::spawn(
            temporary.path(),
            "a",
            "fixture-host",
            &mut command,
            Duration::from_millis(250),
        );
        assert!(result.is_err());
        assert!(matches!(
            claims::read(temporary.path(), "a"),
            claims::Claim::Absent
        ));
        temporary.cleanup()?;
    }
    for (mode, has_reply) in [
        ("leader-exit-held-pipe", false),
        ("final-reply-held-pipe", true),
    ] {
        let temporary = common::TempDirectory::new("browser-adapter-leader-exit")?;
        let root = temporary.path();
        let mut command = fixture::command(root, mode, json!({"root":root}))?;
        let mut adapter = Adapter::spawn(
            root,
            "a",
            "fixture-host",
            &mut command,
            Duration::from_secs(3),
        )?;
        let record = match claims::read(root, "a") {
            claims::Claim::Record(record) => record,
            _ => return Err("Admitted adapter claim absent".into()),
        };
        let started = Instant::now();
        let response = adapter.call("navigate", Map::new(), Duration::from_secs(3));
        let elapsed = started.elapsed();
        if has_reply {
            assert_eq!(response?["navigated"], true);
        } else {
            assert_eq!(
                response
                    .expect_err("An exited producer supplied a reply")
                    .kind(),
                std::io::ErrorKind::UnexpectedEof
            );
        }
        let leaf: u32 = std::fs::read_to_string(root.join("held-pipe-child"))?.parse()?;
        assert!(core::ownership::identity::started_at(leaf)?.is_some());
        assert!(!core::ownership::tree::record_empty(&record)?);
        assert_eq!(
            claims::state(&claims::read(root, "a"), "fixture-host"),
            claims::State::Busy
        );
        adapter.finish(Duration::from_secs(3))?;
        assert!(core::ownership::tree::record_empty(&record)?);
        assert!(matches!(claims::read(root, "a"), claims::Claim::Absent));
        drop(adapter);
        temporary.cleanup()?;
        assert!(
            elapsed < Duration::from_secs(1),
            "A dead producer or accepted final reply was hidden until the RPC deadline"
        );
    }
    Ok(())
}

fn wait_fixture(path: &Path) -> std::io::Result<()> {
    let end = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        if Instant::now() >= end {
            return Err(std::io::Error::other("Fixture marker timed out"));
        }
        std::thread::park_timeout(Duration::from_millis(2));
    }
    Ok(())
}
