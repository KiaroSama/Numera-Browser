use super::*;
use ci::{browser_benchmark as browser, migration, reference};
use numera_xtask::acceptance;
use std::{process::Command, time::Duration};
fn occurrence_fixture() -> ci::Result<Value> {
    migration::discover("frozen-fixture",&[("source.rs".into(),br#"#[cfg(windows)] fn take(){} #[cfg(unix)] fn take(){} #[cfg(feature="other")] fn take(){} fn read(){#[cfg(windows)]fn pipe(){} #[cfg(unix)]fn pipe(){}}"#.to_vec())])
}
fn complete(temp: &Temp, source: &Value) -> ci::Result<Value> {
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
source_test!(
    migration_baseline_provenance,
    "tests/test_migration_gates.py::test_repaired_baseline_is_reachable_without_changing_fixture_provenance",
    {
        assert_eq!(
            reference::ORIGINAL_BASELINE,
            "13b6d9c6973281fe123353eaec05b29d65781a2d"
        );
        assert_eq!(
            reference::ORIGINAL_TREE,
            "760f9873ded13c5ebfecae1f16b385e34f695124"
        );
        assert_eq!(
            String::from_utf8(ci::git(
                &root(),
                &[
                    "rev-parse",
                    "760f9873ded13c5ebfecae1f16b385e34f695124:engine/numera"
                ]
            )?)?
            .trim(),
            "25cd7f34689e4a1688c426fdb7d4a6c4b898998e"
        );
        let entries = reference::entries(&root())?;
        assert!(
            entries
                .iter()
                .any(|row| row.path == "engine-rust/src/models.rs")
        );
        assert!(
            entries
                .iter()
                .all(|row| row.path.starts_with("engine-rust/"))
        );
        Ok(())
    }
);
source_test!(
    migration_frozen_bytes,
    "tests/test_migration_gates.py::test_original_inventory_reads_frozen_bytes_instead_of_working_source",
    {
        let temp = Temp::new("frozen inventory")?;
        let original = b"fn original_contract()->i32{1}";
        let source = temp.write("original/source.rs", original)?;
        temp.write("current/source.rs", b"fn changed_contract()->i32{2}")?;
        let row = migration::describe("source.rs", &fs::read(source)?)?;
        assert_eq!(row["sha256"], ci::sha(original));
        assert_eq!(row["symbols"], json!(["original_contract"]));
        Ok(())
    }
);
source_test!(
    migration_occurrences,
    "tests/test_migration_gates.py::test_inventory_keeps_alternative_definitions_independently_addressable",
    {
        let result = occurrence_fixture()?;
        assert_eq!(
            result["files"][0]["symbols"],
            json!(["take", "take", "take", "read", "read.pipe", "read.pipe"])
        );
        let ids = result["capabilities"]
            .as_array()
            .ok_or("cases")?
            .iter()
            .map(|r| r["id"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            json!([
                "source.rs::take",
                "source.rs::take#2",
                "source.rs::take#3",
                "source.rs::read",
                "source.rs::read.pipe",
                "source.rs::read.pipe#2"
            ])
            .as_array()
            .expect("ids")
            .clone()
        );
        assert_eq!(occurrence_fixture()?, result);
        for row in result["capabilities"].as_array().expect("rows") {
            assert_eq!(row["status"], "pending");
            assert!(row["target"].is_null() && row["test"].is_null());
        }
        let historical = fs::read(
            root().join("engine/tests/fixtures/migration-inventory-before-occurrences.py.txt"),
        )?;
        assert_eq!(
            ci::sha(&historical),
            "22de18dea172c6f67a991fa53ed47dfeaae220f1a139a39f93f5f9f76c274314"
        );
        let unique = ids.iter().map(Value::to_string).collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), ids.len());
        let collapsed = ids
            .iter()
            .map(|v| v.as_str().expect("id").split('#').next().expect("base"))
            .collect::<BTreeSet<_>>();
        assert!(
            collapsed.len() < ids.len(),
            "Guard would not detect the historical occurrence collapse"
        );
        Ok(())
    }
);
source_test!(
    migration_occurrence_gate,
    "tests/test_migration_gates.py::test_capability_gate_accepts_distinct_occurrences_but_refuses_reused_ids",
    {
        let temp = Temp::new("occurrence gate")?;
        let source = occurrence_fixture()?;
        let valid = complete(&temp, &source)?;
        assert!(migration::coverage(&temp.0, &source, &valid)?.is_empty());
        exact_inventory_controls()?;
        for fault in [
            "missing",
            "duplicate",
            "pending",
            "implemented",
            "extra-duplicate",
        ] {
            let mut ledger = valid.clone();
            let rows = ledger["capabilities"].as_array_mut().expect("rows");
            match fault {
                "missing" => {
                    rows.pop();
                }
                "duplicate" => rows[1] = rows[0].clone(),
                "pending" => rows[1]["status"] = json!("pending"),
                "implemented" => rows[1]["status"] = json!("implemented"),
                _ => rows.push(rows[1].clone()),
            };
            let problems = migration::coverage(&temp.0, &source, &ledger)?;
            assert!(!problems.is_empty(), "{fault}");
            if fault == "extra-duplicate" {
                assert_eq!(problems, vec!["Duplicate capability: source.rs::take#2"]);
            }
        }
        Ok(())
    }
);
fn exact_inventory_controls() -> TestResult {
    let baseline = ci::decode(&fs::read(root().join("docs/rust-test-baseline.json"))?)?;
    let mut paths = fs::read_dir(root().join("docs"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    paths.retain(|path| {
        path.file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.starts_with("source-") && v.ends_with("-test-map.json"))
    });
    paths.sort();
    let maps = paths
        .iter()
        .map(|path| ci::decode(&fs::read(path)?))
        .collect::<ci::Result<Vec<_>>>()?;
    ci::coverage_inventory::validate_documents(&root(), &baseline, &maps)?;
    let selected = maps
        .iter()
        .position(|map| map["suite"] == "source_ci")
        .ok_or("CI source map missing")?;
    for damage in ["missing", "duplicate", "kind", "platform"] {
        let mut changed = maps.clone();
        let map = &mut changed[selected];
        match damage {
            "missing" => {
                let removed = map["cases"].as_array_mut().expect("cases").remove(0);
                let count = map["source_count"].as_u64().expect("count") - 1;
                map["source_count"] = json!(count);
                map["implemented_count"] = json!(count);
                let kind = removed["original_type"].as_str().expect("kind");
                let count = map["case_types"][kind].as_u64().expect("kind count") - 1;
                map["case_types"][kind] = json!(count);
            }
            "duplicate" => map["cases"][1] = map["cases"][0].clone(),
            "kind" => map["cases"][0]["original_type"] = json!("standard"),
            _ => map["cases"][0]["native_platforms"] = json!(["windows"]),
        }
        assert!(
            ci::coverage_inventory::validate_documents(&root(), &baseline, &changed).is_err(),
            "{damage}"
        );
    }
    let expected = BTreeSet::from(["exact_original_case".into()]);
    let valid = "running 1 test\ntest exact_original_case ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
    assert_eq!(
        ci::coverage_inventory::validate_expected_output(&expected, valid, 0)?,
        1
    );
    for damaged in [
        valid.replace("test exact_original_case ... ok\n", ""),
        valid.replace("exact_original_case", "invented_replacement"),
        valid.replace(" ... ok", " ... ignored"),
        valid.replace("running 1 test", "running 0 tests"),
    ] {
        assert!(ci::coverage_inventory::validate_expected_output(&expected, &damaged, 0).is_err());
    }
    Ok(())
}
source_test!(
    migration_frozen_ledger,
    "tests/test_migration_gates.py::test_mapped_occurrences_retain_frozen_inventory_and_block_selection",
    {
        let document = ci::decode(&ci::git(
            &root(),
            &[
                "show",
                "3bf1d839fd33e42a10b982bd8607a7052ea7c4cd:docs/migration-coverage.json",
            ],
        )?)?;
        migration::verify_legacy_inventory(&document)?;
        let rows = document["capabilities"].as_array().ok_or("rows")?;
        assert!(
            rows.iter()
                .all(|r| r["target"].is_string() && r["test"].is_string())
        );
        for row in rows {
            for field in ["target", "test"] {
                let path = Path::new(row[field].as_str().ok_or("mapping")?);
                assert!(
                    !path.is_absolute()
                        && path
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_)))
                );
            }
            if row["status"] != "verified" {
                assert!(
                    row["remaining"]
                        .as_str()
                        .is_some_and(|v| !v.trim().is_empty())
                );
            }
        }
        for status in ["pending", "implemented"] {
            let mut ledger = document.clone();
            for row in ledger["capabilities"].as_array_mut().expect("rows") {
                row["status"] = json!(status);
            }
            let problems = migration::coverage(&root(), &document, &ledger)?;
            assert_eq!(problems.len(), 907);
            assert!(
                problems
                    .iter()
                    .all(|p| p.starts_with("Unverified capability: "))
            );
        }
        Ok(())
    }
);
source_test!(
    migration_replay_profile,
    "tests/test_migration_gates.py::test_replay_comparison_preserves_identity_secret_and_creation_time",
    {
        let expected = json!({"id":"fixture","created_at":1000.0,"updated_at":1001.0,"notes":"original","fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});
        let mut observed = expected.clone();
        observed["updated_at"] = json!(1002.0);
        migration::require_profile(&observed, &expected)?;
        assert_eq!(expected["updated_at"], 1001.0);
        for (field, value) in [
            ("id", json!("other")),
            ("created_at", json!(1002.0)),
            ("notes", json!("lost")),
            ("fingerprint", json!({"seed":43})),
            ("proxy", json!({"password":null})),
            ("future", json!([42, 1])),
            ("future", json!([42.0, true])),
        ] {
            let mut changed = observed.clone();
            changed[field] = value;
            assert!(migration::require_profile(&changed, &expected).is_err());
        }
        for timestamp in [Value::Null, json!(true), json!("1002"), json!(-1.0)] {
            let mut changed = observed.clone();
            changed["updated_at"] = timestamp;
            assert!(migration::require_profile(&changed, &expected).is_err());
        }
        Ok(())
    }
);
source_test!(
    migration_replay_snapshot,
    "tests/test_migration_gates.py::test_replay_snapshot_tracks_exact_store_and_session_bytes",
    {
        let temp = Temp::new("replay byte snapshot")?;
        let profile = temp.write("profiles/fixture.json", b"{\"id\":\"fixture\"}\n")?;
        let session = temp.write(
            "user-data/fixture/Default/Cookies",
            b"\0\xffsynthetic session",
        )?;
        temp.write("config.json", b"{\"future\":true}")?;
        temp.write("dashboard-config.json", b"{\"folders\":[]}")?;
        let before = migration::protected(&temp.0)?;
        assert_eq!(before.len(), 4);
        assert_eq!(before["profiles/fixture.json"], fs::read(profile)?);
        temp.write("logs/run.log", b"ignored operational log")?;
        assert_eq!(migration::protected(&temp.0)?, before);
        fs::write(session, b"changed")?;
        assert_ne!(migration::protected(&temp.0)?, before);
        Ok(())
    }
);
source_test!(
    migration_range_verdict,
    "tests/test_migration_gates.py::test_benchmark_range_verdict_refuses_noise_regression_and_invalid_samples",
    {
        for (native, reference, expected) in [
            (json!([1, 2, 3]), json!([4, 5, 6]), "no-regression"),
            (json!([3, 3, 3]), json!([3, 3, 3]), "inconclusive"),
            (json!([7, 8, 9]), json!([4, 5, 6]), "regression"),
            (json!([3, 5, 7]), json!([4, 5, 6]), "inconclusive"),
        ] {
            assert_eq!(browser::verdict(&native, &reference)?, expected);
        }
        for invalid in [
            json!([]),
            json!([0]),
            json!([-1]),
            json!([true]),
            json!([1.0]),
            json!(["1"]),
        ] {
            assert!(browser::verdict(&invalid, &json!([1, 2, 3])).is_err());
            assert!(browser::verdict(&json!([1, 2, 3]), &invalid).is_err());
        }
        Ok(())
    }
);
source_test!(
    migration_failure_projection,
    "tests/test_migration_gates.py::test_benchmark_failure_projection_retains_only_known_stage_type_and_error_code",
    {
        let safe = "[2026-10-18 00:00:00 UTC] [ERROR] [numera.migration-benchmark-sample] Observation failed phase=memory type=OSError winerror=5";
        assert_eq!(
            browser::failure_projection(safe),
            "phase=memory type=OSError winerror=5"
        );
        assert_eq!(
            browser::failure_projection(&safe.replace("memory", "spawn")),
            "phase=spawn type=OSError winerror=5"
        );
        for invalid in [
            format!("{safe} password=synthetic-private"),
            safe.replace("memory", "private-path"),
            safe.replace("OSError", "SyntheticSensitiveError"),
            "raw synthetic-private traceback".into(),
        ] {
            assert_eq!(
                browser::failure_projection(&invalid),
                "phase=unknown type=unknown winerror=None"
            );
        }
        Ok(())
    }
);
source_test!(
    migration_spawn_refusal,
    "tests/test_migration_gates.py::test_benchmark_spawn_failure_reports_phase_without_touching_uncreated_process",
    {
        let mut host = super::browser_benchmark::Host::new("spawn-refusal");
        assert!(browser::observe(&mut host, Duration::from_secs(15)).is_err());
        assert_eq!(host.events, vec!["intent", "spawn"]);
        assert!(!host.created);
        Ok(())
    }
);
struct Natural {
    clock: u64,
    mode: &'static str,
    events: Vec<&'static str>,
    queries: usize,
}
impl browser::Observer for Natural {
    fn register(&mut self) -> TestResult {
        Ok(())
    }
    fn ticks(&mut self) -> u64 {
        self.clock
    }
    fn spawn(&mut self) -> TestResult {
        self.events.push("spawn");
        Ok(())
    }
    fn communicate(&mut self, _: u64) -> ci::Result<Value> {
        self.clock = if self.mode == "expired" {
            15_000_000_000
        } else {
            0
        };
        Ok(json!({"ok":true}))
    }
    fn natural(&mut self, _: u64) -> ci::Result<bool> {
        self.queries += 1;
        if self.mode == "late" {
            self.clock = 15_000_000_000;
            return Ok(true);
        }
        if self.mode == "unknown" {
            return Ok(false);
        }
        Ok(self.mode == "transient" && self.clock >= 250_000_000)
    }
    fn pause(&mut self) -> TestResult {
        self.clock += 10_000_000;
        Ok(())
    }
    fn peak(&mut self) -> ci::Result<(String, u64)> {
        self.events.push("peak");
        Ok(("peak-job-committed-bytes".into(), 1234))
    }
    fn cleanup(&mut self) -> TestResult {
        self.events.push("cleanup");
        Ok(())
    }
    fn reap(&mut self) -> TestResult {
        self.events.push("reap");
        Ok(())
    }
    fn close(&mut self) -> TestResult {
        self.events
            .extend(["stdout-close", "stderr-close", "process-close", "job-close"]);
        Ok(())
    }
}
source_test!(
    migration_natural_deadline,
    "tests/test_migration_gates.py::test_benchmark_requires_natural_settlement_inside_the_original_deadline",
    {
        let historical = fs::read(
            root().join("engine/tests/fixtures/benchmark-sample-before-settlement.py.txt"),
        )?;
        assert_eq!(
            ci::sha(&historical),
            "e9e65689ecff7dd714e7e1f349c442c02f4ad4992c77532d58c45a7bd2f2aaa1"
        );
        for mode in ["transient", "active", "unknown", "expired", "late"] {
            let mut host = Natural {
                clock: 0,
                mode,
                events: Vec::new(),
                queries: 0,
            };
            let result = browser::observe(&mut host, Duration::from_secs(15));
            if mode == "transient" {
                let result = result?;
                assert!(result.elapsed_ns >= 250_000_000);
                assert_eq!(result.bytes, 1234);
                assert!(
                    host.events.iter().position(|v| *v == "peak")
                        < host.events.iter().position(|v| *v == "cleanup")
                );
            } else {
                assert!(result.is_err());
                assert!(!host.events.contains(&"peak"));
                if matches!(mode, "active" | "unknown") {
                    assert!((15_000_000_000..15_050_000_000).contains(&host.clock));
                } else {
                    assert_eq!(host.queries, usize::from(mode == "late"));
                }
            }
            assert_eq!(host.events.iter().filter(|v| **v == "cleanup").count(), 1);
            assert_eq!(
                &host.events[host.events.len() - 2..],
                ["process-close", "job-close"]
            );
        }
        Ok(())
    }
);
source_test!(
    migration_positive_peak,
    "tests/test_migration_gates.py::test_benchmark_peak_requires_supported_platform_and_positive_os_observation",
    {
        assert_eq!(
            browser::linux_peak("linux", &json!(1234))?,
            json!({"metric":"peak-child-rss-bytes","bytes":1234*1024})
        );
        for invalid in [json!(0), json!(-1), json!(true), json!(1.5)] {
            assert!(browser::linux_peak("linux", &invalid).is_err());
        }
        assert!(browser::linux_peak("darwin", &json!(1234)).is_err());
        Ok(())
    }
);
fn compile(temp: &Temp, source: &str) -> ci::Result<PathBuf> {
    let file = temp.write("fixture.rs", source.as_bytes())?;
    let output = temp
        .0
        .join(format!("fixture{}", std::env::consts::EXE_SUFFIX));
    let mut command = Command::new("rustc");
    command
        .args(["--edition=2024", "--crate-name", "migration_fixture"])
        .arg(file)
        .arg("-o")
        .arg(&output);
    if source.contains("numera_core::") {
        let deps = std::env::current_exe()?
            .parent()
            .ok_or("Test dependency directory missing")?
            .to_owned();
        command
            .arg("--extern")
            .arg(format!(
                "numera_core={}",
                super::camoufox_runtime::linked_rlib(&deps, "numera_core")?.display()
            ))
            .arg("-L")
            .arg(format!("dependency={}", deps.display()));
    }
    let result = ci::capture(
        &mut command,
        Duration::from_secs(30),
        Duration::from_secs(20),
        65536,
    )?;
    assert_eq!(
        result.exit,
        0,
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(output)
}
source_test!(
    migration_birth_owned,
    "tests/test_migration_gates.py::test_benchmark_windows_assigns_job_at_birth_and_closes_failed_creation",
    {
        use numera_core::ownership::{identity, test_support, tree::OwnedTree};
        use std::sync::{
            Arc,
            atomic::{AtomicU32, Ordering},
        };
        let temp = Temp::new("native birth owner")?;
        let executable = compile(
            &temp,
            "fn main(){std::thread::sleep(std::time::Duration::from_secs(30));}",
        )?;
        let mut tree =
            OwnedTree::spawn(Command::new(&executable).stdin(std::process::Stdio::null()))?;
        let record = tree.record();
        assert_eq!(record["pid"], tree.pid());
        assert!(record["token"].as_u64().is_some_and(|v| v > 0));
        assert_eq!(
            record["tree"]["kind"],
            if cfg!(windows) { "job" } else { "pgid" }
        );
        tree.settle(Duration::from_secs(5))?;
        struct Refuse(AtomicU32);
        impl test_support::Backend for Refuse {
            fn spawned(&self, record: &Value) -> Option<io::Result<()>> {
                self.0.store(
                    record["pid"].as_u64().expect("pid") as u32,
                    Ordering::SeqCst,
                );
                Some(Err(io::Error::other("Synthetic creation refusal")))
            }
        }
        let fault = Arc::new(Refuse(AtomicU32::new(0)));
        let result = test_support::with_backend(fault.clone(), || {
            OwnedTree::spawn(&mut Command::new(&executable))
        });
        assert!(result.is_err());
        let pid = fault.0.load(Ordering::SeqCst);
        assert!(pid > 0);
        assert!(identity::started_at(pid)?.is_none());
        Ok(())
    }
);
fn entry_creation_fixture(
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
source_test!(
    migration_creation_content,
    "tests/test_migration_gates.py::test_benchmark_creation_compares_new_dashboard_content_not_just_existence",
    {
        let temp = Temp::new("creation content")?;
        let settings = json!({"default_engine":"playwright","future":{"items":[42,true]}});
        let profile =
            json!({"id":"new","name":"Measured creation","created_at":1000.0,"updated_at":1001.0});
        let dashboard = json!({"id":"new","name":"Measured creation","_rev":1,"_created":1000.0});
        let expected =
            entry_creation_fixture(&temp, &settings, &profile, &dashboard, "\n", b"", false)?;
        assert_eq!(
            entry_creation_fixture(&temp, &settings, &profile, &dashboard, "\r\n", b"", false)?,
            expected
        );
        for changed in [
            json!({"default_engine":"camoufox","future":{"items":[42,true]}}),
            json!({"default_engine":"playwright","future":{"items":[42,1]}}),
            json!({"default_engine":"playwright"}),
            json!({"default_engine":"playwright","future":{"items":[42,true]},"unexpected":true}),
            Value::Null,
        ] {
            assert!(
                entry_creation_fixture(&temp, &changed, &profile, &dashboard, "\n", b"", false)
                    .is_err()
            );
        }
        let mut changed = dashboard.clone();
        changed["name"] = json!("lost identity");
        assert_ne!(
            entry_creation_fixture(&temp, &settings, &profile, &changed, "\n", b"", false)?,
            expected,
            "Benchmark discarded new dashboard fields"
        );
        assert!(
            entry_creation_fixture(&temp, &settings, &profile, &json!({}), "\n", b"", false)
                .is_err()
        );
        assert!(
            entry_creation_fixture(
                &temp,
                &settings,
                &profile,
                &dashboard,
                "\n",
                b"not bookkeeping",
                false
            )
            .is_err()
        );
        assert!(
            entry_creation_fixture(&temp, &settings, &profile, &dashboard, "\n", b"", true)
                .is_err()
        );
        Ok(())
    }
);
fn reference_fixture(temp: &Temp) -> ci::Result<(PathBuf, Value)> {
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
source_test!(
    migration_reference_drift,
    "tests/test_migration_gates.py::test_frozen_reference_rejects_drift_and_keeps_current_python_modules",
    {
        let temp = Temp::new("reference drift")?;
        let (directory, receipt) = reference_fixture(&temp)?;
        reference::verify(&directory, &receipt)?;
        assert_eq!(receipt["bridge"].as_array().expect("bridge").len(), 2);
        let source = directory.join("engine-rust/src/lib.rs");
        let original = fs::read(&source)?;
        fs::write(&source, b"pub const VALUE:i32=2;")?;
        assert!(reference::verify(&directory, &receipt).is_err());
        fs::write(&source, original)?;
        fs::write(
            directory.join("unexpected.rs"),
            b"compile_error!(\"unreviewed\");",
        )?;
        assert!(reference::verify(&directory, &receipt).is_err());
        fs::remove_file(directory.join("unexpected.rs"))?;
        reference::verify(&directory, &receipt)?;
        assert!(
            !fs::read_to_string(directory.join("engine-rust/src/native_environment.rs"))?
                .contains("Command::new(&self.browser.python)")
        );
        Ok(())
    }
);
source_test!(
    migration_observer_reference,
    "tests/test_migration_gates.py::test_original_observer_imports_reference_not_current_path",
    {
        let temp = Temp::new("observer source")?;
        temp.write(
            "current/library.rs",
            b"compile_error!(\"current source leaked\");",
        )?;
        let original = temp.write("original/library.rs", b"pub const VALUE:i32=42;")?;
        let source = format!(
            "#[path={:?}] mod reference;fn main(){{assert_eq!(reference::VALUE,42);println!(\"original\");}}",
            original
        );
        let executable = compile(&temp, &source)?;
        let output = ci::capture(
            &mut Command::new(executable),
            Duration::from_secs(5),
            Duration::from_secs(5),
            4096,
        )?;
        assert_eq!(output.exit, 0);
        assert_eq!(output.stdout, b"original\n");
        let path = temp.0.join("exclusive-run");
        let fixture = acceptance::Fixture::create(&path)?;
        fs::write(fixture.path().join("frozen.rs"), fs::read(original)?)?;
        fixture.finish(Ok(()))?;
        assert!(!path.exists());
        Ok(())
    }
);
source_test!(
    migration_empty_ledgers,
    "tests/test_migration_gates.py::test_empty_missing_and_forged_ledgers_cannot_allow_switch",
    {
        let temp = Temp::new("empty ledgers")?;
        let source = occurrence_fixture()?;
        let mut valid = complete(&temp, &source)?;
        for document in [
            json!([]),
            json!({}),
            json!({"capabilities":[]}),
            json!({"files":source["files"],"capabilities":[{"id":"source.rs::other","status":"verified"}]}),
            json!({"files":source["files"],"capabilities":[{"id":"source.rs::take","status":"pending"}]}),
            json!({"files":source["files"],"capabilities":[{"id":"source.rs::take","status":"verified","target":"../outside","test":"../outside"}]}),
        ] {
            assert!(!migration::coverage(&temp.0, &source, &document)?.is_empty());
        }
        valid["capabilities"][0]["target"] = json!("../outside");
        assert!(!migration::coverage(&temp.0, &source, &valid)?.is_empty());
        Ok(())
    }
);
source_test!(
    migration_owned_command,
    "tests/test_migration_gates.py::test_owned_migration_command_propagates_exit_and_bounds_idle",
    {
        let temp = Temp::new("owned command")?;
        let executable = compile(
            &temp,
            "fn main(){if std::env::args().nth(1).as_deref()==Some(\"idle\"){std::thread::sleep(std::time::Duration::from_secs(30));}else{println!(\"ready\");std::process::exit(7);}}",
        )?;
        let output = ci::owned::run(
            &temp.0,
            &mut Command::new(&executable),
            ci::owned::limits(Duration::from_secs(10), Duration::from_secs(5), 4096),
        )?;
        assert_eq!(output.command.exit, 7);
        assert_eq!(output.command.stdout, b"ready\n");
        let started = std::time::Instant::now();
        assert!(
            ci::owned::run(
                &temp.0,
                Command::new(executable).arg("idle"),
                ci::owned::limits(Duration::from_secs(5), Duration::from_millis(500), 4096)
            )
            .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        let mut logs = String::new();
        for entry in fs::read_dir(temp.0.join(".ci-work/logs"))? {
            logs.push_str(&fs::read_to_string(entry?.path())?);
        }
        for phase in [
            "spawn",
            "monitor",
            "settle-tree",
            "reap-leader",
            "drain-output",
            "close-output",
            "remove-temp",
        ] {
            assert!(logs.contains(&format!("phase={phase}")), "{phase}");
        }
        Ok(())
    }
);
source_test!(
    migration_temp_bound,
    "tests/test_migration_gates.py::test_linux_temp_root_fits_chromium_socket_and_keeps_windows_contained",
    {
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
        let invalid = BTreeMap::from([
            ("GITHUB_ACTIONS".into(), "true".into()),
            ("RUNNER_TEMP".into(), format!("/{}", "x".repeat(100))),
        ]);
        assert!(migration::temporary_parent(&long, &invalid, "linux").is_err());
        let unicode = PathBuf::from(format!("/{}", "Ω".repeat(40)));
        assert_eq!(
            migration::temporary_parent(&unicode, &env, "linux")?,
            PathBuf::from("/home/runner/work/_temp")
        );
        Ok(())
    }
);
source_test!(
    migration_temp_owned,
    "tests/test_migration_gates.py::test_owned_temp_is_unique_and_cleanup_removes_only_its_run",
    {
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
);
source_test!(
    migration_log_safety,
    "tests/test_migration_gates.py::test_migration_logs_are_unique_sanitized_closed_and_fallback",
    {
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
);
