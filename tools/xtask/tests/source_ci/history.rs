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
        let clock_case = cases
            .iter()
            .find(|case| case.target == "profile_read_parity")
            .ok_or("Native HTTP clock fixture missing")?;
        for compile in [false, true] {
            let command = history::proof_command(&root(), clock_case, compile, "linux");
            let args = command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(args.iter().any(|arg| arg == "--no-default-features"));
            assert!(
                args.windows(2)
                    .any(|args| args == ["--features", "test-support"])
            );
        }
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
        caller_bridge_controls()?;
        let registry = history::Registry::read()?;
        let cases = registry.cases()?;
        let workflow = workflow::parse(&fs::read_to_string(
            root().join(".github/workflows/native-ci.yml"),
        )?)?;
        workflow::native_failure_evidence(&workflow)?;
        for lost in [
            "build/native-product/.desktop-build/tauri-build*.log",
            "engine-rust/target/cargo-timings/",
            "build/native-product/**/PROVENANCE.json",
        ] {
            let mut changed = workflow.clone();
            let artifact = changed["jobs"]["build"]["steps"]
                .as_array_mut()
                .ok_or("Build steps missing")?
                .iter_mut()
                .find(|step| step["name"] == "Preserve complete failed native packaging evidence")
                .ok_or("Product diagnostic artifact missing")?;
            artifact["with"]["path"] = artifact["with"]["path"]
                .as_str()
                .ok_or("Paths missing")?
                .lines()
                .filter(|path| *path != lost)
                .collect::<Vec<_>>()
                .join("\n")
                .into();
            assert!(workflow::native_failure_evidence(&changed).is_err());
        }
        for case in &cases {
            let command = history::proof_command(&root(), case, true, "win32");
            assert!(
                command
                    .get_envs()
                    .any(|(key, value)| key == "VSCMD_SKIP_SENDTELEMETRY"
                        && value.is_some_and(|value| value == "1"))
            );
        }
        workflow::native_functional_fixture(&workflow)?;
        let mut absent = workflow.clone();
        absent["jobs"]["functional"]["env"]["NUMERA_NATIVE_CANDIDATE"] = Value::Null;
        assert!(workflow::native_functional_fixture(&absent).is_err());
        let mut wrong = workflow.clone();
        wrong["jobs"]["functional"]["env"]["NUMERA_NATIVE_ENGINE"] =
            "numera-native-candidate".into();
        assert!(workflow::native_functional_fixture(&wrong).is_err());
        history::workflow_contract(&workflow)?;
        for (key, value) in [
            (
                "strategy",
                json!({"fail-fast":false,"matrix":{"os":["windows-latest"]}}),
            ),
            (
                "strategy",
                json!({"fail-fast":false,"matrix":{"os":["ubuntu-24.04"]}}),
            ),
            (
                "strategy",
                json!({"fail-fast":true,"matrix":{"os":["ubuntu-24.04","windows-latest"]}}),
            ),
            ("runs-on", json!("windows-latest")),
        ] {
            let mut missing = workflow.clone();
            missing["jobs"]["history"][key] = value;
            assert!(history::workflow_contract(&missing).is_err());
        }
        let mut lost = workflow.clone();
        let steps = lost["jobs"]["history"]["steps"]
            .as_array_mut()
            .ok_or("History steps missing")?;
        let artifact = steps
            .iter_mut()
            .find(|step| step["name"] == "Preserve actual historical compiler diagnostics")
            .ok_or("History artifact missing")?;
        artifact["with"]["path"] = "build/history/".into();
        assert!(history::workflow_contract(&lost).is_err());
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

fn caller_bridge_controls() -> TestResult {
    clock_and_pool_caller_controls()?;
    capture_caller_controls()?;
    let root = root();
    let cases = history::Registry::read()?.cases()?;
    let config = cases
        .iter()
        .find(|case| {
            case.test
                == "settings_instance_cache_matches_original_file_identity_and_detached_results"
        })
        .ok_or("Historical settings cache case missing")?;
    let path = "engine-rust/tests/config_parity.rs";
    let source = config
        .companions
        .get(path)
        .ok_or("Config fixture companion missing")?;
    let original = ci::git(&root, &["show", &format!("{}:{path}", source.commit)])?;
    let current = fs::read(root.join(path))?;
    let selected_before = fs::read(root.join("engine-rust/src/storage/config.rs"))?;
    let bridge =
        ci::history_compat::fixture_replacement(&root, config, path, source, &current, &original)?
            .ok_or("Native config fixture ABI bridge missing")?;
    let patches = bridge.receipt["caller_abi_patches"]
        .as_array()
        .ok_or("Fixture ABI patches absent")?;
    assert!(patches.iter().any(
        |patch| patch[0] == "store.headless_launch().map(Value::Bool)"
            && patch[1] == "Ok(Value::Bool(store.headless_launch()))"
    ));
    assert_eq!(fs::read(root.join(path))?, current);
    assert_eq!(
        fs::read(root.join("engine-rust/src/storage/config.rs"))?,
        selected_before
    );
    let repo = cases
        .iter()
        .find(|case| {
            case.test
                == "ordinary_encrypted_reads_preserve_original_bytes_and_repair_only_real_drift"
        })
        .ok_or("Repo history missing")?;
    let path = "engine-rust/src/http/profile_routes.rs";
    let before = fs::read(root.join(path))?;
    let bridges = ci::history_compat::additional_callers(&root, repo)?;
    let (_, bridge) = bridges
        .iter()
        .find(|(name, _)| name == path)
        .ok_or("HTTP caller bridge missing")?;
    assert_eq!(bridge.receipt["before_sha256"], ci::sha(&before));
    assert_eq!(bridge.receipt["after_sha256"], ci::sha(&bridge.bytes));
    assert_eq!(
        bridge.receipt["request_snapshot_caller_rollback"],
        ci::sha(&before) == "6e12619ba12661ebc352c1fdab9b2ce346e0cbe085ac81c4a68c61346ee7c6ea"
    );
    assert!(!String::from_utf8_lossy(&bridge.bytes).contains("list_for_reconciliation"));
    assert_eq!(
        fs::read(root.join(path))?,
        before,
        "Caller plan mutated live product source"
    );
    let wrong = Temp::new("unknown-history-caller")?;
    wrong.write(path, &[before.as_slice(), b"\n// unreviewed"].concat())?;
    assert!(ci::history_compat::additional_callers(&wrong.0, repo).is_err());
    let selected = cases
        .iter()
        .find(|case| case.files.contains_key(path))
        .ok_or("Selected HTTP historical source missing")?;
    assert!(
        ci::history_compat::additional_callers(&root, selected)?
            .iter()
            .all(|(name, _)| name != path)
    );
    let vault = cases
        .iter()
        .find(|case| {
            case.test == "envelope_classification_matches_original_unicode_and_prefix_contracts"
        })
        .ok_or("Vault history missing")?;
    let bridges = ci::history_compat::additional_callers(&root, vault)?;
    let (_, policy) = bridges
        .iter()
        .find(|(name, _)| name == "engine-rust/src/vault/policy.rs")
        .ok_or("Vault constructor bridge missing")?;
    assert_eq!(
        policy.receipt["historical_source_path"],
        "engine-rust/src/vault/mod.rs"
    );
    assert!(!String::from_utf8_lossy(&policy.bytes).contains("VaultError::message("));
    let receipt_root = Temp::new("distinct-historical-vault-receipts")?;
    for path in [
        "engine-rust/src/vault/mod.rs",
        "engine-rust/src/vault/policy.rs",
        "engine-rust/src/vault/durability.rs",
        "engine-rust/src/ownership/update_admission.rs",
    ] {
        receipt_root.write(path, &fs::read(root.join(path))?)?;
    }
    let mut receipts = Vec::new();
    for hash in [
        "5fbf9a042308ce8405b66c8e2276372272157566cc2e420594e0aa9e1ae677fd",
        "71150ac122b5f5c26199ff9fcac4c042db5fcd403beb89f1a809dd0da925de3b",
    ] {
        let case = cases
            .iter()
            .find(|case| {
                case.files
                    .get("engine-rust/src/vault/mod.rs")
                    .is_some_and(|value| value == hash)
            })
            .ok_or("Crypto-admission historical source missing")?;
        let original = ci::git(
            &root,
            &[
                "show",
                &format!("{}:engine-rust/src/vault/mod.rs", case.commit),
            ],
        )?;
        assert_eq!(ci::sha(&original), hash);
        assert!(String::from_utf8_lossy(&original).contains("pub struct VaultError(String)"));
        let bridges = ci::history_compat::additional_callers(&root, case)?;
        assert_eq!(bridges.len(), 2);
        assert!(
            bridges
                .iter()
                .all(|(path, _)| !case.files.contains_key(path)
                    && !case.companions.contains_key(path))
        );
        assert!(
            bridges
                .iter()
                .any(|(path, bridge)| path == "engine-rust/src/vault/policy.rs"
                    && !String::from_utf8_lossy(&bridge.bytes).contains("VaultError::message("))
        );
        let mut inputs = BTreeMap::new();
        let mut swap = history::SourceSwap::prepare_with(
            &receipt_root.0,
            case,
            &mut inputs,
            |commit, name| {
                assert_eq!(name, "engine-rust/src/vault/mod.rs");
                assert_eq!(commit, case.commit);
                Ok(original.clone())
            },
        )?;
        assert_eq!(
            fs::read(receipt_root.0.join("engine-rust/src/vault/mod.rs"))?,
            original
        );
        swap.restore()?;
        let receipt = receipt_root.0.join("build/history").join(format!(
            "adaptation-{}.json",
            ci::sha(format!("{}:{}:{}", case.commit, case.target, case.test).as_bytes())
        ));
        receipts.push((receipt, hash));
    }
    assert_ne!(receipts[0].0, receipts[1].0);
    for (path, hash) in receipts {
        let saved: Value = serde_json::from_slice(&fs::read(path)?)?;
        let rows = saved.as_array().ok_or("Historical receipt rows absent")?;
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| row["historical_source_sha256"] == hash)
        );
    }
    let recovery = cases
        .iter()
        .find(|case| case.target == "recovery_parity")
        .ok_or("Recovery historical source missing")?;
    let path = "engine-rust/src/vault/transaction.rs";
    let transaction_before = fs::read(root.join(path))?;
    let primary_before = fs::read(root.join("engine-rust/src/vault/recovery.rs"))?;
    let rotation_before = fs::read(root.join("engine-rust/src/vault/rotation_recovery.rs"))?;
    let bridges = ci::history_callers::additional(&root, recovery)?;
    assert_eq!(bridges.len(), 1);
    let (name, bridge) = &bridges[0];
    assert_eq!(name, path);
    assert!(!recovery.files.contains_key(path) && !recovery.companions.contains_key(path));
    assert_eq!(
        bridge.receipt["before_sha256"],
        ci::sha(&transaction_before)
    );
    assert_eq!(bridge.receipt["after_sha256"], ci::sha(&bridge.bytes));
    let changed = String::from_utf8_lossy(&bridge.bytes);
    assert!(!changed.contains("caused_refusal"));
    assert!(!changed.contains("super::recovery::migration_failure"));
    assert!(!changed.contains("super::recovery::rollback_failed"));
    assert!(changed.contains("Vault migration failed: {rollback}; journal retained"));
    assert!(changed.contains("Some(old_salt), Some(old_verifier)"));
    assert!(changed.contains("check_password(old_password, old_salt, old_verifier)"));
    assert_eq!(fs::read(root.join(path))?, transaction_before);
    assert_eq!(
        fs::read(root.join("engine-rust/src/vault/recovery.rs"))?,
        primary_before
    );
    assert_eq!(
        fs::read(root.join("engine-rust/src/vault/rotation_recovery.rs"))?,
        rotation_before
    );
    let wrong = Temp::new("unknown-recovery-history-caller")?;
    wrong.write(
        path,
        &[transaction_before.as_slice(), b"\n// unreviewed"].concat(),
    )?;
    assert!(ci::history_callers::additional(&wrong.0, recovery).is_err());
    let mut selected = recovery.clone();
    selected
        .files
        .insert(path.into(), ci::sha(&transaction_before));
    assert!(ci::history_callers::additional(&root, &selected).is_err());
    let tree = cases
        .iter()
        .find(|case| case.files.contains_key("engine-rust/src/ownership/tree.rs"))
        .ok_or("Original tree uncertainty history missing")?;
    let path = "engine-rust/src/ownership/session.rs";
    let current_session = fs::read(root.join(path))?;
    let primary_before = fs::read(root.join("engine-rust/src/ownership/tree.rs"))?;
    let bridges = ci::history_callers::additional(&root, tree)?;
    assert_eq!(bridges.len(), 1);
    let (name, bridge) = &bridges[0];
    assert_eq!(name, path);
    assert!(!tree.files.contains_key(path) && !tree.companions.contains_key(path));
    assert_eq!(bridge.receipt["before_sha256"], ci::sha(&current_session));
    assert_eq!(bridge.receipt["after_sha256"], ci::sha(&bridge.bytes));
    assert_eq!(
        bridge.receipt["historical_source_path"],
        "engine-rust/src/ownership/tree.rs"
    );
    let changed = String::from_utf8_lossy(&bridge.bytes);
    assert!(!changed.contains("spawn_classified") && !changed.contains("failure.settled"));
    assert!(changed.contains("OwnedTree::spawn(command)"));
    assert!(changed.contains(
        "if let Some(admission) = &mut self.admission {\n                    admission.retain();"
    ));
    assert_eq!(fs::read(root.join(path))?, current_session);
    assert_eq!(
        fs::read(root.join("engine-rust/src/ownership/tree.rs"))?,
        primary_before
    );
    let wrong = Temp::new("unknown-session-history-caller")?;
    wrong.write(
        path,
        &[current_session.as_slice(), b"\n// unreviewed"].concat(),
    )?;
    assert!(ci::history_callers::additional(&wrong.0, tree).is_err());
    let mut selected = tree.clone();
    selected
        .files
        .insert(path.into(), ci::sha(&current_session));
    assert!(ci::history_callers::additional(&root, &selected).is_err());
    let atomic = cases
        .iter()
        .find(|case| {
            case.test == "persisted_json_text_matches_original_escaping_and_platform_newlines"
        })
        .ok_or("Atomic history missing")?;
    let before = fs::read(root.join("engine-rust/src/storage/atomic.rs"))?;
    let bridges = ci::history_compat::additional_callers(&root, atomic)?;
    assert_eq!(bridges.len(), 2);
    assert!(
        bridges
            .iter()
            .all(|(name, _)| !atomic.files.contains_key(name))
    );
    assert_eq!(
        fs::read(root.join("engine-rust/src/storage/atomic.rs"))?,
        before
    );
    Ok(())
}
fn capture_caller_controls() -> TestResult {
    const PRIMARY: &str = "engine-rust/src/ownership/capture.rs";
    const UNITS: &str = "engine-rust/src/ownership/capture_units.rs";
    const ADAPTER: &str = "engine-rust/src/browser/adapter.rs";
    let root = root();
    let cases = history::Registry::read()?.cases()?;
    assert_eq!(cases.len(), 99);
    let case = cases.get(40).ok_or("Reviewed capture case missing")?;
    let selected = ci::git(&root, &["show", &format!("{}:{PRIMARY}", case.commit)])?;
    assert_eq!(
        ci::sha(&selected),
        "aa93ca78468acf0874bf58cf6e1fbd3ca70c7990146ce36e92ff4c3285c51e54"
    );
    let primary_before = fs::read(root.join(PRIMARY))?;
    let before = [
        (UNITS, fs::read(root.join(UNITS))?),
        (ADAPTER, fs::read(root.join(ADAPTER))?),
    ];
    let bridges = ci::history_compat::additional_callers(&root, case)?;
    assert_eq!(bridges.len(), 2);
    for ((name, bridge), (path, original)) in bridges.iter().zip(&before) {
        assert_eq!(name.as_str(), *path);
        assert!(!case.files.contains_key(*path) && !case.companions.contains_key(*path));
        assert_eq!(bridge.receipt["before_sha256"], ci::sha(original));
        assert_eq!(bridge.receipt["after_sha256"], ci::sha(&bridge.bytes));
        assert_eq!(bridge.receipt["historical_source_path"], PRIMARY);
        assert_eq!(
            bridge.receipt["historical_source_sha256"],
            ci::sha(&selected)
        );
        assert_eq!(bridge.receipt["selected_primary_bytes_unchanged"], true);
        assert_eq!(
            bridge.receipt["caller_only_current_green_probe_sha256"],
            "a7b104c8ab212a471a8fe64be3712e3f4507e724f97d458ecf64d97fc59b1924"
        );
        assert!(
            bridge.receipt["secondary_diagnostic_limitation"]
                .as_str()
                .ok_or("Missing historical limitation")?
                .contains("original capture error remains fatal")
        );
        assert_eq!(fs::read(root.join(path))?, *original);
    }
    assert_eq!(
        ci::sha(&bridges[0].1.bytes),
        "c0ce84fc39ef1af75e56033fe485b233743a5b29d07a5a2c88d2b7289189808f"
    );
    assert_eq!(
        ci::sha(&bridges[1].1.bytes),
        "029549a0e5badba41fd2f1b48ce836c18aa09b9417f49526f15b91934a5c5699"
    );
    assert_eq!(fs::read(root.join(PRIMARY))?, primary_before);
    assert_eq!(ci::sha(&selected), case.files[PRIMARY]);
    // Every selector is independent; none may broaden the reviewed graph.
    for damage in [
        "commit",
        "primary",
        "extra-selected",
        "companion",
        "platform",
        "symptom",
    ] {
        let mut changed = case.clone();
        match damage {
            "commit" => changed.commit = "0".repeat(40),
            "primary" => {
                changed.files.insert(PRIMARY.into(), "0".repeat(64));
            }
            "extra-selected" => {
                changed.files.insert(UNITS.into(), ci::sha(&before[0].1));
            }
            "companion" => {
                changed.companions.insert(
                    ADAPTER.into(),
                    history::Source {
                        commit: case.commit.clone(),
                        sha256: ci::sha(&before[1].1),
                    },
                );
            }
            "platform" => changed.platform = Some("linux".into()),
            "symptom" => changed.symptom.push_str(" changed"),
            _ => unreachable!(),
        }
        assert!(
            ci::history_compat::additional_callers(&root, &changed).is_err(),
            "capture caller guard accepted {damage}"
        );
    }
    for damage in ["target", "test"] {
        let mut changed = case.clone();
        if damage == "target" {
            changed.target.push_str("_other");
        } else {
            changed.test.push_str("_other");
        }
        assert!(ci::history_compat::additional_callers(&root, &changed)?.is_empty());
    }
    for damaged_path in [UNITS, ADAPTER] {
        let wrong = Temp::new("unreviewed capture caller")?;
        for (path, original) in &before {
            let mut bytes = original.clone();
            if *path == damaged_path {
                bytes.extend_from_slice(b"\n// unreviewed");
            }
            wrong.write(path, &bytes)?;
        }
        assert!(ci::history_compat::additional_callers(&wrong.0, case).is_err());
    }
    Ok(())
}
fn clock_and_pool_caller_controls() -> TestResult {
    let root = root();
    let cases = history::Registry::read()?.cases()?;
    let clock = cases
        .iter()
        .find(|case| {
            case.test == "relative_activity_http_matches_original_without_rewriting_storage"
        })
        .ok_or("Clock historical case missing")?;
    let fixture = "engine-rust/tests/profile_read_parity.rs";
    let before = fs::read(root.join(fixture))?;
    let replacement =
        ci::history_callers::clock_caller(clock, &before)?.ok_or("Clock caller bridge absent")?;
    assert_eq!(replacement.receipt["before_sha256"], ci::sha(&before));
    assert_eq!(
        replacement.receipt["after_sha256"],
        ci::sha(&replacement.bytes)
    );
    assert!(String::from_utf8_lossy(&replacement.bytes).contains("original_inputs"));
    assert!(String::from_utf8_lossy(&replacement.bytes).contains("expected.len(), 24"));
    assert!(
        ci::history_callers::clock_caller(
            clock,
            &[before.as_slice(), b"\n// unknown fixture"].concat()
        )
        .is_err()
    );
    let symptom = "Relative activity HTTP status differs: \"true\"";
    let valid = failed_transcript(&clock.test, symptom).replace(
        "\n\nfailures:\n    ",
        "\n  left: Number(500)\n right: Number(200)\n\nfailures:\n    ",
    );
    for (text, accepted) in [
        (valid.clone(), true),
        (valid.replace("Number(500)", "Number(401)"), false),
        (valid.replace("Number(200)", "Number(201)"), false),
        (valid.replace("  left: Number(500)\n", ""), false),
        (
            format!(
                "  left: Number(500)\n right: Number(200)\n{}",
                valid.replace("Number(500)", "Number(401)")
            ),
            false,
        ),
        (
            valid.replace("differs: \"true\"", "differs: \"future\""),
            false,
        ),
    ] {
        let mut evidence = ci::history_callers::negative_evidence(clock);
        for line in text.lines() {
            evidence.accept(line);
        }
        assert_eq!(evidence.verify(101).is_ok(), accepted);
    }
    assert_eq!(fs::read(root.join(fixture))?, before);
    let pool = cases
        .iter()
        .find(|case| case.target == "pool_parse_parity")
        .ok_or("Pool historical case missing")?;
    let before_pool = fs::read(root.join("engine-rust/src/pool.rs"))?;
    let bridges = ci::history_callers::additional(&root, pool)?;
    assert_eq!(bridges.len(), 2);
    for (path, bridge) in bridges {
        assert!(!pool.files.contains_key(&path) && !pool.companions.contains_key(&path));
        assert_eq!(
            bridge.receipt["before_sha256"],
            ci::digest(&root.join(&path))?
        );
        assert_eq!(bridge.receipt["after_sha256"], ci::sha(&bridge.bytes));
        if path.ends_with("acquisition_routes.rs") {
            for field in [
                "NUMERA_UPDATE_TOKEN",
                "NUMERA_UPDATE_TICKET",
                "NUMERA_UPDATE_ACTIVATION",
                "NUMERA_NATIVE_JOB_GATE",
            ] {
                assert!(String::from_utf8_lossy(&bridge.bytes).contains(field));
            }
        } else {
            assert!(
                String::from_utf8_lossy(&bridge.bytes)
                    .contains("driver.family().options_backend()")
            );
        }
    }
    assert_eq!(fs::read(root.join("engine-rust/src/pool.rs"))?, before_pool);
    let path = "engine-rust/src/network/proxy_check.rs";
    let source = pool.companions.get(path).ok_or("Relay companion absent")?;
    let old = ci::git(&root, &["show", &format!("{}:{path}", source.commit)])?;
    assert_eq!(ci::sha(&old), source.sha256);
    let replacement = ci::history_callers::companion(pool, path, source, &old)?
        .ok_or("Relay caller bridge absent")?;
    assert!(String::from_utf8_lossy(&replacement.bytes).contains("Url::parse(&relay_url)"));
    let mut primary = pool.clone();
    primary.companions.remove(path);
    primary.files.insert(path.into(), source.sha256.clone());
    assert!(ci::history_callers::companion(&primary, path, source, &old)?.is_none());
    Ok(())
}
fn failure(damage: &str) -> TestResult {
    let case = &history::Registry::read()?.cases()?[0];
    let mut evidence = evidence::Failure::new(&case.test, &case.symptom);
    for line in failed_transcript(&case.test, &case.symptom).lines() {
        let removed = match damage {
            "empty" => line == "running 1 test",
            "name" => line == format!("test {} ... FAILED", case.test),
            "symptom" => line.starts_with("assertion left == right failed:"),
            "summary" => line.starts_with("test result:"),
            _ => false,
        };
        if !removed {
            evidence.accept(line);
        }
    }
    verdict(
        evidence.verify(if damage == "exit" { 0 } else { 101 }),
        damage == "None",
    );
    if damage == "None" {
        let good = failed_transcript(&case.test, &case.symptom);
        let unrelated = failed_transcript(&case.test, "unrelated production assertion");
        let compiler = json!({"reason":"compiler-message","message":{"rendered":case.symptom}});
        for poisoned in [
            format!("{compiler}\n{unrelated}"),
            unrelated.replace(
                &format!("thread '{}'", case.test),
                &format!("earlier printed {}\nthread '{}'", case.symptom, case.test),
            ),
            format!("{good}{good}"),
            good.replacen("running 1 test", "running 2 tests", 1),
            good.replace(
                &format!("thread '{}'", case.test),
                "thread 'unrelated_worker'",
            ),
            good.replace("failures:\n    ", "failures:\n    unrelated_test\n    "),
            unrelated.replace(
                "unrelated production assertion",
                &format!(
                    "unrelated production assertion\n  left: {{\"data\":\"{}\"}}",
                    case.symptom
                ),
            ),
            good.replace(
                &format!("test {} ... FAILED", case.test),
                &format!("test unrelated ... FAILED\ntest {} ... FAILED", case.test),
            ),
        ] {
            let mut refused = evidence::Failure::new(&case.test, &case.symptom);
            for line in poisoned.lines() {
                refused.accept(line);
            }
            assert!(
                refused.verify(101).is_err(),
                "Unrelated or mixed transcript manufactured a named RED proof"
            );
        }
    }
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
            let output = failed_transcript(
                &case.test,
                if damage == "assertion" {
                    "unrelated failure"
                } else {
                    &case.symptom
                },
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
