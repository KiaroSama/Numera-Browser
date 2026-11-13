use super::*;
fn child(executable: &Path, root: Option<&Path>, args: &[&str]) -> Command {
    let mut c = numera_core::mcp_command::child_command(
        executable,
        root,
        None,
        args.iter().map(|s| (*s).into()).collect(),
    );
    for key in [
        "NUMERA_API_TOKEN",
        "NUMERA_PASSWORD",
        "NUMERA_UPDATE_TOKEN",
        "NUMERA_UPDATE_TICKET",
        "NUMERA_UPDATE_ACTIVATION",
    ] {
        c.env_remove(key);
    }
    c
}
fn source(path: &str) -> io::Result<String> {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(path))
}

original!(
    test_source_mode_runs_the_module_through_the_interpreter,
    "tests/test_self_command.py::test_source_mode_runs_the_module_through_the_interpreter",
    {
        let t = common::TempDirectory::new("self-source")?;
        let mut command = child(
            Path::new(env!("CARGO_BIN_EXE_numera-engine")),
            Some(t.path()),
            &["list", "--json"],
        );
        let args = command
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            vec!["--data-dir", &t.path().to_string_lossy(), "list", "--json"]
        );
        assert_eq!(
            command.get_program(),
            std::ffi::OsStr::new(env!("CARGO_BIN_EXE_numera-engine"))
        );
        let result = command.output()?;
        assert!(result.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&result.stdout)?,
            json!({"total":0,"profiles":[]})
        );
        t.cleanup()?;

        Ok(())
    }
);
original!(
    test_frozen_mode_invokes_the_executable_directly,
    "tests/test_self_command.py::test_frozen_mode_invokes_the_executable_directly",
    {
        let c = child(Path::new(r"C:\app\numera.exe"), None, &["launch", "abc"]);
        assert_eq!(c.get_program(), std::ffi::OsStr::new(r"C:\app\numera.exe"));
        assert_eq!(c.get_args().collect::<Vec<_>>(), vec!["launch", "abc"]);
        assert!(!c.get_args().any(|s| s == "-m" || s == "numera"));

        Ok(())
    }
);
original!(
    test_a_path_with_spaces_stays_one_argument,
    "tests/test_self_command.py::test_a_path_with_spaces_stays_one_argument",
    {
        let t = common::TempDirectory::new("self-spaces")?;
        let executable = t.path().join(if cfg!(windows) {
            "Numera with spaces.exe"
        } else {
            "Numera with spaces"
        });
        fs::copy(env!("CARGO_BIN_EXE_numera-engine"), &executable)?;
        let root = t.path().join("data with spaces");
        let mut c = child(&executable, Some(&root), &["list", "--json"]);
        assert_eq!(c.get_program(), executable.as_os_str());
        assert_eq!(
            c.get_args().collect::<Vec<_>>(),
            vec![
                std::ffi::OsStr::new("--data-dir"),
                root.as_os_str(),
                std::ffi::OsStr::new("list"),
                std::ffi::OsStr::new("--json")
            ]
        );
        let output = c.output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout)?,
            json!({"total":0,"profiles":[]})
        );

        Ok(())
    }
);
original!(
    test_no_arguments_is_the_bare_program,
    "tests/test_self_command.py::test_no_arguments_is_the_bare_program",
    {
        let c = child(Path::new("/opt/numera/numera"), None, &[]);
        assert_eq!(c.get_program(), std::ffi::OsStr::new("/opt/numera/numera"));
        assert_eq!(c.get_args().count(), 0);

        Ok(())
    }
);
original!(
    test_no_call_site_builds_its_own_self_command_numera_jobs_f99edf,
    "tests/test_self_command.py::test_no_call_site_builds_its_own_self_command[numera.jobs]",
    {
        let source = source("job_supervisor.rs")?;
        assert!(
            !source.contains("\"-m\""),
            "numera.jobs introduced interpreter self invocation"
        );
        let args = numera_core::jobs::child_argv(
            &[
                "launch".into(),
                "abc".into(),
                "--detach".into(),
                "--password".into(),
                "fixture".into(),
            ],
            Path::new("data with spaces"),
        );
        assert_eq!(
            args,
            vec!["--data-dir", "data with spaces", "launch", "abc"]
        );
        Ok(())
    }
);
original!(
    test_no_call_site_builds_its_own_self_command_numera_mcp_6c7157,
    "tests/test_self_command.py::test_no_call_site_builds_its_own_self_command[numera.mcp]",
    {
        let source = source("mcp_command.rs")?;
        assert!(
            !source.contains("\"-m\""),
            "numera.mcp introduced interpreter self invocation"
        );
        let c = child(Path::new("native engine"), None, &["launch", "abc"]);
        assert_eq!(c.get_args().collect::<Vec<_>>(), vec!["launch", "abc"]);
        assert!(source.contains("child_command(executable"));
        Ok(())
    }
);
original!(
    test_no_call_site_builds_its_own_self_command_numera_server_routes_browser_eced67,
    "tests/test_self_command.py::test_no_call_site_builds_its_own_self_command[numera.server.routes_browser]",
    {
        let source = source("http/launch_routes.rs")?;
        assert!(
            !source.contains("\"-m\""),
            "numera.server.routes_browser introduced interpreter self invocation"
        );
        let t = common::TempDirectory::new("self-browser-route")?;
        let h = Http::new(t.path(), None)?;
        let response = h.request("POST", "/api/profiles/missing/launch", json!({}))?;
        assert_eq!(response.0, 404);
        assert!(response.1.to_string().contains("profile"));
        Ok(())
    }
);
original!(
    test_no_call_site_builds_its_own_self_command_numera_server_routes_schedules_ed6980,
    "tests/test_self_command.py::test_no_call_site_builds_its_own_self_command[numera.server.routes_schedules]",
    {
        let source = source("http/schedule_routes.rs")?;
        assert!(
            !source.contains("\"-m\""),
            "numera.server.routes_schedules introduced interpreter self invocation"
        );
        let t = common::TempDirectory::new("self-schedule-route")?;
        let h = Http::new(t.path(), None)?;
        let response = h.request(
            "POST",
            "/api/schedules",
            json!({"profileName":"missing","everyHours":6}),
        )?;
        assert_eq!(response.0, 404);
        assert!(response.1.to_string().contains("profile"));
        Ok(())
    }
);
original!(
    test_the_frozen_entry_would_reject_the_old_command,
    "tests/test_self_command.py::test_the_frozen_entry_would_reject_the_old_command",
    {
        let failure = numera_core::cli::registry()
            .try_get_matches_from(["numera", "-m", "numera", "launch", "abc"]);
        assert!(failure.is_err());
        let output = child(
            Path::new(env!("CARGO_BIN_EXE_numera-engine")),
            None,
            &["-m", "numera", "launch", "abc"],
        )
        .output()?;
        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr)?.contains("-m"));

        Ok(())
    }
);
original!(
    test_a_frozen_build_rejects_the_old_shape_for_real,
    "tests/test_self_command.py::test_a_frozen_build_rejects_the_old_shape_for_real",
    {
        let t = common::TempDirectory::new("self-frozen-shape")?;
        let output = child(
            Path::new(env!("CARGO_BIN_EXE_numera-engine")),
            Some(t.path()),
            &["-m", "numera", "launch", "x"],
        )
        .output()?;
        assert_eq!(output.status.code(), Some(2));
        let stderr = String::from_utf8(output.stderr)?;
        assert!(stderr.contains("-m"));
        assert!(
            !t.path().join("profiles").exists(),
            "usage rejection mutated data before parsing"
        );

        Ok(())
    }
);
