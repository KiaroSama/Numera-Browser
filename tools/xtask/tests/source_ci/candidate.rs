use super::*;
use ci::candidate;
use std::{cell::Cell, time::Duration};
fn metadata(root: &Path) -> Value {
    let targets=[("numera-native-candidate","examples/native_candidate.rs","native-candidate"),("numera-browser-runtime","src/bin/browser_runtime.rs","native-browser"),("numera-engine","src/bin/engine.rs","native-product"),("numera-http-observer","src/bin/http_observer.rs","native-product")].into_iter().map(|(name,path,feature)|json!({"name":name,"kind":["bin"],"src_path":root.join("engine-rust").join(path),"required-features":[feature],"test":false})).collect::<Vec<_>>();
    json!({"packages":[{"name":"numera-core","features":{"default":[],"native-candidate":[],"native-browser":[],"native-product":[]},"targets":targets}]})
}
source_test!(
    candidate_target_isolation,
    "tests/test_native_candidate.py::test_candidate_target_is_explicit_and_not_part_of_default_builds",
    {
        let value = metadata(&root());
        candidate::targets(&value, &root())?;
        for damage in [
            "candidate-default",
            "browser-only-default",
            "implicit-candidate",
            "foreign",
            "source",
            "missing-worker",
            "implicit-worker",
            "worker-tests",
        ] {
            let mut changed = value.clone();
            let package = &mut changed["packages"][0];
            match damage {
                "candidate-default" => package["features"]["default"] = json!(["native-candidate"]),
                "browser-only-default" => {
                    package["features"]["default"] = json!(["native-browser"])
                }
                "implicit-candidate" => package["targets"][0]["required-features"] = json!([]),
                "foreign" => {
                    let mut target = package["targets"][0].clone();
                    target["name"] = json!("unexpected");
                    package["targets"]
                        .as_array_mut()
                        .expect("targets")
                        .push(target);
                }
                "source" => {
                    package["targets"][0]["src_path"] =
                        json!(root().join("engine-rust/src/main.rs"))
                }
                "missing-worker" => {
                    package["targets"]
                        .as_array_mut()
                        .expect("targets")
                        .remove(1);
                }
                "implicit-worker" => package["targets"][1]["required-features"] = json!([]),
                _ => package["targets"][1]["test"] = json!(true),
            };
            assert!(candidate::targets(&changed, &root()).is_err(), "{damage}");
        }
        Ok(())
    }
);
fn artifact(root: &Path, release: bool) -> Value {
    json!({"reason":"compiler-artifact","manifest_path":root.join("engine-rust/Cargo.toml"),"target":{"name":candidate::BINARY,"kind":["bin"]},"features":["native-candidate"],"profile":{"test":false,"opt_level":if release{"3"}else{"0"},"debug_assertions":!release},"executable":root.join("engine-rust/target").join(if release{"release/numera-native-candidate"}else{"fixture-candidate"})})
}
fn report(rows: &[Value]) -> Vec<u8> {
    rows.iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        .into_bytes()
}
source_test!(
    candidate_compiler_evidence,
    "tests/test_native_candidate.py::test_cargo_report_requires_success_and_one_current_enabled_binary",
    {
        let temp = Temp::new("candidate-evidence")?;
        let artifact = artifact(&temp.0, false);
        let complete = json!({"reason":"build-finished","success":true});
        assert_eq!(
            candidate::artifact(
                &temp.0,
                &report(&[artifact.clone(), complete.clone()]),
                false
            )?,
            PathBuf::from(artifact["executable"].as_str().expect("binary"))
        );
        for damage in [
            "unfinished",
            "failed",
            "duplicate",
            "disabled",
            "test",
            "foreign",
        ] {
            let mut changed = artifact.clone();
            let mut rows = vec![changed.clone(), complete.clone()];
            match damage {
                "unfinished" => {
                    rows.pop();
                }
                "failed" => rows[1]["success"] = json!(false),
                "duplicate" => rows.insert(1, changed),
                "disabled" => rows[0]["features"] = json!([]),
                "test" => rows[0]["profile"]["test"] = json!(true),
                _ => {
                    changed["executable"] = json!(temp.0.join("foreign"));
                    rows[0] = changed;
                }
            }
            assert!(
                candidate::artifact(&temp.0, &report(&rows), false).is_err(),
                "{damage}"
            );
        }
        Ok(())
    }
);
source_test!(
    candidate_optimized_evidence,
    "tests/test_native_candidate.py::test_candidate_release_artifact_requires_current_optimized_profile",
    {
        let temp = Temp::new("candidate-release")?;
        let value = artifact(&temp.0, true);
        let completed = json!({"reason":"build-finished","success":true});
        candidate::artifact(&temp.0, &report(&[value.clone(), completed.clone()]), true)?;
        for profile in [
            json!({"test":false,"opt_level":"0","debug_assertions":false}),
            json!({"test":false,"opt_level":"3","debug_assertions":true}),
            json!({"test":false}),
        ] {
            let mut changed = value.clone();
            changed["profile"] = profile;
            assert!(
                candidate::artifact(&temp.0, &report(&[changed, completed.clone()]), true).is_err()
            );
        }
        Ok(())
    }
);
fn boxed<T>(result: Result<T, Box<dyn std::error::Error>>) -> ci::Result<T> {
    result.map_err(|error| io::Error::other(error.to_string()).into())
}
source_test!(
    candidate_layout_integrity,
    "tests/test_native_candidate.py::test_candidate_layout_readback_detects_mutation_missing_and_unlisted_files",
    {
        use numera_xtask::{
            artifact_identity::BuildIdentity,
            product,
            resources::{self, FileRecord, Receipt, SourceOffer},
        };
        let temp = Temp::new("candidate layout with spaces")?;
        let resources = temp.0.join("resources");
        fs::create_dir(&resources)?;
        let mut files = Vec::new();
        let mut add = |name: String, bytes: Vec<u8>| -> TestResult {
            let path = resources.join(&name);
            fs::create_dir_all(path.parent().expect("parent"))?;
            fs::write(&path, &bytes)?;
            files.push(FileRecord {
                path: name,
                size: bytes.len() as u64,
                sha256: ci::sha(&bytes),
            });
            Ok(())
        };
        let node = if cfg!(windows) { "node.exe" } else { "node" };
        for backend in ["playwright", "patchright", "camoufox", "cloak"] {
            add(
                format!("drivers/{backend}/{node}"),
                b"inert vendor node".to_vec(),
            )?;
            add(
                format!("drivers/{backend}/package/cli.js"),
                b"inert vendor entry".to_vec(),
            )?;
            add(
                format!("drivers/{backend}/package/package.json"),
                serde_json::to_vec(
                    &json!({"version":if backend=="camoufox"{"1.62.0"}else{"1.64.0"}}),
                )?,
            )?;
        }
        let mut browser_manifest = json!({"camoufox":{"addons":[],"version":"158.0.1"}});
        for backend in ["playwright", "patchright", "camoufox"] {
            let bytes = format!("inert {backend} browser").into_bytes();
            let path = format!("{backend}/browser");
            browser_manifest[backend]["executable"] = json!(path);
            browser_manifest[backend]["sha256"] = json!(ci::sha(&bytes));
            add(format!("browsers/{path}"), bytes)?;
        }
        add(
            "browsers/manifest.json".into(),
            serde_json::to_vec(&browser_manifest)?,
        )?;
        add(
            "notices/VENDOR-LICENSES.txt".into(),
            b"full inert terms".to_vec(),
        )?;
        add(
            "sources/third-party/camoufox-source.zip".into(),
            b"inert corresponding source".to_vec(),
        )?;
        let receipt = Receipt {
            schema: 1,
            platform: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            files,
            source_offers: [
                "node",
                "playwright",
                "patchright",
                "camoufox",
                "chromium",
                "firefox",
            ]
            .into_iter()
            .map(|component| SourceOffer {
                component: component.into(),
                version: "1.0.0".into(),
                license: if component == "camoufox" {
                    "MPL-2.0"
                } else {
                    "fixture terms"
                }
                .into(),
                source_url: format!("https://example.invalid/source/{component}"),
                archive: (component == "camoufox")
                    .then(|| "sources/third-party/camoufox-source.zip".into()),
            })
            .collect(),
        };
        fs::write(
            resources.join(resources::RECEIPT),
            serde_json::to_vec(&receipt)?,
        )?;
        let identity = BuildIdentity {
            source_sha: "a".repeat(40),
            generation: 42,
            platform: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
        };
        boxed(Receipt::validate(&resources, &identity))?;
        let web = temp.0.join("web");
        fs::create_dir_all(web.join("assets"))?;
        fs::write(web.join("index.html"), b"inert dashboard")?;
        fs::write(web.join("assets/dashboard.wasm"), b"\0asm\x01\0\0\0")?;
        fs::write(
            web.join("ci-source.json"),
            serde_json::to_vec(
                &json!({"source_sha":identity.source_sha,"version":env!("CARGO_PKG_VERSION"),"runtime":"rust-wasm32","source_dirty":false}),
            )?,
        )?;
        boxed(product::verify_dashboard(&web, &identity))?;
        let engine = temp.write("inert-engine", b"inert compiled artifact")?;
        let worker = temp.write("inert-worker", b"inert compiled SDK worker")?;
        let source = temp.write("source.zip", b"inert source fixture")?;
        let root = root();
        let inputs = product::StageInputs {
            root: &root,
            source: &source,
            dashboard: &web,
            resources_root: &resources,
            receipt: &receipt,
            engine: &engine,
            browser: &worker,
            identity: &identity,
        };
        let output = temp.0.join("candidate with spaces");
        boxed(product::stage(&output, &inputs))?;
        let installed = output.join("engine");
        numera_core::installation::verify(&installed)?;
        let path = installed.join(format!(
            "numera-browser-runtime{}",
            std::env::consts::EXE_SUFFIX
        ));
        let original = fs::read(&path)?;
        for damage in ["changed", "missing", "extra"] {
            match damage {
                "changed" => fs::write(&path, b"changed")?,
                "missing" => fs::remove_file(&path)?,
                _ => fs::write(installed.join("unlisted.txt"), b"unlisted")?,
            };
            assert!(numera_core::installation::verify(&installed).is_err());
            if damage == "extra" {
                fs::remove_file(installed.join("unlisted.txt"))?;
            }
            fs::write(&path, &original)?;
        }
        numera_core::installation::verify(&installed)?;
        assert!(product::stage(&output, &inputs).is_err());
        fs::remove_file(web.join("index.html"))?;
        assert!(product::verify_dashboard(&web, &identity).is_err());
        Ok(())
    }
);
fn program(temp: &Temp) -> ci::Result<PathBuf> {
    let source = r#"use numera_core::ownership::tree::OwnedTree;use std::{process::{Command,Stdio},io::Write,time::Duration};fn main(){let args=std::env::args_os().collect::<Vec<_>>();if args[1]=="wait"{std::thread::sleep(Duration::from_secs(30));return;}let tree=OwnedTree::spawn(Command::new(std::env::current_exe().expect("exe")).arg("wait").stdin(Stdio::null())).expect("owned child");let mut record=tree.record();record["pidToken"]=record["token"].take();std::fs::write(&args[2],serde_json::to_vec(&record).expect("record")).expect("marker");println!("separate owned child published");std::io::stdout().flush().expect("flush");std::mem::forget(tree);std::thread::sleep(Duration::from_secs(30));}"#;
    let path = temp.write("nested-fixture.rs", source.as_bytes())?;
    let dependencies = std::env::current_exe()?
        .parent()
        .ok_or("Dependency directory missing")?
        .to_owned();
    let core = super::camoufox_runtime::linked_rlib(&dependencies, "numera_core")?;
    let json = super::camoufox_runtime::linked_rlib(&dependencies, "serde_json")?;
    let output = temp
        .0
        .join(format!("nested-fixture{}", std::env::consts::EXE_SUFFIX));
    let compiled = ci::capture(
        std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-name",
                "candidate_nested_probe",
                "--extern",
            ])
            .arg(format!("numera_core={}", core.display()))
            .arg("--extern")
            .arg(format!("serde_json={}", json.display()))
            .arg("-L")
            .arg(format!("dependency={}", dependencies.display()))
            .arg(path)
            .arg("-o")
            .arg(&output),
        Duration::from_secs(45),
        Duration::from_secs(30),
        65536,
    )?;
    assert_eq!(
        compiled.exit,
        0,
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    Ok(output)
}
source_test!(
    candidate_outer_timeout,
    "tests/test_native_candidate.py::test_outer_timeout_settles_reported_separate_group",
    {
        let temp = Temp::new("candidate-outer-timeout")?;
        let program = program(&temp)?;
        let marker = temp.0.join("child.json");
        let result = ci::owned::run(
            &temp.0,
            std::process::Command::new(&program)
                .arg("parent")
                .arg(&marker),
            ci::owned::limits(Duration::from_secs(5), Duration::from_millis(600), 65536),
        );
        assert!(result.is_err());
        let record = ci::decode(&fs::read(marker)?)?;
        let empty = numera_core::ownership::recorded_tree::empty(&record)?;
        if !empty {
            numera_core::ownership::recorded_tree::settle(&record, Duration::from_secs(5))?;
        }
        assert!(empty, "Outer owner left its separately grouped child alive");
        Ok(())
    }
);
source_test!(
    candidate_late_receipt,
    "tests/test_native_candidate.py::test_outer_cleanup_rereads_units_published_while_stopping_a_producer",
    {
        use numera_core::ownership::{recorded_tree, tree::OwnedTree, unit_report};
        let temp = Temp::new("candidate-late-receipt")?;
        let program = program(&temp)?;
        fs::create_dir_all(temp.0.join("jobs"))?;
        let report = temp.0.join("jobs/verification.units");
        unit_report::initialize(&report)?;
        let mut first = OwnedTree::spawn(std::process::Command::new(&program).arg("wait"))?;
        let initial = first.record();
        let attempt = unit_report::begin_at(&report)?;
        unit_report::record_at(&report, &initial, &attempt)?;
        let mut late = None;
        let mut seen = Vec::new();
        let settled =
            unit_report::settle_with(&report, Duration::from_secs(10), None, |record, budget| {
                seen.push(record["pid"].as_u64().expect("PID"));
                let result = recorded_tree::settle(record, budget)?;
                if late.is_none() {
                    let tree = OwnedTree::spawn(std::process::Command::new(&program).arg("wait"))?;
                    let attempt = unit_report::begin_at(&report)?;
                    unit_report::record_at(&report, &tree.record(), &attempt)?;
                    late = Some(tree);
                }
                Ok(result)
            })?;
        first.settle(Duration::from_secs(3))?;
        let mut late = late.ok_or("Late transfer was never published")?;
        let pid = late.pid();
        late.settle(Duration::from_secs(3))?;
        assert!(settled);
        assert!(
            seen.contains(&u64::from(pid)),
            "Late SDK ownership was omitted"
        );
        Ok(())
    }
);
fn other_cache((platform, engine): (&str, &str)) -> TestResult {
    let mut env = BTreeMap::from([
        ("HOME".into(), "synthetic".into()),
        ("USERPROFILE".into(), "synthetic".into()),
        ("XDG_CACHE_HOME".into(), "existing".into()),
    ]);
    let before = env.clone();
    candidate::bind_sdk_cache(&mut env, engine, platform, || {
        panic!("Other SDK prerequisite was introduced")
    })?;
    assert_eq!(env, before);
    Ok(())
}
source_cases!("tests/test_native_candidate.py::test_candidate_cache_binding_does_not_add_other_sdk_prerequisites",other_cache,[candidate_cache_linux=>("linux-camoufox",("linux","camoufox")),candidate_cache_macos=>("darwin-camoufox",("darwin","camoufox")),candidate_cache_pw=>("win32-playwright",("win32","playwright")),candidate_cache_pr=>("win32-patchright",("win32","patchright")),candidate_cache_cloak=>("win32-cloak",("win32","cloak"))]);
fn bound_cache(prior: &str) -> TestResult {
    let temp = Temp::new("parent cache with spaces")?;
    let root = temp.0.join("effective local appdata");
    fs::create_dir(&root)?;
    let mut parent = BTreeMap::from([(
        "USERPROFILE".into(),
        temp.0
            .join("actual parent home")
            .to_string_lossy()
            .into_owned(),
    )]);
    if prior != "missing" {
        parent.insert(
            "WIN_PD_OVERRIDE_LOCAL_APPDATA".into(),
            if prior == "relative" {
                "relative".into()
            } else {
                root.to_string_lossy().into_owned()
            },
        );
    }
    let original = parent.clone();
    let mut env = parent.clone();
    env.insert("HOME".into(), "synthetic".into());
    env.insert("USERPROFILE".into(), "synthetic".into());
    let calls = Cell::new(0);
    candidate::bind_sdk_cache(&mut env, "camoufox", "win32", || {
        calls.set(calls.get() + 1);
        assert_eq!(parent, original);
        Ok(root.clone())
    })?;
    assert_eq!(calls.get(), 1);
    assert_eq!(env["WIN_PD_OVERRIDE_LOCAL_APPDATA"], root.to_string_lossy());
    assert_eq!(env["HOME"], "synthetic");
    assert_eq!(env["USERPROFILE"], "synthetic");
    assert_eq!(parent, original);
    Ok(())
}
source_cases!("tests/test_native_candidate.py::test_candidate_cache_binding_captures_effective_parent_root",bound_cache,[candidate_cache_missing=>("missing","missing"),candidate_cache_relative=>("relative","relative"),candidate_cache_absolute=>("absolute","absolute")]);
fn bad_cache(invalid: &str) -> TestResult {
    let temp = Temp::new("bad-parent-cache")?;
    let file = temp.write("file", b"inert")?;
    let value = match invalid {
        "empty" => PathBuf::new(),
        "relative" => PathBuf::from("relative"),
        "missing" => temp.0.join("absent"),
        _ => file,
    };
    let mut env = BTreeMap::from([("USERPROFILE".into(), "synthetic".into())]);
    let before = env.clone();
    assert!(candidate::bind_sdk_cache(&mut env, "camoufox", "win32", || Ok(value)).is_err());
    assert_eq!(env, before);
    Ok(())
}
source_cases!("tests/test_native_candidate.py::test_candidate_cache_binding_refuses_unavailable_root",bad_cache,[candidate_cache_empty=>("empty","empty"),candidate_cache_bad_relative=>("relative","relative"),candidate_cache_absent=>("missing","missing"),candidate_cache_file=>("file","file")]);
source_test!(
    candidate_safe_diagnostics,
    "tests/test_native_candidate.py::test_candidate_failure_diagnostics_keep_only_bounded_safe_events",
    {
        let temp = Temp::new("candidate-diagnostics")?;
        let logs = temp.0.join("logs");
        fs::create_dir(&logs)?;
        let prefix = "[2026-10-17 16:54:37 UTC] [INFO] [numera.browser-adapter] ";
        temp.write("logs/browser-adapter_fixture.log",format!("{prefix}Camoufox SDK import complete\n{prefix}Operation failed operation=open type=FileNotFoundError\n{prefix}Operation failed operation=open type=private_fixture_value\n{prefix}SDK timing operation=open elapsed_ns=1234\n{prefix}SDK timing operation=cleanup elapsed_ns=56\n{prefix}SDK timing operation=private_fixture_value elapsed_ns=1\n{prefix}SDK timing operation=open elapsed_ns=-1\n{prefix}SDK timing operation=open elapsed_ns=123456789012345678901\n{prefix}secret=private_fixture_value\nunstructured private_fixture_value\n").as_bytes())?;
        let native = "[2026-10-17 16:54:37 UTC] [INFO] [entry] ";
        let mut messages = format!("{native}Startup\n");
        for phase in [
            "startup",
            "prepare",
            "metadata",
            "execute",
            "shutdown",
            "browser_spawn",
            "browser_hello",
            "browser_open",
            "browser_cookies_read",
            "browser_close",
            "browser_settle",
        ] {
            messages += &format!("{native}Native timing phase={phase} elapsed_ns=123\n");
        }
        messages += &format!(
            "{native}Native timing phase=execute elapsed_ns=0\n{native}Native timing phase=shutdown elapsed_ns=18446744073709551615\n{native}Native timing phase=private_fixture_value elapsed_ns=1\n{native}Native timing phase=execute elapsed_ns=1 secret=private_fixture_value\n"
        );
        temp.write("logs/native-cli_fixture.log", messages.as_bytes())?;
        temp.write("logs/unapproved.log", b"unapproved")?;
        temp.write("logs/browser-adapter_large.log", &vec![b'x'; 65537])?;
        let linked = temp.write("logs/browser-adapter_link.log", b"linked ownership fixture")?;
        let events = candidate::diagnostics_checked(&logs, |path| {
            if path == linked {
                return Err(io::Error::other("fixture symbolic link").into());
            }
            ci::regular(path, false)?;
            Ok(())
        })?;
        let text = events.join("\n");
        assert!(
            text.contains("Camoufox SDK import complete")
                && text.contains("type=FileNotFoundError")
                && text.contains("type=unknown")
                && text.contains("elapsed_ns=1234")
                && text.contains("elapsed_ns=56")
                && text.contains("elapsed_ns=0")
                && text.contains("elapsed_ns=18446744073709551615")
                && text.contains("omitted=3")
        );
        assert!(
            !text.contains("private_fixture_value")
                && !text.contains("elapsed_ns=-1")
                && !text.contains("123456789012345678901")
        );
        assert!(events.len() <= 65);
        assert_eq!(
            fs::metadata(logs.join("browser-adapter_large.log"))?.len(),
            65537
        );
        temp.write(
            "logs/browser-adapter_fixture.log",
            format!("{prefix}Camoufox SDK import complete\n")
                .repeat(100)
                .as_bytes(),
        )?;
        assert_eq!(candidate::diagnostics(&logs)?.len(), 65);
        Ok(())
    }
);
