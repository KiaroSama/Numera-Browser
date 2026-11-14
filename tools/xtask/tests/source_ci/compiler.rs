use super::*;
use ci::compiler as phase;
use numera_core::ownership::{
    capture,
    capture_units::Units,
    file_stat::{self, Stat},
};
use std::{
    fs::File,
    io::{Read, Write},
    process::Command,
    time::Duration,
};
fn trace(root: &Path, path: &Path) -> Value {
    json!({"reason":"compiler-message","manifest_path":root.join("engine-rust/Cargo.toml"),"target":{"name":"numera_core","kind":["lib"]},"message":{"level":"INFO","target":phase::TARGET,"fields":{"message":format!("preparing rlib to {}",serde_json::to_string(&path.to_string_lossy()).expect("pathJSON"))}}})
}
fn artifact(root: &Path, path: &Path, fresh: bool) -> Value {
    json!({"reason":"compiler-artifact","manifest_path":root.join("engine-rust/Cargo.toml"),"target":{"name":"numera_core","kind":["lib"]},"profile":{"test":false},"fresh":fresh,"filenames":[path]})
}
fn fixture(root: &Path) -> ci::Result<(PathBuf, Vec<u8>, Vec<u8>)> {
    let directory = root.join("engine-rust/target/debug/.fingerprint/numera-core-0123456789abcdef");
    fs::create_dir_all(&directory)?;
    let diagnostic = b"{\"message\":\"required diagnostic\",\"level\":\"warning\"}\r\n".to_vec();
    let mut raw = serde_json::to_vec(&trace(root, &root.join("private.rlib"))["message"])?;
    raw.push(b'\n');
    let mut all = diagnostic.clone();
    all.extend(&raw);
    let output = directory.join("output-lib-numera_core");
    fs::write(&output, &all)?;
    Ok((output, diagnostic, all))
}
fn event(scope: &phase::Scope, value: &Value) -> TestResult {
    scope.line(&serde_json::to_string(value)?);
    Ok(())
}
fn projection(damage: &str) -> TestResult {
    if damage == "None" && super::progress_support::child()? {
        return Ok(());
    }
    if damage == "None" && streamed_child()? {
        return Ok(());
    }
    let tmp = Temp::new("compiler-projection")?;
    let path = tmp
        .0
        .join("engine-rust/target/debug/deps/libnumera_core-0123456789abcdef.rlib");
    let mut tick = 0;
    let scope = phase::Scope::with_projection(
        phase::Projection::with_clock(&tmp.0, 7, move || {
            tick += 100;
            tick
        }),
        true,
    );
    let mut receipt = artifact(&tmp.0, &path, damage == "fresh");
    if damage == "foreign" {
        receipt["manifest_path"] = json!(tmp.0.join("foreign/Cargo.toml"));
    } else if damage == "wrong-file" {
        receipt["filenames"] = json!([path.with_file_name("foreign.rlib")]);
    }
    if damage == "reordered" {
        event(&scope, &receipt)?;
    }
    if !["missing", "fresh"].contains(&damage) {
        event(&scope, &trace(&tmp.0, &path))?;
    }
    if damage == "duplicate" {
        event(&scope, &trace(&tmp.0, &path))?;
    }
    if damage == "malformed" {
        scope.line("{\"private malformed compiler data");
    }
    if damage != "reordered" {
        event(&scope, &receipt)?;
    }
    event(
        &scope,
        &json!({"reason":"build-script-executed","env":[["PRIVATE_VALUE","never-publish"]]}),
    )?;
    if damage != "unfinished" {
        event(&scope, &json!({"reason":"build-finished","success":true}))?;
    }
    scope.line("running 1 test");
    let (result, rows) = scope.finish(Ok(()), Ok(0));
    result?;
    assert!(!phase::active());
    let joined = rows.join("\n");
    for forbidden in [
        tmp.0.to_string_lossy().as_ref(),
        "private directory",
        "never-publish",
        "PRIVATE_VALUE",
        "manifest_path",
        "numera_core-",
    ] {
        assert!(
            !joined.contains(forbidden),
            "compiler projection published raw metadata"
        );
    }
    assert!(rows.contains(&"running 1 test".into()));
    let status = if damage == "None" {
        "complete"
    } else if damage == "fresh" {
        "fresh"
    } else {
        "unavailable"
    };
    let summary = rows.last().expect("summary");
    assert!(summary.contains(&format!("status={status}")));
    assert!(summary.contains("receipt-interval-not-exclusive-compiler"));
    if damage == "None" {
        assert!(
            rows.iter()
                .any(|s| s.contains("event=archive-start elapsed_ns=100"))
        );
        assert!(rows.iter().any(|s| {
            s.contains("event=archive-complete elapsed_ns=200 archive_to_artifact_ns=100")
        }));
        live_stream()?;
        super::progress_support::controls()?;
    }
    Ok(())
}
// This is a native libtest child, never a product environment branch. A local
// acknowledgment makes the two real pipe receipts observably separate.
fn streamed_child() -> ci::Result<bool> {
    const MARKER: &str = "NUMERA_COMPILER_STREAM_TEST";
    let name = std::thread::current().name().unwrap_or("").to_owned();
    if std::env::var(MARKER).as_deref() != Ok(name.as_str()) {
        return Ok(false);
    }
    let root = PathBuf::from(
        std::env::var_os("NUMERA_COMPILER_STREAM_ROOT").ok_or("fixture root missing")?,
    );
    let path = root.join("engine-rust/target/debug/deps/libnumera_core-0123456789abcdef.rlib");
    let mut peer = std::net::TcpStream::connect(std::env::var("NUMERA_COMPILER_STREAM_SOCKET")?)?;
    peer.set_read_timeout(Some(Duration::from_secs(3)))?;
    writeln!(std::io::stderr(), "{}", trace(&root, &path))?;
    std::io::stderr().flush()?;
    let mut ack = [0];
    peer.read_exact(&mut ack)?;
    std::thread::sleep(Duration::from_millis(25));
    writeln!(std::io::stderr(), "{}", artifact(&root, &path, false))?;
    writeln!(
        std::io::stderr(),
        "{}",
        json!({"reason":"build-finished","success":true})
    )?;
    Ok(true)
}
fn live_stream() -> TestResult {
    let tmp = Temp::new("compiler-stream")?;
    let name = std::thread::current()
        .name()
        .ok_or("test name missing")?
        .to_owned();
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--exact", &name, "--nocapture", "--test-threads=1"])
        .env("NUMERA_COMPILER_STREAM_TEST", &name)
        .env("NUMERA_COMPILER_STREAM_ROOT", &tmp.0)
        .env(
            "NUMERA_COMPILER_STREAM_SOCKET",
            listener.local_addr()?.to_string(),
        );
    let mut units = Units::new(&tmp.0, &mut command)?;
    let scope = phase::Scope::new(&tmp.0, 0, true);
    let mut acked = false;
    let mut callback_error = None;
    let output = units.run_observed_streamed(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(5),
            idle: Duration::from_secs(3),
            bytes: 262144,
            cleanup: Duration::from_secs(3),
        },
        || false,
        |_, _| {},
        |index, bytes| {
            if index == 1 {
                phase::observe_current(index, bytes);
                if !acked && bytes.contains(&b'\n') {
                    let end = std::time::Instant::now() + Duration::from_secs(2);
                    loop {
                        match listener.accept() {
                            Ok((mut peer, _)) => {
                                if let Err(error) = peer.write_all(b"1") {
                                    callback_error = Some(error);
                                }
                                acked = true;
                                break;
                            }
                            Err(e)
                                if e.kind() == io::ErrorKind::WouldBlock
                                    && std::time::Instant::now() < end =>
                            {
                                std::thread::sleep(Duration::from_millis(2))
                            }
                            Err(e) => {
                                callback_error = Some(e);
                                break;
                            }
                        }
                    }
                }
            }
        },
    )?;
    units.settle(Duration::from_secs(2))?;
    if let Some(error) = callback_error {
        return Err(error.into());
    }
    assert_eq!(output.exit, 0, "native streaming fixture failed");
    let (result, rows) = scope.finish(Ok(()), Ok(0));
    result?;
    assert!(rows.last().expect("summary").contains("status=complete"));
    let interval = rows
        .iter()
        .find_map(|line| line.split("archive_to_artifact_ns=").nth(1))
        .ok_or("live interval missing")?
        .parse::<u64>()?;
    assert!(
        interval >= 20_000_000,
        "compiler receipts were projected after buffering"
    );
    assert!(!rows.join("\n").contains("manifest_path"));
    Ok(())
}
fn malformed(payload: Value) -> TestResult {
    let tmp = Temp::new("compiler-json")?;
    let scope = phase::Scope::new(&tmp.0, 0, true);
    event(&scope, &json!({"private":payload}))?;
    scope.line(&format!("{{{}", "private".repeat(10000)));
    let (result, rows) = scope.finish(Ok(()), Ok(0));
    result?;
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains("status=unavailable"));
    assert!(!rows[0].contains("private"));
    Ok(())
}
fn scoped(enabled: bool, failure: bool) -> TestResult {
    let tmp = Temp::new("compiler-scope")?;
    let parent = BTreeMap::from([
        ("RUSTC_LOG".into(), "existing-filter".into()),
        (
            "RUSTC_LOG_OUTPUT_TARGET".into(),
            "private-existing-file".into(),
        ),
        ("RUSTC_LOG_FORMAT_JSON".into(), "prior-format".into()),
        ("RUSTC_LOG_BACKTRACE".into(), "prior-backtrace".into()),
    ]);
    let before = parent.clone();
    let outer = phase::Scope::new(&tmp.0, usize::MAX, true);
    let scope = phase::Scope::new(&tmp.0, 0, enabled);
    let mut command = Command::new("cargo");
    command.envs(&parent);
    phase::configure(&mut command, enabled);
    let mut actual = parent.clone();
    for (key, value) in command.get_envs() {
        let key = key.to_string_lossy().into_owned();
        if let Some(value) = value {
            actual.insert(key, value.to_string_lossy().into_owned());
        } else {
            actual.remove(&key);
        }
    }
    assert_eq!(actual, phase::environment(&parent, enabled));
    if enabled {
        assert_eq!(
            actual,
            BTreeMap::from([
                ("RUSTC_LOG".into(), format!("{}=info", phase::TARGET)),
                ("RUSTC_LOG_FORMAT_JSON".into(), "1".into()),
                ("RUSTC_LOG_COLOR".into(), "never".into())
            ])
        );
    } else {
        assert_eq!(actual, parent);
    }
    let original: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(
        io::ErrorKind::TimedOut,
        "original proof error",
    ));
    let address = (&*original) as *const dyn std::error::Error as *const ();
    let result = if failure { Err(original) } else { Ok(()) };
    let (result, _) = scope.finish(result, Ok(0));
    if failure {
        let error = result.expect_err("original error");
        assert_eq!(
            (&*error) as *const dyn std::error::Error as *const (),
            address
        );
    } else {
        result?;
    }
    assert_eq!(parent, before);
    phase::observe_current(0, b"Owned untouched subscriber\n");
    let (result, rows) = outer.finish(Ok(()), Ok(0));
    result?;
    assert!(rows.contains(&"Owned untouched subscriber".into()));
    assert!(!phase::active());
    assert!(!tmp.0.join("private-existing-file").exists());
    Ok(())
}
fn proof(platform: &str, failure: bool, invalid: bool) -> TestResult {
    let tmp = Temp::new("compiler-proof")?;
    let source = tmp.write("engine-rust/src/models.rs", b"fixed")?;
    let old = b"previous defect";
    let case = history::Case {
        target: "model_parity".into(),
        test: "fixture_named_assertion".into(),
        symptom: "fixture required assertion".into(),
        commit: "a".repeat(40),
        files: BTreeMap::from([("engine-rust/src/models.rs".into(), ci::sha(old))]),
        companions: BTreeMap::new(),
        platform: None,
    };
    let mut inputs = BTreeMap::from([(
        (
            case.commit.clone(),
            "engine-rust/src/models.rs".into(),
            ci::sha(old),
        ),
        old.to_vec(),
    )]);
    let mut calls = Vec::new();
    let original: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(
        io::ErrorKind::TimedOut,
        "original proof failure",
    ));
    let address = (&*original) as *const dyn std::error::Error as *const ();
    let mut original = Some(original);
    let mut cleanup_calls = 0;
    let result = history::prove_cases_with_cleanup(
        &tmp.0,
        std::slice::from_ref(&case),
        platform,
        &mut inputs,
        |compile, case, command| {
            assert_eq!(fs::read(&source)?, old);
            let argv = command
                .get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            calls.push(if compile { "cold" } else { "proof" });
            assert_eq!(
                argv.contains(&"--message-format=json".into()),
                !compile && platform == "win32"
            );
            if !compile {
                let limits = history::proof_limits(false);
                assert_eq!(limits.wall, Duration::from_secs(180));
                assert_eq!(limits.idle, Duration::from_secs(90));
                assert_eq!(
                    &argv[argv.len() - 4..],
                    [
                        case.test.clone(),
                        "--exact".into(),
                        "--test-threads=1".into(),
                        "--color=never".into()
                    ]
                );
                if failure {
                    return Err(original.take().expect("original once"));
                }
            }
            let stdout = if compile || invalid {
                Vec::new()
            } else {
                failed_transcript(&case.test, &case.symptom).into_bytes()
            };
            Ok(ci::owned::Output {
                command: capture::Output {
                    exit: if compile { 0 } else { 101 },
                    stdout,
                    stderr: Vec::new(),
                },
                memory: Value::Null,
            })
        },
        |_| {
            cleanup_calls += 1;
            if invalid {
                Err(phase::CacheError::new("os"))
            } else {
                Ok(0)
            }
        },
    );
    assert_eq!(calls, ["cold", "proof"]);
    assert_eq!(fs::read(&source)?, b"fixed");
    assert!(!phase::active());
    assert_eq!(cleanup_calls, if platform == "win32" { 2 } else { 0 });
    if failure {
        let error = result.expect_err("original proof error");
        assert_eq!(
            (&*error) as *const dyn std::error::Error as *const (),
            address
        );
    } else if invalid {
        assert!(
            result
                .expect_err("invalid semantic proof")
                .to_string()
                .contains("Historical native regression did not fail")
        );
    } else {
        result?;
    }
    if platform == "linux" && !failure && !invalid {
        fixture_bridge_controls()?;
        let metadata = history::Registry::read()?
            .cases()?
            .into_iter()
            .find(|row| row.target == "native_environment")
            .ok_or("Metadata historical case missing")?;
        let path = "engine-rust/src/native_environment.rs";
        let old = history::case_sources(&metadata)?
            .remove(path)
            .ok_or("Metadata source missing")?;
        let fixed = fs::read(root().join(path))?;
        let replacement = ci::history_compat::replacement(&metadata, path, &old, &fixed)?
            .ok_or("Metadata semantic adaptation missing")?;
        assert_eq!(
            replacement.receipt["mode"],
            "native-equivalent-semantic-mutation"
        );
        assert_eq!(replacement.receipt["literal_historical_execution"], false);
        assert_eq!(
            replacement.receipt["current_before_sha256"],
            ci::sha(&fixed)
        );
        assert_eq!(
            replacement.receipt["mutated_after_sha256"],
            ci::sha(&replacement.bytes)
        );
        assert!(String::from_utf8_lossy(&replacement.bytes).contains(".numera-metadata-cache"));
        let mut unreviewed = metadata.clone();
        unreviewed.commit = "b".repeat(40);
        assert!(ci::history_compat::replacement(&unreviewed, path, &old, &fixed).is_err());
        assert!(
            ci::history_compat::replacement(
                &metadata,
                path,
                &old,
                b"different native source shape"
            )
            .is_err()
        );
        let synthetic = "q".repeat(1000);
        let refused = history::prove_cases_with(
            &tmp.0,
            std::slice::from_ref(&case),
            platform,
            &mut inputs,
            |compile, _, _| {
                assert!(
                    compile,
                    "A compiler failure must prevent semantic execution"
                );
                Ok(ci::owned::Output {
                    command: capture::Output {
                        exit: 101,
                        stdout: Vec::new(),
                        stderr: format!("error: independent native dependency is unavailable offline\ntoken={synthetic}\n").into_bytes(),
                    },
                    memory: Value::Null,
                })
            },
        );
        assert!(
            refused
                .expect_err("Compilation must precede semantic proof")
                .to_string()
                .contains("Historical source must compile")
        );
        let path = tmp.0.join("build/history").join(format!(
            "compile-{}.stderr.log",
            ci::sha(format!("{}:{}", case.target, case.test).as_bytes())
        ));
        let diagnostic = fs::read_to_string(path)?;
        assert!(diagnostic.contains("independent native dependency"));
        assert!(diagnostic.contains("[REDACTED]"));
        assert!(!diagnostic.contains(&synthetic));
        assert_eq!(fs::read(&source)?, b"fixed");
    }
    if invalid {
        let poisoned = history::prove_cases_with(
            &tmp.0,
            std::slice::from_ref(&case),
            "win32",
            &mut inputs,
            |compile, case, _| {
                let stdout = if compile {
                    Vec::new()
                } else {
                    format!(
                        "{}\n{}",
                        json!({"reason":"compiler-message","message":{"rendered":case.symptom}}),
                        failed_transcript(&case.test, "different production assertion")
                    )
                    .into_bytes()
                };
                Ok(ci::owned::Output {
                    command: capture::Output {
                        exit: if compile { 0 } else { 101 },
                        stdout,
                        stderr: Vec::new(),
                    },
                    memory: Value::Null,
                })
            },
        );
        assert!(
            poisoned
                .expect_err(
                    "compiler diagnostics must not manufacture the required runtime assertion"
                )
                .to_string()
                .contains("Historical native regression did not fail")
        );
        let (cache, diagnostic, _) = fixture(&tmp.0)?;
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            history::prove_cases_with(
                &tmp.0,
                std::slice::from_ref(&case),
                "win32",
                &mut inputs,
                |compile, _, _| {
                    if compile {
                        return Ok(ci::owned::Output {
                            command: capture::Output {
                                exit: 0,
                                stdout: Vec::new(),
                                stderr: Vec::new(),
                            },
                            memory: Value::Null,
                        });
                    }
                    // Only the post-proof pass can remove this newly emitted record.
                    let _ = fixture(&tmp.0)?;
                    panic!("controlled native proof unwind");
                },
            )
        }));
        assert!(unwind.is_err());
        assert_eq!(fs::read(&cache)?, diagnostic);
        assert_eq!(fs::read(tmp.0.join("engine-rust/src/models.rs"))?, b"fixed");
        assert!(!phase::active());
    }
    Ok(())
}

fn fixture_bridge_controls() -> TestResult {
    let root = root();
    let cases = history::Registry::read()?.cases()?;
    for (target, old_hash, count) in [
        (
            "cli_profile_parity",
            "656a8fc05ffd03f89914c586cb0fa2bc66ffde8c7f23d292269a4c702f904fe9",
            0,
        ),
        (
            "config_parity",
            "3bdc096885588ad4640650934e1118c5068c112afb045d9815be6586f4432c1a",
            2,
        ),
        (
            "config_parity",
            "a13add91f09aca8457fee8f40e50e2bc999808f9282c0a068a51a395afd24574",
            5,
        ),
        (
            "integer_seed_parity",
            "a00dffeec7f9fc93972b58cc4696958b5f4f7d6a2e37b7b90fe708a7365107cb",
            1,
        ),
        (
            "lock_interop_parity",
            "48b2c974fbf8761af26708db181fe5fae3e974216dbb6a73bc68a95a538b3b84",
            5,
        ),
        (
            "repo_lookup_parity",
            "3267e368991ce775f580b66f2068211d2835dddcf98eb37c205a2e371386d406",
            3,
        ),
        (
            "schedule_store_parity",
            "0419aa623eb9ec63e9d442af7e683756e7d4e04436c56ef0537691c6386504a4",
            2,
        ),
    ] {
        let path = format!("engine-rust/tests/{target}.rs");
        let (case, source) = cases
            .iter()
            .filter(|case| case.target == target || count == 0)
            .find_map(|case| {
                history::case_sources(case)
                    .ok()?
                    .remove(&path)
                    .filter(|source| source.sha256 == old_hash)
                    .map(|source| (case, source))
            })
            .ok_or("Reviewed historical fixture case missing")?;
        let current = fs::read(root.join(&path))?;
        let old = ci::git(&root, &["show", &format!("{}:{path}", source.commit)])?;
        assert_eq!(ci::sha(&old), old_hash);
        if count == 0 {
            // This companion is never selected by an owning --test command;
            // its literal Git bytes need no transport bridge.
            assert!(
                ci::history_compat::fixture_replacement(
                    &root, case, &path, &source, &current, &old
                )?
                .is_none()
            );
            continue;
        }
        let bridge =
            ci::history_compat::fixture_replacement(&root, case, &path, &source, &current, &old)?
                .ok_or("Reviewed fixture bridge missing")?;
        assert_eq!(bridge.receipt["mode"], "reviewed-native-fixture-bridge");
        assert_eq!(bridge.receipt["literal_historical_execution"], false);
        assert_eq!(
            bridge.receipt["original_tests"]
                .as_array()
                .ok_or("Original names missing")?
                .len(),
            count
        );
        assert_eq!(
            bridge.receipt["mutated_after_sha256"],
            ci::sha(&bridge.bytes)
        );
        let changed = [current.as_slice(), b"\n// unreviewed change"].concat();
        assert!(
            ci::history_compat::fixture_replacement(&root, case, &path, &source, &changed, &old)
                .is_err()
        );
        let mut wrong = source.clone();
        wrong.sha256 = "0".repeat(64);
        assert!(
            ci::history_compat::fixture_replacement(&root, case, &path, &wrong, &current, &old)?
                .is_none()
        );
        for (production, selected) in &case.files {
            if production.starts_with("engine-rust/src/") {
                let selected = history::Source {
                    commit: case.commit.clone(),
                    sha256: selected.clone(),
                };
                assert!(
                    ci::history_compat::fixture_replacement(
                        &root, case, production, &selected, &current, &old
                    )?
                    .is_none()
                );
                assert!(
                    ci::history_compat::companion_replacement(case, production, &selected, &old)?
                        .is_none()
                );
            }
        }
    }
    Ok(())
}
struct Access {
    fault: Option<phase::Step>,
    reads: usize,
    platform: Option<String>,
    damage: Option<String>,
}
impl Access {
    fn new() -> Self {
        Self {
            fault: None,
            reads: 0,
            platform: None,
            damage: None,
        }
    }
}
impl phase::Access for Access {
    fn before(
        &mut self,
        step: phase::Step,
        _: &Path,
    ) -> std::result::Result<(), phase::CacheError> {
        if step == phase::Step::Read {
            self.reads += 1;
        }
        if self.fault == Some(step) {
            return Err(phase::CacheError::new("os"));
        }
        Ok(())
    }
    fn path(&mut self, path: &Path) -> std::result::Result<Stat, phase::CacheError> {
        let mut stat = file_stat::path(path)?;
        if self.platform.is_some() {
            stat.changed_ns = 100;
            stat.birth_ns = if self.damage.as_deref() == Some("legacy") {
                None
            } else {
                Some(100)
            };
        }
        Ok(stat)
    }
    fn file(&mut self, file: &File) -> std::result::Result<Stat, phase::CacheError> {
        let mut stat = file_stat::file(file)?;
        if let Some(platform) = self.platform.as_deref() {
            stat.changed_ns = if platform == "win32" && self.damage.as_deref() != Some("legacy") {
                200
            } else {
                100
            };
            stat.birth_ns = if self.damage.as_deref() == Some("legacy") {
                None
            } else {
                Some(100)
            };
            if self.damage.as_deref() == Some("inode") {
                stat.inode += 1;
            }
            if self.damage.as_deref() == Some("change-time") && platform != "win32" {
                stat.changed_ns += 1;
            }
        }
        Ok(stat)
    }
}
fn cache(damage: &str) -> TestResult {
    let tmp = Temp::new("compiler-cache")?;
    let (path, diagnostic, mut payload) = fixture(&tmp.0)?;
    let untouched = path.with_file_name("lib-numera_core.json");
    fs::write(&untouched, b"fingerprint bytes must not change")?;
    let mut access = Access::new();
    let mut limits = phase::Limits::default();
    match damage {
        "malformed" => {
            payload.extend_from_slice(b"{private malformed record\n");
            fs::write(&path, &payload)?;
        }
        "oversized" => limits.bytes = 32,
        "linked" => fs::hard_link(&path, path.with_file_name("shared-output"))?,
        "cleanup-error" => access.fault = Some(phase::Step::Read),
        _ => (),
    };
    let scope = phase::Scope::new(&tmp.0, 0, true);
    let cleaned = phase::sanitize_with(
        &tmp.0,
        &mut phase::Seen::new(),
        &mut access,
        limits,
        if cfg!(windows) { "win32" } else { "linux" },
    );
    let (result, rows) = scope.finish(Ok(()), cleaned);
    if damage == "None" {
        result?;
        assert_eq!(fs::read(&path)?, diagnostic);
        assert!(rows.iter().any(|r| r.contains("cache_records_removed=1")));
    } else {
        assert_eq!(
            result.expect_err("uncertain compiler cache").to_string(),
            "Compiler diagnostic cache could not be sanitized"
        );
        assert_eq!(fs::read(&path)?, payload);
        let cause = match damage {
            "malformed" => "json",
            "oversized" => "bytes",
            "linked" => "ownership",
            _ => "os",
        };
        assert!(rows.iter().any(|r| r.contains(&format!(
            "status=cache-cleanup-failed cause={cause} errno=0 winerror=0"
        ))));
    }
    assert_eq!(fs::read(untouched)?, b"fingerprint bytes must not change");
    assert!(!phase::active());
    assert!(rows.iter().all(|r| !r.contains("private")));
    Ok(())
}
fn large(kind: &str) -> TestResult {
    let tmp = Temp::new("compiler-large-cache")?;
    let (path, diagnostic, _) = fixture(&tmp.0)?;
    let mut event = trace(&tmp.0, &tmp.0.join("private.rlib"))["message"].clone();
    event["fields"]["message"] =
        format!("private payload {}", "x".repeat(phase::LINE_BYTES)).into();
    if kind == "warning" {
        event["level"] = "WARN".into();
    } else if kind == "foreign" {
        event["target"] = "unrelated::compiler".into();
    }
    let mut raw = serde_json::to_vec(&event)?;
    raw.push(b'\n');
    if kind == "malformed" {
        raw.truncate(raw.len() - 2);
        raw.push(b'\n');
    }
    assert!(raw.len() > phase::LINE_BYTES && raw.len() < phase::Limits::default().bytes as usize);
    let mut payload = diagnostic.clone();
    payload.extend(raw);
    fs::write(&path, &payload)?;
    let mut seen = phase::Seen::new();
    let scope = phase::Scope::new(&tmp.0, 0, true);
    let (result, rows) = scope.finish(Ok(()), phase::sanitize(&tmp.0, &mut seen));
    if kind == "scoped" {
        result?;
        assert_eq!(fs::read(&path)?, diagnostic);
        assert!(seen.contains_key(&path));
        assert!(rows.iter().any(|r| r.contains("cache_records_removed=1")));
    } else {
        assert!(result.is_err());
        assert_eq!(fs::read(&path)?, payload);
        assert!(seen.is_empty());
    }
    assert!(!phase::active());
    assert!(rows.iter().all(|r| !r.contains("private")));
    assert_eq!(
        fs::read_dir(path.parent().expect("cache directory"))?.count(),
        1
    );
    Ok(())
}
fn cleanup(failure: bool, cause: &'static str, codes: Value, expected: Value) -> TestResult {
    let tmp = Temp::new("compiler-cleanup")?;
    let outer = phase::Scope::new(&tmp.0, usize::MAX, true);
    let scope = phase::Scope::new(&tmp.0, 0, true);
    let error = phase::CacheError::codes(cause, &codes[0], &codes[1]);
    let original: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(
        io::ErrorKind::TimedOut,
        "original proof timeout",
    ));
    let address = (&*original) as *const dyn std::error::Error as *const ();
    let result = if failure { Err(original) } else { Ok(()) };
    let (result, rows) = scope.finish(result, Err(error));
    let caught = result.expect_err("cleanup failure");
    if failure {
        assert_eq!(
            (&*caught) as *const dyn std::error::Error as *const (),
            address
        );
    } else {
        assert_eq!(
            caught.to_string(),
            "Compiler diagnostic cache could not be sanitized"
        );
    }
    let expected = format!(
        "Historical compiler index=0 status=cache-cleanup-failed cause={cause} errno={} winerror={}",
        expected[0], expected[1]
    );
    assert!(rows.contains(&expected));
    assert!(rows.iter().all(|r| !r.contains("private")));
    phase::observe_current(0, b"Owned untouched subscriber\n");
    let (result, outer_rows) = outer.finish(Ok(()), Ok(0));
    result?;
    assert!(outer_rows.contains(&"Owned untouched subscriber".into()));
    assert!(!phase::active());
    Ok(())
}
fn cache_policy(damage: &str) -> TestResult {
    let document = workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/native-ci.yml"),
    )?)?;
    let mut steps = document["jobs"]["history"]["steps"].clone();
    let cache = steps
        .as_array_mut()
        .ok_or("steps missing")?
        .iter_mut()
        .find(|s| {
            s["uses"]
                .as_str()
                .is_some_and(|u| u.starts_with("Swatinem/rust-cache@"))
        })
        .ok_or("cache missing")?;
    match damage {
        "missing" => {
            cache["with"]
                .as_object_mut()
                .expect("cachewith")
                .remove("save-if");
        }
        "save" => cache["with"]["save-if"] = "true".into(),
        "workspace" => cache["with"]["cache-workspace-crates"] = "true".into(),
        _ => (),
    };
    let result = phase::cache_policy(&steps);
    if damage == "None" {
        result?;
    } else {
        assert!(
            result
                .expect_err("persistent compiler traces")
                .to_string()
                .contains("Historical compiler diagnostics must not enter Windows caches")
        );
    }
    Ok(())
}
fn atomic(damage: &str) -> TestResult {
    let tmp = Temp::new("compiler-atomic")?;
    let (path, diagnostic, payload) = fixture(&tmp.0)?;
    let mut seen = phase::Seen::new();
    let mut access = Access::new();
    access.fault = match damage {
        "write" => Some(phase::Step::Write),
        "flush" => Some(phase::Step::Flush),
        "replace" => Some(phase::Step::Replace),
        _ => None,
    };
    let result = phase::sanitize_with(
        &tmp.0,
        &mut seen,
        &mut access,
        phase::Limits::default(),
        if cfg!(windows) { "win32" } else { "linux" },
    );
    if damage == "None" {
        assert_eq!(result?, 1);
        assert_eq!(fs::read(&path)?, diagnostic);
    } else {
        assert_eq!(result.expect_err("real atomic boundary fault").cause, "os");
        assert_eq!(fs::read(&path)?, payload);
        assert!(seen.is_empty());
    }
    assert_eq!(
        fs::read_dir(path.parent().expect("cache directory"))?.count(),
        1
    );
    Ok(())
}
fn unchanged() -> TestResult {
    let tmp = Temp::new("compiler-unchanged")?;
    let (path, diagnostic, _) = fixture(&tmp.0)?;
    fs::write(&path, &diagnostic)?;
    let before = file_stat::path(&path)?;
    let mut access = Access::new();
    let mut seen = phase::Seen::new();
    for _ in 0..2 {
        assert_eq!(
            phase::sanitize_with(
                &tmp.0,
                &mut seen,
                &mut access,
                phase::Limits::default(),
                if cfg!(windows) { "win32" } else { "linux" }
            )?,
            0
        );
    }
    assert_eq!(access.reads, 1);
    assert_eq!(file_stat::path(&path)?, before);
    assert_eq!(fs::read(path)?, diagnostic);
    Ok(())
}
fn unicode() -> TestResult {
    let tmp = Temp::new("compiler-unicode")?;
    let root = tmp.0.join("unicode\u{200d}path");
    let path = root.join("engine-rust/target/debug/deps/libnumera_core-0123456789abcdef.rlib");
    let mut trace = trace(&root, &path);
    let quoted = serde_json::to_string(&path.to_string_lossy())?.replace('\u{200d}', r"\u{200d}");
    trace["message"]["fields"]["message"] = format!("preparing rlib to {quoted}").into();
    let scope = phase::Scope::new(&root, 0, true);
    for value in [
        trace,
        artifact(&root, &path, false),
        json!({"reason":"build-finished","success":true}),
    ] {
        event(&scope, &value)?;
    }
    let (result, rows) = scope.finish(Ok(()), Ok(0));
    result?;
    assert!(rows.last().expect("summary").contains("status=complete"));
    assert!(
        rows.iter()
            .all(|r| !r.contains("unicode") && !r.contains(path.to_string_lossy().as_ref()))
    );
    Ok(())
}
fn stat_contract(platform: &str, damage: &str) -> TestResult {
    let tmp = Temp::new("compiler-stat")?;
    let (path, diagnostic, payload) = fixture(&tmp.0)?;
    let mut access = Access::new();
    access.platform = Some(platform.into());
    access.damage = Some(damage.into());
    let scope = phase::Scope::new(&tmp.0, 0, true);
    let cleaned = phase::sanitize_with(
        &tmp.0,
        &mut phase::Seen::new(),
        &mut access,
        phase::Limits::default(),
        platform,
    );
    let (result, _) = scope.finish(Ok(()), cleaned);
    if damage == "inode" || (damage == "change-time" && platform != "win32") {
        assert!(result.is_err());
        assert_eq!(fs::read(path)?, payload);
    } else {
        result?;
        assert_eq!(fs::read(path)?, diagnostic);
    }
    assert!(!phase::active());
    Ok(())
}
// Generated individually reported source cases follow.
source_test!(
    c000_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[None]",
    { projection("None") }
);
source_test!(
    c001_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[missing]",
    { projection("missing") }
);
source_test!(
    c002_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[duplicate]",
    { projection("duplicate") }
);
source_test!(
    c003_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[malformed]",
    { projection("malformed") }
);
source_test!(
    c004_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[foreign]",
    { projection("foreign") }
);
source_test!(
    c005_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[wrong-file]",
    { projection("wrong-file") }
);
source_test!(
    c006_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[reordered]",
    { projection("reordered") }
);
source_test!(
    c007_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[unfinished]",
    { projection("unfinished") }
);
source_test!(
    c008_phase_projection_requires_one_matching_archive_and_artifact,
    "tests/test_native_history_phase.py::test_phase_projection_requires_one_matching_archive_and_artifact[fresh]",
    { projection("fresh") }
);
source_test!(
    c000_malformed_json_never_reaches_handlers,
    "tests/test_native_history_phase.py::test_malformed_json_never_reaches_handlers[None]",
    { malformed(Value::Null) }
);
source_test!(
    c001_malformed_json_never_reaches_handlers,
    "tests/test_native_history_phase.py::test_malformed_json_never_reaches_handlers[payload1]",
    { malformed(json!([])) }
);
source_test!(
    c002_malformed_json_never_reaches_handlers,
    "tests/test_native_history_phase.py::test_malformed_json_never_reaches_handlers[private text]",
    { malformed(json!("private text")) }
);
source_test!(
    c003_malformed_json_never_reaches_handlers,
    "tests/test_native_history_phase.py::test_malformed_json_never_reaches_handlers[payload3]",
    { malformed(json!({"fields": null})) }
);
source_test!(
    c004_malformed_json_never_reaches_handlers,
    "tests/test_native_history_phase.py::test_malformed_json_never_reaches_handlers[payload4]",
    { malformed(json!({"reason": "build-finished", "success": "private"})) }
);
source_test!(
    c005_malformed_json_never_reaches_handlers,
    "tests/test_native_history_phase.py::test_malformed_json_never_reaches_handlers[payload5]",
    { malformed(json!({"target": "rustc_codegen_ssa::back::link", "fields": {"message": []}})) }
);
source_test!(
    c000_scoped_environment_and_logger_restore_after_success_or_failure,
    "tests/test_native_history_phase.py::test_scoped_environment_and_logger_restore_after_success_or_failure[False-False]",
    { scoped(false, false) }
);
source_test!(
    c001_scoped_environment_and_logger_restore_after_success_or_failure,
    "tests/test_native_history_phase.py::test_scoped_environment_and_logger_restore_after_success_or_failure[False-True]",
    { scoped(true, false) }
);
source_test!(
    c002_scoped_environment_and_logger_restore_after_success_or_failure,
    "tests/test_native_history_phase.py::test_scoped_environment_and_logger_restore_after_success_or_failure[True-False]",
    { scoped(false, true) }
);
source_test!(
    c003_scoped_environment_and_logger_restore_after_success_or_failure,
    "tests/test_native_history_phase.py::test_scoped_environment_and_logger_restore_after_success_or_failure[True-True]",
    { scoped(true, true) }
);
source_test!(
    c000_public_proof_preserves_assertions_source_and_diagnostic_scope,
    "tests/test_native_history_phase.py::test_public_proof_preserves_assertions_source_and_diagnostic_scope[False-win32]",
    { proof("win32", false, false) }
);
source_test!(
    c001_public_proof_preserves_assertions_source_and_diagnostic_scope,
    "tests/test_native_history_phase.py::test_public_proof_preserves_assertions_source_and_diagnostic_scope[False-linux]",
    { proof("linux", false, false) }
);
source_test!(
    c002_public_proof_preserves_assertions_source_and_diagnostic_scope,
    "tests/test_native_history_phase.py::test_public_proof_preserves_assertions_source_and_diagnostic_scope[True-win32]",
    { proof("win32", true, false) }
);
source_test!(
    c003_public_proof_preserves_assertions_source_and_diagnostic_scope,
    "tests/test_native_history_phase.py::test_public_proof_preserves_assertions_source_and_diagnostic_scope[True-linux]",
    { proof("linux", true, false) }
);
source_test!(
    c000_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty,
    "tests/test_native_history_phase.py::test_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty[None]",
    { cache("None") }
);
source_test!(
    c001_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty,
    "tests/test_native_history_phase.py::test_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty[malformed]",
    { cache("malformed") }
);
source_test!(
    c002_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty,
    "tests/test_native_history_phase.py::test_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty[oversized]",
    { cache("oversized") }
);
source_test!(
    c003_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty,
    "tests/test_native_history_phase.py::test_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty[linked]",
    { cache("linked") }
);
source_test!(
    c004_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty,
    "tests/test_native_history_phase.py::test_generated_compiler_cache_preserves_diagnostics_and_refuses_uncertainty[cleanup-error]",
    { cache("cleanup-error") }
);
source_test!(
    c000_large_cached_trace_only_removes_the_exact_scoped_info,
    "tests/test_native_history_phase.py::test_large_cached_trace_only_removes_the_exact_scoped_info[scoped]",
    { large("scoped") }
);
source_test!(
    c001_large_cached_trace_only_removes_the_exact_scoped_info,
    "tests/test_native_history_phase.py::test_large_cached_trace_only_removes_the_exact_scoped_info[warning]",
    { large("warning") }
);
source_test!(
    c002_large_cached_trace_only_removes_the_exact_scoped_info,
    "tests/test_native_history_phase.py::test_large_cached_trace_only_removes_the_exact_scoped_info[foreign]",
    { large("foreign") }
);
source_test!(
    c003_large_cached_trace_only_removes_the_exact_scoped_info,
    "tests/test_native_history_phase.py::test_large_cached_trace_only_removes_the_exact_scoped_info[malformed]",
    { large("malformed") }
);
source_test!(
    c000_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes0-expected0-False]",
    { cleanup(false, "os", json!([13, 5]), json!([13, 5])) }
);
source_test!(
    c001_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes0-expected0-True]",
    { cleanup(true, "os", json!([13, 5]), json!([13, 5])) }
);
source_test!(
    c002_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes1-expected1-False]",
    { cleanup(false, "os", json!([-1, 4294967296_u64]), json!([0, 0])) }
);
source_test!(
    c003_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes1-expected1-True]",
    { cleanup(true, "os", json!([-1, 4294967296_u64]), json!([0, 0])) }
);
source_test!(
    c004_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes2-expected2-False]",
    { cleanup(false, "os", json!([true, "private code"]), json!([0, 0])) }
);
source_test!(
    c005_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes2-expected2-True]",
    { cleanup(true, "os", json!([true, "private code"]), json!([0, 0])) }
);
source_test!(
    c006_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes3-expected3-False]",
    { cleanup(false, "os", json!([null, null]), json!([0, 0])) }
);
source_test!(
    c007_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes3-expected3-True]",
    { cleanup(true, "os", json!([null, null]), json!([0, 0])) }
);
source_test!(
    c008_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes4-expected4-False]",
    {
        cleanup(
            false,
            "os",
            json!([4294967295_u64, 4294967295_u64]),
            json!([4294967295_u64, 4294967295_u64]),
        )
    }
);
source_test!(
    c009_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[os-codes4-expected4-True]",
    {
        cleanup(
            true,
            "os",
            json!([4294967295_u64, 4294967295_u64]),
            json!([4294967295_u64, 4294967295_u64]),
        )
    }
);
source_test!(
    c010_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[encoding-codes5-expected5-False]",
    { cleanup(false, "encoding", json!([0, 0]), json!([0, 0])) }
);
source_test!(
    c011_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[encoding-codes5-expected5-True]",
    { cleanup(true, "encoding", json!([0, 0]), json!([0, 0])) }
);
source_test!(
    c012_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[validation-codes6-expected6-False]",
    { cleanup(false, "validation", json!([0, 0]), json!([0, 0])) }
);
source_test!(
    c013_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[validation-codes6-expected6-True]",
    { cleanup(true, "validation", json!([0, 0]), json!([0, 0])) }
);
source_test!(
    c014_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[recursion-codes7-expected7-False]",
    { cleanup(false, "recursion", json!([0, 0]), json!([0, 0])) }
);
source_test!(
    c015_cache_cleanup_keeps_original_proof_error,
    "tests/test_native_history_phase.py::test_cache_cleanup_keeps_original_proof_error[recursion-codes7-expected7-True]",
    { cleanup(true, "recursion", json!([0, 0]), json!([0, 0])) }
);
source_test!(
    c000_windows_compiler_trace_cache_never_leaves_the_hosted_job,
    "tests/test_native_history_phase.py::test_windows_compiler_trace_cache_never_leaves_the_hosted_job[None]",
    { cache_policy("None") }
);
source_test!(
    c001_windows_compiler_trace_cache_never_leaves_the_hosted_job,
    "tests/test_native_history_phase.py::test_windows_compiler_trace_cache_never_leaves_the_hosted_job[missing]",
    { cache_policy("missing") }
);
source_test!(
    c002_windows_compiler_trace_cache_never_leaves_the_hosted_job,
    "tests/test_native_history_phase.py::test_windows_compiler_trace_cache_never_leaves_the_hosted_job[save]",
    { cache_policy("save") }
);
source_test!(
    c003_windows_compiler_trace_cache_never_leaves_the_hosted_job,
    "tests/test_native_history_phase.py::test_windows_compiler_trace_cache_never_leaves_the_hosted_job[workspace]",
    { cache_policy("workspace") }
);
source_test!(
    c000_atomic_cache_filter_keeps_original_bytes_until_replacement,
    "tests/test_native_history_phase.py::test_atomic_cache_filter_keeps_original_bytes_until_replacement[None]",
    { atomic("None") }
);
source_test!(
    c001_atomic_cache_filter_keeps_original_bytes_until_replacement,
    "tests/test_native_history_phase.py::test_atomic_cache_filter_keeps_original_bytes_until_replacement[write]",
    { atomic("write") }
);
source_test!(
    c002_atomic_cache_filter_keeps_original_bytes_until_replacement,
    "tests/test_native_history_phase.py::test_atomic_cache_filter_keeps_original_bytes_until_replacement[flush]",
    { atomic("flush") }
);
source_test!(
    c003_atomic_cache_filter_keeps_original_bytes_until_replacement,
    "tests/test_native_history_phase.py::test_atomic_cache_filter_keeps_original_bytes_until_replacement[replace]",
    { atomic("replace") }
);
source_test!(
    c000_unchanged_diagnostic_cache_needs_no_write_and_is_not_reread,
    "tests/test_native_history_phase.py::test_unchanged_diagnostic_cache_needs_no_write_and_is_not_reread",
    { unchanged() }
);
source_test!(
    c000_rust_debug_unicode_path_binds_to_cargo_artifact,
    "tests/test_native_history_phase.py::test_rust_debug_unicode_path_binds_to_cargo_artifact",
    { unicode() }
);
source_test!(
    c000_returned_invalid_proof_takes_precedence_over_cache_cleanup,
    "tests/test_native_history_phase.py::test_returned_invalid_proof_takes_precedence_over_cache_cleanup",
    { proof("win32", false, true) }
);
source_test!(
    c000_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[None-win32]",
    { stat_contract("win32", "None") }
);
source_test!(
    c001_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[None-linux]",
    { stat_contract("linux", "None") }
);
source_test!(
    c002_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[inode-win32]",
    { stat_contract("win32", "inode") }
);
source_test!(
    c003_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[inode-linux]",
    { stat_contract("linux", "inode") }
);
source_test!(
    c004_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[change-time-win32]",
    { stat_contract("win32", "change-time") }
);
source_test!(
    c005_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[change-time-linux]",
    { stat_contract("linux", "change-time") }
);
source_test!(
    c006_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[legacy-win32]",
    { stat_contract("win32", "legacy") }
);
source_test!(
    c007_cache_identity_respects_platform_stat_contract,
    "tests/test_native_history_phase.py::test_cache_identity_respects_platform_stat_contract[legacy-linux]",
    { stat_contract("linux", "legacy") }
);
