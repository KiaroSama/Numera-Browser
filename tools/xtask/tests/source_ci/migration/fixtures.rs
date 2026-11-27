use super::*;

pub(super) fn occurrence_fixture() -> ci::Result<Value> {
    migration::discover("frozen-fixture",&[("source.rs".into(),br#"#[cfg(windows)] fn take(){} #[cfg(unix)] fn take(){} #[cfg(feature="other")] fn take(){} fn read(){#[cfg(windows)]fn pipe(){} #[cfg(unix)]fn pipe(){}}"#.to_vec())])
}
pub(super) fn complete(temp: &Temp, source: &Value) -> ci::Result<Value> {
    temp.write("target.rs", b"fn native(){}")?;
    temp.write("tests.rs", b"#[test]fn acceptance(){}")?;
    let mut ledger = source.clone();
    for row in ledger["capabilities"]
        .as_array_mut()
        .ok_or("capabilities")?
    {
        row["status"] = json!("verified");
        row["target"] = json!("target.rs");
        row["test"] = json!("tests.rs");
    }
    Ok(ledger)
}

pub(super) fn entry_creation_fixture(
    temp: &Temp,
    settings: &Value,
    profile: &Value,
    dashboard: &Value,
    newline: &str,
    lock_content: &[u8],
    extra_lock: bool,
) -> ci::Result<Value> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let number = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let template = temp.0.join(format!("template-{number}"));
    fs::create_dir(&template)?;
    let initial = json!({"default_engine":"playwright","future":{"items":[42,true]}});
    fs::write(
        template.join("config.json"),
        serde_json::to_string_pretty(&initial)?.replace('\n', "\r\n"),
    )?;
    let workspace = temp.0.join(format!("pair-{number}"));
    fs::create_dir(&workspace)?;
    let (_, payload) = ci::entry_benchmark::observation(
        &template,
        &workspace,
        ci::entry_benchmark::Scenario::Create,
        &Value::Null,
        |data, _| {
            fs::write(
                data.join("config.json"),
                serde_json::to_string_pretty(settings)?.replace('\n', newline),
            )?;
            for (directory, record) in [("profiles", profile), ("dashboard", dashboard)] {
                let path = data.join(directory).join("new.json");
                fs::create_dir_all(path.parent().expect("parent"))?;
                fs::write(&path, record.to_string())?;
                let lock = numera_core::storage::locks::DataLock::for_file(&path)?;
                fs::create_dir_all(lock.path().parent().expect("lock parent"))?;
                fs::write(lock.path(), lock_content)?;
                if extra_lock {
                    fs::write(
                        lock.path()
                            .parent()
                            .expect("lock parent")
                            .join("unrelated.lock"),
                        b"",
                    )?;
                }
            }
            Ok(browser::Sample {
                elapsed_ns: 1,
                metric: "fixture".into(),
                bytes: 1,
                payload: json!({"ok":true,"profile":profile}),
                diagnostics: None,
            })
        },
    )?;
    Ok(payload)
}

pub(super) fn reference_fixture(temp: &Temp) -> ci::Result<(PathBuf, Value)> {
    let worker = temp
        .write("current native SDK", b"inert worker bytes")?
        .canonicalize()?;
    let mut entries = Vec::new();
    for (path, blob) in reference::BRIDGE_FILES {
        entries.push(reference::Entry {
            path: path.into(),
            blob: blob.into(),
            bytes: ci::git(&root(), &["cat-file", "blob", blob])?,
            executable: false,
        });
    }
    entries.push(reference::Entry {
        path: "engine-rust/src/lib.rs".into(),
        blob: "a".repeat(40),
        bytes: b"pub const VALUE:i32=1;".to_vec(),
        executable: false,
    });
    let destination = temp.0.join("frozen-source");
    let receipt = reference::materialize(&destination, &entries, &worker)?;
    Ok((destination, receipt))
}

pub(super) fn temp_bound() -> TestResult {
    let env = BTreeMap::from([
        ("GITHUB_ACTIONS".into(), "true".into()),
        ("RUNNER_TEMP".into(), "/home/runner/work/_temp".into()),
    ]);
    assert_eq!(
        migration::temporary_parent(Path::new("/srv/numera"), &env, "linux")?,
        PathBuf::from("/srv/numera/.ci-work")
    );
    assert_eq!(
        migration::temporary_parent(
            Path::new("/home/runner/work/Numera-Browser/Numera-Browser"),
            &env,
            "linux"
        )?,
        PathBuf::from("/home/runner/work/_temp")
    );
    let long = PathBuf::from(format!("/{}", "a".repeat(100)));
    assert_eq!(
        migration::temporary_parent(&long, &env, "win32")?,
        long.join(".ci-work")
    );
    assert!(migration::temporary_parent(&long, &BTreeMap::new(), "linux").is_err());
    for temp in [
        format!("/{}", "x".repeat(100)),
        "relative".into(),
        r"D:\a\_temp".into(),
    ] {
        let invalid = BTreeMap::from([
            ("GITHUB_ACTIONS".into(), "true".into()),
            ("RUNNER_TEMP".into(), temp),
        ]);
        assert!(migration::temporary_parent(&long, &invalid, "linux").is_err());
    }
    let unicode = PathBuf::from(format!("/{}", "Ω".repeat(40)));
    assert_eq!(
        migration::temporary_parent(&unicode, &env, "linux")?,
        PathBuf::from("/home/runner/work/_temp")
    );
    Ok(())
}

pub(super) fn temp_owned() -> TestResult {
    let temp = Temp::new("unique temp")?;
    let env = BTreeMap::from([
        ("GITHUB_ACTIONS".into(), "true".into()),
        ("RUNNER_TEMP".into(), "/tmp".into()),
    ]);
    let first = migration::Temporary::new(
        &temp.0,
        &env,
        if cfg!(target_os = "linux") {
            "linux"
        } else {
            "win32"
        },
    )?;
    let second = migration::Temporary::new(
        &temp.0,
        &env,
        if cfg!(target_os = "linux") {
            "linux"
        } else {
            "win32"
        },
    )?;
    assert_ne!(first.path(), second.path());
    let first_path = first.path().to_owned();
    let second_path = second.path().to_owned();
    #[cfg(target_os = "linux")]
    {
        let path = first.path().join("org.chromium.Chromium.123456");
        fs::create_dir(&path)?;
        let socket = path.join("SingletonSocket");
        assert!(socket.as_os_str().as_encoded_bytes().len() <= 107);
        let listener = std::os::unix::net::UnixListener::bind(socket)?;
        drop(listener);
    }
    first.finish()?;
    assert!(!first_path.exists());
    assert!(second_path.is_dir());
    second.finish()?;
    assert!(!second_path.exists());
    Ok(())
}

pub(super) fn log_safety() -> TestResult {
    let temp = Temp::new("safe log path with spaces")?;
    let mut paths = Vec::new();
    for _ in 0..2 {
        let log = numera_core::runtime_log::RuntimeLog::new(&temp.0, "migration-fixture");
        log.message("WARNING", "migration", "password=synthetic-fixture-value");
        log.event("INFO", "migration", "exit=0");
        paths.push(log.path().ok_or("Log file missing")?.to_owned());
    }
    assert_ne!(paths[0], paths[1]);
    for path in paths {
        let text = fs::read_to_string(&path)?;
        assert!(!text.contains("synthetic-fixture-value"));
        assert!(text.contains("[WARNING]") && text.contains("UTC]") && text.contains("exit=0"));
        fs::remove_file(path)?;
    }
    let blocked = temp.write("not-a-directory", b"fixture")?;
    let log = numera_core::runtime_log::RuntimeLog::new(&blocked, "migration-fallback");
    assert!(log.path().is_none());
    log.message("ERROR", "migration", "token=synthetic-fixture-value");
    let executable = compile(
        &temp,
        "fn main(){let directory=std::env::args_os().nth(1).unwrap();let log=numera_core::runtime_log::RuntimeLog::new(std::path::Path::new(&directory),\"migration-fallback\");assert!(log.path().is_none());log.message(\"ERROR\",\"migration\",\"token=synthetic-fixture-value\");}",
    )?;
    let output = ci::capture(
        Command::new(executable).arg(&blocked),
        Duration::from_secs(5),
        Duration::from_secs(5),
        4096,
    )?;
    assert_eq!(output.exit, 0);
    let console = std::str::from_utf8(&output.stderr)?;
    assert!(console.contains("file logging unavailable"));
    assert!(!console.contains("synthetic-fixture-value"));
    Ok(())
}
