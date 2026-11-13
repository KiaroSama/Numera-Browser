use numera_core::{ownership::capture, runtime_log::RuntimeLog};
use serde::Deserialize;
use std::{fs, path::Path, process::Command, time::Duration};
mod common;
#[allow(dead_code)]
#[path = "common/http_fixture.rs"]
mod http_fixture;
#[path = "logging_parity/retention.rs"]
mod retention;

#[derive(Deserialize)]
struct Observation {
    input: String,
    expected: String,
}

#[test]
fn runtime_messages_match_original_redaction_at_file_and_fallback_sinks()
-> Result<(), Box<dyn std::error::Error>> {
    if let Some(input) = std::env::var_os("NUMERA_LOG_MESSAGE_FIXTURE") {
        let directory = std::env::var_os("NUMERA_LOG_MESSAGE_DIRECTORY")
            .ok_or("Log fixture directory missing")?;
        let observations: Vec<Observation> = serde_json::from_slice(&fs::read(input)?)?;
        let unavailable = Path::new(&directory).is_file();
        let log = RuntimeLog::new(Path::new(&directory), "native-log-test");
        assert_eq!(
            log.path().is_none(),
            unavailable,
            "Log sink availability differs"
        );
        for observation in observations {
            log.message(
                if unavailable { "WARNING" } else { "ERROR" },
                "fixture",
                &observation.input,
            );
        }
        return Ok(());
    }
    let temporary = common::TempDirectory::new("redacted log α")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let observations: Vec<Observation> = serde_json::from_str(include_str!(
        "fixtures/characterization/runtime_messages_match_original_redaction_at_file_and_fallback_sinks.json"
    ))?;
    assert!(
        observations.len() > 4000,
        "Redaction boundary corpus missing"
    );
    let log = RuntimeLog::new(
        &temporary.path().join("logs with spaces"),
        "native-log-test",
    );
    let path = log.path().ok_or("Runtime file missing")?.to_owned();
    let mut expected = String::new();
    for row in &observations {
        log.message("INFO", "fixture", &row.input);
        expected.push_str(&row.expected);
        expected.push('\n');
    }
    drop(log);
    let text = fs::read_to_string(&path)?;
    let actual = messages(&text, "INFO")?;
    assert_eq!(actual, expected, "Runtime credential redaction differs");
    // A non-directory deterministically forces the real stderr fallback.
    let blocker = temporary.path().join("unavailable");
    fs::write(&blocker, b"retained")?;
    let record = temporary.path().join("messages.json");
    fs::write(
        &record,
        include_bytes!(
            "fixtures/characterization/runtime_messages_match_original_redaction_at_file_and_fallback_sinks.json"
        ),
    )?;
    for directory in [&blocker, &temporary.path().join("error logs")] {
        let unavailable = directory == &blocker;
        let level = if unavailable { "WARNING" } else { "ERROR" };
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args([
                "--exact",
                "runtime_messages_match_original_redaction_at_file_and_fallback_sinks",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("NUMERA_LOG_MESSAGE_FIXTURE", &record)
            .env("NUMERA_LOG_MESSAGE_DIRECTORY", directory);
        let output = capture::run(
            &mut child,
            capture::Limits {
                wall: Duration::from_secs(10),
                idle: Duration::from_secs(10),
                bytes: 4 * 1024 * 1024,
                cleanup: Duration::from_secs(5),
            },
        )?;
        assert_eq!(output.exit, 0, "Logging sink child failed");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.lines().any(|line| line == "running 1 test"));
        assert!(stdout.contains("\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "));
        let stderr = String::from_utf8(output.stderr)?;
        let entries = if unavailable {
            let (warning, entries) = stderr
                .split_once('\n')
                .ok_or("Log fallback warning missing")?;
            assert_eq!(
                warning,
                "Runtime file logging unavailable; safe lifecycle diagnostics remain on stderr"
            );
            entries
        } else {
            let files = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
            assert_eq!(files.len(), 1);
            assert_eq!(
                messages(&fs::read_to_string(files[0].path())?, level)?,
                expected,
                "Error file credential redaction differs"
            );
            &stderr
        };
        assert_eq!(
            messages(entries, level)?,
            expected,
            "Console credential redaction differs"
        );
    }
    assert_eq!(fs::read(&blocker)?, b"retained");
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn server_scheduler_errors_reach_the_runtime_file_without_starting_an_sdk()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{
        browser::runtime::Runtime,
        http::server::{Options, Server},
    };
    use std::time::Instant;

    let temporary = common::TempDirectory::new("scheduler log α")?;
    let root = temporary.path();
    let schedules = br#"[{"everyHours":"password=synthetic-value","lastRun":1}]"#;
    fs::write(root.join("schedules.json"), schedules)?;
    let mut server = Server::start(Options {
        root: root.into(),
        address: ([127, 0, 0, 1], 0).into(),
        token: String::new(),
        origins: String::new(),
        insecure_override: String::new(),
        password: None,
        host: "fixture-host".into(),
        installed: Default::default(),
        browser: Runtime {
            adapter: root.join("never-start-sdk"),
            cwd: root.into(),
            home: root.into(),
            logs: root.join("logs"),
            country: None,
            relay: None,
            binary_options: Default::default(),
            deployment: None,
        },
        dashboard: None,
    })?;
    let observed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || -> Result<(), Box<dyn std::error::Error>> {
            server.wait_ready(Duration::from_secs(5))?;
            let entries = fs::read_dir(root.join("logs"))?.collect::<Result<Vec<_>, _>>()?;
            assert_eq!(entries.len(), 1, "Scheduler launched an SDK log");
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let text = fs::read_to_string(entries[0].path())?;
                if text.contains("[ERROR] [scheduler] Scheduler tick failed: Stored schedule time must be numeric\n") {
                    assert!(!text.contains("synthetic-value"));
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Scheduler error runtime log missing"
                );
                std::thread::park_timeout(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(10)),
                );
            }
            Ok(())
        },
    ));
    let address = server.address();
    let stopped = server.stop(Duration::from_secs(5));
    drop(server);
    stopped?;
    assert!(std::net::TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    assert_eq!(fs::read(root.join("schedules.json"))?, schedules);
    match observed {
        Ok(result) => result?,
        Err(panic) => std::panic::resume_unwind(panic),
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn serve_verbose_matches_original_debug_admission_through_cli_dispatch()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{cli, native_entry, native_environment::Environment};
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    if let Some(root) = std::env::var_os("NUMERA_VERBOSE_FIXTURE") {
        let root = Path::new(&root);
        let verbose = std::env::var("NUMERA_VERBOSE_EXPECTED")? == "true";
        let mut argv = vec!["serve", "--host", "127.0.0.1", "--port", "0"];
        if verbose {
            argv.push("--verbose");
        }
        let mut command = cli::registry();
        command.build();
        let parsed = command
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(argv.iter().copied()))?;
        let values = cli::reflection::values(&command, &parsed);
        let environment = Environment {
            app: root.into(),
            home: root.into(),
            cwd: root.into(),
            host: "fixture-host".into(),
            browser: numera_core::browser::runtime::Runtime {
                adapter: root.join("absent-sdk"),
                cwd: root.into(),
                home: root.into(),
                logs: root.join("logs"),
                country: None,
                relay: None,
                binary_options: Default::default(),
                deployment: None,
            },
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let signal = cancelled.clone();
        let directory = root.join("logs");
        let watcher = std::thread::spawn(move || -> std::io::Result<()> {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let observed = (|| {
                loop {
                    for entry in fs::read_dir(&directory)? {
                        let entry = entry?;
                        if entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("native-http_")
                            && fs::read_to_string(entry.path())?
                                .contains("[INFO] [server] Readiness published\n")
                        {
                            return Ok(());
                        }
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(std::io::Error::other("Verbose server readiness missing"));
                    }
                    std::thread::park_timeout(
                        deadline
                            .saturating_duration_since(std::time::Instant::now())
                            .min(Duration::from_millis(10)),
                    );
                }
            })();
            signal.store(true, Ordering::Release);
            observed
        });
        let log = RuntimeLog::new(&root.join("entry logs"), "native-entry-test");
        let mut output = cli::output::Output::new(Vec::new(), Vec::new(), false, false);
        let result = native_entry::dispatch(
            values.as_object().ok_or("Parsed serve args missing")?,
            &argv.into_iter().map(String::from).collect::<Vec<_>>(),
            &mut output,
            environment,
            root,
            cancelled,
            &log,
        );
        watcher.join().map_err(|_| "Readiness watcher panicked")??;
        assert_eq!(result?, 0, "Verbose server dispatch failed");
        let logs = fs::read_dir(root.join("logs"))?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(logs.len(), 1, "Verbose serving launched an SDK");
        let text = fs::read_to_string(logs[0].path())?;
        assert!(text.contains("[INFO] [server] Readiness published\n"));
        assert!(text.contains("[INFO] [server] Shutdown settled\n"));
        assert_eq!(
            text.contains("[DEBUG] [server] Router prepared\n"),
            verbose,
            "Serve verbose debug admission differs"
        );
        assert!(!text.contains("private-verbose-sentinel"));
        return Ok(());
    }
    let temporary = common::TempDirectory::new("verbose logs α")?;
    let expected: Vec<bool> = serde_json::from_value(http_fixture::oracle("log-verbosity")?)?;
    assert_eq!(
        expected,
        [false, true],
        "Original verbosity observations missing"
    );
    for (index, verbose) in expected.into_iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        fs::create_dir_all(root.join("logs"))?;
        let mut child = Command::new(std::env::current_exe()?);
        child
            .args([
                "--exact",
                "serve_verbose_matches_original_debug_admission_through_cli_dispatch",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("NUMERA_VERBOSE_FIXTURE", &root)
            .env("NUMERA_VERBOSE_EXPECTED", verbose.to_string())
            .env("NUMERA_PASSWORD", "private-verbose-sentinel")
            .env_remove("NUMERA_PARENT_PID")
            .env_remove("NUMERA_API_TOKEN")
            .env_remove("NUMERA_ALLOWED_ORIGINS")
            .env_remove("NUMERA_ALLOW_INSECURE_BIND");
        let output = capture::run(
            &mut child,
            capture::Limits {
                wall: Duration::from_secs(10),
                idle: Duration::from_secs(10),
                bytes: 65536,
                cleanup: Duration::from_secs(5),
            },
        )?;
        assert_eq!(output.exit, 0, "Serve verbose debug admission differs");
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.lines().any(|line| line == "running 1 test"));
        assert!(stdout.contains("\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; "));
        assert!(
            output.stderr.is_empty(),
            "Verbose server wrote private diagnostics"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

fn messages(text: &str, level: &str) -> Result<String, Box<dyn std::error::Error>> {
    let marker = format!("] [{level}] [fixture] ");
    let mut result = String::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with('[') && line.contains(&marker) {
            let (stamp, message) = line.split_once(&marker).ok_or("Log entry missing")?;
            assert!(stamp.ends_with(" UTC"), "UTC timestamp missing");
            result.push_str(message);
        } else {
            result.push_str(line);
        }
    }
    Ok(result)
}
