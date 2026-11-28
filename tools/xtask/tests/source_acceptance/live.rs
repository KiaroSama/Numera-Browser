use super::*;
use numera_xtask::ci::{evidence, release, scope, workflow};
#[path = "reviewed_secrets.rs"]
mod reviewed_secrets;
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
        let ci = workflow::parse(&fs::read_to_string(
            root.join(".github/workflows/native-ci.yml"),
        )?)?;
        let jobs = &ci["jobs"];
        workflow::bounded_jobs(&ci)?;
        let matrix = &jobs["live"]["strategy"]["matrix"];
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
        assert_eq!(jobs["live"]["strategy"]["max-parallel"], 2);
        let expected = jobs
            .as_object()
            .ok_or("jobs missing")?
            .keys()
            .filter(|name| !["required", "promote", "publish"].contains(&name.as_str()))
            .cloned()
            .collect();
        workflow::required_jobs(jobs, &expected)?;
        workflow::native_automatic(&ci)?;
        let installer = jobs["build"]["steps"]
            .as_array()
            .ok_or("Native product build steps missing")?
            .iter()
            .find(|step| step["name"] == "Preserve unsigned native installers")
            .ok_or("Native product installer upload missing")?;
        let paths = installer["with"]["path"]
            .as_str()
            .ok_or("Native product installer paths missing")?;
        for path in [
            "build/native-product/*.msi",
            "build/native-product/*.deb",
            "build/native-product/*.AppImage",
            "build/native-product/*.app.zip",
            "build/native-product/*.dmg",
        ] {
            assert!(paths.lines().any(|line| line == path));
        }
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
        let root = source_root();
        let documents = json!({
            "ci":workflow::parse(&fs::read_to_string(root.join(".github/workflows/ci.yml"))?)?,
            "release":workflow::parse(&fs::read_to_string(root.join(".github/workflows/release-review.yml"))?)?
        });
        // The native release boundary independently verifies signed products
        // after download; it has read-only authority and never publishes them.
        release::release_workflows(&documents)?;
        let steps = documents["release"]["jobs"]["review"]["steps"]
            .as_array()
            .ok_or("Signed product review steps missing")?;
        let download = steps
            .iter()
            .position(|step| {
                step["uses"]
                    .as_str()
                    .is_some_and(|name| name.starts_with("actions/download-artifact@"))
            })
            .ok_or("Signed product artifact download missing")?;
        let verification = steps.iter().position(|step| {
            step["run"] == "cargo run --manifest-path tools/xtask/Cargo.toml --locked -- ci signed-product-review build/owner-review \"${{ inputs.source_sha }}\""
        }).ok_or("Exact-source signed product verification missing")?;
        assert!(download < verification);
        let mut skipped = documents.clone();
        skipped["release"]["jobs"]["review"]["steps"]
            .as_array_mut()
            .ok_or("Review steps missing")?
            .remove(verification);
        assert!(release::release_workflows(&skipped).is_err());
        let mut softened = documents.clone();
        softened["release"]["jobs"]["review"]["steps"][verification]["continue-on-error"] =
            true.into();
        assert!(release::release_workflows(&softened).is_err());

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
            .map(|name| json!({"run":format!("cargo run --manifest-path tools/xtask/Cargo.toml --locked -- ci acceptance {name}")}))
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
            .map(|name| json!({"run":format!("cargo run --manifest-path tools/xtask/Cargo.toml --locked -- ci acceptance {name}")}))
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
            .map(|name| json!({"run":format!("cargo run --manifest-path tools/xtask/Cargo.toml --locked -- ci acceptance {name}")}))
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
#[rustfmt::skip]
source_test!(test_independent_native_acceptance_precedes_the_required_benchmark_duplicate_f9998d, "tests/test_live_engine_contract.py::test_independent_native_acceptance_precedes_the_required_benchmark[duplicate]", { live_contracts_1_0() });
#[rustfmt::skip]
source_test!(test_native_development_optimizes_real_crypto_without_weakening_checks, "tests/test_live_engine_contract.py::test_native_development_optimizes_real_crypto_without_weakening_checks", { live_contracts_1_1() });
#[rustfmt::skip]
source_test!(test_discovered_native_targets_are_all_selected_and_build_is_separate, "tests/test_live_engine_contract.py::test_discovered_native_targets_are_all_selected_and_build_is_separate", { live_contracts_1_2() });
#[rustfmt::skip]
source_test!(test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test_none_b75a6d, "tests/test_live_engine_contract.py::test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test[None]", { live_contracts_1_3() });
#[rustfmt::skip]
source_test!(test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test_fetch_7fb2cb, "tests/test_live_engine_contract.py::test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test[fetch]", { live_contracts_1_4() });
#[rustfmt::skip]
source_test!(test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test_proof_a56dbb, "tests/test_live_engine_contract.py::test_native_history_precedes_fixed_build_and_never_runs_in_fixed_test[proof]", { live_contracts_1_5() });
#[rustfmt::skip]
source_test!(test_native_progress_projects_artifacts_without_exposing_cargo_payloads_build_0_5774a0, "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[build-0]", { live_contracts_1_6() });
#[rustfmt::skip]
source_test!(test_native_progress_projects_artifacts_without_exposing_cargo_payloads_build_101_0bd2e9, "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[build-101]", { live_contracts_1_7() });
#[rustfmt::skip]
source_test!(test_native_progress_projects_artifacts_without_exposing_cargo_payloads_test_0_75e841, "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[test-0]", { live_contracts_1_8() });
#[rustfmt::skip]
source_test!(test_native_progress_projects_artifacts_without_exposing_cargo_payloads_test_101_5a0a1a, "tests/test_live_engine_contract.py::test_native_progress_projects_artifacts_without_exposing_cargo_payloads[test-101]", { live_contracts_1_9() });
#[rustfmt::skip]
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_none_10624b, "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[None]", { live_contracts_1_10() });
#[rustfmt::skip]
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_missing_f83784, "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[missing]", { live_contracts_1_11() });
#[rustfmt::skip]
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_prerequisite_skip_536fe9, "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[prerequisite-skip]", { live_contracts_1_12() });
#[rustfmt::skip]
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_failed_19a983, "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[failed]", { live_contracts_1_13() });
#[rustfmt::skip]
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_unexpected_b3d807, "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[unexpected]", { live_contracts_1_14() });
#[rustfmt::skip]
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_wholly_capability_skipped_221689, "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[wholly-capability-skipped]", { live_contracts_1_15() });
#[rustfmt::skip]
source_test!(test_suite_report_rejects_omissions_false_green_and_missing_prerequisites_xfail_cd3911, "tests/test_live_engine_contract.py::test_suite_report_rejects_omissions_false_green_and_missing_prerequisites[xfail]", { live_contracts_1_16() });
#[rustfmt::skip]
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_win32_test_posix_observation_errors_posix_process_group_observation_true_475014, "tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[win32-test_posix_observation_errors-POSIX process-group observation-True]", { live_contracts_1_17() });
#[rustfmt::skip]
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_win32_test_shell_death_cleanup_windows_owns_the_engine_through_a_job_object_true_4de299, "tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[win32-test_shell_death_cleanup-Windows owns the engine through a job object-True]", { live_contracts_1_18() });
#[rustfmt::skip]
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_linux_test_posix_observation_errors_posix_process_group_observation_false_29346e, "tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[linux-test_posix_observation_errors-POSIX process-group observation-False]", { live_contracts_1_19() });
#[rustfmt::skip]
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_darwin_test_posix_observation_errors_requires_linux_procfs_false_23cbd1, "tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[darwin-test_posix_observation_errors-requires Linux procfs-False]", { live_contracts_1_20() });
#[rustfmt::skip]
source_test!(test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites_win32_test_parent_ownership_liveness_is_only_answerable_on_posix_false_936d87, "tests/test_live_engine_contract.py::test_whole_module_os_exceptions_do_not_waive_mixed_platform_suites[win32-test_parent_ownership-liveness is only answerable on POSIX-False]", { live_contracts_1_21() });
#[rustfmt::skip]
source_test!(test_native_guard_rejects_ignored_library_cases_ignore_92a7b8, "tests/test_live_engine_contract.py::test_native_guard_rejects_ignored_library_cases[#[ignore]]", { live_contracts_1_22() });
#[rustfmt::skip]
source_test!(test_native_guard_rejects_ignored_library_cases_ignore_unmapped_424208, "tests/test_live_engine_contract.py::test_native_guard_rejects_ignored_library_cases[#[ignore = \"unmapped\"]]", { live_contracts_1_23() });
#[rustfmt::skip]
source_test!(test_python_audit_excludes_only_local_distribution_and_keeps_every_dependency, "tests/test_live_engine_contract.py::test_python_audit_excludes_only_local_distribution_and_keeps_every_dependency", { live_contracts_2_0() });
#[rustfmt::skip]
source_test!(test_utf8_gate_rejects_invalid_text_without_decoding_binary, "tests/test_live_engine_contract.py::test_utf8_gate_rejects_invalid_text_without_decoding_binary", { live_contracts_2_1() });
#[rustfmt::skip]
source_test!(test_dashboard_guard_requires_every_discovered_file_and_actual_pass_none_bf61fe, "tests/test_live_engine_contract.py::test_dashboard_guard_requires_every_discovered_file_and_actual_pass[None]", { live_contracts_2_2() });
#[rustfmt::skip]
source_test!(test_dashboard_guard_requires_every_discovered_file_and_actual_pass_missing_787b81, "tests/test_live_engine_contract.py::test_dashboard_guard_requires_every_discovered_file_and_actual_pass[missing]", { live_contracts_2_3() });
#[rustfmt::skip]
source_test!(test_dashboard_guard_requires_every_discovered_file_and_actual_pass_skipped_37707b, "tests/test_live_engine_contract.py::test_dashboard_guard_requires_every_discovered_file_and_actual_pass[skipped]", { live_contracts_2_4() });
#[rustfmt::skip]
source_test!(test_secret_exceptions_are_only_exact_reviewed_rule_path_and_literal, "tests/test_live_engine_contract.py::test_secret_exceptions_are_only_exact_reviewed_rule_path_and_literal", { live_contracts_2_5() });
#[rustfmt::skip]
source_test!(test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions_lf_04adc8, "tests/test_live_engine_contract.py::test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions[lf]", { live_contracts_2_6() });
#[rustfmt::skip]
source_test!(test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions_crlf_f678c7, "tests/test_live_engine_contract.py::test_strict_secret_config_keeps_rules_but_removes_inherited_path_omissions[crlf]", { live_contracts_2_7() });
include!("live_contracts_1.rs");
include!("live_contracts_2.rs");
