use super::*;
source_cases!("tests/test_camoufox_runtime.py::test_prepare_requires_real_camoufox_font_bundle_without_launch_or_mutation",fonts,[camo_fonts_none_grouped=>("None-grouped",("None","grouped")),camo_fonts_none_legacy=>("None-legacy",("None","legacy")),camo_fonts_none_flat=>("None-flat",("None","flat")),camo_fonts_config_grouped=>("config-grouped",("config","grouped")),camo_fonts_config_legacy=>("config-legacy",("config","legacy")),camo_fonts_config_flat=>("config-flat",("config","flat")),camo_fonts_empty_fonts_grouped=>("empty-fonts-grouped",("empty-fonts","grouped")),camo_fonts_empty_fonts_legacy=>("empty-fonts-legacy",("empty-fonts","legacy")),camo_fonts_empty_fonts_flat=>("empty-fonts-flat",("empty-fonts","flat")),camo_fonts_bad_groups_grouped=>("bad-groups-grouped",("bad-groups","grouped")),camo_fonts_bad_groups_legacy=>("bad-groups-legacy",("bad-groups","legacy")),camo_fonts_bad_groups_flat=>("bad-groups-flat",("bad-groups","flat")),camo_fonts_missing_group_grouped=>("missing-group-grouped",("missing-group","grouped")),camo_fonts_missing_group_legacy=>("missing-group-legacy",("missing-group","legacy")),camo_fonts_missing_group_flat=>("missing-group-flat",("missing-group","flat")),camo_fonts_wrong_binding_grouped=>("wrong-binding-grouped",("wrong-binding","grouped")),camo_fonts_wrong_binding_legacy=>("wrong-binding-legacy",("wrong-binding","legacy")),camo_fonts_wrong_binding_flat=>("wrong-binding-flat",("wrong-binding","flat")),camo_fonts_malformed_config_grouped=>("malformed-config-grouped",("malformed-config","grouped")),camo_fonts_malformed_config_legacy=>("malformed-config-legacy",("malformed-config","legacy")),camo_fonts_malformed_config_flat=>("malformed-config-flat",("malformed-config","flat")),camo_fonts_comment_marker_grouped=>("comment-marker-grouped",("comment-marker","grouped")),camo_fonts_comment_marker_legacy=>("comment-marker-legacy",("comment-marker","legacy")),camo_fonts_comment_marker_flat=>("comment-marker-flat",("comment-marker","flat")),camo_fonts_cdata_marker_grouped=>("cdata-marker-grouped",("cdata-marker","grouped")),camo_fonts_cdata_marker_legacy=>("cdata-marker-legacy",("cdata-marker","legacy")),camo_fonts_cdata_marker_flat=>("cdata-marker-flat",("cdata-marker","flat"))]);
source_cases!("tests/test_camoufox_runtime.py::test_font_regression_rejects_the_previous_missing_guard",mutant,[camo_mutant_config=>("config","config"),camo_mutant_empty_fonts=>("empty-fonts","empty-fonts"),camo_mutant_wrong_binding=>("wrong-binding","wrong-binding"),camo_mutant_comment_marker=>("comment-marker","comment-marker"),camo_mutant_cdata_marker=>("cdata-marker","cdata-marker")]);
use ci::camoufox_runtime::{self as runtime, Captured, Host, Invocation, PACKAGES, Runner};
fn host() -> Host {
    Host {
        actions: "true".into(),
        repository: "KiaroSama/Numera-Browser".into(),
        runner: "Linux".into(),
        runner_arch: "X64".into(),
        engine: "camoufox".into(),
        system: "Linux".into(),
        machine: "x86_64".into(),
        distro: "ubuntu".into(),
        release: "24.04".into(),
        sdk: "1.62.0".into(),
        browser: "158.0.1".into(),
    }
}
fn status(absent: &[&str]) -> Vec<u8> {
    PACKAGES
        .iter()
        .filter(|name| !absent.contains(name))
        .map(|name| format!("{name}\tamd64\tinstalled\tok\n"))
        .collect::<String>()
        .into_bytes()
}
struct Commands {
    missing: Vec<&'static str>,
    damage: &'static str,
    calls: Vec<Invocation>,
    queries: usize,
}
impl Runner for Commands {
    fn command(&mut self, invocation: &Invocation) -> ci::Result<Captured> {
        self.calls.push(invocation.clone());
        assert!(invocation.seconds > 0);
        assert_eq!(invocation.environment["LC_ALL"], "C.UTF-8");
        assert_eq!(invocation.environment["LANGUAGE"], "C");
        if invocation.argv[0] != "dpkg-query" {
            assert_eq!(
                &invocation.argv[..7],
                &[
                    "sudo",
                    "-n",
                    "env",
                    "DEBIAN_FRONTEND=noninteractive",
                    "LC_ALL=C.UTF-8",
                    "LANGUAGE=C",
                    "apt-get"
                ]
            );
            let failed = self.damage == "update-failed" && invocation.argv[7] == "update"
                || self.damage == "install-failed" && invocation.argv[7] == "install";
            return Ok(Captured {
                exit: if failed { 100 } else { 0 },
                stdout: Vec::new(),
            });
        }
        self.queries += 1;
        assert_eq!(invocation.seconds, 10);
        assert_eq!(&invocation.argv[invocation.argv.len() - 30..], PACKAGES);
        if self.damage == "query-timeout" {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "private diagnostic").into());
        }
        let absent = if self.queries == 1 || self.damage == "postcondition" {
            self.missing.as_slice()
        } else {
            &[]
        };
        let mut stdout = String::from_utf8(status(absent))?;
        let mut exit = i32::from(!absent.is_empty());
        match self.damage {
            "query-fatal" => exit = 2,
            "query-malformed" => stdout = "not a package record\n".into(),
            "duplicate" => stdout += &format!("{}\n", stdout.lines().next().expect("status")),
            "foreign-package" => stdout += "unexpected\tamd64\tinstalled\tok\n",
            "foreign-architecture" => stdout = stdout.replacen("amd64", "arm64", 1),
            "reinst-required" => stdout = stdout.replacen("\tok\n", "\treinstreq\n", 1),
            "half-configured" => stdout = stdout.replacen("installed", "half-configured", 1),
            "empty-success" => stdout.clear(),
            "contradictory-exit" => exit = 1,
            _ => (),
        }
        Ok(Captured {
            exit,
            stdout: stdout.into_bytes(),
        })
    }
}
fn installed(missing: Vec<&'static str>) -> TestResult {
    let temp = Temp::new("camo-runtime")?;
    let mut commands = Commands {
        missing: missing.clone(),
        damage: "None",
        calls: Vec::new(),
        queries: 0,
    };
    assert_eq!(
        runtime::logged(&host(), &mut commands, &temp.0.join("logs")),
        0
    );
    if missing.is_empty() {
        assert_eq!(commands.calls.len(), 1);
        assert_eq!(commands.queries, 1);
    } else {
        assert_eq!(commands.calls.len(), 4);
        assert_eq!(commands.queries, 2);
        assert_eq!(&commands.calls[1].argv[7..], &["update"]);
        let mut tail = [
            "install",
            "-y",
            "--no-install-recommends",
            "--no-remove",
            "--no-upgrade",
        ]
        .map(str::to_owned)
        .to_vec();
        tail.extend(missing.into_iter().map(str::to_owned));
        assert_eq!(commands.calls[2].argv[7..], tail);
    }
    commands.missing.clear();
    assert_eq!(
        runtime::logged(&host(), &mut commands, &temp.0.join("logs")),
        0
    );
    let files = fs::read_dir(temp.0.join("logs"))?.collect::<io::Result<Vec<_>>>()?;
    assert_eq!(files.len(), 2);
    for entry in files {
        assert!(fs::read_to_string(entry.path())?.contains("Runtime packages verified count=30"));
    }
    Ok(())
}
source_cases!("tests/test_camoufox_runtime.py::test_entrypoint_installs_only_missing_runtime_and_rechecks_all",installed,[camo_runtime_installed=>("installed",vec![]),camo_runtime_one=>("one-missing",vec!["libpci3"]),camo_runtime_two=>("two-missing",vec!["libpci3","zstd"])]);
fn refusal(damage: &'static str) -> TestResult {
    let temp = Temp::new("camo-runtime-refusal")?;
    let mut host = host();
    match damage {
        "foreign-repo" => host.repository = "other/repo".into(),
        "local" => host.actions = "false".into(),
        "engine" => host.engine = "playwright".into(),
        "runner-arch" => host.runner_arch = "ARM64".into(),
        "host-arch" => host.machine = "aarch64".into(),
        "distro" => host.distro = "debian".into(),
        "release" => host.release = "26.04".into(),
        "sdk" => host.sdk = "unsupported".into(),
        _ => (),
    };
    let missing = if ["update-failed", "install-failed", "postcondition"].contains(&damage) {
        vec!["libpci3"]
    } else {
        Vec::new()
    };
    let mut commands = Commands {
        missing,
        damage,
        calls: Vec::new(),
        queries: 0,
    };
    assert_eq!(
        runtime::logged(&host, &mut commands, &temp.0.join("logs")),
        1
    );
    let expected = if [
        "foreign-repo",
        "local",
        "engine",
        "runner-arch",
        "host-arch",
        "distro",
        "release",
        "sdk",
    ]
    .contains(&damage)
    {
        0
    } else {
        match damage {
            "postcondition" => 4,
            "install-failed" => 3,
            "update-failed" => 2,
            _ => 1,
        }
    };
    assert_eq!(commands.calls.len(), expected);
    let files = fs::read_dir(temp.0.join("logs"))?.collect::<io::Result<Vec<_>>>()?;
    assert_eq!(files.len(), 1);
    let text = fs::read_to_string(files[0].path())?;
    assert!(text.contains("[ERROR]") && text.contains("exit=1"));
    assert!(!text.contains("private diagnostic"));
    Ok(())
}
source_cases!("tests/test_camoufox_runtime.py::test_entrypoint_refuses_unproved_host_observation_or_install",refusal,[camo_foreign=>("foreign-repo","foreign-repo"),camo_local=>("local","local"),camo_engine=>("engine","engine"),camo_runner_arch=>("runner-arch","runner-arch"),camo_host_arch=>("host-arch","host-arch"),camo_distro=>("distro","distro"),camo_release=>("release","release"),camo_sdk=>("sdk","sdk"),camo_query_fatal=>("query-fatal","query-fatal"),camo_query_timeout=>("query-timeout","query-timeout"),camo_query_malformed=>("query-malformed","query-malformed"),camo_duplicate=>("duplicate","duplicate"),camo_foreign_package=>("foreign-package","foreign-package"),camo_foreign_arch=>("foreign-architecture","foreign-architecture"),camo_reinstall=>("reinst-required","reinst-required"),camo_half=>("half-configured","half-configured"),camo_empty=>("empty-success","empty-success"),camo_contradictory=>("contradictory-exit","contradictory-exit"),camo_update=>("update-failed","update-failed"),camo_install=>("install-failed","install-failed"),camo_postcondition=>("postcondition","postcondition")]);
source_test!(
    camo_workflow,
    "tests/test_camoufox_runtime.py::test_workflow_keeps_camoufox_runtime_and_real_sdk_acceptance_bounded",
    {
        let workflow = workflow::parse(&fs::read_to_string(
            root().join(".github/workflows/native-ci.yml"),
        )?)?;
        let job = &workflow["jobs"]["live"];
        assert_eq!(job["runs-on"], "${{ matrix.os }}");
        assert_eq!(
            job["strategy"]["matrix"]["os"],
            json!(["ubuntu-24.04", "windows-latest"])
        );
        assert_eq!(
            job["strategy"]["matrix"]["engine"],
            json!(["playwright", "patchright", "camoufox", "cloak"])
        );
        assert_eq!(job["strategy"]["max-parallel"], 2);
        assert_eq!(job["strategy"]["fail-fast"], false);
        assert_eq!(job["timeout-minutes"], 35);
        let step = job["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .find(|step| step["name"] == "Install Firefox system dependencies")
            .expect("Camo runtime gate");
        assert_eq!(
            step["if"],
            "runner.os == 'Linux' && matrix.engine == 'camoufox'"
        );
        assert!(
            step["run"]
                .as_str()
                .expect("command")
                .contains("ci owned --wall 300 --idle 120 -- cargo xtask ci camoufox-runtime")
        );
        workflow::step_once(
            &job["steps"],
            "./.github/actions/native-candidate-acceptance",
            true,
        )?;
        let acceptance = workflow::parse(&fs::read_to_string(
            root().join(".github/actions/native-candidate-acceptance/action.yml"),
        )?)?;
        workflow::step_once(
            &acceptance["runs"]["steps"],
            "selected_sdk_opens_real_persistent_context_in_native_owned_tree",
            true,
        )?;
        Ok(())
    }
);
fn bundle(temp: &Temp, layout: &str, damage: &str) -> ci::Result<PathBuf> {
    let base = "browser with spaces";
    let binary = temp.write(&format!("{base}/camoufox-bin"), b"not executed")?;
    let config = if layout == "legacy" {
        "fontconfigs"
    } else {
        "fontconfig"
    };
    for identity in ["linux", "macos", "windows"] {
        temp.write(
            &format!("{base}/{config}/{identity}/fonts.conf"),
            b"<fontconfig><dir prefix=\"cwd\">fonts</dir></fontconfig>",
        )?;
    }
    let folders = if layout == "grouped" {
        temp.write(
            &format!("{base}/fonts/groups.json"),
            br#"{"readBy":{"lin":["L"],"mac":["M","MX"],"win":["W"]}}"#,
        )?;
        vec!["L", "M", "MX", "W"]
    } else if layout == "legacy" {
        vec!["linux", "macos", "windows"]
    } else {
        vec![""]
    };
    for folder in &folders {
        temp.write(
            &format!("{base}/fonts/{folder}/fixture.ttf"),
            b"\0\x01\0\0synthetic font header",
        )?;
    }
    match damage {
        "config" => fs::remove_file(temp.0.join(format!("{base}/{config}/macos/fonts.conf")))?,
        "empty-fonts" => {
            for folder in &folders {
                fs::remove_file(temp.0.join(format!("{base}/fonts/{folder}/fixture.ttf")))?;
            }
        }
        "bad-groups" => {
            temp.write(&format!("{base}/fonts/groups.json"), b"{invalid")?;
        }
        "missing-group" => {
            temp.write(
                &format!("{base}/fonts/groups.json"),
                br#"{"readBy":{"lin":["LX"],"mac":["LX"],"win":["LX"]}}"#,
            )?;
        }
        "wrong-binding" | "malformed-config" | "comment-marker" | "cdata-marker" => {
            let content = match damage {
                "wrong-binding" => "<fontconfig><dir>/missing</dir></fontconfig>",
                "malformed-config" => "<fontconfig>",
                "comment-marker" => {
                    "<fontconfig><!-- <dir prefix=\"cwd\">fonts</dir> --><dir prefix='cwd'>fonts</dir></fontconfig>"
                }
                _ => {
                    "<fontconfig><![CDATA[<dir prefix=\"cwd\">fonts</dir>]]><dir prefix='cwd'>fonts</dir></fontconfig>"
                }
            };
            temp.write(
                &format!("{base}/{config}/linux/fonts.conf"),
                content.as_bytes(),
            )?;
        }
        _ => (),
    };
    Ok(binary)
}
fn bytes(root: &Path) -> ci::Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn walk(base: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) -> TestResult {
        for entry in fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(base, &path, result)?;
            } else {
                result.insert(path.strip_prefix(base)?.to_owned(), fs::read(path)?);
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result)?;
    Ok(result)
}
fn fonts((damage, layout): (&str, &str)) -> TestResult {
    let temp = Temp::new("camo-fonts")?;
    let binary = bundle(&temp, layout, damage)?;
    let before = bytes(binary.parent().expect("bundle"))?;
    let result = numera_core::browser::camoufox_fonts::prepare(&binary);
    assert_eq!(
        result.is_ok(),
        damage == "None",
        "{damage}-{layout}: {result:?}"
    );
    if damage == "None" {
        assert_eq!(result?, binary.canonicalize()?);
    }
    assert_eq!(bytes(binary.parent().expect("bundle"))?, before);
    Ok(())
}
pub(super) fn linked_rlib(dependencies: &Path, name: &str) -> ci::Result<PathBuf> {
    let prefix = format!("lib{name}-");
    let mut paths = fs::read_dir(dependencies)?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".rlib"))
        })
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    paths.pop().ok_or_else(|| {
        io::Error::other("Production Rust parser library missing from native test target").into()
    })
}
fn mutant(damage: &str) -> TestResult {
    let temp = Temp::new("camo-font-mutant")?;
    let binary = bundle(&temp, "grouped", damage)?;
    let source = fs::read_to_string(root().join("engine-rust/src/browser/camoufox_fonts.rs"))?;
    assert_eq!(
        source.matches("    require(root)?;").count(),
        1,
        "Production guard changed; update its removal control"
    );
    let dependencies = std::env::current_exe()?
        .parent()
        .ok_or("Test dependency directory missing")?
        .to_owned();
    let parser = linked_rlib(&dependencies, "quick_xml")?;
    let json = linked_rlib(&dependencies, "serde_json")?;
    for removed in [false, true] {
        let module = if removed {
            source.replace("    require(root)?;", "")
        } else {
            source.clone()
        };
        let program = format!(
            "mod fonts {{\n{module}\n}}\nfn main(){{let path=std::path::PathBuf::from(std::env::args_os().nth(1).expect(\"binary\"));if fonts::prepare(&path).is_err(){{std::process::exit(1);}}}}\n"
        );
        let path = temp.write(
            if removed { "mutant.rs" } else { "guarded.rs" },
            program.as_bytes(),
        )?;
        let output = temp.0.join(format!(
            "font-{}{}",
            if removed { "mutant" } else { "guarded" },
            std::env::consts::EXE_SUFFIX
        ));
        let compiled = ci::capture(
            std::process::Command::new("rustc")
                .args([
                    "--edition=2024",
                    "--crate-name",
                    "camo_font_guard_probe",
                    "--extern",
                ])
                .arg(format!("quick_xml={}", parser.display()))
                .arg("--extern")
                .arg(format!("serde_json={}", json.display()))
                .arg("-L")
                .arg(format!("dependency={}", dependencies.display()))
                .arg(&path)
                .arg("-o")
                .arg(&output),
            std::time::Duration::from_secs(45),
            std::time::Duration::from_secs(30),
            65536,
        )?;
        assert_eq!(
            compiled.exit,
            0,
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let ran = ci::capture(
            std::process::Command::new(output).arg(&binary),
            std::time::Duration::from_secs(5),
            std::time::Duration::from_secs(5),
            65536,
        )?;
        assert_eq!(
            ran.exit == 0,
            removed,
            "Actual guard removal did not change the damaged bundle verdict"
        );
    }
    Ok(())
}
