use super::*;
use numera_core::ownership::capture;
fn key(case: &history::Case) -> (&str, &str, &str) {
    (&case.target, &case.test, &case.commit)
}
fn without_companions(case: &history::Case) -> history::Case {
    let mut case = case.clone();
    case.companions.clear();
    case
}
fn registration(damage: &str) -> TestResult {
    let mut targets = scope::targets(&root())?;
    match damage {
        "missing-relay" => targets.retain(|name| name != "relay_request_parity"),
        "missing-receipt" => targets.retain(|name| name != "ownership_receipt_parity"),
        "unmapped-target" => targets.push("unmapped_fixture".into()),
        _ => (),
    };
    verdict(scope::target_registration(&targets), damage == "None");
    Ok(())
}
source_cases!("tests/test_native_regressions.py::test_oracle_verification_requires_complete_native_target_registration",registration,[history_registration_good=>("None","None"),history_registration_relay=>("missing-relay","missing-relay"),history_registration_receipt=>("missing-receipt","missing-receipt"),history_registration_foreign=>("unmapped-target","unmapped-target")]);
source_test!(
    history_storage_coherence,
    "tests/test_native_regressions.py::test_option_reader_companions_preserve_selected_defects_and_prior_snapshots",
    {
        let registry = history::Registry::read()?;
        let storage = registry.storage();
        assert_eq!(storage.len(), registry.base.len() + 1);
        let mut grouped = 0;
        for before in &registry.base {
            let after = storage
                .iter()
                .find(|case| key(case) == key(before))
                .expect("samecase");
            assert_eq!(without_companions(after), without_companions(before));
            for (name, value) in &before.companions {
                assert_eq!(after.companions.get(name), Some(value));
            }
            let touched = before
                .files
                .keys()
                .chain(before.companions.keys())
                .collect::<BTreeSet<_>>();
            if touched
                .iter()
                .any(|name| registry.storage_sources.contains_key(*name))
            {
                grouped += 1;
                let sources = history::case_sources(after)?;
                assert!(
                    registry
                        .storage_sources
                        .keys()
                        .all(|name| sources.contains_key(name))
                );
            } else {
                assert_eq!(after, before);
            }
        }
        assert_eq!(grouped, 9);
        let added = storage.last().expect("storagecase");
        assert_eq!(
            added.test,
            "settings_errors::storage_settings_errors_match_original_lazy_reads_and_preserve_bytes"
        );
        assert_eq!(added.files, registry.storage_sources);
        assert_eq!(added.commit, registry.storage_commit);
        assert_eq!(added.symptom, "Storage settings admission differs");
        Ok(())
    }
);
source_test!(
    history_settings_coherence,
    "tests/test_native_regressions.py::test_settings_signature_companions_preserve_all_existing_proofs",
    {
        let registry = history::Registry::read()?;
        let previous = registry.storage();
        let settings = registry.settings();
        assert_eq!(settings.len(), previous.len() + 2);
        for (before, after) in previous.iter().zip(&settings) {
            assert_eq!(without_companions(before), without_companions(after));
            for (name, value) in &before.companions {
                assert_eq!(after.companions.get(name), Some(value));
            }
            let touched = before
                .files
                .keys()
                .chain(before.companions.keys())
                .collect::<BTreeSet<_>>();
            if touched.iter().any(|name| {
                registry.settings_sources.contains_key(*name)
                    || registry.storage_sources.contains_key(*name)
            }) {
                let sources = history::case_sources(after)?;
                for (name, hash) in &registry.settings_sources {
                    assert!(sources.contains_key(name));
                    if !touched.contains(name) {
                        assert_eq!(
                            sources[name],
                            history::Source {
                                commit: registry.settings_commit.clone(),
                                sha256: hash.clone()
                            }
                        );
                    }
                }
            } else {
                assert_eq!(before, after);
            }
        }
        assert_eq!(settings[settings.len() - 2].target, "cli_settings_parity");
        assert_eq!(
            settings[settings.len() - 2].files,
            registry.settings_sources
        );
        let extension = settings.last().expect("extension");
        assert_eq!(extension.target, "config_parity");
        assert_eq!(extension.commit, "b8e589f28e57e5734be2bacbd000bb11cc454ba2");
        assert_eq!(
            extension.files["engine-rust/src/storage/config.rs"],
            "2c037f3fcb63a1d210c84437a72285a371a9c1fe89d85d510fe3c649142c406f"
        );
        Ok(())
    }
);
source_test!(
    history_extension_coherence,
    "tests/test_native_regressions.py::test_extension_history_follows_settings_coherence_without_contaminating_other_proofs",
    {
        let registry = history::Registry::read()?;
        let previous = registry.settings();
        let extension = registry.extension();
        let mut corrected = 0;
        for (before, after) in previous.iter().zip(&extension) {
            if before
                .files
                .contains_key("engine-rust/src/storage/extensions.rs")
            {
                corrected += 1;
                assert_eq!(without_companions(before), without_companions(after));
                assert!(before.companions.is_empty());
                assert_eq!(after.companions.len(), 1);
                assert_eq!(
                    after.companions["engine-rust/src/http/extension_routes.rs"],
                    history::Source {
                        commit: "edca9d09805ccdc2d29735c8977a4354c71223b7".into(),
                        sha256: "d6624367937929bd0e719243925d12a94787382525c01c0717e58e916dea2ac2"
                            .into()
                    }
                );
            } else {
                assert_eq!(before, after);
            }
        }
        assert_eq!(corrected, 3);
        let cases = registry.cases()?;
        assert_eq!(&cases[..extension.len()], extension);
        assert_eq!(&cases[extension.len()..], registry.current);
        Ok(())
    }
);
fn selected_case(target: &str) -> ci::Result<history::Case> {
    let cases = history::Registry::read()?.cases()?;
    let selected = cases
        .into_iter()
        .filter(|case| case.target == target)
        .collect::<Vec<_>>();
    assert_eq!(selected.len(), 1);
    Ok(selected[0].clone())
}
source_test!(
    history_schedule_exact,
    "tests/test_native_regressions.py::test_schedule_history_preserves_text_failure_without_later_api_callers",
    {
        let case = selected_case("schedule_store_parity")?;
        assert_eq!(
            case.test,
            "schedule_text_admission_matches_original_without_replacing_bytes"
        );
        assert_eq!(case.symptom, "Schedule text admission differs");
        assert_eq!(case.commit, "b9d7399732c140d6141f37633486fffd90a2e30a");
        assert_eq!(
            case.files["engine-rust/src/storage/schedules.rs"],
            "749856bedd3df0be79cbd3853014fc75d24bb8b9320dcc9a09c32747ccf47527"
        );
        assert_eq!(case.companions.len(), 2);
        assert_eq!(
            case.companions["engine-rust/tests/schedule_store_parity.rs"].commit,
            "93db8f8b93d8b8e961bc548a3174da560c16c0b3"
        );
        history::case_sources(&case)?;
        Ok(())
    }
);
source_test!(
    history_pool_exact,
    "tests/test_native_regressions.py::test_pool_history_keeps_a_compatible_importer_without_replacing_the_selected_defect",
    {
        let case = selected_case("pool_parse_parity")?;
        assert_eq!(
            case.test,
            "pooled_proxy_text_boundaries_match_original_full_results"
        );
        assert_eq!(case.commit, "ef6ebe26ba183fbbd9892d077d70fdcd8f7f7c64");
        assert_eq!(
            case.files["engine-rust/src/pool.rs"],
            "5381266449dafc5eca9009fe1d900c3aed760da00a3855b8d122dc58772bb5ab"
        );
        assert_eq!(
            case.files["engine-rust/src/pool_normalize.rs"],
            "f97331c9da3246f9ad993df1cacfe80f9d59d5494fd6db4c9c8cd26b05cd17e7"
        );
        assert_eq!(
            case.companions["engine-rust/src/importers.rs"].commit,
            "4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1"
        );
        assert_eq!(
            case.companions["engine-rust/src/browser/options.rs"].sha256,
            "48f97356e1d6232fa5a84ab619d8bb7f7d9fc25f6b7f7623447339f5de488263"
        );
        history::case_sources(&case)?;
        Ok(())
    }
);
source_test!(
    history_scheduler_exact,
    "tests/test_native_regressions.py::test_scheduler_row_history_requires_the_original_isolation_failure",
    {
        let cases = history::Registry::read()?.cases()?;
        let case=cases.iter().find(|case|case.test=="scheduler_rows::skipped_malformed_schedule_ids_do_not_abort_later_occurrences").expect("scheduler");
        assert_eq!(case.symptom, "Scheduler row isolation differs");
        assert_eq!(case.commit, "7d8108a31551330c34a7d183bb04335422b0bcb0");
        assert_eq!(
            case.files["engine-rust/src/http/scheduler.rs"],
            "3e0e0e7fc208acc286c5006d15d172658df9e08078192a19c7c42e62c39d4a0c"
        );
        Ok(())
    }
);
source_test!(
    history_ascii_exact,
    "tests/test_native_regressions.py::test_ascii_refusal_history_requires_the_shared_representation_failure",
    {
        let cases = history::Registry::read()?.cases()?;
        let case=cases.iter().find(|case|case.test=="ascii_refusal_representation_matches_original_without_exposing_secret_values").expect("ascii");
        assert_eq!(case.symptom, "ASCII refusal representation differs");
        assert_eq!(case.commit, "65f5dfd15f2a4b6cc7fd375e858a76b153c83862");
        assert_eq!(
            case.files["engine-rust/src/validation.rs"],
            "d505c76357437baee6a8ef76d7626f099adbf59b8f2829cd8b4dfc73c1e8d4e6"
        );
        Ok(())
    }
);
source_test!(
    history_legacy_exact,
    "tests/test_native_regressions.py::test_legacy_read_history_requires_the_existing_http_assertion",
    {
        let cases = history::Registry::read()?.cases()?;
        let case = cases
            .iter()
            .find(|case| {
                case.test
                    == "read_view::legacy_read_views_match_original_core_http_and_name_admission"
            })
            .expect("legacy");
        assert_eq!(case.symptom, "Legacy HTTP read view differs");
        assert_eq!(case.commit, "65f5dfd15f2a4b6cc7fd375e858a76b153c83862");
        assert_eq!(
            case.files["engine-rust/src/http/profile_routes.rs"],
            "9c3fcf9caf0bc6fa97ba48b62341b527fb1bc6f017e3859720a257b8d88bd202"
        );
        Ok(())
    }
);
source_test!(
    history_current_appended,
    "tests/test_native_regressions.py::test_current_caller_history_is_appended_after_compatibility_overlays",
    {
        let registry = history::Registry::read()?;
        let cases = registry.cases()?;
        assert_eq!(registry.current.len(), 52);
        assert_eq!(&cases[cases.len() - 52..], registry.current);
        for case in &registry.current {
            assert!(case.companions.is_empty());
            history::case_sources(case)?;
        }
        assert_eq!(
            registry.current[31].commit,
            "1a46f71178be5c482ecad04f14666eadaa52dc0d"
        );
        assert_eq!(
            registry.current[51].commit,
            "dccccf2ad40820606b1c74f4de951cb163e0fe6a"
        );
        Ok(())
    }
);
fn failure(damage: &str) -> TestResult {
    let case = &history::Registry::read()?.cases()?[0];
    let mut evidence = evidence::Failure::new(&case.test, &case.symptom);
    for(key,line)in[("empty","running 1 test".into()),("name",format!("test {} ... FAILED",case.test)),("symptom",format!("assertion left == right failed: {}",case.symptom)),("summary","test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s".into())]{if key!=damage{evidence.accept(&line);}}
    verdict(
        evidence.verify(if damage == "exit" { 0 } else { 101 }),
        damage == "None",
    );
    Ok(())
}
source_cases!("tests/test_native_regressions.py::test_failure_proof_requires_the_exact_production_assertion",failure,[history_failure_good=>("None","None"),history_failure_exit=>("exit","exit"),history_failure_empty=>("empty","empty"),history_failure_name=>("name","name"),history_failure_symptom=>("symptom","symptom"),history_failure_summary=>("summary","summary")]);
fn hosted(damage: &str) -> TestResult {
    let sha = "a".repeat(40);
    let wrong = "b".repeat(40);
    verdict(
        history::hosted_clean(
            if damage == "local" { "false" } else { "true" },
            if damage == "repository" {
                "other/repository"
            } else {
                "KiaroSama/Numera-Browser"
            },
            if damage == "sha" { "invalid" } else { &sha },
            if damage == "head" { &wrong } else { &sha },
            damage != "dirty",
        ),
        damage == "None",
    );
    Ok(())
}
source_cases!("tests/test_native_regressions.py::test_historical_execution_requires_the_exact_clean_hosted_source",hosted,[history_host_good=>("None","None"),history_host_local=>("local","local"),history_host_repository=>("repository","repository"),history_host_sha=>("sha","sha"),history_host_head=>("head","head"),history_host_dirty=>("dirty","dirty")]);
fn restoration(
    (target, incremental, damage, platform): (&str, Option<&str>, &str, &str),
) -> TestResult {
    let temp = Temp::new("historical")?;
    let current = b"fixed production bytes\n";
    let historical = b"historical production bytes\n";
    let fixed_test = b"fixed test bytes\n";
    let old_test = b"original assertion bytes\n";
    let source = temp.write("engine-rust/src/models.rs", current)?;
    let companion = temp.write("engine-rust/tests/config_parity.rs", fixed_test)?;
    let mut case = history::Registry::read()?.cases()?[0].clone();
    case.target = target.into();
    case.files = BTreeMap::from([("engine-rust/src/models.rs".into(), ci::sha(historical))]);
    case.companions = BTreeMap::from([(
        "engine-rust/tests/config_parity.rs".into(),
        history::Source {
            commit: "b".repeat(40),
            sha256: ci::sha(old_test),
        },
    )]);
    case.platform = Some(platform.into());
    let mut excluded = case.clone();
    excluded.platform = Some(
        if platform == "win32" {
            "linux"
        } else {
            "win32"
        }
        .into(),
    );
    excluded.commit = "excluded-source-must-not-be-read".into();
    let mut inputs = BTreeMap::from([
        (
            (
                case.commit.clone(),
                "engine-rust/src/models.rs".into(),
                ci::sha(historical),
            ),
            if damage == "integrity" {
                b"changed".to_vec()
            } else {
                historical.to_vec()
            },
        ),
        (
            (
                "b".repeat(40),
                "engine-rust/tests/config_parity.rs".into(),
                ci::sha(old_test),
            ),
            if damage == "companion" {
                b"changed".to_vec()
            } else {
                old_test.to_vec()
            },
        ),
    ]);
    let mut cases = vec![excluded, case.clone()];
    if damage == "None" {
        cases.push(case.clone());
    }
    let prior_incremental = std::env::var_os("CARGO_INCREMENTAL");
    let mut calls = 0;
    let mut compiled = 0;
    let result = history::prove_cases_with(
        &temp.0,
        &cases,
        platform,
        &mut inputs,
        |compile, case, command| {
            assert_eq!(fs::read(&source)?, historical);
            assert_eq!(fs::read(&companion)?, old_test);
            let argv = command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(argv.contains(&"--locked".into()) && argv.contains(&"--offline".into()));
            assert_eq!(
                argv.iter().filter(|arg| arg.as_str() == "--config").count(),
                1
            );
            assert!(argv.contains(&"profile.dev.package.numera-core.opt-level=0".into()));
            assert!(argv.contains(&"--timings".into()));
            assert_eq!(
                command
                    .get_envs()
                    .find(|(key, _)| *key == "CARGO_INCREMENTAL")
                    .and_then(|(_, value)| value),
                Some(std::ffi::OsStr::new("0"))
            );
            if let Some(prior) = incremental {
                assert!(["0", "1"].contains(&prior));
            }
            if compile {
                compiled += 1;
                assert_eq!(argv.last().map(String::as_str), Some("--no-run"));
                if damage == "warm" {
                    return Err(io::Error::other("fixture compiler failure").into());
                }
                return Ok(ci::owned::Output {
                    command: capture::Output {
                        exit: 0,
                        stdout: Vec::new(),
                        stderr: Vec::new(),
                    },
                    memory: Value::Null,
                });
            }
            calls += 1;
            assert_eq!(
                &argv[argv.len() - 4..],
                &[
                    case.test.clone(),
                    "--exact".into(),
                    "--test-threads=1".into(),
                    "--color=never".into()
                ]
            );
            assert_eq!(
                argv.contains(&"--message-format=json".into()),
                platform == "win32"
            );
            if damage == "launch" {
                return Err(
                    io::Error::new(io::ErrorKind::TimedOut, "fixture runner timeout").into(),
                );
            }
            let output = format!(
                "running 1 test\ntest {} ... FAILED\n{}\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.01s\n",
                case.test,
                if damage == "assertion" {
                    "unrelated failure"
                } else {
                    &case.symptom
                }
            );
            Ok(ci::owned::Output {
                command: capture::Output {
                    exit: 101,
                    stdout: output.into_bytes(),
                    stderr: Vec::new(),
                },
                memory: Value::Null,
            })
        },
    );
    assert_eq!(result.is_ok(), damage == "None", "{result:?}");
    assert_eq!(fs::read(source)?, current);
    assert_eq!(fs::read(companion)?, fixed_test);
    assert_eq!(std::env::var_os("CARGO_INCREMENTAL"), prior_incremental);
    assert_eq!(
        calls,
        if damage == "None" {
            2
        } else if ["integrity", "companion", "warm"].contains(&damage) {
            0
        } else {
            1
        }
    );
    assert_eq!(
        compiled,
        usize::from(!["integrity", "companion"].contains(&damage))
    );
    Ok(())
}
source_cases!("tests/test_native_regressions.py::test_historical_runner_restores_source_and_environment",restoration,[
 history_restore_model_parity_unset_good_win32=>("model_parity-None-None-win32",("model_parity",None,"None","win32")),
 history_restore_model_parity_unset_good_linux=>("model_parity-None-None-linux",("model_parity",None,"None","linux")),
 history_restore_model_parity_unset_warm_win32=>("model_parity-None-warm-win32",("model_parity",None,"warm","win32")),
 history_restore_model_parity_unset_warm_linux=>("model_parity-None-warm-linux",("model_parity",None,"warm","linux")),
 history_restore_model_parity_unset_launch_win32=>("model_parity-None-launch-win32",("model_parity",None,"launch","win32")),
 history_restore_model_parity_unset_launch_linux=>("model_parity-None-launch-linux",("model_parity",None,"launch","linux")),
 history_restore_model_parity_unset_assertion_win32=>("model_parity-None-assertion-win32",("model_parity",None,"assertion","win32")),
 history_restore_model_parity_unset_assertion_linux=>("model_parity-None-assertion-linux",("model_parity",None,"assertion","linux")),
 history_restore_model_parity_unset_integrity_win32=>("model_parity-None-integrity-win32",("model_parity",None,"integrity","win32")),
 history_restore_model_parity_unset_integrity_linux=>("model_parity-None-integrity-linux",("model_parity",None,"integrity","linux")),
 history_restore_model_parity_unset_companion_win32=>("model_parity-None-companion-win32",("model_parity",None,"companion","win32")),
 history_restore_model_parity_unset_companion_linux=>("model_parity-None-companion-linux",("model_parity",None,"companion","linux")),
 history_restore_model_parity_0_good_win32=>("model_parity-0-None-win32",("model_parity",Some("0"),"None","win32")),
 history_restore_model_parity_0_good_linux=>("model_parity-0-None-linux",("model_parity",Some("0"),"None","linux")),
 history_restore_model_parity_0_warm_win32=>("model_parity-0-warm-win32",("model_parity",Some("0"),"warm","win32")),
 history_restore_model_parity_0_warm_linux=>("model_parity-0-warm-linux",("model_parity",Some("0"),"warm","linux")),
 history_restore_model_parity_0_launch_win32=>("model_parity-0-launch-win32",("model_parity",Some("0"),"launch","win32")),
 history_restore_model_parity_0_launch_linux=>("model_parity-0-launch-linux",("model_parity",Some("0"),"launch","linux")),
 history_restore_model_parity_0_assertion_win32=>("model_parity-0-assertion-win32",("model_parity",Some("0"),"assertion","win32")),
 history_restore_model_parity_0_assertion_linux=>("model_parity-0-assertion-linux",("model_parity",Some("0"),"assertion","linux")),
 history_restore_model_parity_0_integrity_win32=>("model_parity-0-integrity-win32",("model_parity",Some("0"),"integrity","win32")),
 history_restore_model_parity_0_integrity_linux=>("model_parity-0-integrity-linux",("model_parity",Some("0"),"integrity","linux")),
 history_restore_model_parity_0_companion_win32=>("model_parity-0-companion-win32",("model_parity",Some("0"),"companion","win32")),
 history_restore_model_parity_0_companion_linux=>("model_parity-0-companion-linux",("model_parity",Some("0"),"companion","linux")),
 history_restore_model_parity_1_good_win32=>("model_parity-1-None-win32",("model_parity",Some("1"),"None","win32")),
 history_restore_model_parity_1_good_linux=>("model_parity-1-None-linux",("model_parity",Some("1"),"None","linux")),
 history_restore_model_parity_1_warm_win32=>("model_parity-1-warm-win32",("model_parity",Some("1"),"warm","win32")),
 history_restore_model_parity_1_warm_linux=>("model_parity-1-warm-linux",("model_parity",Some("1"),"warm","linux")),
 history_restore_model_parity_1_launch_win32=>("model_parity-1-launch-win32",("model_parity",Some("1"),"launch","win32")),
 history_restore_model_parity_1_launch_linux=>("model_parity-1-launch-linux",("model_parity",Some("1"),"launch","linux")),
 history_restore_model_parity_1_assertion_win32=>("model_parity-1-assertion-win32",("model_parity",Some("1"),"assertion","win32")),
 history_restore_model_parity_1_assertion_linux=>("model_parity-1-assertion-linux",("model_parity",Some("1"),"assertion","linux")),
 history_restore_model_parity_1_integrity_win32=>("model_parity-1-integrity-win32",("model_parity",Some("1"),"integrity","win32")),
 history_restore_model_parity_1_integrity_linux=>("model_parity-1-integrity-linux",("model_parity",Some("1"),"integrity","linux")),
 history_restore_model_parity_1_companion_win32=>("model_parity-1-companion-win32",("model_parity",Some("1"),"companion","win32")),
 history_restore_model_parity_1_companion_linux=>("model_parity-1-companion-linux",("model_parity",Some("1"),"companion","linux")),
 history_restore_lib_unset_good_win32=>("lib-None-None-win32",("lib",None,"None","win32")),
 history_restore_lib_unset_good_linux=>("lib-None-None-linux",("lib",None,"None","linux")),
 history_restore_lib_unset_warm_win32=>("lib-None-warm-win32",("lib",None,"warm","win32")),
 history_restore_lib_unset_warm_linux=>("lib-None-warm-linux",("lib",None,"warm","linux")),
 history_restore_lib_unset_launch_win32=>("lib-None-launch-win32",("lib",None,"launch","win32")),
 history_restore_lib_unset_launch_linux=>("lib-None-launch-linux",("lib",None,"launch","linux")),
 history_restore_lib_unset_assertion_win32=>("lib-None-assertion-win32",("lib",None,"assertion","win32")),
 history_restore_lib_unset_assertion_linux=>("lib-None-assertion-linux",("lib",None,"assertion","linux")),
 history_restore_lib_unset_integrity_win32=>("lib-None-integrity-win32",("lib",None,"integrity","win32")),
 history_restore_lib_unset_integrity_linux=>("lib-None-integrity-linux",("lib",None,"integrity","linux")),
 history_restore_lib_unset_companion_win32=>("lib-None-companion-win32",("lib",None,"companion","win32")),
 history_restore_lib_unset_companion_linux=>("lib-None-companion-linux",("lib",None,"companion","linux")),
 history_restore_lib_0_good_win32=>("lib-0-None-win32",("lib",Some("0"),"None","win32")),
 history_restore_lib_0_good_linux=>("lib-0-None-linux",("lib",Some("0"),"None","linux")),
 history_restore_lib_0_warm_win32=>("lib-0-warm-win32",("lib",Some("0"),"warm","win32")),
 history_restore_lib_0_warm_linux=>("lib-0-warm-linux",("lib",Some("0"),"warm","linux")),
 history_restore_lib_0_launch_win32=>("lib-0-launch-win32",("lib",Some("0"),"launch","win32")),
 history_restore_lib_0_launch_linux=>("lib-0-launch-linux",("lib",Some("0"),"launch","linux")),
 history_restore_lib_0_assertion_win32=>("lib-0-assertion-win32",("lib",Some("0"),"assertion","win32")),
 history_restore_lib_0_assertion_linux=>("lib-0-assertion-linux",("lib",Some("0"),"assertion","linux")),
 history_restore_lib_0_integrity_win32=>("lib-0-integrity-win32",("lib",Some("0"),"integrity","win32")),
 history_restore_lib_0_integrity_linux=>("lib-0-integrity-linux",("lib",Some("0"),"integrity","linux")),
 history_restore_lib_0_companion_win32=>("lib-0-companion-win32",("lib",Some("0"),"companion","win32")),
 history_restore_lib_0_companion_linux=>("lib-0-companion-linux",("lib",Some("0"),"companion","linux")),
 history_restore_lib_1_good_win32=>("lib-1-None-win32",("lib",Some("1"),"None","win32")),
 history_restore_lib_1_good_linux=>("lib-1-None-linux",("lib",Some("1"),"None","linux")),
 history_restore_lib_1_warm_win32=>("lib-1-warm-win32",("lib",Some("1"),"warm","win32")),
 history_restore_lib_1_warm_linux=>("lib-1-warm-linux",("lib",Some("1"),"warm","linux")),
 history_restore_lib_1_launch_win32=>("lib-1-launch-win32",("lib",Some("1"),"launch","win32")),
 history_restore_lib_1_launch_linux=>("lib-1-launch-linux",("lib",Some("1"),"launch","linux")),
 history_restore_lib_1_assertion_win32=>("lib-1-assertion-win32",("lib",Some("1"),"assertion","win32")),
 history_restore_lib_1_assertion_linux=>("lib-1-assertion-linux",("lib",Some("1"),"assertion","linux")),
 history_restore_lib_1_integrity_win32=>("lib-1-integrity-win32",("lib",Some("1"),"integrity","win32")),
 history_restore_lib_1_integrity_linux=>("lib-1-integrity-linux",("lib",Some("1"),"integrity","linux")),
 history_restore_lib_1_companion_win32=>("lib-1-companion-win32",("lib",Some("1"),"companion","win32")),
 history_restore_lib_1_companion_linux=>("lib-1-companion-linux",("lib",Some("1"),"companion","linux"))
]);
