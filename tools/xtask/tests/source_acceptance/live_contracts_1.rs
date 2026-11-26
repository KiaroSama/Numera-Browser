fn live_contracts_1_0() -> TestResult {
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

fn live_contracts_1_1() -> TestResult {
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
            "profile.dev.package.\"sha2:0.11.2\"".into()
        ])
    );
    assert!(profiles.values().all(|value| value == "opt-level = 1"));

    Ok(())
}

fn live_contracts_1_2() -> TestResult {
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

fn live_contracts_1_3() -> TestResult {
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

fn live_contracts_1_4() -> TestResult {
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

fn live_contracts_1_5() -> TestResult {
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

fn live_contracts_1_6() -> TestResult {
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
    let native = scope::native_command(&source_root(), "build", &scope::targets(&source_root())?)?;
    assert!(
        native
            .get_args()
            .any(|s| s == "--message-format=json,json-render-diagnostics")
    );

    Ok(())
}

fn live_contracts_1_7() -> TestResult {
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
    let native = scope::native_command(&source_root(), "build", &scope::targets(&source_root())?)?;
    assert!(
        native
            .get_args()
            .any(|s| s == "--message-format=json,json-render-diagnostics")
    );

    Ok(())
}

fn live_contracts_1_8() -> TestResult {
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
    let native = scope::native_command(&source_root(), "test", &scope::targets(&source_root())?)?;
    assert!(
        native
            .get_args()
            .any(|s| s == "--message-format=json,json-render-diagnostics")
    );

    Ok(())
}

fn live_contracts_1_9() -> TestResult {
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
    let native = scope::native_command(&source_root(), "test", &scope::targets(&source_root())?)?;
    assert!(
        native
            .get_args()
            .any(|s| s == "--message-format=json,json-render-diagnostics")
    );

    Ok(())
}

fn live_contracts_1_10() -> TestResult {
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

fn live_contracts_1_11() -> TestResult {
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

fn live_contracts_1_12() -> TestResult {
    let damage = "prerequisite-skip";

    let expected = expected_modules(&["test_one", "test_two"]);
    let mut rows = source_rows(&["test_one", "test_two"]);
    rows[0]["cases"][0]["status"] = json!("skipped");
    rows[0]["cases"][0]["reason"] = json!("could not import prerequisite");
    let result = verify_modules(&expected, &rows, "linux");
    assert_eq!(result.is_ok(), damage == "None");
    if damage == "None" {
        assert_eq!(result?, json!({"passed":2,"explicit_limits":0,"suites":2}));
    }

    Ok(())
}

fn live_contracts_1_13() -> TestResult {
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

fn live_contracts_1_14() -> TestResult {
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

fn live_contracts_1_15() -> TestResult {
    let damage = "wholly-capability-skipped";

    let expected = expected_modules(&["test_one", "test_two"]);
    let mut rows = source_rows(&["test_one", "test_two"]);
    rows[0]["cases"][0]["status"] = json!("cfg-excluded");
    rows[0]["cases"][0]["reason"] = json!("fixture capability unavailable");
    let result = verify_modules(&expected, &rows, "linux");
    assert_eq!(result.is_ok(), damage == "None");
    if damage == "None" {
        assert_eq!(result?, json!({"passed":2,"explicit_limits":0,"suites":2}));
    }

    Ok(())
}

fn live_contracts_1_16() -> TestResult {
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

fn live_contracts_1_17() -> TestResult {
    let expected = expected_modules(&["test_posix_observation_errors", "test_control"]);
    let mut rows = source_rows(&["test_posix_observation_errors", "test_control"]);
    rows[0]["cases"][0]["status"] = json!("cfg-excluded");
    rows[0]["cases"][0]["reason"] = json!("POSIX process-group observation");
    let result = verify_modules(&expected, &rows, "win32");
    assert!(result.is_ok());
    if true {
        assert_eq!(result?["explicit_limits"], 1);
    }

    Ok(())
}

fn live_contracts_1_18() -> TestResult {
    let expected = expected_modules(&["test_shell_death_cleanup", "test_control"]);
    let mut rows = source_rows(&["test_shell_death_cleanup", "test_control"]);
    rows[0]["cases"][0]["status"] = json!("cfg-excluded");
    rows[0]["cases"][0]["reason"] = json!("Windows owns the engine through a job object");
    let result = verify_modules(&expected, &rows, "win32");
    assert!(result.is_ok());
    if true {
        assert_eq!(result?["explicit_limits"], 1);
    }

    Ok(())
}

fn live_contracts_1_19() -> TestResult {
    let expected = expected_modules(&["test_posix_observation_errors", "test_control"]);
    let mut rows = source_rows(&["test_posix_observation_errors", "test_control"]);
    rows[0]["cases"][0]["status"] = json!("cfg-excluded");
    rows[0]["cases"][0]["reason"] = json!("POSIX process-group observation");
    let result = verify_modules(&expected, &rows, "linux");
    assert!(result.is_err());
    if false {
        assert_eq!(result?["explicit_limits"], 1);
    }

    Ok(())
}

fn live_contracts_1_20() -> TestResult {
    let expected = expected_modules(&["test_posix_observation_errors", "test_control"]);
    let mut rows = source_rows(&["test_posix_observation_errors", "test_control"]);
    rows[0]["cases"][0]["status"] = json!("cfg-excluded");
    rows[0]["cases"][0]["reason"] = json!("requires Linux procfs");
    let result = verify_modules(&expected, &rows, "darwin");
    assert!(result.is_err());
    if false {
        assert_eq!(result?["explicit_limits"], 1);
    }

    Ok(())
}

fn live_contracts_1_21() -> TestResult {
    let expected = expected_modules(&["test_parent_ownership", "test_control"]);
    let mut rows = source_rows(&["test_parent_ownership", "test_control"]);
    rows[0]["cases"][0]["status"] = json!("cfg-excluded");
    rows[0]["cases"][0]["reason"] = json!("liveness is only answerable on POSIX");
    let result = verify_modules(&expected, &rows, "win32");
    assert!(result.is_err());
    if false {
        assert_eq!(result?["explicit_limits"], 1);
    }

    Ok(())
}

fn live_contracts_1_22() -> TestResult {
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

fn live_contracts_1_23() -> TestResult {
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
