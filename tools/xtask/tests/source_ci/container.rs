use super::*;
use numera_core::browser::container_smoke::{self as smoke, Driver, Phase};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = Box::pin(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("controlled SDK boundary unexpectedly blocked"),
    }
}
fn label(phase: Phase) -> &'static str {
    match phase {
        Phase::Start => "driver-start",
        Phase::Launch => "launch",
        Phase::Context => "context",
        Phase::Page => "page",
        Phase::Content => "content",
        Phase::Text => "text",
        Phase::Evaluate => "evaluate",
        Phase::ClosePage => "close-page",
        Phase::CloseContext => "close-context",
        Phase::CloseBrowser => "close-browser",
        Phase::ConnectionCheck => "connection-check",
        Phase::Stop => "driver-stop",
    }
}
struct Controlled {
    version: String,
    root: bool,
    executable: PathBuf,
    events: Vec<&'static str>,
    fault: String,
}
impl Controlled {
    fn new(temp: &Temp) -> ci::Result<Self> {
        let executable = temp.write("browsers/chromium/chrome", b"fixture, not a real browser")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            version: "1.65.1".into(),
            root: false,
            executable,
            events: Vec::new(),
            fault: String::new(),
        })
    }
}
impl Driver for Controlled {
    fn installed_version(&self) -> io::Result<String> {
        Ok(self.version.clone())
    }
    fn non_root(&self) -> io::Result<bool> {
        Ok(!self.root)
    }
    fn executable(&self, root: &Path, path: &Path) -> io::Result<PathBuf> {
        if self.fault == "not-executable" {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture execute access refused",
            ));
        }
        smoke::image_executable(root, path)
    }
    fn perform(&mut self, phase: Phase) -> Pin<Box<dyn Future<Output = io::Result<Value>> + '_>> {
        Box::pin(async move {
            let event = label(phase);
            self.events.push(event);
            if self.fault == event {
                return Err(io::Error::other("password=synthetic-do-not-log-this"));
            }
            Ok(match phase {
                Phase::Start => json!(self.executable),
                Phase::Text => json!(if self.fault == "wrong-dom" {
                    "wrong"
                } else {
                    smoke::PAGE_TEXT
                }),
                Phase::Evaluate => json!(if self.fault == "wrong-js" { -1 } else { 42 }),
                Phase::ConnectionCheck => json!(self.fault == "still-connected"),
                _ => Value::Null,
            })
        })
    }
}
fn pin() -> ci::Result<String> {
    Ok(
        numera_xtask::provision::pins()?["drivers"]["playwright"]["version"]
            .as_str()
            .ok_or("Reviewed Playwright version missing")?
            .into(),
    )
}
source_test!(
    container_pin,
    "tests/test_container_browser_contract.py::test_container_definition_pins_client_to_literal_image",
    {
        let pin = pin()?;
        assert_eq!(pin, "1.65.1");
        assert!(smoke::exact_version(&pin));
        let docker = fs::read_to_string(root().join("Dockerfile"))?;
        assert!(docker.contains("FROM ubuntu:24.04") && docker.contains("--verify-installation"));
        assert!(
            docker
                .split_ascii_whitespace()
                .any(|package| package == "libatomic1"),
            "the bundled Node runtime requires libatomic.so.1"
        );
        assert!(!docker.contains("pip install") && !docker.contains("npm"));
        ci::container::definition(&root())?;
        Ok(())
    }
);
source_test!(
    container_ci_offline,
    "tests/test_container_browser_contract.py::test_container_ci_requires_bundled_browser_without_network_or_host_cache",
    {
        let workflows = json!({"native-ci":workflow::parse(&fs::read_to_string(root().join(".github/workflows/native-ci.yml"))?)?,"container":workflow::parse(&fs::read_to_string(root().join(".github/workflows/container.yml"))?)?});
        ci::container::contract(&workflows)?;
        let command = ci::container::smoke_command("numera-browser:ci", "1.65.1", "owned-fixture");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.windows(2).any(|pair| pair == ["--network", "none"]));
        for required in ["--init", "--shm-size=1g", "--container-smoke"] {
            assert!(args.iter().any(|arg| arg == required));
        }
        let workflow = workflows["container"].to_string();
        assert!(
            !workflow.contains("continue-on-error")
                && !workflow.contains("--mount")
                && !workflow.contains("--volume")
        );
        let mut evidence = ci::container::PhaseEvidence::default();
        assert!(
            evidence
                .observe(b"vendor password=synthetic-private\nNative image browser phase=Sta")
                .is_empty()
        );
        assert_eq!(
            evidence.observe(
                b"rt status=started\nNative image browser phase=Launch status=completed\n"
            ),
            [
                "Native image browser phase=Start status=started",
                "Native image browser phase=Launch status=completed"
            ]
        );
        assert!(evidence.observe(b"Native image browser phase=password=synthetic-private status=started\nNative image browser phase=Start status=started token=synthetic-private\n").is_empty());
        let record = format!(
            "Native image browser diagnostic={}\n",
            json!({"phase":"Start","category":"protocol-library","message":"Missing library; password=synthetic-private"})
        );
        let mut diagnostic = ci::container::PhaseEvidence::default();
        assert!(diagnostic.observe(&record.as_bytes()[..20]).is_empty());
        let seen = diagnostic.observe(&record.as_bytes()[20..]);
        assert_eq!(seen.len(), 1);
        assert!(
            seen[0].contains("Missing library")
                && seen[0].contains("[REDACTED]")
                && !seen[0].contains("synthetic-private")
        );
        for invalid in [
            json!({"phase":"bad","category":"sdk","message":"refused"}),
            json!({"phase":"Start","category":"raw-vendor","message":"refused"}),
            json!({"phase":"Start","category":"sdk","message":"refused","extra":true}),
            json!({"phase":"Start","category":"sdk","message":null}),
            json!({"phase":"Start","category":"sdk"}),
        ] {
            let mut parser = ci::container::PhaseEvidence::default();
            assert!(
                parser
                    .observe(format!("Native image browser diagnostic={invalid}\n").as_bytes())
                    .is_empty()
            );
        }
        assert!(ci::container::PhaseEvidence::default().observe(b"Native image browser diagnostic={\"phase\":\"Start\",\"phase\":\"Stop\",\"category\":\"sdk\",\"message\":\"duplicate\"}\n").is_empty());
        let large = format!(
            "Native image browser diagnostic={}\n",
            json!({"phase":"Start","category":"sdk","message":"é".repeat(10000)})
        );
        let clipped = ci::container::PhaseEvidence::default().observe(large.as_bytes());
        assert_eq!(clipped.len(), 1);
        assert!(clipped[0].len() <= 16384 && !clipped[0].contains('\u{fffd}'));
        let parsed: Value = serde_json::from_str(
            clipped[0]
                .strip_prefix("Native image browser diagnostic=")
                .ok_or("Diagnostic prefix missing")?,
        )?;
        assert_eq!(parsed["phase"], "Start");
        let mut bounded = ci::container::PhaseEvidence::default();
        assert!(bounded.observe(&vec![b'x'; 65536]).is_empty());
        assert!(
            bounded
                .observe(b"\nNative image browser phase=Start status=started\n")
                .is_empty()
        );
        Ok(())
    }
);
fn resolve(pin: Option<&str>, conflict: bool) -> ci::Result<(i32, Value)> {
    let temp = Temp::new("offline-vendor-resolver")?;
    let selected = pin.unwrap_or(">=1.40");
    let extra = if conflict {
        "\nconflict = { package = \"native_vendor_driver\", version = \"=1.66.0\" }"
    } else {
        ""
    };
    temp.write("Cargo.toml",format!("[package]\nname=\"numera_container_fixture\"\nversion=\"1.0.0\"\nedition=\"2024\"\n[workspace]\n[dependencies]\ndriver={{package=\"native_vendor_driver\",version=\"{selected}\"}}{extra}\n[patch.crates-io]\nold={{package=\"native_vendor_driver\",path=\"old\"}}\nnew={{package=\"native_vendor_driver\",path=\"new\"}}\n").as_bytes())?;
    temp.write("src/lib.rs", b"pub fn fixture() {}\n")?;
    for (directory, version) in [("old", "1.65.1"), ("new", "1.66.0")] {
        temp.write(&format!("{directory}/Cargo.toml"),format!("[package]\nname=\"native_vendor_driver\"\nversion=\"{version}\"\nedition=\"2024\"\n").as_bytes())?;
        temp.write(&format!("{directory}/src/lib.rs"), b"pub fn driver() {}\n")?;
    }
    let output = ci::capture(
        std::process::Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--format-version=1",
                "--manifest-path",
            ])
            .arg(temp.0.join("Cargo.toml")),
        Duration::from_secs(20),
        Duration::from_secs(10),
        128 * 1024,
    )?;
    let metadata = if output.exit == 0 {
        ci::decode(&output.stdout)?
    } else {
        Value::Null
    };
    Ok((output.exit, metadata))
}
fn selected_version(metadata: &Value) -> &str {
    metadata["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .find(|package| package["name"] == "native_vendor_driver")
        .expect("resolved vendor")["version"]
        .as_str()
        .expect("version")
}
source_test!(
    container_offline_pin,
    "tests/test_container_browser_contract.py::test_container_install_does_not_select_newer_client",
    {
        let (exit, metadata) = resolve(Some("=1.65.1"), false)?;
        assert_eq!(exit, 0);
        assert_eq!(selected_version(&metadata), pin()?);
        Ok(())
    }
);
source_test!(
    container_unconstrained_control,
    "tests/test_container_browser_contract.py::test_unconstrained_baseline_control_selects_newer_client",
    {
        let (exit, metadata) = resolve(None, false)?;
        assert_eq!(exit, 0);
        assert_eq!(selected_version(&metadata), "1.66.0");
        Ok(())
    }
);
source_test!(
    container_conflicting_pin,
    "tests/test_container_browser_contract.py::test_conflicting_client_requirement_is_refused",
    {
        let (exit, _) = resolve(Some("=1.65.1"), true)?;
        assert_ne!(exit, 0);
        Ok(())
    }
);
source_test!(
    container_orderly_resources,
    "tests/test_container_browser_contract.py::test_smoke_success_closes_all_resources_and_driver",
    {
        let temp = Temp::new("image-smoke")?;
        let mut driver = Controlled::new(&temp)?;
        ready(smoke::check(&mut driver, &pin()?, &temp.0.join("browsers")))?;
        assert_eq!(
            &driver.events[driver.events.len() - 5..],
            &[
                "close-page",
                "close-context",
                "close-browser",
                "connection-check",
                "driver-stop"
            ]
        );
        Ok(())
    }
);
fn invalid_version(version: &str) -> TestResult {
    let temp = Temp::new("invalid-sdk-version")?;
    let mut driver = Controlled::new(&temp)?;
    assert!(ready(smoke::check(&mut driver, version, &temp.0.join("browsers"))).is_err());
    assert!(driver.events.is_empty());
    Ok(())
}
source_cases!("tests/test_container_browser_contract.py::test_missing_or_invalid_version_never_starts_driver",invalid_version,[container_version_empty=>("",""),container_version_latest=>("latest","latest"),container_version_short=>("1.62","1.62"),container_version_range=>("^1.62.0","^1.62.0"),container_version_newline=>("1.62.0\\n","1.62.0\n")]);
source_test!(
    container_version_mismatch,
    "tests/test_container_browser_contract.py::test_mismatched_client_never_starts_driver",
    {
        let temp = Temp::new("mismatch")?;
        let mut driver = Controlled::new(&temp)?;
        driver.version = "0.0.0".into();
        assert!(ready(smoke::check(&mut driver, &pin()?, &temp.0.join("browsers"))).is_err());
        assert!(driver.events.is_empty());
        Ok(())
    }
);
source_test!(
    container_root_refused,
    "tests/test_container_browser_contract.py::test_root_execution_is_refused",
    {
        let temp = Temp::new("root-image")?;
        let mut driver = Controlled::new(&temp)?;
        driver.root = true;
        assert!(ready(smoke::check(&mut driver, &pin()?, &temp.0.join("browsers"))).is_err());
        assert!(driver.events.is_empty());
        Ok(())
    }
);
fn executable(damage: &str) -> TestResult {
    let temp = Temp::new("image-path")?;
    let mut driver = Controlled::new(&temp)?;
    match damage {
        "absent" => fs::remove_file(&driver.executable)?,
        "directory" => {
            fs::remove_file(&driver.executable)?;
            fs::create_dir(&driver.executable)?;
        }
        "not-executable" => driver.fault = damage.into(),
        _ => driver.executable = temp.write("host-cache-browser", b"fixture")?,
    };
    assert!(ready(smoke::check(&mut driver, &pin()?, &temp.0.join("browsers"))).is_err());
    assert!(!driver.events.contains(&"launch"));
    assert_eq!(driver.events.last(), Some(&"driver-stop"));
    Ok(())
}
source_cases!("tests/test_container_browser_contract.py::test_invalid_image_executable_never_launches",executable,[container_exe_missing=>("absent","absent"),container_exe_directory=>("directory","directory"),container_exe_access=>("not-executable","not-executable"),container_exe_escape=>("outside-image","outside-image")]);
fn failure(failure: &str) -> TestResult {
    let temp = Temp::new("image-fault")?;
    let mut driver = Controlled::new(&temp)?;
    driver.fault = failure.into();
    assert_eq!(
        ready(smoke::logged(
            &mut driver,
            &pin()?,
            &temp.0.join("browsers"),
            &temp.0.join("logs")
        )),
        1
    );
    assert_eq!(driver.events.last(), Some(&"driver-stop"));
    if failure != "launch" {
        assert!(driver.events.contains(&"close-browser"));
    }
    if !["launch", "context"].contains(&failure) {
        assert!(driver.events.contains(&"close-context"));
    }
    if !["launch", "context", "page"].contains(&failure) {
        assert!(driver.events.contains(&"close-page"));
    }
    Ok(())
}
source_cases!("tests/test_container_browser_contract.py::test_failures_never_become_success_and_cleanup_continues",failure,[container_fail_launch=>("launch","launch"),container_fail_context=>("context","context"),container_fail_page=>("page","page"),container_fail_content=>("content","content"),container_fail_text=>("text","text"),container_fail_evaluate=>("evaluate","evaluate"),container_fail_dom=>("wrong-dom","wrong-dom"),container_fail_js=>("wrong-js","wrong-js"),container_fail_close_page=>("close-page","close-page"),container_fail_close_context=>("close-context","close-context"),container_fail_close_browser=>("close-browser","close-browser"),container_fail_connection=>("connection-check","connection-check"),container_fail_connected=>("still-connected","still-connected"),container_fail_stop=>("driver-stop","driver-stop")]);
source_test!(
    container_logs,
    "tests/test_container_browser_contract.py::test_smoke_run_logs_are_unique_sanitized_and_closed",
    {
        let temp = Temp::new("smoke-log-path with spaces")?;
        for _ in 0..2 {
            let mut driver = Controlled::new(&temp)?;
            driver.fault = "launch".into();
            assert_eq!(
                ready(smoke::logged(
                    &mut driver,
                    &pin()?,
                    &temp.0.join("browsers"),
                    &temp.0.join("logs")
                )),
                1
            );
        }
        let files = fs::read_dir(temp.0.join("logs"))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        assert_eq!(files.len(), 2);
        for path in files {
            let text = fs::read_to_string(&path)?;
            assert!(!text.contains("synthetic-do-not-log-this"));
            assert!(text.contains("UTC]") && text.contains("[ERROR]") && text.contains("exit=1"));
            fs::remove_file(path)?;
        }
        Ok(())
    }
);
source_test!(
    container_cli_missing_manifest,
    "tests/test_container_browser_contract.py::test_cli_returns_failure_without_an_image_manifest",
    {
        let output = ci::capture(
            std::process::Command::new(env!("CARGO_BIN_EXE_numera-xtask"))
                .args(["ci", "container-browser", "numera-browser:ci", ""])
                .env_remove("NUMERA_CONTAINER_PLAYWRIGHT_VERSION")
                .env("UNRELATED_SECRET", "synthetic-do-not-log-this"),
            Duration::from_secs(10),
            Duration::from_secs(5),
            65536,
        )?;
        assert_ne!(output.exit, 0);
        let text = String::from_utf8(output.stderr)?;
        assert!(text.contains("exact Playwright release"));
        assert!(!text.contains("synthetic-do-not-log-this"));
        Ok(())
    }
);

source_test!(
    docker_copy_sources,
    "tests/test_docker.py::test_every_copy_source_exists",
    {
        let text = fs::read_to_string(root().join("Dockerfile"))?;
        let sources = ci::container::copy_sources(&text)?;
        assert_eq!(
            sources,
            vec![PathBuf::from("build/container/Numera Browser/engine/")]
        );
        let temp = Temp::new("native-copy-context")?;
        fs::create_dir_all(temp.0.join(&sources[0]))?;
        ci::container::context(&temp.0, &text)?;
        fs::remove_dir_all(temp.0.join("build"))?;
        assert!(ci::container::context(&temp.0, &text).is_err());
        Ok(())
    }
);
source_test!(
    docker_notices,
    "tests/test_docker.py::test_container_keeps_full_license_and_independent_notices",
    {
        let text = fs::read_to_string(root().join("Dockerfile"))?;
        for notice in [
            "LICENSE",
            "docs/licensing.md",
            "docs/browser-notices.txt",
            "docs/migration-random-notices.txt",
            "engine-rust/src/http/documentation-assets/NOTICE.md",
        ] {
            assert!(text.contains(notice));
            assert!(root().join(notice).is_file());
        }
        let product = fs::read_to_string(root().join("tools/xtask/src/product.rs"))?;
        assert!(product.contains("receipt.copy(resources_root, &runtime)"));
        let ignored = fs::read_to_string(root().join(".dockerignore"))?;
        assert!(ignored.contains("!build/container/Numera Browser/engine/**"));
        Ok(())
    }
);
source_test!(
    docker_browser_client,
    "tests/test_docker.py::test_the_browser_image_matches_the_playwright_client",
    {
        assert_eq!(pin()?, "1.65.1");
        let source = fs::read_to_string(root().join("tools/xtask/src/provision.rs"))?;
        assert!(
            source.contains("package/cli.js")
                && source.contains("browsers.json")
                && source.contains("browserVersion")
        );
        ci::container::definition(&root())?;
        Ok(())
    }
);
source_test!(
    docker_dashboard_path,
    "tests/test_docker.py::test_the_dashboard_lands_where_the_engine_serves_it",
    {
        let product = fs::read_to_string(root().join("tools/xtask/src/product.rs"))?;
        assert!(product.contains("runtime.join(\"web\")"));
        let serving = fs::read_to_string(root().join("engine-rust/src/http/dashboard.rs"))?;
        assert!(serving.contains("web"));
        ci::container::definition(&root())?;
        Ok(())
    }
);
source_test!(
    docker_health_native,
    "tests/test_docker.py::test_the_healthcheck_is_valid_python",
    {
        let text = fs::read_to_string(root().join("Dockerfile"))?;
        let command = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("CMD ["))
            .ok_or("Native container health command missing")?;
        let argv: Vec<String> = serde_json::from_str(&format!("[{command}"))?;
        assert_eq!(argv, vec!["/app/numera-engine", "--http-healthcheck"]);
        let engine = fs::read_to_string(root().join("engine-rust/src/bin/engine.rs"))?;
        assert!(engine.contains("native_entry"));
        assert!(engine.contains("--http-healthcheck"));
        Ok(())
    }
);
source_test!(
    docker_non_root,
    "tests/test_docker.py::test_the_container_does_not_run_as_root",
    {
        let text = fs::read_to_string(root().join("Dockerfile"))?;
        let user = text
            .lines()
            .find_map(|line| line.strip_prefix("USER "))
            .ok_or("Container USER missing")?;
        assert_eq!(user, "numera");
        Ok(())
    }
);
source_test!(
    docker_shared_memory,
    "tests/test_docker.py::test_shared_memory_is_raised_for_chromium",
    {
        let compose = workflow::parse(&fs::read_to_string(root().join("docker-compose.yml"))?)?;
        assert_eq!(compose["services"]["numera"]["shm_size"], "1gb");
        Ok(())
    }
);
source_test!(
    docker_loopback,
    "tests/test_docker.py::test_compose_publishes_to_loopback_only",
    {
        let compose = workflow::parse(&fs::read_to_string(root().join("docker-compose.yml"))?)?;
        let ports = compose["services"]["numera"]["ports"]
            .as_array()
            .ok_or("Ports missing")?;
        assert!(
            !ports.is_empty()
                && ports.iter().all(|port| port
                    .as_str()
                    .is_some_and(|port| port.starts_with("127.0.0.1:")))
        );
        Ok(())
    }
);
source_test!(
    docker_token_required,
    "tests/test_docker.py::test_compose_refuses_to_start_without_a_token",
    {
        let compose = workflow::parse(&fs::read_to_string(root().join("docker-compose.yml"))?)?;
        assert!(
            compose["services"]["numera"]["environment"]["NUMERA_API_TOKEN"]
                .as_str()
                .is_some_and(|value| value.starts_with("${NUMERA_API_TOKEN:?"))
        );
        Ok(())
    }
);
source_test!(
    docker_private_context,
    "tests/test_docker.py::test_dockerignore_keeps_local_only_files_out_of_the_image",
    {
        let text = fs::read_to_string(root().join(".dockerignore"))?;
        let lines = text.lines().collect::<BTreeSet<_>>();
        for private in [
            ".git",
            ".ai",
            ".ignoreme",
            "secrets.md",
            ".venv",
            "graphify-out",
        ] {
            assert!(lines.contains(private));
        }
        assert!(lines.contains("**"));
        Ok(())
    }
);
