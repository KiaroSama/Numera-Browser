use super::*;
use ci::browser_benchmark as browser;
use numera_core::ownership::unit_report::Report;
use std::time::Duration;
fn sample(value: u64, payload: Value) -> browser::Sample {
    browser::Sample {
        elapsed_ns: value,
        metric: "fixture".into(),
        bytes: value,
        payload,
        diagnostics: None,
    }
}
fn baseline(mode: &str) -> TestResult {
    let temp = Temp::new("browser baseline")?;
    let mut calls = Vec::new();
    let mut seeded = Vec::new();
    let result = browser::baseline(&temp.0, "camoufox", |data, args| {
        calls.push(args.to_vec());
        let operation = args[0].as_str();
        let payload = if operation == "create" {
            assert_eq!(&args[4..], ["--engine", "camoufox", "--json"]);
            fs::create_dir_all(data.join("user-data/0123456789ab"))?;
            json!({"profile":{"id":"0123456789ab"}})
        } else if args[1] == "import" {
            assert_eq!(&args[4..], ["--clear", "--json"]);
            seeded = ci::decode(&fs::read(&args[3])?)?["cookies"]
                .as_array()
                .cloned()
                .ok_or("cookies")?;
            if matches!(mode, "seed-refusal" | "stderr-refusal") {
                return Ok(numera_xtask::acceptance::Captured {
                    exit: i32::from(mode == "seed-refusal"),
                    stdout: "{\"error\":\"private-value\"}".into(),
                    stderr: "Traceback: private-path\nRuntimeError: do-not-expose-credential\n"
                        .into(),
                });
            }
            json!({"ok":true,"imported":2,"cleared":true})
        } else {
            let rows = if mode == "lost-cookie" {
                seeded[..1].to_vec()
            } else {
                seeded.iter().rev().cloned().collect()
            };
            fs::write(
                &args[3],
                serde_json::to_vec(&json!({"cookies":rows,"origins":[]}))?,
            )?;
            json!({"ok":true,"count":if mode=="metadata"{1}else{2},"file":args[3],"format":"json"})
        };
        Ok(numera_xtask::acceptance::Captured {
            exit: 0,
            stdout: payload.to_string(),
            stderr: String::new(),
        })
    });
    if mode == "normal" {
        let result = result?;
        assert_eq!(result.id, "0123456789ab");
        assert_eq!(
            result.payload["cookies"],
            json!(seeded.iter().rev().cloned().collect::<Vec<_>>())
        );
        assert_eq!(
            fs::read(
                result
                    .root
                    .join("user-data")
                    .join(&result.id)
                    .join("sentinel.bin")
            )?,
            browser::SENTINEL
        );
        assert_eq!(calls.len(), 3);
        assert_eq!(browser::fixture_expiry(1000)?, 2_593_000);
        assert!(browser::fixture_expiry(u64::MAX).is_err());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let expiry = seeded[0]["expires"]
            .as_f64()
            .ok_or("canonical floating fixture expiry")?;
        assert!(expiry > now as f64 && expiry <= (now + 30 * 24 * 60 * 60) as f64);
        assert_eq!(seeded[1]["expires"], seeded[0]["expires"]);
        let wanted = Value::Array(seeded.clone());
        for (field, value, descriptor) in [
            (
                "expires",
                json!(1),
                "field=expires expected=float actual=integer",
            ),
            (
                "expires",
                json!(1.0),
                "field=expires expected=float actual=float",
            ),
            (
                "httpOnly",
                json!(1),
                "field=httpOnly expected=boolean actual=integer",
            ),
            (
                "value",
                json!("private-cookie-material"),
                "field=value expected=string actual=string",
            ),
            (
                "unreviewed-private-key",
                json!("private-cookie-material"),
                "other-fields differ",
            ),
        ] {
            let mut changed = wanted.clone();
            changed[0][field] = value;
            let error = browser::require_cookie_persistence(&changed, &wanted)
                .err()
                .ok_or("Changed cookie accepted")?
                .to_string();
            assert!(
                error.contains(descriptor),
                "redacted field/type diagnostic missing"
            );
            assert!(error.len() < 2048);
            for secret in [
                "synthetic",
                "fixture.test",
                "private-cookie-material",
                "unreviewed-private-key",
            ] {
                assert!(
                    !error.contains(secret),
                    "Cookie material appeared in diagnostics"
                );
            }
        }
        let mut missing = wanted.clone();
        missing[0]
            .as_object_mut()
            .ok_or("cookie object")?
            .remove("sameSite");
        assert!(
            browser::require_cookie_persistence(&missing, &wanted)
                .err()
                .ok_or("Missing field accepted")?
                .to_string()
                .contains("field=sameSite expected=string actual=missing")
        );
        let swapped = Value::Array(seeded.iter().rev().cloned().collect());
        assert!(browser::require_cookie_persistence(&swapped, &wanted).is_err());
        let mut large = vec![json!({"value":"private-cookie-material"}); 100];
        large[0]["name"] = json!("private-cookie-name");
        let error = browser::require_cookie_persistence(&Value::Array(large), &wanted)
            .err()
            .ok_or("Different array accepted")?
            .to_string();
        assert!(error.len() < 2048 && !error.contains("private-cookie"));
    } else {
        let error = result.err().ok_or("Damaged seed was accepted")?.to_string();
        if matches!(mode, "seed-refusal" | "stderr-refusal") {
            assert!(
                error.contains("operation=cookies-import")
                    && error.contains("type=RuntimeError")
                    && error.contains("stdout_bytes=25")
            );
            for secret in ["private-path", "private-value", "do-not-expose"] {
                assert!(!error.contains(secret));
            }
        }
    }
    Ok(())
}
source_cases!("tests/test_browser_benchmark.py::test_browser_baseline_uses_original_and_checks_actual_persistence",baseline,[browser_baseline_normal=>("normal","normal"),browser_baseline_seed=>("seed-refusal","seed-refusal"),browser_baseline_stderr=>("stderr-refusal","stderr-refusal"),browser_baseline_lost=>("lost-cookie","lost-cookie"),browser_baseline_metadata=>("metadata","metadata")]);
fn environment(cache: Option<&str>) -> TestResult {
    let temp = Temp::new("browser environment")?;
    let mut parent = BTreeMap::from([
        ("NUMERA_PASSWORD".into(), "must-not-forward".into()),
        ("NUMERA_API_TOKEN".into(), "must-not-forward".into()),
        ("NUMERA_DATA_DIR".into(), "must-not-forward".into()),
        ("NUMERA_HELPER_UNITS".into(), "retained-parent".into()),
        ("XDG_CACHE_HOME".into(), "existing-sdk-cache".into()),
        (
            "HOME".into(),
            temp.0.join("untouched home").to_string_lossy().into_owned(),
        ),
    ]);
    if let Some(cache) = cache {
        parent.insert("CLOAKBROWSER_CACHE_DIR".into(), cache.into());
    }
    let worker = temp.0.join("native worker");
    let env = browser::environment(&parent, &temp.0, &worker, "camoufox", "linux")?;
    assert_eq!(env["HOME"], env["USERPROFILE"]);
    assert_eq!(env["HOME"], temp.0.join("synthetic home").to_string_lossy());
    assert_eq!(env["NUMERA_BROWSER_ADAPTER"], worker.to_string_lossy());
    assert_eq!(env["NUMERA_BROWSER_RUNTIME"], worker.to_string_lossy());
    assert_eq!(env["NUMERA_HELPER_UNITS"], "retained-parent");
    assert_eq!(env["XDG_CACHE_HOME"], "existing-sdk-cache");
    assert_eq!(env.get("CLOAKBROWSER_CACHE_DIR").map(String::as_str), cache);
    assert!(!temp.0.join("untouched home").exists());
    for key in ["NUMERA_PASSWORD", "NUMERA_API_TOKEN", "NUMERA_DATA_DIR"] {
        assert!(!env.contains_key(key));
    }
    Ok(())
}
source_cases!("tests/test_browser_benchmark.py::test_browser_environment_binds_sdk_without_inheriting_real_application_state",environment,[browser_env_none=>("None",None),browser_env_cloak=>("explicit-cloak-cache",Some("explicit-cloak-cache"))]);
fn fixture(temp: &Temp) -> ci::Result<PathBuf> {
    let root = temp.0.join("baseline");
    fs::create_dir_all(root.join("profiles"))?;
    fs::write(root.join("profiles/fixture.json"), b"{\"future\":[true,7]}")?;
    fs::create_dir_all(root.join("user-data/fixture"))?;
    fs::write(
        root.join("user-data/fixture/sentinel.bin"),
        browser::SENTINEL,
    )?;
    Ok(root)
}
fn pair(mode: &str) -> TestResult {
    let temp = Temp::new("browser pair")?;
    let root = fixture(&temp)?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("127.0.0.1:+12345", root.join("user-data/fixture/lock"))?;
    }
    let workspace = temp.0.join("browser-pair-actual");
    fs::create_dir(&workspace)?;
    let expected =
        json!({"cookies":[{"name":"one","expires":1900000000},{"name":"two","httpOnly":true}]});
    let mut calls = 0;
    let result = browser::observation(&root, &workspace, "fixture", |data, destination| {
        calls += 1;
        let mut payload = expected.clone();
        match mode {
            "cookie" => payload["cookies"][0]["name"] = json!("changed"),
            "order" => payload["cookies"]
                .as_array_mut()
                .expect("cookies")
                .reverse(),
            "type" => payload["cookies"][1]["httpOnly"] = json!(1),
            "profile" => fs::write(data.join("profiles/fixture.json"), b"changed")?,
            "sentinel" => fs::write(data.join("user-data/fixture/sentinel.bin"), b"changed")?,
            _ => {}
        }
        let logs = workspace.join("logs");
        fs::create_dir(&logs)?;
        fs::write(
            logs.join("native-cli_sample.log"),
            "[2026 UTC] [INFO] [entry] Native timing phase=browser_hello elapsed_ns=456\n[2026 UTC] [INFO] [entry] private=value\n",
        )?;
        fs::write(destination, serde_json::to_vec(&payload)?)?;
        if mode == "exit" {
            return ci::fail("Browser observation exit refused");
        }
        let mut payload = json!({"ok":true,"count":2,"file":destination,"format":"json"});
        if mode == "metadata" {
            payload["count"] = json!(1);
        }
        let mut value = sample(7, payload);
        value.bytes = 1234;
        Ok(value)
    });
    if matches!(mode, "profile" | "sentinel" | "metadata" | "exit") {
        assert!(result.is_err());
    } else {
        let (sample, payload) = result?;
        assert_eq!(sample.elapsed_ns, 7);
        assert_eq!(sample.bytes, 1234);
        assert_eq!(payload == expected, mode == "normal");
    }
    assert_eq!(calls, 1);
    assert!(workspace.is_dir());
    assert_eq!(
        fs::read(root.join("profiles/fixture.json"))?,
        b"{\"future\":[true,7]}"
    );
    #[cfg(unix)]
    {
        assert_eq!(
            fs::read_link(root.join("user-data/fixture/lock"))?,
            PathBuf::from("127.0.0.1:+12345")
        );
        assert_eq!(
            fs::read_link(workspace.join("data with spaces/user-data/fixture/lock"))?,
            PathBuf::from("127.0.0.1:+12345")
        );
    }
    let events = ci::candidate::diagnostics(&workspace.join("logs"))?;
    assert!(events.iter().any(|e| e.contains("elapsed_ns=456")));
    assert!(!events.iter().any(|e| e.contains("private=value")));
    assert!(events.len() <= 65);
    Ok(())
}
source_cases!("tests/test_browser_benchmark.py::test_browser_pair_checks_full_export_and_preserves_failed_inputs",pair,[browser_pair_normal=>("normal","normal"),browser_pair_cookie=>("cookie","cookie"),browser_pair_order=>("order","order"),browser_pair_type=>("type","type"),browser_pair_profile=>("profile","profile"),browser_pair_sentinel=>("sentinel","sentinel"),browser_pair_metadata=>("metadata","metadata"),browser_pair_exit=>("exit","exit")]);
fn pairs(mode: &str) -> TestResult {
    if ["normal", "parity", "inconclusive", "metric", "write"].contains(&mode) {
        entry_pairs(mode)?;
    }
    let temp = Temp::new("browser all pairs")?;
    let parent = temp.0.join(".ci-work/migration-temp");
    let destination = temp.0.join("measurements.json");
    if mode == "write" {
        fs::write(&destination, "prior evidence")?;
    }
    let baseline = json!({"cookies":[{"name":"one"},{"name":"two"}],"origins":[]});
    let mut observed = Vec::new();
    let mut diagnostic_runs = Vec::new();
    let result = browser::run_pairs(
        &parent,
        &destination,
        "playwright",
        &baseline,
        |runtime, workspace, index, diagnostic| {
            fs::write(workspace.join("input.json"), b"{\"synthetic\":true}")?;
            if diagnostic {
                assert!(destination.is_file());
                let saved = ci::decode(&fs::read(&destination)?)?;
                assert_eq!(
                    saved["results"][0]["samples"]
                        .as_array()
                        .expect("samples")
                        .len(),
                    7
                );
                assert_eq!(saved["results"][0]["memory"], "inconclusive");
                diagnostic_runs.push(runtime.to_owned());
                if mode == "diagnostic-refusal" {
                    return ci::fail("Synthetic diagnostic refusal");
                }
                if mode == "diagnostic-cleanup" {
                    return ci::fail("Synthetic diagnostic cleanup unproved");
                }
            } else {
                assert_eq!(index, (observed.len() as i32 / 2) - 1);
                observed.push(runtime.to_owned());
            }
            let value = if runtime == "native"
                && !matches!(
                    mode,
                    "inconclusive" | "diagnostic-refusal" | "diagnostic-cleanup"
                ) {
                1
            } else {
                100
            };
            let mut value = sample(value, Value::Null);
            if mode == "metric" {
                value.metric = runtime.into();
            }
            Ok((
                value,
                if mode == "parity" {
                    json!(runtime)
                } else {
                    baseline.clone()
                },
            ))
        },
    );
    if mode == "normal" {
        let report = result?;
        assert_eq!(report["comparison"], "native-checkpoint-to-native-product");
        assert_eq!(fs::read_dir(&parent)?.count(), 0);
    } else {
        assert!(result.is_err());
        let run = fs::read_dir(&parent)?
            .next()
            .ok_or("Retained run missing")??
            .path();
        assert!(run.is_dir());
        let count = fs::read_dir(run)?
            .filter_map(|v| v.ok())
            .filter(|e| e.path().join("input.json").is_file())
            .count();
        assert_eq!(count, observed.len() + diagnostic_runs.len());
    }
    let expected = if mode == "inconclusive" {
        vec!["reference", "native"]
    } else if matches!(mode, "diagnostic-refusal" | "diagnostic-cleanup") {
        vec!["reference"]
    } else {
        vec![]
    };
    assert_eq!(diagnostic_runs, expected);
    if mode != "parity" {
        let expected = (-1i32..7)
            .flat_map(|i| {
                if i % 2 == 0 {
                    ["reference", "native"]
                } else {
                    ["native", "reference"]
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(observed, expected);
    }
    if mode == "write" {
        assert_eq!(fs::read_to_string(destination)?, "prior evidence");
    } else if matches!(
        mode,
        "normal" | "inconclusive" | "diagnostic-refusal" | "diagnostic-cleanup"
    ) {
        let report = ci::decode(&fs::read(destination)?)?;
        assert_eq!(report["engine"], "playwright");
        assert_eq!(report["repetitions"], 7);
        assert_eq!(report["warmup_pairs"], 1);
    }
    Ok(())
}
fn entry_pairs(mode: &str) -> TestResult {
    use ci::entry_benchmark::{self as entry, Scenario};
    let temp = Temp::new("entry all five pairs")?;
    let parent = temp.0.join("inputs");
    let destination = temp.0.join("entry-performance.json");
    if mode == "write" {
        fs::write(&destination, b"prior evidence")?;
    }
    let mut observed = Vec::<(Scenario, String, i32)>::new();
    let result = entry::run_pairs(
        &parent,
        &destination,
        &json!({"comparison":"native-checkpoint-to-native-product"}),
        |scenario, runtime, workspace, repetition| {
            fs::write(
                workspace.join("input.json"),
                b"synthetic retained measurement input",
            )?;
            observed.push((scenario, runtime.into(), repetition));
            let value = if runtime == "native" && mode != "inconclusive" {
                1
            } else {
                100
            };
            let mut value = sample(value, Value::Null);
            if mode == "metric" {
                value.metric = runtime.into();
            }
            Ok((
                value,
                if mode == "parity" {
                    json!(runtime)
                } else {
                    json!({"complete":[true,42,"preserved"]})
                },
            ))
        },
    );
    if mode == "normal" {
        let report = result?;
        assert_eq!(report["results"].as_array().expect("results").len(), 5);
        assert_eq!(fs::read_dir(&parent)?.count(), 0);
    } else {
        assert!(result.is_err());
        let run = fs::read_dir(&parent)?
            .next()
            .ok_or("Retained entry run missing")??
            .path();
        assert_eq!(
            fs::read_dir(run)?
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().join("input.json").is_file())
                .count(),
            observed.len()
        );
    }
    let scenarios = if matches!(mode, "parity" | "metric") {
        &entry::SCENARIOS[..1]
    } else {
        &entry::SCENARIOS[..]
    };
    let expected = scenarios
        .iter()
        .flat_map(|scenario| {
            (-1i32..7).flat_map(move |repetition| {
                let order = if repetition % 2 == 0 {
                    ["reference", "native"]
                } else {
                    ["native", "reference"]
                };
                order
                    .into_iter()
                    .map(move |runtime| (*scenario, runtime.to_owned(), repetition))
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed,
        if mode == "parity" {
            expected[..2].to_vec()
        } else {
            expected
        }
    );
    if mode == "write" {
        assert_eq!(fs::read_to_string(&destination)?, "prior evidence");
    } else if matches!(mode, "normal" | "inconclusive") {
        let report = ci::decode(&fs::read(destination)?)?;
        assert_eq!(
            report["results"]
                .as_array()
                .expect("results")
                .iter()
                .map(|row| row["scenario"].as_str().expect("scenario"))
                .collect::<Vec<_>>(),
            vec!["create", "list", "vault-read", "backup", "http-list"]
        );
        assert_eq!(report["warmup_pairs"], 1);
        assert_eq!(report["repetitions"], 7);
        for row in report["results"].as_array().expect("results") {
            assert_eq!(row["samples"].as_array().expect("samples").len(), 7);
            assert_eq!(
                row["time"],
                if mode == "normal" {
                    "no-regression"
                } else {
                    "inconclusive"
                }
            );
            assert_eq!(
                row["memory"],
                if mode == "normal" {
                    "no-regression"
                } else {
                    "inconclusive"
                }
            );
        }
    }
    Ok(())
}
source_cases!("tests/test_browser_benchmark.py::test_browser_run_requires_all_pairs_and_retains_unaccepted_evidence",pairs,[browser_run_normal=>("normal","normal"),browser_run_parity=>("parity","parity"),browser_run_inconclusive=>("inconclusive","inconclusive"),browser_run_diagnostic_refusal=>("diagnostic-refusal","diagnostic-refusal"),browser_run_diagnostic_cleanup=>("diagnostic-cleanup","diagnostic-cleanup"),browser_run_metric=>("metric","metric"),browser_run_write=>("write","write")]);

pub(super) struct Host {
    pub mode: String,
    pub clock: u64,
    pub events: Vec<&'static str>,
    pub reads: usize,
    pub sleeps: Vec<u64>,
    pub created: bool,
}
impl Host {
    pub fn new(mode: &str) -> Self {
        Self {
            mode: mode.into(),
            clock: 0,
            events: Vec::new(),
            reads: 0,
            sleeps: Vec::new(),
            created: false,
        }
    }
}
impl browser::Observer for Host {
    fn register(&mut self) -> TestResult {
        self.register_before(self.clock.saturating_add(2_000_000_000))
    }
    fn register_before(&mut self, deadline: u64) -> TestResult {
        if matches!(
            self.mode.as_str(),
            "registration-consumes-wall" | "registration-late"
        ) {
            self.clock = deadline - u64::from(self.mode == "registration-consumes-wall");
            self.events.push("intent");
            return Ok(());
        }
        let mode = self.mode.clone();
        let reads = &mut self.reads;
        let sleeps = &mut self.sleeps;
        let clock = std::cell::Cell::new(self.clock);
        let result = browser::registration(
            42,
            4200,
            deadline,
            || clock.get(),
            |_| {
                *reads += 1;
                if mode == "absent" {
                    return ci::fail("Observer receipt absent");
                }
                let mut trees = vec![json!({"pid":41,"pidToken":4100})];
                if mode != "missing-self" {
                    trees.push(json!({"pid":42,"pidToken":if mode=="wrong-self"{9999}else{4200}}));
                }
                Ok(Report {
                    trees,
                    complete: mode != "incomplete-before"
                        && !(mode == "pending-self" && *reads == 1),
                    problem: None,
                })
            },
            |duration| {
                sleeps.push(u64::try_from(duration.as_millis())?);
                clock.set(clock.get() + u64::try_from(duration.as_nanos())?);
                Ok(())
            },
            |_| Ok(true),
        );
        self.clock = clock.get();
        result?;
        self.events.push("intent");
        Ok(())
    }
    fn ticks(&mut self) -> u64 {
        self.clock
    }
    fn spawn(&mut self) -> TestResult {
        self.events.push("spawn");
        if self.mode == "spawn-refusal" {
            return ci::fail("Synthetic spawn refusal");
        }
        self.created = true;
        Ok(())
    }
    fn owned(&self) -> bool {
        self.created
    }
    fn communicate(&mut self, _: u64) -> ci::Result<Value> {
        self.events.push("capture");
        self.clock += 7;
        Ok(json!({"ok":true}))
    }
    fn natural(&mut self, deadline: u64) -> ci::Result<bool> {
        assert!(deadline <= 90_000_000_000);
        self.events.extend(["direct-proof", "sdk-proof"]);
        if self.mode == "incomplete" {
            return ci::fail("Nested receipt incomplete");
        }
        Ok(self.mode != "live")
    }
    fn pause(&mut self) -> TestResult {
        self.sleeps.push(1);
        self.clock += 1_000_000_000;
        Ok(())
    }
    fn peak(&mut self) -> ci::Result<(String, u64)> {
        self.events.push("peak");
        Ok(("fixture".into(), 1234))
    }
    fn diagnostic(&mut self) -> ci::Result<Option<Value>> {
        if self.mode == "diagnostics" {
            self.events.push("snapshot");
            Ok(Some(json!({"coverage":"sampled-not-lifetime-attribution"})))
        } else {
            Ok(None)
        }
    }
    fn cleanup(&mut self) -> TestResult {
        self.events.extend(["direct-cleanup", "sdk-cleanup"]);
        if self.mode == "cleanup" {
            return ci::fail("Nested cleanup refused");
        }
        Ok(())
    }
    fn reap(&mut self) -> TestResult {
        self.events.push("reap");
        if self.mode == "wait" {
            return ci::fail("Leader reap failure");
        }
        Ok(())
    }
    fn close(&mut self) -> TestResult {
        self.events
            .extend(["stdout-close", "stderr-close", "process-close", "job-close"]);
        Ok(())
    }
}
fn lifecycle(mode: &str) -> TestResult {
    let mut host = Host::new(mode);
    let result = browser::observe(&mut host, Duration::from_secs(90));
    if matches!(mode, "normal" | "pending-self" | "diagnostics") {
        let result = result?;
        assert_eq!(result.payload, json!({"ok":true}));
        assert_eq!(result.elapsed_ns, 7);
        assert_eq!(result.bytes, 1234);
        assert_eq!(result.diagnostics.is_some(), mode == "diagnostics");
        let pos = |e| host.events.iter().position(|v| *v == e).expect("event");
        assert!(pos("sdk-proof") < pos("peak") && pos("peak") < pos("sdk-cleanup"));
        assert_eq!(
            host.sleeps,
            if mode == "pending-self" {
                vec![10]
            } else {
                vec![]
            }
        );
    } else {
        assert!(result.is_err());
        if !matches!(mode, "cleanup" | "wait") {
            assert!(!host.events.contains(&"peak"));
        }
    }
    if matches!(
        mode,
        "absent" | "missing-self" | "wrong-self" | "incomplete-before"
    ) {
        assert!(!host.events.contains(&"spawn"));
    } else {
        for event in [
            "direct-cleanup",
            "sdk-cleanup",
            "stdout-close",
            "stderr-close",
        ] {
            assert_eq!(host.events.iter().filter(|v| **v == event).count(), 1);
        }
        assert_eq!(
            &host.events[host.events.len() - 2..],
            ["process-close", "job-close"]
        );
    }
    if mode == "pending-self" {
        parent_publication("delayed")?;
    }
    if mode == "incomplete-before" {
        for publication in ["held-lock", "malformed", "expired"] {
            parent_publication(publication)?;
        }
    }
    if mode == "wrong-self" {
        parent_publication("wrong-birth")?;
    }
    registration_deadline_controls(mode)?;
    Ok(())
}
fn registration_deadline_controls(mode: &str) -> TestResult {
    use std::cell::Cell;
    if mode == "incomplete-before" {
        let mut late = Host::new("registration-late");
        assert!(browser::observe(&mut late, Duration::from_nanos(50)).is_err());
        assert!(!late.events.contains(&"spawn"));
        let mut consumed = Host::new("registration-consumes-wall");
        assert!(browser::observe(&mut consumed, Duration::from_nanos(50)).is_err());
        assert!(consumed.events.contains(&"spawn"));
        assert!(!consumed.events.contains(&"peak"));
        assert!(consumed.events.contains(&"sdk-cleanup"));
    }
    let cases: &[&str] = match mode {
        "pending-self" => &["many-pending", "complete-without-self", "prior-reused-pid"],
        "missing-self" => &["missing", "short-wall"],
        "wrong-self" => &["wrong-birth", "wrong-tree", "verification-error"],
        "incomplete-before" => &["expired", "late-read", "late-tree"],
        "absent" => &["read-error"],
        _ => &[],
    };
    for &case in cases {
        let clock = Cell::new(0u64);
        let reads = Cell::new(0usize);
        let verified = Cell::new(0usize);
        let pauses = std::cell::RefCell::new(Vec::new());
        let deadline = match case {
            "expired" => 0,
            "short-wall" => 15_000_000,
            _ => 90_000_000_000,
        };
        let result = browser::registration(
            42,
            4200,
            deadline,
            || clock.get(),
            |end| {
                assert_eq!(end, deadline.min(2_000_000_000));
                reads.set(reads.get() + 1);
                if case == "read-error" {
                    return Err(
                        io::Error::new(io::ErrorKind::PermissionDenied, "Receipt refused").into(),
                    );
                }
                if case == "late-read" {
                    clock.set(end);
                }
                let waiting = match case {
                    "missing" | "short-wall" => true,
                    "many-pending" | "complete-without-self" => reads.get() < 5,
                    _ => false,
                };
                let mut trees = vec![json!({"pid":41,"pidToken":4100})];
                if case == "prior-reused-pid" {
                    trees.push(json!({"pid":42,"pidToken":4199}));
                }
                if !waiting {
                    trees
                        .push(json!({"pid":42,"pidToken":if case=="wrong-birth" {9999}else{4200}}));
                }
                Ok(Report {
                    trees,
                    complete: case != "many-pending" || !waiting,
                    problem: None,
                })
            },
            |duration| {
                let nanoseconds = u64::try_from(duration.as_nanos())?;
                assert!(nanoseconds > 0 && nanoseconds <= 10_000_000);
                pauses.borrow_mut().push(nanoseconds);
                clock.set(clock.get() + nanoseconds);
                Ok(())
            },
            |_| {
                verified.set(verified.get() + 1);
                if case == "verification-error" {
                    return Err(
                        io::Error::new(io::ErrorKind::PermissionDenied, "Tree refused").into(),
                    );
                }
                if case == "late-tree" {
                    clock.set(2_000_000_000);
                }
                Ok(case != "wrong-tree")
            },
        );
        if matches!(
            case,
            "many-pending" | "complete-without-self" | "prior-reused-pid"
        ) {
            let expected = if case == "prior-reused-pid" {
                BTreeSet::from([(41, 4100), (42, 4199), (42, 4200)])
            } else {
                BTreeSet::from([(41, 4100), (42, 4200)])
            };
            assert_eq!(result?, expected);
            assert_eq!(reads.get(), if case == "prior-reused-pid" { 1 } else { 5 });
            assert_eq!(verified.get(), 1);
            assert_eq!(
                clock.get(),
                if case == "prior-reused-pid" {
                    0
                } else {
                    40_000_000
                }
            );
        } else {
            let error = result.expect_err("Invalid or late parent registration was accepted");
            if matches!(case, "read-error" | "verification-error") {
                assert_eq!(
                    error.downcast_ref::<io::Error>().map(io::Error::kind),
                    Some(io::ErrorKind::PermissionDenied)
                );
            }
        }
        match case {
            "missing" => {
                assert_eq!(clock.get(), 2_000_000_000);
                assert_eq!(reads.get(), 200);
            }
            "short-wall" => {
                assert_eq!(&*pauses.borrow(), &[10_000_000, 5_000_000]);
                assert_eq!(clock.get(), 15_000_000);
            }
            "expired" => {
                assert_eq!(reads.get(), 0);
                assert_eq!(verified.get(), 0);
            }
            "wrong-birth" | "late-read" => {
                assert_eq!(reads.get(), 1);
                assert_eq!(verified.get(), 0);
            }
            "wrong-tree" | "late-tree" | "verification-error" => assert_eq!(verified.get(), 1),
            _ => {}
        }
    }
    Ok(())
}
fn parent_publication(publication: &str) -> TestResult {
    // This is the real post-OS-spawn/pre-receipt hook. The observer must not
    // mistake its parent's still-pending transfer for a missing identity.
    let temp = Temp::new("observer parent publication")?;
    let source = r#"
use numera_xtask::ci::{self,browser_benchmark::{Native,observe}};
use numera_core::ownership::{test_support::{self,Backend},unit_report};
use serde_json::json;
use std::{fs,io::{self,Write},path::{Path,PathBuf},process::Command,sync::Arc,time::{Duration,Instant}};
struct Delay { root:PathBuf, mode:String }
impl Backend for Delay {
 fn spawned(&self,record:&serde_json::Value)->Option<io::Result<()>> {
  Some((|| {
   let report=PathBuf::from(std::env::var_os(unit_report::ENV).ok_or_else(||io::Error::other("Parent receipt missing"))?);
   let lock=numera_core::storage::locks::DataLock::for_file(&report)?;
   let held=if self.mode=="held-lock" {Some(lock.acquire(Duration::from_secs(1))?)}else{None};
   let original=if matches!(self.mode.as_str(),"malformed"|"wrong-birth") {
    let _guard=lock.acquire(Duration::from_secs(1))?;
    let original=fs::read(&report)?;
    if self.mode=="malformed" {
     fs::OpenOptions::new().append(true).open(&report)?.write_all(b"{broken receipt\n")?;
    }
    Some(original)
   }else{None};
   if self.mode=="wrong-birth" {
    let pending=String::from_utf8(original.as_ref().ok_or_else(||io::Error::other("Original receipt missing"))?.clone()).map_err(io::Error::other)?;
    let attempt=pending.lines().filter_map(|line|serde_json::from_str::<serde_json::Value>(line).ok()).filter(|row|row["event"]=="pending").last().and_then(|row|row["attempt"].as_str().map(str::to_owned)).ok_or_else(||io::Error::other("Actual pending attempt missing"))?;
    let mut wrong=record.clone();
    wrong["token"]=(record["token"].as_u64().ok_or_else(||io::Error::other("Actual birth missing"))?+1).into();
    unit_report::record_at(&report,&wrong,&attempt)?;
   }
   let deadline=Instant::now()+Duration::from_secs(3);
   while !self.root.join("observer-started").is_file() {
    if Instant::now()>=deadline {return Err(io::Error::other("Observer did not enter registration"));}
    std::thread::sleep(Duration::from_millis(1));
   }
   std::thread::sleep(Duration::from_millis(80));
   if self.mode!="delayed" {
    let deadline=Instant::now()+Duration::from_millis(400);
    while !self.root.join("observer-refused").is_file() && Instant::now()<deadline {
     std::thread::sleep(Duration::from_millis(1));
    }
   }
   if self.root.join("observer-refused").is_file() {
    fs::write(self.root.join("refused-before-publication"),b"actual observer refused while publication held")?;
   }
   drop(held);
   if let Some(original)=original {
    let _guard=lock.acquire(Duration::from_secs(1))?;
    fs::write(&report,original)?;
   }
   Ok(())
  })())
 }
}
fn main()->ci::Result<()> {
 let args=std::env::args_os().skip(1).collect::<Vec<_>>();
 let mode=args[0].to_str().ok_or("Mode missing")?;
 let root=Path::new(&args[1]);
 let publication=args[2].to_str().ok_or("Publication mode missing")?;
 if mode=="app" {
  fs::write(root.join("application-started"),b"actual application executed")?;
  println!("{{\"ok\":true}}");return Ok(());
 }
 if mode=="observer" {
  fs::write(root.join("observer-started"),b"actual observer entered")?;
  let mut command=Command::new(std::env::current_exe()?);
  command.arg("app").arg(root).arg(publication);
  let report=std::env::var_os(unit_report::ENV).ok_or("Parent receipt missing")?;
  let mut native=Native::new(command,report.into(),false)?;
  let wall=if publication=="delayed" {Duration::from_secs(2)}else{Duration::from_millis(30)};
  let result=observe(&mut native,wall);
  match result {
   Ok(sample)=>println!("{}",json!({"sample":sample.summary(),"payload":sample.payload})),
   Err(error)=>{fs::write(root.join("observer-refused"),b"refused before application spawn")?;println!("{}",json!({"refused":true}));eprintln!("{error}");std::process::exit(1);}
  }
  return Ok(());
 }
 let mut command=Command::new(std::env::current_exe()?);
 command.arg("observer").arg(root).arg(publication);
 let output=test_support::with_backend(Arc::new(Delay{root:root.into(),mode:publication.into()}),||
  ci::owned::run(root,&mut command,ci::owned::limits(Duration::from_secs(5),Duration::from_secs(5),16384)))?.command;
 println!("{}",json!({"exit":output.exit,"stdout":String::from_utf8(output.stdout)?,"stderr":String::from_utf8(output.stderr)?}));
 Ok(())
}
"#;
    let executable = super::migration::compile(&temp, source)?;
    fs::create_dir(temp.0.join("jobs"))?;
    let report = temp.0.join("jobs/registration.units");
    numera_core::ownership::unit_report::initialize(&report)?;
    let output = ci::capture(
        std::process::Command::new(executable)
            .arg("parent")
            .arg(&temp.0)
            .arg(publication)
            .env(numera_core::ownership::unit_report::ENV, &report),
        Duration::from_secs(8),
        Duration::from_secs(8),
        32768,
    )?;
    assert_eq!(output.exit, 0, "Parent publication helper failed");
    let answer = ci::decode(&output.stdout)?;
    if publication == "delayed" && answer["exit"] != 0 {
        assert!(
            answer["stderr"].as_str()
                == Some("Browser observer receipt registration or birth identity missing\n"),
            "Observer refused outside the expected parent receipt boundary"
        );
        assert!(!temp.0.join("application-started").exists());
    }
    let observed = ci::decode(
        answer["stdout"]
            .as_str()
            .ok_or("Observer stdout missing")?
            .as_bytes(),
    )?;
    if publication == "delayed" {
        assert_eq!(
            answer["exit"], 0,
            "Observer refused a genuine delayed parent publication"
        );
        assert!(temp.0.join("application-started").is_file());
        assert_eq!(observed["payload"], json!({"ok":true}));
        assert!(answer["stderr"].as_str().is_some_and(str::is_empty));
    } else {
        assert_eq!(answer["exit"], 1);
        assert_eq!(observed["refused"], true);
        assert!(!temp.0.join("application-started").exists());
        assert!(temp.0.join("refused-before-publication").is_file());
    }
    Ok(())
}
source_cases!("tests/test_browser_benchmark.py::test_browser_sample_requires_natural_nested_settlement_before_peak",lifecycle,[browser_lifecycle_normal=>("normal","normal"),browser_lifecycle_pending=>("pending-self","pending-self"),browser_lifecycle_absent=>("absent","absent"),browser_lifecycle_missing=>("missing-self","missing-self"),browser_lifecycle_wrong=>("wrong-self","wrong-self"),browser_lifecycle_incomplete_before=>("incomplete-before","incomplete-before"),browser_lifecycle_incomplete=>("incomplete","incomplete"),browser_lifecycle_live=>("live","live"),browser_lifecycle_cleanup=>("cleanup","cleanup"),browser_lifecycle_wait=>("wait","wait"),browser_lifecycle_diagnostics=>("diagnostics","diagnostics")]);
fn history(damage: &str) -> TestResult {
    let mut native = workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/native-ci.yml"),
    )?)?;
    let mut rust = workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/rust-candidate.yml"),
    )?)?;
    let steps = native["jobs"]["live"]["steps"]
        .as_array_mut()
        .expect("steps");
    match damage {
        "missing" => {
            steps.remove(0);
        }
        "shallow" => steps[0]["with"]["fetch-depth"] = json!(1),
        "conditional" => steps[0]["if"] = json!("false"),
        "duplicate" => steps.push(steps[0].clone()),
        "late" => {
            let checkout = steps.remove(0);
            steps.push(checkout);
        }
        _ => {}
    }
    if damage == "moving-host" {
        native["jobs"]["live"]["strategy"]["matrix"]["os"][0] = json!("ubuntu-latest");
    }
    if damage == "missing-windows" {
        native["jobs"]["live"]["strategy"]["matrix"]["os"]
            .as_array_mut()
            .expect("os")
            .retain(|v| v != "windows-latest");
    }
    verdict(browser::workflow_history(&native, &rust), damage == "None");
    let native = workflow::parse(&fs::read_to_string(
        root().join(".github/workflows/native-ci.yml"),
    )?)?;
    let steps = rust["jobs"]["tooling"]["steps"]
        .as_array_mut()
        .expect("steps");
    let index = steps
        .iter()
        .position(|s| {
            s["run"].as_str().is_some_and(|r| {
                r.contains("cargo clippy")
                    && r.contains("native-browser")
                    && r.contains("--all-targets")
            })
        })
        .ok_or("Required SDK Clippy missing")?;
    match damage {
        "missing" => {
            steps.remove(index);
        }
        "duplicate" => steps.push(steps[index].clone()),
        "conditional" => steps[index]["if"] = json!("false"),
        "None" => {}
        _ => steps[index]["continue-on-error"] = json!(true),
    }
    verdict(browser::workflow_history(&native, &rust), damage == "None");
    Ok(())
}
source_cases!("tests/test_browser_benchmark.py::test_browser_workflow_requires_original_history_before_acceptance",history,[browser_history_normal=>("None","None"),browser_history_missing=>("missing","missing"),browser_history_shallow=>("shallow","shallow"),browser_history_conditional=>("conditional","conditional"),browser_history_duplicate=>("duplicate","duplicate"),browser_history_late=>("late","late"),browser_history_moving=>("moving-host","moving-host"),browser_history_windows=>("missing-windows","missing-windows")]);
fn measurement(damage: &str) -> TestResult {
    let mut action = workflow::parse(&fs::read_to_string(
        root().join(".github/actions/native-candidate-acceptance/action.yml"),
    )?)?;
    browser::workflow_measurement(&action)?;
    let build = workflow::step_once(&action["runs"]["steps"], "build-candidate", true)?;
    let functional = workflow::step_once(&action["runs"]["steps"], "ci acceptance ", true)?;
    let measurement = workflow::step_once(&action["runs"]["steps"], "ci browser-benchmark", true)?;
    let upload = action["runs"]["steps"]
        .as_array()
        .expect("steps")
        .iter()
        .position(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|value| value.starts_with("actions/upload-artifact@"))
        })
        .expect("upload");
    let steps = action["runs"]["steps"].as_array_mut().expect("steps");
    match damage {
        "unoptimized" => {
            steps[build]["run"] = json!(
                steps[build]["run"]
                    .as_str()
                    .expect("build")
                    .replace(" --release", "")
            )
        }
        "missing" => {
            steps.remove(measurement);
        }
        "softened" => steps[measurement]["continue-on-error"] = json!(true),
        "skipped" => steps[measurement]["if"] = json!("false"),
        "artifact" => steps[upload]["with"]["if-no-files-found"] = json!("warn"),
        _ => steps.swap(functional, measurement),
    }
    assert!(browser::workflow_measurement(&action).is_err());
    Ok(())
}
source_cases!("tests/test_browser_benchmark.py::test_browser_workflow_guard_rejects_missing_or_softened_measurement",measurement,[browser_measure_unoptimized=>("unoptimized","unoptimized"),browser_measure_missing=>("missing","missing"),browser_measure_softened=>("softened","softened"),browser_measure_skipped=>("skipped","skipped"),browser_measure_artifact=>("artifact","artifact"),browser_measure_order=>("order","order")]);
