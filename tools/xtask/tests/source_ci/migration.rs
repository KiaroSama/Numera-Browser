use super::*;
use ci::{browser_benchmark as browser, migration, reference};
use numera_xtask::acceptance;
use std::{process::Command, time::Duration};

#[path = "migration/benchmark_controls.rs"]
mod benchmark_controls;
#[path = "migration/fixtures.rs"]
mod fixtures;
use benchmark_controls::{exact_inventory_controls, natural_deadline};
use fixtures::{
    complete, entry_creation_fixture, log_safety, occurrence_fixture, reference_fixture,
    temp_bound, temp_owned,
};

fn historical_metadata(test: &str, expected: &str) -> TestResult {
    let map = ci::decode(&fs::read(root().join("docs/source-ci-test-map.json"))?)?;
    let row = map["cases"]
        .as_array()
        .ok_or("Cases missing")?
        .iter()
        .find(|row| row["rust_test"] == test)
        .ok_or("Historical characterization metadata missing")?;
    assert_eq!(row["historical_program_sha256"], expected);
    Ok(())
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
        historical_metadata(
            "migration::migration_occurrences",
            "22de18dea172c6f67a991fa53ed47dfeaae220f1a139a39f93f5f9f76c274314",
        )?;
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
        // Reproduce the old collapse as a mutation of the real Rust inventory,
        // then require the production coverage gate to report every lost ID.
        let temp = Temp::new("collapsed Rust occurrences")?;
        let mut mutant = complete(&temp, &result)?;
        for row in mutant["capabilities"].as_array_mut().expect("capabilities") {
            row["id"] = json!(
                row["id"]
                    .as_str()
                    .expect("id")
                    .split('#')
                    .next()
                    .expect("base")
            );
        }
        assert_eq!(
            migration::coverage(&temp.0, &result, &mutant)?,
            vec![
                "Duplicate capability: source.rs::take",
                "Duplicate capability: source.rs::take",
                "Duplicate capability: source.rs::read.pipe",
                "Missing capability: source.rs::take#2",
                "Missing capability: source.rs::take#3",
                "Missing capability: source.rs::read.pipe#2",
            ]
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
source_test!(
    migration_natural_deadline,
    "tests/test_migration_gates.py::test_benchmark_requires_natural_settlement_inside_the_original_deadline",
    { natural_deadline() }
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
pub(super) fn compile(temp: &Temp, source: &str) -> ci::Result<PathBuf> {
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
    let mut dependencies = ["numera_core", "numera_xtask", "serde_json"]
        .into_iter()
        .filter(|name| source.contains(&format!("{name}::")))
        .peekable();
    if dependencies.peek().is_some() {
        let artifacts = cargo_artifacts::Artifacts::current()?;
        for dependency in dependencies {
            command.arg("--extern").arg(format!(
                "{dependency}={}",
                artifacts.library(dependency)?.display()
            ));
        }
        artifacts.add_dependency_search_paths(&mut command, &temp.0.join("rustc-search.args"))?;
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
            r#"#![cfg_attr(windows, windows_subsystem = "windows")]
            fn main(){
                match std::env::args().nth(1).as_deref(){
                    Some("idle")=>std::thread::sleep(std::time::Duration::from_secs(30)),
                    Some("temp")=>{
                        let paths=["TMPDIR","TMP","TEMP"].map(|key|std::path::PathBuf::from(std::env::var_os(key).expect("owned temp variable")));
                        let physical=std::fs::canonicalize(&paths[0]).expect("owned temp exists");
                        assert_eq!(std::fs::canonicalize(std::env::temp_dir()).expect("OS temp identity"),physical);
                        std::fs::write(paths[0].join("owned-temp-marker"),b"same physical directory").expect("temp marker");
                        for path in paths{
                            assert_eq!(std::fs::canonicalize(&path).expect("temp identity"),physical);
                            assert_eq!(std::fs::read(path.join("owned-temp-marker")).expect("shared marker"),b"same physical directory");
                            #[cfg(windows)]
                            assert!(matches!(path.components().next(),Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::UNC(..))));
                        }
                        println!("owned-temp-ok");
                    },
                    _=>{println!("ready");std::process::exit(7);}
                }
            }"#,
        )?;
        let directory = temp.0.join("legacy temporary with spaces");
        fs::create_dir(&directory)?;
        let physical = directory.canonicalize()?;
        let exported = ci::owned_paths::temporary_environment_path(&physical)?;
        assert_eq!(exported.canonicalize()?, physical);
        assert!(ci::owned_paths::temporary_environment_path(&directory.join("missing")).is_err());
        #[cfg(not(windows))]
        assert_eq!(
            ci::owned_paths::temporary_environment_path(&directory)?,
            directory
        );
        #[cfg(windows)]
        {
            assert!(
                matches!(exported.components().next(),Some(std::path::Component::Prefix(prefix)) if matches!(prefix.kind(),std::path::Prefix::Disk(_)|std::path::Prefix::UNC(..)))
            );
            let unsafe_name = physical.join("trailing space ");
            fs::create_dir(&unsafe_name)?;
            assert_eq!(
                ci::owned_paths::temporary_environment_path(&unsafe_name)
                    .expect_err("unsafe NT temporary path")
                    .kind(),
                io::ErrorKind::InvalidInput
            );
            fs::remove_dir(&unsafe_name)?;
        }
        let mut probe = Command::new(&executable);
        probe.arg("temp");
        let output = ci::owned::run(
            &temp.0,
            &mut probe,
            ci::owned::limits(Duration::from_secs(10), Duration::from_secs(5), 4096),
        )?;
        assert_eq!(output.command.exit, 0);
        assert_eq!(output.command.stdout, b"owned-temp-ok\n");
        let exported = probe
            .get_envs()
            .filter_map(|(key, value)| {
                ["TMPDIR", "TMP", "TEMP"]
                    .iter()
                    .any(|expected| key == *expected)
                    .then_some(value)
                    .flatten()
            })
            .collect::<Vec<_>>();
        assert_eq!(exported.len(), 3);
        assert!(exported.iter().all(|path| *path == exported[0]));
        assert!(!Path::new(exported[0]).join("owned-temp-marker").exists());
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
    { temp_bound() }
);
source_test!(
    migration_temp_owned,
    "tests/test_migration_gates.py::test_owned_temp_is_unique_and_cleanup_removes_only_its_run",
    { temp_owned() }
);
source_test!(
    migration_log_safety,
    "tests/test_migration_gates.py::test_migration_logs_are_unique_sanitized_closed_and_fallback",
    { log_safety() }
);
