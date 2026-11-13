use super::*;
use numera_xtask::ci::{evidence, release, scope, workflow};
fn expected_modules(names: &[&str]) -> BTreeMap<String, BTreeSet<String>> {
    names
        .iter()
        .map(|name| ((*name).into(), BTreeSet::from(["actual_path".into()])))
        .collect()
}
fn source_rows(names: &[&str]) -> Value {
    json!(
        names
            .iter()
            .map(|name| json!({"module":name,"cases":[{"id":"actual_path","status":"passed"}]}))
            .collect::<Vec<_>>()
    )
}
struct Phases {
    events: Vec<&'static str>,
    fault: Option<&'static str>,
}
impl NativePhases for Phases {
    fn fetch(&mut self) -> Result<()> {
        self.events.push("fetch");
        if self.fault == Some("fetch") {
            return ci::fail("fixture fetch failed");
        }
        Ok(())
    }
    fn history(&mut self) -> Result<()> {
        self.events.push("proof");
        if self.fault == Some("proof") {
            return ci::fail("fixture proof failed");
        }
        Ok(())
    }
    fn build(&mut self) -> Result<()> {
        self.events.push("fixed-build");
        Ok(())
    }
    fn test(&mut self) -> Result<()> {
        self.events.push("fixed-test");
        Ok(())
    }
}

source_test!(
    test_report_requires_both_selected_tests_to_pass_none_dd19e7,
    "tests/test_live_engine_contract.py::test_report_requires_both_selected_tests_to_pass[None]",
    {
        let damage = "None";
        let t = Temp::new()?;
        let expected = vec![
            "owned_session_tree[patchright]".to_owned(),
            "hard_killed_owner_cleanup[patchright]".into(),
        ];
        let observed = expected.clone();
        let outcomes = vec!["ok", "ok"];
        let mut text = format!("running {} tests\n", observed.len());
        for (name, status) in observed.iter().zip(outcomes) {
            text.push_str(&format!("test {name} ... {status}\n"));
        }
        text.push_str("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n");
        let path = t.path().join("results.txt");
        fs::write(&path, &text)?;
        let mut report = evidence::Current::new(expected)?;
        for line in fs::read_to_string(path)?.lines() {
            report.accept(line);
        }
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_report_requires_both_selected_tests_to_pass_missing_1c3afa,
    "tests/test_live_engine_contract.py::test_report_requires_both_selected_tests_to_pass[missing]",
    {
        let damage = "missing";
        let t = Temp::new()?;
        let expected = vec![
            "owned_session_tree[patchright]".to_owned(),
            "hard_killed_owner_cleanup[patchright]".into(),
        ];
        let mut observed = expected.clone();
        let mut outcomes = vec!["ok", "ok"];
        observed.remove(0);
        outcomes.remove(0);
        let mut text = format!("running {} tests\n", observed.len());
        for (name, status) in observed.iter().zip(outcomes) {
            text.push_str(&format!("test {name} ... {status}\n"));
        }
        text.push_str("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n");
        let path = t.path().join("results.txt");
        fs::write(&path, &text)?;
        let mut report = evidence::Current::new(expected)?;
        for line in fs::read_to_string(path)?.lines() {
            report.accept(line);
        }
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_report_requires_both_selected_tests_to_pass_wrong_engine_728576,
    "tests/test_live_engine_contract.py::test_report_requires_both_selected_tests_to_pass[wrong-engine]",
    {
        let damage = "wrong-engine";
        let t = Temp::new()?;
        let expected = vec![
            "owned_session_tree[patchright]".to_owned(),
            "hard_killed_owner_cleanup[patchright]".into(),
        ];
        let mut observed = expected.clone();
        let outcomes = vec!["ok", "ok"];
        observed[0] = "owned_session_tree[playwright]".into();
        let mut text = format!("running {} tests\n", observed.len());
        for (name, status) in observed.iter().zip(outcomes) {
            text.push_str(&format!("test {name} ... {status}\n"));
        }
        text.push_str("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n");
        let path = t.path().join("results.txt");
        fs::write(&path, &text)?;
        let mut report = evidence::Current::new(expected)?;
        for line in fs::read_to_string(path)?.lines() {
            report.accept(line);
        }
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_report_requires_both_selected_tests_to_pass_duplicate_b0fad9,
    "tests/test_live_engine_contract.py::test_report_requires_both_selected_tests_to_pass[duplicate]",
    {
        let damage = "duplicate";
        let t = Temp::new()?;
        let expected = vec![
            "owned_session_tree[patchright]".to_owned(),
            "hard_killed_owner_cleanup[patchright]".into(),
        ];
        let mut observed = expected.clone();
        let outcomes = vec!["ok", "ok"];
        observed[1] = observed[0].clone();
        let mut text = format!("running {} tests\n", observed.len());
        for (name, status) in observed.iter().zip(outcomes) {
            text.push_str(&format!("test {name} ... {status}\n"));
        }
        text.push_str("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n");
        let path = t.path().join("results.txt");
        fs::write(&path, &text)?;
        let mut report = evidence::Current::new(expected)?;
        for line in fs::read_to_string(path)?.lines() {
            report.accept(line);
        }
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_report_requires_both_selected_tests_to_pass_skipped_57cc3e,
    "tests/test_live_engine_contract.py::test_report_requires_both_selected_tests_to_pass[skipped]",
    {
        let damage = "skipped";
        let t = Temp::new()?;
        let expected = vec![
            "owned_session_tree[patchright]".to_owned(),
            "hard_killed_owner_cleanup[patchright]".into(),
        ];
        let observed = expected.clone();
        let mut outcomes = vec!["ok", "ok"];
        outcomes[0] = "ignored";
        let mut text = format!("running {} tests\n", observed.len());
        for (name, status) in observed.iter().zip(outcomes) {
            text.push_str(&format!("test {name} ... {status}\n"));
        }
        text.push_str("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n");
        let path = t.path().join("results.txt");
        fs::write(&path, &text)?;
        let mut report = evidence::Current::new(expected)?;
        for line in fs::read_to_string(path)?.lines() {
            report.accept(line);
        }
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_report_requires_both_selected_tests_to_pass_failure_a69a0a,
    "tests/test_live_engine_contract.py::test_report_requires_both_selected_tests_to_pass[failure]",
    {
        let damage = "failure";
        let t = Temp::new()?;
        let expected = vec![
            "owned_session_tree[patchright]".to_owned(),
            "hard_killed_owner_cleanup[patchright]".into(),
        ];
        let observed = expected.clone();
        let mut outcomes = vec!["ok", "ok"];
        outcomes[0] = "FAILED";
        let mut text = format!("running {} tests\n", observed.len());
        for (name, status) in observed.iter().zip(outcomes) {
            text.push_str(&format!("test {name} ... {status}\n"));
        }
        text.push_str("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n");
        let path = t.path().join("results.txt");
        fs::write(&path, &text)?;
        let mut report = evidence::Current::new(expected)?;
        for line in fs::read_to_string(path)?.lines() {
            report.accept(line);
        }
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_report_requires_both_selected_tests_to_pass_error_27023b,
    "tests/test_live_engine_contract.py::test_report_requires_both_selected_tests_to_pass[error]",
    {
        let damage = "error";
        let t = Temp::new()?;
        let expected = vec![
            "owned_session_tree[patchright]".to_owned(),
            "hard_killed_owner_cleanup[patchright]".into(),
        ];
        let observed = expected.clone();
        let mut outcomes = vec!["ok", "ok"];
        outcomes[0] = "error";
        let mut text = format!("running {} tests\n", observed.len());
        for (name, status) in observed.iter().zip(outcomes) {
            text.push_str(&format!("test {name} ... {status}\n"));
        }
        text.push_str("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n");
        let path = t.path().join("results.txt");
        fs::write(&path, &text)?;
        let mut report = evidence::Current::new(expected)?;
        for line in fs::read_to_string(path)?.lines() {
            report.accept(line);
        }
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_missing_report_and_browser_are_failures,
    "tests/test_live_engine_contract.py::test_missing_report_and_browser_are_failures",
    {
        let t = Temp::new()?;
        assert_eq!(
            fs::read(t.path().join("missing.txt"))
                .err()
                .ok_or("missing report accepted")?
                .kind(),
            io::ErrorKind::NotFound
        );
        assert!(ci::regular(&t.path().join("missing-browser"), false).is_err());
        let environment = t.path().join("environment");
        assert!(
            export_native(
                "cloak",
                &t.path().join("missing-browser"),
                Some(&environment)
            )
            .is_err()
        );
        assert!(!environment.exists());

        Ok(())
    }
);
source_test!(
    test_native_handoff_exports_only_the_verified_cloak_path,
    "tests/test_live_engine_contract.py::test_native_handoff_exports_only_the_verified_cloak_path",
    {
        let t = Temp::new()?;
        let binary = t.path().join("browser with spaces — fixture");
        fs::write(&binary, b"not executed")?;
        let environment = t.path().join("github environment");
        fs::write(&environment, b"EXISTING=value\n")?;
        export_native("cloak", &binary, Some(&environment))?;
        assert_eq!(
            fs::read_to_string(&environment)?,
            format!(
                "EXISTING=value\nCLOAKBROWSER_BINARY_PATH={}\n",
                binary.canonicalize()?.display()
            )
        );
        let before = fs::read(&environment)?;
        export_native("playwright", &binary, Some(&environment))?;
        assert_eq!(fs::read(&environment)?, before);
        assert!(export_native("cloak", &t.path().join("missing"), Some(&environment)).is_err());
        assert_eq!(fs::read(environment)?, before);
        assert!(
            export_native("cloak", &binary, None)
                .err()
                .ok_or("missing environment accepted")?
                .to_string()
                .contains("GitHub environment file")
        );

        Ok(())
    }
);
source_test!(
    test_native_handoff_rejects_environment_injection_n_0bdb05,
    "tests/test_live_engine_contract.py::test_native_handoff_rejects_environment_injection[\\n]",
    {
        let t = Temp::new()?;
        let environment = t.path().join("github environment");
        let value = format!("/fixture/browser{}INJECTED=value", "\u{a}");
        assert!(
            export_value(&value, Some(&environment))
                .err()
                .ok_or("control character accepted")?
                .to_string()
                .contains("invalid environment characters")
        );
        assert!(!environment.exists());

        Ok(())
    }
);
source_test!(
    test_native_handoff_rejects_environment_injection_r_76c9a0,
    "tests/test_live_engine_contract.py::test_native_handoff_rejects_environment_injection[\\r]",
    {
        let t = Temp::new()?;
        let environment = t.path().join("github environment");
        let value = format!("/fixture/browser{}INJECTED=value", "\u{d}");
        assert!(
            export_value(&value, Some(&environment))
                .err()
                .ok_or("control character accepted")?
                .to_string()
                .contains("invalid environment characters")
        );
        assert!(!environment.exists());

        Ok(())
    }
);
source_test!(
    test_native_handoff_rejects_environment_injection_x00_e3569e,
    "tests/test_live_engine_contract.py::test_native_handoff_rejects_environment_injection[\\x00]",
    {
        let t = Temp::new()?;
        let environment = t.path().join("github environment");
        let value = format!("/fixture/browser{}INJECTED=value", "\u{0}");
        assert!(
            export_value(&value, Some(&environment))
                .err()
                .ok_or("control character accepted")?
                .to_string()
                .contains("invalid environment characters")
        );
        assert!(!environment.exists());

        Ok(())
    }
);
source_test!(
    test_native_handoff_rejects_environment_injection_x1f_20c1c4,
    "tests/test_live_engine_contract.py::test_native_handoff_rejects_environment_injection[\\x1f]",
    {
        let t = Temp::new()?;
        let environment = t.path().join("github environment");
        let value = format!("/fixture/browser{}INJECTED=value", "\u{1f}");
        assert!(
            export_value(&value, Some(&environment))
                .err()
                .ok_or("control character accepted")?
                .to_string()
                .contains("invalid environment characters")
        );
        assert!(!environment.exists());

        Ok(())
    }
);
source_test!(
    test_native_handoff_rejects_environment_injection_x7f_28d792,
    "tests/test_live_engine_contract.py::test_native_handoff_rejects_environment_injection[\\x7f]",
    {
        let t = Temp::new()?;
        let environment = t.path().join("github environment");
        let value = format!("/fixture/browser{}INJECTED=value", "\u{7f}");
        assert!(
            export_value(&value, Some(&environment))
                .err()
                .ok_or("control character accepted")?
                .to_string()
                .contains("invalid environment characters")
        );
        assert!(!environment.exists());

        Ok(())
    }
);
source_test!(
    test_every_supported_engine_is_required_on_both_platforms,
    "tests/test_live_engine_contract.py::test_every_supported_engine_is_required_on_both_platforms",
    {
        let root = source_root();
        let ci = workflow::parse(&fs::read_to_string(root.join(".github/workflows/ci.yml"))?)?;
        let jobs = &ci["jobs"];
        workflow::bounded_jobs(&ci)?;
        let matrix = &jobs["engine-live"]["strategy"]["matrix"];
        let engines = matrix["engine"]
            .as_array()
            .ok_or("live engine matrix missing")?
            .iter()
            .map(|v| v.as_str().ok_or("engine name missing"))
            .collect::<std::result::Result<BTreeSet<_>, _>>()?;
        assert_eq!(
            engines,
            BTreeSet::from(["playwright", "patchright", "camoufox", "cloak"])
        );
        let platforms = matrix["os"]
            .as_array()
            .ok_or("live platform matrix missing")?;
        assert!(
            platforms
                .iter()
                .any(|v| v.as_str().is_some_and(|s| s.starts_with("ubuntu")))
        );
        assert!(
            platforms
                .iter()
                .any(|v| v.as_str().is_some_and(|s| s.starts_with("windows")))
        );
        assert_eq!(jobs["engine-live"]["strategy"]["max-parallel"], 2);
        let expected = jobs
            .as_object()
            .ok_or("jobs missing")?
            .keys()
            .filter(|name| !["required", "promote", "publish"].contains(&name.as_str()))
            .cloned()
            .collect();
        workflow::required_jobs(jobs, &expected)?;
        let automatic = workflow::parse(&fs::read_to_string(
            root.join(".github/workflows/rust-candidate.yml"),
        )?)?;
        workflow::native_automatic(&automatic)?;
        let tools = numera_core::cli::reflection::tools();
        assert!(tools.iter().any(|tool| tool["name"] == "engines"));
        let native_action = fs::read_to_string(
            root.join(".github/actions/native-candidate-acceptance/action.yml"),
        )?;
        assert!(!native_action.contains("continue-on-error"));
        assert!(native_action.contains("--test browser_adapter_live"));

        Ok(())
    }
);
source_test!(
    test_release_downloads_and_publishes_only_verified_manifest_products,
    "tests/test_live_engine_contract.py::test_release_downloads_and_publishes_only_verified_manifest_products",
    {
        let t = Temp::new()?;
        let names = [
            "numera-server.tar.gz",
            "numera-linux-x64.tar.gz",
            "numera-macos-arm64.tar.gz",
            "numera-windows-x64.zip",
        ];
        let mut assets = Vec::new();
        for name in names {
            let path = t.path().join(name);
            fs::write(&path, name.as_bytes())?;
            assets.push(release::asset(&path)?);
        }
        let sha = "a".repeat(40);
        let manifest = json!({"schema":1,"source_sha":sha,"assets":assets});
        release::verify_distribution(t.path(), &manifest, &sha, false)?;
        assert!(release::verify_distribution(t.path(), &manifest, &"b".repeat(40), false).is_err());
        fs::write(t.path().join(names[0]), b"tampered")?;
        assert!(release::verify_distribution(t.path(), &manifest, &sha, false).is_err());
        fs::write(t.path().join(names[0]), names[0].as_bytes())?;
        fs::write(
            t.path().join("unreviewed.bin"),
            b"unexpected publication payload",
        )?;
        assert!(release::verify_distribution(t.path(), &manifest, &sha, false).is_err());
        let workflow = fs::read_to_string(source_root().join(".github/workflows/release.yml"))?;
        assert!(!workflow.contains("continue-on-error"));
        assert!(workflow.contains("digest-mismatch: error"));
        assert!(workflow.contains("SOURCE_SHA"));
        assert!(workflow.contains("update-manifest.sig"));
        assert!(workflow.contains("--prerelease --latest=false"));

        Ok(())
    }
);
source_test!(
    test_independent_native_acceptance_precedes_the_required_benchmark_none_8d61b2,
    "tests/test_live_engine_contract.py::test_independent_native_acceptance_precedes_the_required_benchmark[None]",
    {
        let damage = "None";
        let names = [
            "http", "parent", "console", "tty", "jobs", "signal", "default",
        ];
        let mut steps = names
            .iter()
            .map(|name| json!({"run":format!("cargo xtask ci acceptance {name}")}))
            .collect::<Vec<_>>();
        let performance = json!({"uses":"./.github/actions/native-entry-performance"});
        steps.push(performance.clone());
        let commands = names
            .iter()
            .map(|s| format!("ci acceptance {s}"))
            .collect::<Vec<_>>();
        let commands = commands.iter().map(String::as_str).collect::<Vec<_>>();
        let result = workflow::ordered(
            &json!(steps),
            &commands,
            "./.github/actions/native-entry-performance",
        );
        assert_eq!(result.is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_independent_native_acceptance_precedes_the_required_benchmark_old_order_9d5fd6,
    "tests/test_live_engine_contract.py::test_independent_native_acceptance_precedes_the_required_benchmark[old-order]",
    {
        let damage = "old-order";
        let names = [
            "http", "parent", "console", "tty", "jobs", "signal", "default",
        ];
        let mut steps = names
            .iter()
            .map(|name| json!({"run":format!("cargo xtask ci acceptance {name}")}))
            .collect::<Vec<_>>();
        let performance = json!({"uses":"./.github/actions/native-entry-performance"});
        steps.push(performance.clone());
        let performance = steps.pop().ok_or("performance missing")?;
        steps.insert(0, performance);
        let commands = names
            .iter()
            .map(|s| format!("ci acceptance {s}"))
            .collect::<Vec<_>>();
        let commands = commands.iter().map(String::as_str).collect::<Vec<_>>();
        let result = workflow::ordered(
            &json!(steps),
            &commands,
            "./.github/actions/native-entry-performance",
        );
        assert_eq!(result.is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_independent_native_acceptance_precedes_the_required_benchmark_missing_f9c561,
    "tests/test_live_engine_contract.py::test_independent_native_acceptance_precedes_the_required_benchmark[missing]",
    {
        let damage = "missing";
        let names = [
            "http", "parent", "console", "tty", "jobs", "signal", "default",
        ];
        let mut steps = names
            .iter()
            .map(|name| json!({"run":format!("cargo xtask ci acceptance {name}")}))
            .collect::<Vec<_>>();
        let performance = json!({"uses":"./.github/actions/native-entry-performance"});
        steps.push(performance.clone());
        steps.remove(0);
        let commands = names
            .iter()
            .map(|s| format!("ci acceptance {s}"))
            .collect::<Vec<_>>();
        let commands = commands.iter().map(String::as_str).collect::<Vec<_>>();
        let result = workflow::ordered(
            &json!(steps),
            &commands,
            "./.github/actions/native-entry-performance",
        );
        assert_eq!(result.is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_independent_native_acceptance_precedes_the_required_benchmark_duplicate_f9998d,
    "tests/test_live_engine_contract.py::test_independent_native_acceptance_precedes_the_required_benchmark[duplicate]",
    {
        let damage = "duplicate";
        let names = [
            "http", "parent", "console", "tty", "jobs", "signal", "default",
        ];
        let mut steps = names
            .iter()
            .map(|name| json!({"run":format!("cargo xtask ci acceptance {name}")}))
            .collect::<Vec<_>>();
        let performance = json!({"uses":"./.github/actions/native-entry-performance"});
        steps.push(performance.clone());
        steps.push(performance);
        let commands = names
            .iter()
            .map(|s| format!("ci acceptance {s}"))
            .collect::<Vec<_>>();
        let commands = commands.iter().map(String::as_str).collect::<Vec<_>>();
        let result = workflow::ordered(
            &json!(steps),
            &commands,
            "./.github/actions/native-entry-performance",
        );
        assert_eq!(result.is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_native_development_optimizes_real_crypto_without_weakening_checks,
    "tests/test_live_engine_contract.py::test_native_development_optimizes_real_crypto_without_weakening_checks",
    {
        let manifest = fs::read_to_string(source_root().join("engine-rust/Cargo.toml"))?;
        let sections = regex::Regex::new(r"(?ms)^\[(profile\.dev\.[^\]]+)\]\s*\n([^\[]*)")?;
        let comments = regex::Regex::new(r"(?m)^\s*#.*$")?;
        let profiles = sections
            .captures_iter(&manifest)
            .map(|c| {
                (
                    c[1].to_owned(),
                    comments.replace_all(&c[2], "").trim().to_owned(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            profiles.keys().cloned().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "profile.dev.package.numera-core".into(),
                "profile.dev.package.\"sha2:0.11.1\"".into()
            ])
        );
        assert!(profiles.values().all(|value| value == "opt-level = 1"));

        Ok(())
    }
);
source_test!(
    test_discovered_native_targets_are_all_selected_and_build_is_separate,
    "tests/test_live_engine_contract.py::test_discovered_native_targets_are_all_selected_and_build_is_separate",
    {
        let root = source_root();
        let targets = scope::targets(&root)?;
        let test = scope::native_command(&root, "test", &targets)?;
        let build = scope::native_command(&root, "build", &targets)?;
        let args = test
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let selected = args
            .windows(2)
            .filter(|w| w[0] == "--test")
            .map(|w| w[1].clone())
            .collect::<BTreeSet<_>>();
        let expected = targets
            .into_iter()
            .filter(|name| name != "browser_adapter_live")
            .collect::<BTreeSet<_>>();
        assert_eq!(selected, expected);
        assert!(args.iter().any(|s| s == "--lib"));
        assert!(args.iter().any(|s| s == "--no-fail-fast"));
        assert!(args.iter().any(|s| s == "--test-threads=2"));
        assert!(!args.iter().any(|s| s == "--no-run"));
        let args = build
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.iter().any(|s| s == "--no-run"));
        assert!(!args.iter().any(|s| s == "--"));
        for command in [&build, &test] {
            let args = command
                .get_args()
                .map(|s| s.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let format = args
                .iter()
                .position(|s| s == "--message-format=json,json-render-diagnostics")
                .ok_or("native compiler progress option missing")?;
            assert!(format < args.iter().position(|s| s == "--").unwrap_or(args.len()));
        }

        Ok(())
    }
);
source_test!(
    test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test_none_b75a6d,
    "tests/test_live_engine_contract.py::test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test[None]",
    {
        let damage = "None";
        let mut runner = Phases {
            events: Vec::new(),
            fault: None,
        };
        let result = run_phase("history", &mut runner);
        if damage == "None" {
            result?;
            assert_eq!(runner.events, vec!["fetch", "proof"]);
            runner.events.clear();
            run_phase("build", &mut runner)?;
            assert_eq!(runner.events, vec!["fixed-build"]);
            runner.events.clear();
            run_phase("test", &mut runner)?;
            assert_eq!(runner.events, vec!["fixed-test"]);
        } else {
            assert!(result.is_err());
            assert_eq!(runner.events, vec!["fetch", "proof"]);
        }
        let root = source_root();
        let registry = ci::history::Registry::read()?.cases()?;
        assert!(!registry.is_empty());
        let proof = ci::history::proof_command(&root, &registry[0], true, "linux");
        assert!(proof.get_args().any(|s| s == "--no-run"));

        Ok(())
    }
);
source_test!(
    test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test_fetch_7fb2cb,
    "tests/test_live_engine_contract.py::test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test[fetch]",
    {
        let damage = "fetch";
        let mut runner = Phases {
            events: Vec::new(),
            fault: Some("fetch"),
        };
        let result = run_phase("history", &mut runner);
        if damage == "None" {
            result?;
            assert_eq!(runner.events, vec!["fetch", "proof"]);
            runner.events.clear();
            run_phase("build", &mut runner)?;
            assert_eq!(runner.events, vec!["fixed-build"]);
            runner.events.clear();
            run_phase("test", &mut runner)?;
            assert_eq!(runner.events, vec!["fixed-test"]);
        } else {
            assert!(result.is_err());
            assert_eq!(runner.events, vec!["fetch"]);
        }
        let root = source_root();
        let registry = ci::history::Registry::read()?.cases()?;
        assert!(!registry.is_empty());
        let proof = ci::history::proof_command(&root, &registry[0], true, "linux");
        assert!(proof.get_args().any(|s| s == "--no-run"));

        Ok(())
    }
);
source_test!(
    test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test_proof_a56dbb,
    "tests/test_live_engine_contract.py::test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test[proof]",
    {
        let damage = "proof";
        let mut runner = Phases {
            events: Vec::new(),
            fault: Some("proof"),
        };
        let result = run_phase("history", &mut runner);
        if damage == "None" {
            result?;
            assert_eq!(runner.events, vec!["fetch", "proof"]);
            runner.events.clear();
            run_phase("build", &mut runner)?;
            assert_eq!(runner.events, vec!["fixed-build"]);
            runner.events.clear();
            run_phase("test", &mut runner)?;
            assert_eq!(runner.events, vec!["fixed-test"]);
        } else {
            assert!(result.is_err());
            assert_eq!(runner.events, vec!["fetch", "proof"]);
        }
        let root = source_root();
        let registry = ci::history::Registry::read()?.cases()?;
        assert!(!registry.is_empty());
        let proof = ci::history::proof_command(&root, &registry[0], true, "linux");
        assert!(proof.get_args().any(|s| s == "--no-run"));

        Ok(())
    }
);
source_test!(
    test_native_progress_projects_artifacts_without_exposing_cargo_payloads_build_0_5774a0,
    "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[build-0]",
    {
        let exit_code: i32 = 0;
        let t = Temp::new()?;
        let private = "synthetic-build-environment-value";
        let records = vec![
            json!({"reason":"compiler-artifact","target":{"name":"numera_core"},"fresh":false,"filenames":[private],"manifest_path":private}),
            json!({"reason":"build-script-executed","env":[["BUILD_VALUE",private]]}),
            json!({"reason":"compiler-message","message":{"rendered":private}}),
            json!({"reason":"future-event","value":private}),
            json!({"reason":"compiler-artifact","target":{"name":format!("{private}\n::error::")},"fresh":true}),
            json!({"reason":"compiler-artifact","target":null,"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"wrong_flag"},"fresh":1}),
            json!({"reason":"compiler-artifact","target":{"name":"x".repeat(129)},"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"cached-dependency"},"fresh":true}),
            json!({"reason":"build-finished","success":"true"}),
            json!({"reason":"build-finished","success":exit_code == 0}),
        ];
        let mut output = "{malformed-build-record\n".to_owned();
        for record in records {
            output.push_str(&format!("{record}\n"));
        }
        output.push_str(concat!(
            "{\"ordinary\":\"test-payload\"}\nrunning 1 test\ntest contract ... ",
            "ok",
            "\n"
        ));
        let path = t.path().join("native-output");
        fs::write(&path, output)?;
        let h = helper()?;
        let captured = execute(
            command(&h, "replay-output").arg(path).arg(0.to_string()),
            None,
        )?;
        assert_eq!(captured.exit, 0);
        let mut progress = evidence::CargoProgress::default();
        let lines = captured
            .stdout
            .lines()
            .filter_map(|line| progress.project(line))
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "Cargo artifact target=numera_core fresh=false".to_owned(),
                "Cargo artifact target=cached-dependency fresh=true".into(),
                format!("Cargo build finished success={}", exit_code == 0),
                "{\"ordinary\":\"test-payload\"}".into(),
                "running 1 test".into(),
                concat!("test contract ... ", "ok").into()
            ]
        );
        assert!(!lines.join("\n").contains(private));
        let native =
            scope::native_command(&source_root(), "build", &scope::targets(&source_root())?)?;
        assert!(
            native
                .get_args()
                .any(|s| s == "--message-format=json,json-render-diagnostics")
        );

        Ok(())
    }
);
source_test!(
    test_native_progress_projects_artifacts_without_exposing_cargo_payloads_build_101_0bd2e9,
    "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[build-101]",
    {
        let exit_code: i32 = 101;
        let t = Temp::new()?;
        let private = "synthetic-build-environment-value";
        let records = vec![
            json!({"reason":"compiler-artifact","target":{"name":"numera_core"},"fresh":false,"filenames":[private],"manifest_path":private}),
            json!({"reason":"build-script-executed","env":[["BUILD_VALUE",private]]}),
            json!({"reason":"compiler-message","message":{"rendered":private}}),
            json!({"reason":"future-event","value":private}),
            json!({"reason":"compiler-artifact","target":{"name":format!("{private}\n::error::")},"fresh":true}),
            json!({"reason":"compiler-artifact","target":null,"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"wrong_flag"},"fresh":1}),
            json!({"reason":"compiler-artifact","target":{"name":"x".repeat(129)},"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"cached-dependency"},"fresh":true}),
            json!({"reason":"build-finished","success":"true"}),
            json!({"reason":"build-finished","success":exit_code == 0}),
        ];
        let mut output = "{malformed-build-record\n".to_owned();
        for record in records {
            output.push_str(&format!("{record}\n"));
        }
        output.push_str(concat!(
            "{\"ordinary\":\"test-payload\"}\nrunning 1 test\ntest contract ... ",
            "FAILED",
            "\n"
        ));
        let path = t.path().join("native-output");
        fs::write(&path, output)?;
        let h = helper()?;
        let captured = execute(
            command(&h, "replay-output").arg(path).arg(101.to_string()),
            None,
        )?;
        assert_eq!(captured.exit, 101);
        let mut progress = evidence::CargoProgress::default();
        let lines = captured
            .stdout
            .lines()
            .filter_map(|line| progress.project(line))
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "Cargo artifact target=numera_core fresh=false".to_owned(),
                "Cargo artifact target=cached-dependency fresh=true".into(),
                format!("Cargo build finished success={}", exit_code == 0),
                "{\"ordinary\":\"test-payload\"}".into(),
                "running 1 test".into(),
                concat!("test contract ... ", "FAILED").into()
            ]
        );
        assert!(!lines.join("\n").contains(private));
        let native =
            scope::native_command(&source_root(), "build", &scope::targets(&source_root())?)?;
        assert!(
            native
                .get_args()
                .any(|s| s == "--message-format=json,json-render-diagnostics")
        );

        Ok(())
    }
);
source_test!(
    test_native_progress_projects_artifacts_without_exposing_cargo_payloads_test_0_75e841,
    "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[test-0]",
    {
        let exit_code: i32 = 0;
        let t = Temp::new()?;
        let private = "synthetic-build-environment-value";
        let records = vec![
            json!({"reason":"compiler-artifact","target":{"name":"numera_core"},"fresh":false,"filenames":[private],"manifest_path":private}),
            json!({"reason":"build-script-executed","env":[["BUILD_VALUE",private]]}),
            json!({"reason":"compiler-message","message":{"rendered":private}}),
            json!({"reason":"future-event","value":private}),
            json!({"reason":"compiler-artifact","target":{"name":format!("{private}\n::error::")},"fresh":true}),
            json!({"reason":"compiler-artifact","target":null,"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"wrong_flag"},"fresh":1}),
            json!({"reason":"compiler-artifact","target":{"name":"x".repeat(129)},"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"cached-dependency"},"fresh":true}),
            json!({"reason":"build-finished","success":"true"}),
            json!({"reason":"build-finished","success":exit_code == 0}),
        ];
        let mut output = "{malformed-build-record\n".to_owned();
        for record in records {
            output.push_str(&format!("{record}\n"));
        }
        output.push_str(concat!(
            "{\"ordinary\":\"test-payload\"}\nrunning 1 test\ntest contract ... ",
            "ok",
            "\n"
        ));
        let path = t.path().join("native-output");
        fs::write(&path, output)?;
        let h = helper()?;
        let captured = execute(
            command(&h, "replay-output").arg(path).arg(0.to_string()),
            None,
        )?;
        assert_eq!(captured.exit, 0);
        let mut progress = evidence::CargoProgress::default();
        let lines = captured
            .stdout
            .lines()
            .filter_map(|line| progress.project(line))
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "Cargo artifact target=numera_core fresh=false".to_owned(),
                "Cargo artifact target=cached-dependency fresh=true".into(),
                format!("Cargo build finished success={}", exit_code == 0),
                "{\"ordinary\":\"test-payload\"}".into(),
                "running 1 test".into(),
                concat!("test contract ... ", "ok").into()
            ]
        );
        assert!(!lines.join("\n").contains(private));
        let native =
            scope::native_command(&source_root(), "test", &scope::targets(&source_root())?)?;
        assert!(
            native
                .get_args()
                .any(|s| s == "--message-format=json,json-render-diagnostics")
        );

        Ok(())
    }
);
source_test!(
    test_native_progress_projects_artifacts_without_exposing_cargo_payloads_test_101_5a0a1a,
    "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[test-101]",
    {
        let exit_code: i32 = 101;
        let t = Temp::new()?;
        let private = "synthetic-build-environment-value";
        let records = vec![
            json!({"reason":"compiler-artifact","target":{"name":"numera_core"},"fresh":false,"filenames":[private],"manifest_path":private}),
            json!({"reason":"build-script-executed","env":[["BUILD_VALUE",private]]}),
            json!({"reason":"compiler-message","message":{"rendered":private}}),
            json!({"reason":"future-event","value":private}),
            json!({"reason":"compiler-artifact","target":{"name":format!("{private}\n::error::")},"fresh":true}),
            json!({"reason":"compiler-artifact","target":null,"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"wrong_flag"},"fresh":1}),
            json!({"reason":"compiler-artifact","target":{"name":"x".repeat(129)},"fresh":false}),
            json!({"reason":"compiler-artifact","target":{"name":"cached-dependency"},"fresh":true}),
            json!({"reason":"build-finished","success":"true"}),
            json!({"reason":"build-finished","success":exit_code == 0}),
        ];
        let mut output = "{malformed-build-record\n".to_owned();
        for record in records {
            output.push_str(&format!("{record}\n"));
        }
        output.push_str(concat!(
            "{\"ordinary\":\"test-payload\"}\nrunning 1 test\ntest contract ... ",
            "FAILED",
            "\n"
        ));
        let path = t.path().join("native-output");
        fs::write(&path, output)?;
        let h = helper()?;
        let captured = execute(
            command(&h, "replay-output").arg(path).arg(101.to_string()),
            None,
        )?;
        assert_eq!(captured.exit, 101);
        let mut progress = evidence::CargoProgress::default();
        let lines = captured
            .stdout
            .lines()
            .filter_map(|line| progress.project(line))
            .collect::<Vec<_>>();
        assert_eq!(
            lines,
            vec![
                "Cargo artifact target=numera_core fresh=false".to_owned(),
                "Cargo artifact target=cached-dependency fresh=true".into(),
                format!("Cargo build finished success={}", exit_code == 0),
                "{\"ordinary\":\"test-payload\"}".into(),
                "running 1 test".into(),
                concat!("test contract ... ", "FAILED").into()
            ]
        );
        assert!(!lines.join("\n").contains(private));
        let native =
            scope::native_command(&source_root(), "test", &scope::targets(&source_root())?)?;
        assert!(
            native
                .get_args()
                .any(|s| s == "--message-format=json,json-render-diagnostics")
        );

        Ok(())
    }
);
source_test!(
    test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_none_10624b,
    "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[None]",
    {
        let damage = "None";
        let expected = expected_modules(&["test_one", "test_two"]);
        let rows = source_rows(&["test_one", "test_two"]);
        let result = verify_modules(&expected, &rows, "linux");
        assert_eq!(result.is_ok(), damage == "None");
        if damage == "None" {
            assert_eq!(result?, json!({"passed":2,"explicit_limits":0,"suites":2}));
        }

        Ok(())
    }
);
source_test!(
    test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_missing_f83784,
    "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[missing]",
    {
        let damage = "missing";
        let expected = expected_modules(&["test_one", "test_two"]);
        let mut rows = source_rows(&["test_one", "test_two"]);
        rows.as_array_mut().ok_or("rows missing")?.pop();
        let result = verify_modules(&expected, &rows, "linux");
        assert_eq!(result.is_ok(), damage == "None");
        if damage == "None" {
            assert_eq!(result?, json!({"passed":2,"explicit_limits":0,"suites":2}));
        }

        Ok(())
    }
);
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_prerequisite_skip_536fe9,"tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[prerequisite-skip]", {let damage="prerequisite-skip";

let expected=expected_modules(&["test_one","test_two"]);let mut rows=source_rows(&["test_one","test_two"]);rows[0]["cases"][0]["status"]=json!("skipped");rows[0]["cases"][0]["reason"]=json!("could not import prerequisite");
let result=verify_modules(&expected,&rows,"linux");assert_eq!(result.is_ok(),damage=="None");if damage=="None"{assert_eq!(result?,json!({"passed":2,"explicit_limits":0,"suites":2}));}

Ok(())
});
source_test!(
    test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_failed_19a983,
    "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[failed]",
    {
        let damage = "failed";
        let expected = expected_modules(&["test_one", "test_two"]);
        let mut rows = source_rows(&["test_one", "test_two"]);
        rows[0]["cases"][0]["status"] = json!("failed");
        let result = verify_modules(&expected, &rows, "linux");
        assert_eq!(result.is_ok(), damage == "None");
        if damage == "None" {
            assert_eq!(result?, json!({"passed":2,"explicit_limits":0,"suites":2}));
        }

        Ok(())
    }
);
source_test!(
    test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_unexpected_b3d807,
    "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[unexpected]",
    {
        let damage = "unexpected";
        let expected = expected_modules(&["test_one", "test_two"]);
        let mut rows = source_rows(&["test_one", "test_two"]);
        rows[0]["module"] = json!("unclaimed");
        let result = verify_modules(&expected, &rows, "linux");
        assert_eq!(result.is_ok(), damage == "None");
        if damage == "None" {
            assert_eq!(result?, json!({"passed":2,"explicit_limits":0,"suites":2}));
        }

        Ok(())
    }
);
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_wholly_capability_skipped_221689,"tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[wholly-capability-skipped]", {let damage="wholly-capability-skipped";

let expected=expected_modules(&["test_one","test_two"]);let mut rows=source_rows(&["test_one","test_two"]);rows[0]["cases"][0]["status"]=json!("cfg-excluded");rows[0]["cases"][0]["reason"]=json!("fixture capability unavailable");
let result=verify_modules(&expected,&rows,"linux");assert_eq!(result.is_ok(),damage=="None");if damage=="None"{assert_eq!(result?,json!({"passed":2,"explicit_limits":0,"suites":2}));}

Ok(())
});
source_test!(
    test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_xfail_cd3911,
    "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[xfail]",
    {
        let damage = "xfail";
        let expected = expected_modules(&["test_one", "test_two"]);
        let mut rows = source_rows(&["test_one", "test_two"]);
        rows[0]["cases"][0]["status"] = json!("xfail");
        let result = verify_modules(&expected, &rows, "linux");
        assert_eq!(result.is_ok(), damage == "None");
        if damage == "None" {
            assert_eq!(result?, json!({"passed":2,"explicit_limits":0,"suites":2}));
        }

        Ok(())
    }
);
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_win32_test_posix_observation_errors_posix_process_group_observation_true_475014,"tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[win32-test_posix_observation_errors-POSIX process-group observation-True]", {

let expected=expected_modules(&["test_posix_observation_errors","test_control"]);let mut rows=source_rows(&["test_posix_observation_errors","test_control"]);rows[0]["cases"][0]["status"]=json!("cfg-excluded");rows[0]["cases"][0]["reason"]=json!("POSIX process-group observation");let result=verify_modules(&expected,&rows,"win32");assert!(result.is_ok());if true{assert_eq!(result?["explicit_limits"],1);}

Ok(())
});
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_win32_test_shell_death_cleanup_windows_owns_the_engine_through_a_job_object_true_4de299,"tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[win32-test_shell_death_cleanup-Windows owns the engine through a job object-True]", {

let expected=expected_modules(&["test_shell_death_cleanup","test_control"]);let mut rows=source_rows(&["test_shell_death_cleanup","test_control"]);rows[0]["cases"][0]["status"]=json!("cfg-excluded");rows[0]["cases"][0]["reason"]=json!("Windows owns the engine through a job object");let result=verify_modules(&expected,&rows,"win32");assert!(result.is_ok());if true{assert_eq!(result?["explicit_limits"],1);}

Ok(())
});
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_linux_test_posix_observation_errors_posix_process_group_observation_false_29346e,"tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[linux-test_posix_observation_errors-POSIX process-group observation-False]", {

let expected=expected_modules(&["test_posix_observation_errors","test_control"]);let mut rows=source_rows(&["test_posix_observation_errors","test_control"]);rows[0]["cases"][0]["status"]=json!("cfg-excluded");rows[0]["cases"][0]["reason"]=json!("POSIX process-group observation");let result=verify_modules(&expected,&rows,"linux");assert!(result.is_err());if false{assert_eq!(result?["explicit_limits"],1);}

Ok(())
});
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_darwin_test_posix_observation_errors_requires_linux_procfs_false_23cbd1,"tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[darwin-test_posix_observation_errors-requires Linux procfs-False]", {

let expected=expected_modules(&["test_posix_observation_errors","test_control"]);let mut rows=source_rows(&["test_posix_observation_errors","test_control"]);rows[0]["cases"][0]["status"]=json!("cfg-excluded");rows[0]["cases"][0]["reason"]=json!("requires Linux procfs");let result=verify_modules(&expected,&rows,"darwin");assert!(result.is_err());if false{assert_eq!(result?["explicit_limits"],1);}

Ok(())
});
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_win32_test_parent_ownership_liveness_is_only_answerable_on_posix_false_936d87,"tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[win32-test_parent_ownership-liveness is only answerable on POSIX-False]", {

let expected=expected_modules(&["test_parent_ownership","test_control"]);let mut rows=source_rows(&["test_parent_ownership","test_control"]);rows[0]["cases"][0]["status"]=json!("cfg-excluded");rows[0]["cases"][0]["reason"]=json!("liveness is only answerable on POSIX");let result=verify_modules(&expected,&rows,"win32");assert!(result.is_err());if false{assert_eq!(result?["explicit_limits"],1);}

Ok(())
});
source_test!(
    test_native_guard_rejects_ignored_library_cases_ignore_92a7b8,
    "tests/test_live_engine_contract.py::test_native_guard_rejects_ignored_library_cases[#[ignore]]",
    {
        let t = Temp::new()?;
        let library = t.path().join("library_tests.rs");
        fs::write(&library, "#[test]\nfn required_case() {}\n")?;
        reject_ignored_library(std::slice::from_ref(&library))?;
        fs::write(
            &library,
            format!("#[test]\n{}\nfn required_case() {{}}\n", "#[ignore]"),
        )?;
        assert!(
            reject_ignored_library(&[library])
                .err()
                .ok_or("ignored case accepted")?
                .to_string()
                .contains("unmapped ignored")
        );

        Ok(())
    }
);
source_test!(
    test_native_guard_rejects_ignored_library_cases_ignore_unmapped_424208,
    "tests/test_live_engine_contract.py::test_native_guard_rejects_ignored_library_cases[#[ignore = \"unmapped\"]]",
    {
        let t = Temp::new()?;
        let library = t.path().join("library_tests.rs");
        fs::write(&library, "#[test]\nfn required_case() {}\n")?;
        reject_ignored_library(std::slice::from_ref(&library))?;
        fs::write(
            &library,
            format!(
                "#[test]\n{}\nfn required_case() {{}}\n",
                "#[ignore = \"unmapped\"]"
            ),
        )?;
        assert!(
            reject_ignored_library(&[library])
                .err()
                .ok_or("ignored case accepted")?
                .to_string()
                .contains("unmapped ignored")
        );

        Ok(())
    }
);
source_test!(
    test_python_audit_excludes_only_local_distribution_and_keeps_every_dependency,
    "tests/test_live_engine_contract.py::test_python_audit_excludes_only_local_distribution_and_keeps_every_dependency",
    {
        let registry = "registry+https://github.com/rust-lang/crates.io-index";
        let local = json!({"name":"numera-core","version":"0.36.0","source":null});
        let packages = vec![
            local.clone(),
            json!({"name":"axum","version":"0.8.9","source":registry}),
            json!({"name":"cargo-audit","version":"0.22.0","source":registry}),
            json!({"name":"ring","version":"0.17.14","source":registry}),
        ];
        let retained = locked_dependencies(&json!({"packages":packages}), &["numera-core"])?;
        assert_eq!(retained, packages[1..]);
        let mut invalid = Vec::new();
        invalid.push(packages[1..].to_vec());
        let mut p = packages.clone();
        p.push(local);
        invalid.push(p);
        let mut p = packages.clone();
        p.push(json!({"name":"private-lib","version":"1.0.0","source":null}));
        invalid.push(p);
        let mut p = packages.clone();
        p.push(json!({"name":"path-lib","version":"1.0.0","source":"path+/other"}));
        invalid.push(p);
        let mut p = packages.clone();
        p.push(json!({"name":"unresolved","version":">=1","source":registry}));
        invalid.push(p);
        for p in invalid {
            assert!(locked_dependencies(&json!({"packages":p}), &["numera-core"]).is_err());
        }

        Ok(())
    }
);
source_test!(
    test_utf8_gate_rejects_invalid_text_without_decoding_binary,
    "tests/test_live_engine_contract.py::test_utf8_gate_rejects_invalid_text_without_decoding_binary",
    {
        let t = Temp::new()?;
        let text = t.path().join("source.rs");
        let binary = t.path().join("image.png");
        fs::write(&text, "let value = \"Ω\";\n")?;
        fs::write(&binary, b"\x89PNG\xff\x00")?;
        assert_eq!(scope::check_utf8(&[text.clone(), binary.clone()])?, 1);
        fs::write(&text, b"let value = \"\xff\";\n")?;
        assert!(scope::check_utf8(&[text, binary]).is_err());

        Ok(())
    }
);
source_test!(
    test_dashboard_guard_requires_every_discovered_file_and_actual_pass_none_bf61fe,
    "tests/test_live_engine_contract.py::test_dashboard_guard_requires_every_discovered_file_and_actual_pass[None]",
    {
        let damage = "None";
        let expected = vec![
            "one::renders_native_component".to_owned(),
            "two::renders_native_component".into(),
        ];
        let mut report = evidence::Current::wasm(expected.clone())?;
        report.accept("running 2 tests");
        for (index, name) in expected.iter().enumerate() {
            if false && index == 1 {
                continue;
            }
            report.accept(&format!(
                "test {name} ... {}",
                if false && index == 0 { "ignored" } else { "ok" }
            ));
        }
        report.accept(
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s",
        );
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_dashboard_guard_requires_every_discovered_file_and_actual_pass_missing_787b81,
    "tests/test_live_engine_contract.py::test_dashboard_guard_requires_every_discovered_file_and_actual_pass[missing]",
    {
        let damage = "missing";
        let expected = vec![
            "one::renders_native_component".to_owned(),
            "two::renders_native_component".into(),
        ];
        let mut report = evidence::Current::wasm(expected.clone())?;
        report.accept("running 2 tests");
        for (index, name) in expected.iter().enumerate() {
            if true && index == 1 {
                continue;
            }
            report.accept(&format!(
                "test {name} ... {}",
                if false && index == 0 { "ignored" } else { "ok" }
            ));
        }
        report.accept(
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s",
        );
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_dashboard_guard_requires_every_discovered_file_and_actual_pass_skipped_37707b,
    "tests/test_live_engine_contract.py::test_dashboard_guard_requires_every_discovered_file_and_actual_pass[skipped]",
    {
        let damage = "skipped";
        let expected = vec![
            "one::renders_native_component".to_owned(),
            "two::renders_native_component".into(),
        ];
        let mut report = evidence::Current::wasm(expected.clone())?;
        report.accept("running 2 tests");
        for (index, name) in expected.iter().enumerate() {
            if false && index == 1 {
                continue;
            }
            report.accept(&format!(
                "test {name} ... {}",
                if true && index == 0 { "ignored" } else { "ok" }
            ));
        }
        report.accept(
            "test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s",
        );
        assert_eq!(report.verify(0).is_ok(), damage == "None");

        Ok(())
    }
);
source_test!(
    test_secret_exceptions_are_only_exact_reviewed_rule_path_and_literal,
    "tests/test_live_engine_contract.py::test_secret_exceptions_are_only_exact_reviewed_rule_path_and_literal",
    {
        let root = source_root();
        let config = fs::read_to_string(root.join("scripts/ci/gitleaks.toml"))?;
        assert!(config.contains("useDefault = true"));
        assert!(!config.contains("disabledRules"));
        assert_eq!(config.matches("[[rules.allowlists]]").count(), 4);
        assert_eq!(config.matches("condition = \"AND\"").count(), 4);
        assert_eq!(config.matches("regexTarget = \"secret\"").count(), 4);
        let literal = regex::Regex::new(r"\x27{3}([^\x27]+)\x27{3}")?;
        let mut patterns = Vec::new();
        let mut paths = Vec::new();
        for block in config.split("[[rules.allowlists]]").skip(1) {
            for line in block.lines() {
                if line.starts_with("regexes =") {
                    patterns.extend(literal.captures_iter(line).map(|c| c[1].to_owned()));
                }
                if line.starts_with("paths =") {
                    paths.push(
                        literal.captures(line).ok_or("allowlist path missing")?[1].to_owned(),
                    );
                }
            }
        }
        let expected = [
            "f577f82af741d32085294020c093c7decd65544fe2f43c88bc76768245a5b409",
            "ac215b3e0151a72e3e3e5b898b61827612735ed8fdf7865d7a4c90959a242e03",
            "4c0b89ca29dd4fd13eeefc41a69fee0b64d8244e5fc8e2654ac31ad7c96fc426",
            "7e65f47df01dbc8677d2ced7c8ad02a06afbb21e07c1e872713d2bfaf8773128",
            "bf23e95aa0ff67388daa3728f36a013b9ddafdc2bd280decb03bacbaad9eb8a1",
        ];
        assert_eq!(patterns.len(), expected.len());
        for (pattern, digest) in patterns.iter().zip(expected) {
            assert!(pattern.starts_with('^') && pattern.ends_with('$'));
            assert_eq!(ci::sha(&pattern.as_bytes()[1..pattern.len() - 1]), digest);
            let regex = regex::Regex::new(pattern)?;
            assert!(regex.is_match(&pattern[1..pattern.len() - 1]));
            assert!(!regex.is_match(&format!("{}changed", &pattern[1..pattern.len() - 1])));
        }
        let positive = [
            (
                "engine-rust/tests/fixtures/request-auth.json",
                "engine-rust/tests/fixtures/unreviewed.json",
            ),
            (
                "engine-rust/src/http/documentation-assets/provenance.json",
                "engine-rust/src/http/documentation-assets/unreviewed.json",
            ),
            (
                "engine/tests/test_portable_root.py",
                "engine/tests/test_new_credential.py",
            ),
            (
                "desktop/src-tauri/build.rs",
                "desktop/src-tauri/src/unreviewed.rs",
            ),
        ];
        for (path, (accepted, refused)) in paths.iter().zip(positive) {
            let regex = regex::Regex::new(path)?;
            assert!(regex.is_match(accepted));
            assert!(!regex.is_match(refused));
        }
        let assets = root.join("engine-rust/src/http/documentation-assets");
        let provenance = ci::decode(&fs::read(assets.join("provenance.json"))?)?;
        for name in ["openapi.json", "oauth2-redirect.html"] {
            assert_eq!(
                ci::digest(&assets.join(name))?,
                provenance["sha256"][name]
                    .as_str()
                    .ok_or("asset hash missing")?
            );
        }
        use base64::Engine as _;
        let pem = fs::read_to_string(root.join("docs/release-signing-key.pem"))?;
        let body = pem
            .lines()
            .filter(|s| !s.starts_with("---"))
            .collect::<String>();
        let der = base64::engine::general_purpose::STANDARD.decode(body)?;
        assert_eq!(
            &der[..12],
            &[
                0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00
            ]
        );
        let public = der[12..]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            patterns.last().ok_or("public allowlist missing")?,
            &format!("^{public}$")
        );
        assert!(
            fs::read_to_string(root.join("desktop/src-tauri/src/updates/manifest.rs"))?
                .contains(&format!("pub const PUBLIC_KEY: &str = \"{public}\";"))
        );

        Ok(())
    }
);
source_test!(
    test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions_lf_04adc8,
    "tests/test_live_engine_contract.py::test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions[lf]",
    {
        let t = Temp::new()?;
        let base = t.path().join("official.toml");
        let output = t.path().join("strict.toml");
        let official = "[allowlist]\ndescription = \"official values\"\npaths = [\n    \"package-lock.json\",\n    \"asset[0-9].json\",\n]\nregexes = [\"^false$\"]\n[[rules]]\nid = \"required-rule\"\nregex = \"required-detection\"\npath = \"required-selector\"\n[[rules.allowlists]]\npaths = [\"hidden.txt\"]\nregexes = [\"^official-constant$\"]\n[[rules.allowlists]]\npaths = [\"path-only.txt\"]\n[[rules]]\nid = \"another-required-rule\"\nregex = \"another-detection\"\n";
        let payload = official.as_bytes().to_vec();
        fs::write(&base, &payload)?;
        assert_eq!(fs::read(&base)?, payload);
        assert!(
            scope::prepare_secret_config_checked(&source_root(), &base, &output, &"0".repeat(64))
                .err()
                .ok_or("wrong checksum accepted")?
                .to_string()
                .contains("checksum")
        );
        assert_eq!(fs::read(&base)?, payload);
        assert!(!output.exists());
        scope::prepare_secret_config_checked(&source_root(), &base, &output, &ci::sha(&payload))?;
        let changed = fs::read_to_string(base)?;
        assert_eq!(changed.matches("paths = []").count(), 1);
        for removed in [
            "package-lock.json",
            "hidden.txt",
            "path-only.txt",
            "[[rules.allowlists]]",
            "official-constant",
        ] {
            assert!(!changed.contains(removed));
        }
        for required in [
            "another-detection",
            "required-detection",
            "path = \"required-selector\"",
            "regexes = [\"^false$\"]",
        ] {
            assert!(changed.contains(required));
        }
        let config = fs::read_to_string(output)?;
        assert!(!config.contains("useDefault"));
        assert!(config.contains("path = "));
        assert_eq!(config.matches("condition = \"AND\"").count(), 4);

        Ok(())
    }
);
source_test!(
    test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions_crlf_f678c7,
    "tests/test_live_engine_contract.py::test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions[crlf]",
    {
        let t = Temp::new()?;
        let base = t.path().join("official.toml");
        let output = t.path().join("strict.toml");
        let official = "[allowlist]\ndescription = \"official values\"\npaths = [\n    \"package-lock.json\",\n    \"asset[0-9].json\",\n]\nregexes = [\"^false$\"]\n[[rules]]\nid = \"required-rule\"\nregex = \"required-detection\"\npath = \"required-selector\"\n[[rules.allowlists]]\npaths = [\"hidden.txt\"]\nregexes = [\"^official-constant$\"]\n[[rules.allowlists]]\npaths = [\"path-only.txt\"]\n[[rules]]\nid = \"another-required-rule\"\nregex = \"another-detection\"\n";
        let payload = official.replace('\n', "\r\n").into_bytes();
        fs::write(&base, &payload)?;
        assert_eq!(fs::read(&base)?, payload);
        assert!(
            scope::prepare_secret_config_checked(&source_root(), &base, &output, &"0".repeat(64))
                .err()
                .ok_or("wrong checksum accepted")?
                .to_string()
                .contains("checksum")
        );
        assert_eq!(fs::read(&base)?, payload);
        assert!(!output.exists());
        scope::prepare_secret_config_checked(&source_root(), &base, &output, &ci::sha(&payload))?;
        let changed = fs::read_to_string(base)?;
        assert_eq!(changed.matches("paths = []").count(), 1);
        for removed in [
            "package-lock.json",
            "hidden.txt",
            "path-only.txt",
            "[[rules.allowlists]]",
            "official-constant",
        ] {
            assert!(!changed.contains(removed));
        }
        for required in [
            "another-detection",
            "required-detection",
            "path = \"required-selector\"",
            "regexes = [\"^false$\"]",
        ] {
            assert!(changed.contains(required));
        }
        let config = fs::read_to_string(output)?;
        assert!(!config.contains("useDefault"));
        assert!(config.contains("path = "));
        assert_eq!(config.matches("condition = \"AND\"").count(), 4);

        Ok(())
    }
);
