use super::*;
source_test!(
    history_input_directory_metadata_history_retains_exact_prior_source_and_assertion,
    "tests/test_native_history_inputs.py::test_directory_metadata_history_retains_exact_prior_source_and_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"first_run_parity\",\"test\":\"imported_new_directories_preserve_original_metadata_without_replacing_existing_modes\",\"symptom\":\"First-run directory metadata differs\",\"commit\":\"1a46f71178be5c482ecad04f14666eadaa52dc0d\",\"files\":{\"engine-rust/src/storage/first_run.rs\":\"6a9afa7356a02b123d13ebf8d5742a67e4d6e2bf8abff0d1cb8f2459a94c5670\"}}",
        )?;
        assert_eq!(registry.current[31], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 31], expected);
        Ok(())
    }
);
source_test!(
    history_input_cli_integer_history_retains_exact_prior_source_and_assertion,
    "tests/test_native_history_inputs.py::test_cli_integer_history_retains_exact_prior_source_and_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"cli_registry_parity\",\"test\":\"integer_options_preserve_original_digit_bound_before_zero_normalization\",\"symptom\":\"CLI integer digit admission differs\",\"commit\":\"ff56fc25f25a4ad9874e644b85295ccd622fd34f\",\"files\":{\"engine-rust/src/cli/definitions.rs\":\"62577821c1120e9518a30277bb165d4e77c103ef5803c06157a2a4bc4922c4a7\"}}",
        )?;
        assert_eq!(registry.current[32], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 32], expected);
        Ok(())
    }
);
source_test!(
    history_input_hint_peer_disconnect_history_retains_compile_compatible_source_and_assertion,
    "tests/test_native_history_inputs.py::test_hint_peer_disconnect_history_retains_compile_compatible_source_and_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"browser_hints_parity\",\"test\":\"hint_response_peer_disconnects_preserve_other_io_failures\",\"symptom\":\"Hint response peer disconnect escaped\",\"commit\":\"f173f3ae59c91fc9524507f91fe6d936b2c4a0d4\",\"files\":{\"engine-rust/tests/common/hint_site.rs\":\"4dcfdaa91e4079186c5b7c9268ad50efa06c7f6f5782170c4af8fa2172a879ae\"}}",
        )?;
        assert_eq!(registry.current[33], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 33], expected);
        Ok(())
    }
);
source_test!(
    history_input_cli_option_token_history_retains_prior_parser_and_named_assertion,
    "tests/test_native_history_inputs.py::test_cli_option_token_history_retains_prior_parser_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"cli_registry_parity\",\"test\":\"separate_option_values_match_original_token_classification\",\"symptom\":\"CLI option token admission differs\",\"commit\":\"c299ef506c593623bfd0646b3d2efe1d83596532\",\"files\":{\"engine-rust/src/cli/mod.rs\":\"c534342a6f7db08b99641bd52e02e729d3abb68a1e78257b0c502c077479fcd8\"}}",
        )?;
        assert_eq!(registry.current[34], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 34], expected);
        Ok(())
    }
);
source_test!(
    history_input_fifo_import_history_retains_exact_blocking_source_and_named_assertion,
    "tests/test_native_history_inputs.py::test_fifo_import_history_retains_exact_blocking_source_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"first_run_failure\",\"test\":\"fifo_import_matches_original_refusal_continuation_and_retry\",\"symptom\":\"First-run FIFO refusal differs\",\"commit\":\"f13c271b158663ca44091efba7539a11b6542c59\",\"platform\":\"linux\",\"files\":{\"engine-rust/src/storage/first_run.rs\":\"ae6dcc19e630d1b110837c55f4726fe300309470772945a6904a634692f59b43\"}}",
        )?;
        assert_eq!(registry.current[35], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 35], expected);
        Ok(())
    }
);
source_test!(
    history_input_import_attribute_history_retains_exact_prior_source_and_named_assertion,
    "tests/test_native_history_inputs.py::test_import_attribute_history_retains_exact_prior_source_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"first_run_metadata_parity\",\"test\":\"imported_attributes_match_original_without_replacing_destination_metadata\",\"symptom\":\"First-run extended attributes differ\",\"commit\":\"ce84984c7ea2da7cb78ca8a27917fa051c96ab5d\",\"platform\":\"linux\",\"files\":{\"engine-rust/src/storage/first_run.rs\":\"fe024803ade4378889782ce206386660c92a790bde9d1d24ca0451080cb470d4\"}}",
        )?;
        assert_eq!(registry.current[36], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 36], expected);
        Ok(())
    }
);
source_test!(
    history_input_cli_positional_history_retains_prior_parser_and_named_assertion,
    "tests/test_native_history_inputs.py::test_cli_positional_history_retains_prior_parser_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"cli_registry_parity\",\"test\":\"positional_values_preserve_original_admission_and_option_boundaries\",\"symptom\":\"CLI positional token admission differs\",\"commit\":\"f0bc556e932e75c6034c9282d7ba6b8c4b573bdf\",\"files\":{\"engine-rust/src/cli/mod.rs\":\"43b939bcaeb5acef515b2b22cef85f85842a1667547c17811fca17102935aba5\"}}",
        )?;
        assert_eq!(registry.current[37], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 37], expected);
        Ok(())
    }
);
source_test!(
    history_input_short_help_history_retains_prior_normalizer_and_named_assertion,
    "tests/test_native_history_inputs.py::test_short_help_history_retains_prior_normalizer_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"cli_registry_parity\",\"test\":\"short_help_preserves_original_explicit_tail_admission\",\"symptom\":\"CLI short-help token admission differs\",\"commit\":\"a104df072bf48ca200440d5f40298a9ce9b9f24d\",\"files\":{\"engine-rust/src/cli/option_tokens.rs\":\"dd7ca91090c4cdbfb2e2c17f5f9108432595f3bd28e7356708b4284d938634da\"}}",
        )?;
        assert_eq!(registry.current[38], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 38], expected);
        Ok(())
    }
);
source_test!(
    history_input_variadic_history_retains_coherent_prior_parser_and_named_assertion,
    "tests/test_native_history_inputs.py::test_variadic_history_retains_coherent_prior_parser_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"cli_registry_parity\",\"test\":\"variadic::variadic_positionals_preserve_original_option_separated_runs\",\"symptom\":\"CLI variadic run admission differs\",\"commit\":\"e382b5432e7a2bceab89849b037e2b3727fc76c8\",\"files\":{\"engine-rust/src/cli/mod.rs\":\"7aa0a8e0303f0e674891d554bfcf38e394569ac6e1219b2c11ad85785be92a54\",\"engine-rust/src/cli/positional_tokens.rs\":\"cfe23726c81a6a2f04c827c6dc68f4da68bfcc00cffce2c84bec95847d63b093\"}}",
        )?;
        assert_eq!(registry.current[39], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 39], expected);
        Ok(())
    }
);
source_test!(
    history_input_log_retention_history_retains_prior_sink_and_named_assertion,
    "tests/test_native_history_inputs.py::test_log_retention_history_retains_prior_sink_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"logging_parity\",\"test\":\"retention::run_retention_matches_original_mtime_order_and_namespace\",\"symptom\":\"Runtime log retention differs\",\"commit\":\"3be53623c6dbd55e22ab23dcbe28dc7c4852053d\",\"files\":{\"engine-rust/src/runtime_log.rs\":\"78db62948c0519ffadbcf154574caac10078bee0577c6269a0161e4191743590\"}}",
        )?;
        assert_eq!(registry.current[40], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 40], expected);
        Ok(())
    }
);
source_test!(
    history_input_nested_copy_history_retains_prior_importer_and_named_assertion,
    "tests/test_native_history_inputs.py::test_nested_copy_history_retains_prior_importer_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"first_run_parity\",\"test\":\"nested_copy::new_subtrees_preserve_original_nested_lock_entries\",\"symptom\":\"First-run nested copy differs\",\"commit\":\"c44a948d6f194368fc8316efb127b46cfd80d9d7\",\"files\":{\"engine-rust/src/storage/first_run.rs\":\"a7e702675adb3f12e7495d625d0cb06aa400c594d4997526f8027b6dbb578f73\"}}",
        )?;
        assert_eq!(registry.current[41], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 41], expected);
        Ok(())
    }
);
source_test!(
    history_input_unicode_representation_history_retains_prior_formatter_and_named_assertion,
    "tests/test_native_history_inputs.py::test_unicode_representation_history_retains_prior_formatter_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"validation_parity\",\"test\":\"unicode_representation_matches_original_public_validation_and_argv\",\"symptom\":\"Unicode representation differs\",\"commit\":\"57f0d46506f11248640c2fa2095a05e087c20c65\",\"files\":{\"engine-rust/src/validation.rs\":\"5397a1882c0a5cdf867e4d21a353322da76129db2d982fe6093c4eeb9c7d0591\"}}",
        )?;
        assert_eq!(registry.current[42], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 42], expected);
        Ok(())
    }
);
source_test!(
    history_input_numeric_representation_history_retains_prior_formatter_and_named_assertion,
    "tests/test_native_history_inputs.py::test_numeric_representation_history_retains_prior_formatter_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"validation_parity\",\"test\":\"finite_numbers_match_original_validation_and_mcp_representation\",\"symptom\":\"Numeric representation differs\",\"commit\":\"65e9dbd4b9a1b15ee76a27d7cf3947b18455c21d\",\"files\":{\"engine-rust/src/validation.rs\":\"01a2a4718ff2792ffd80ab5f130261007409ed58e3aea1ffaf28d8eae3327158\"}}",
        )?;
        assert_eq!(registry.current[43], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 43], expected);
        Ok(())
    }
);
source_test!(
    history_input_fraction_admission_history_retains_prior_comparison_and_named_assertion,
    "tests/test_native_history_inputs.py::test_fraction_admission_history_retains_prior_comparison_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"validation_parity\",\"test\":\"permitted_fraction_spellings_match_original_numeric_admission\",\"symptom\":\"Fraction numeric admission differs\",\"commit\":\"75acb553e30728dd58472facc5acd09791de38a1\",\"files\":{\"engine-rust/src/validation.rs\":\"d67e56a33198c01e40b4e2fe0ddcfbb4429dd83c4de45c4972e65ed21b486066\"}}",
        )?;
        assert_eq!(registry.current[44], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 44], expected);
        Ok(())
    }
);
source_test!(
    history_input_import_access_time_history_retains_prior_snapshot_and_named_assertion,
    "tests/test_native_history_inputs.py::test_import_access_time_history_retains_prior_snapshot_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"first_run_metadata_parity\",\"test\":\"direct_copy_access_times_match_original_without_changing_copytree_snapshots\",\"symptom\":\"First-run access-time copy policy differs\",\"commit\":\"b6bab47c1f34872e11061586ae5c5291cfe1c056\",\"platform\":\"linux\",\"files\":{\"engine-rust/src/storage/first_run.rs\":\"7f07dcc91641071ebb5e5576c3e0bcf3186400fd9e0d199f227231503a2ea002\"}}",
        )?;
        assert_eq!(registry.current[45], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 45], expected);
        Ok(())
    }
);
source_test!(
    history_input_container_conversion_history_retains_prior_formatter_and_named_assertion,
    "tests/test_native_history_inputs.py::test_container_conversion_history_retains_prior_formatter_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"conversion_parity\",\"test\":\"container_text_conversion_matches_original_profiles_and_roundtrips\",\"symptom\":\"Container text conversion differs\",\"commit\":\"12a76ec9f205fd6c2e760eaf28bdd64aa0105c8b\",\"files\":{\"engine-rust/src/conversion.rs\":\"c52af49700c56939aa482ba1c155aed5b861258e7404cf4c5069df70e65ceef4\"}}",
        )?;
        assert_eq!(registry.current[46], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 46], expected);
        Ok(())
    }
);
source_test!(
    history_input_copytree_error_history_retains_prior_importer_and_named_assertion,
    "tests/test_native_history_inputs.py::test_copytree_error_history_retains_prior_importer_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"first_run_failure\",\"test\":\"copytree_errors::new_subtree_errors_match_original_continuation_summaries_and_retry\",\"symptom\":\"First-run copytree failure aggregation differs\",\"commit\":\"da41e44368222813bb929cf579fd915baeeea7b1\",\"platform\":\"linux\",\"files\":{\"engine-rust/src/storage/first_run.rs\":\"6e865c0e82abea1c6cdb7ad793d5bc2eab39c05bedc37cfbd311d92854e5da1b\"}}",
        )?;
        assert_eq!(registry.current[47], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 47], expected);
        Ok(())
    }
);
source_test!(
    history_input_import_profile_count_history_retains_prior_scan_and_named_assertion,
    "tests/test_native_history_inputs.py::test_import_profile_count_history_retains_prior_scan_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"first_run_failure\",\"test\":\"profile_count::advisory_profile_count_matches_original_when_destination_scan_is_denied\",\"symptom\":\"First-run advisory profile count differs\",\"commit\":\"c2615a4e5e90afc60c943142818e37906a952a47\",\"platform\":\"linux\",\"files\":{\"engine-rust/src/storage/first_run.rs\":\"81ebc1b78e948ce3c23a275db4b69e15c2555c6c7f2938b5d1a80b447f8669f8\"}}",
        )?;
        assert_eq!(registry.current[48], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 48], expected);
        Ok(())
    }
);
source_test!(
    history_input_version_flag_history_retains_exact_prior_registry_and_named_assertion,
    "tests/test_native_history_inputs.py::test_version_flag_history_retains_exact_prior_registry_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"cli_registry_parity\",\"test\":\"version::version_flag_matches_original_root_scope_and_value_admission\",\"symptom\":\"CLI version flag admission differs\",\"commit\":\"a071ff7f2ceb55cd7ed9f05198692e9ac19bcb17\",\"files\":{\"engine-rust/src/cli/mod.rs\":\"6d6cfccdbed4a0ebf60ed438d17f0ab5d5e688813a61162287e360d4feb4150a\"}}",
        )?;
        assert_eq!(registry.current[49], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 49], expected);
        Ok(())
    }
);
source_test!(
    history_input_deferred_display_history_retains_prior_registry_and_named_assertion,
    "tests/test_native_history_inputs.py::test_deferred_display_history_retains_prior_registry_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"cli_registry_parity\",\"test\":\"version::version_flag_matches_original_root_scope_and_value_admission\",\"symptom\":\"CLI version flag admission differs\",\"commit\":\"77cab4fb18ac3dcace76a34bc54c13d3239a70ed\",\"files\":{\"engine-rust/src/cli/mod.rs\":\"1a838cce524b7b46713ea101e3b45160b3f34a979b44ed3e89a2ec7a058000cb\"}}",
        )?;
        assert_eq!(registry.current[50], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 50], expected);
        Ok(())
    }
);
source_test!(
    history_input_vault_text_history_retains_prior_authenticated_decoder_and_named_assertion,
    "tests/test_native_history_inputs.py::test_vault_text_history_retains_prior_authenticated_decoder_and_named_assertion",
    {
        let registry = history::Registry::read()?;
        assert_eq!(registry.current.len(), 52);
        let expected: history::Case = serde_json::from_str(
            "{\"target\":\"vault_parity\",\"test\":\"crypto_text_admission_matches_original_and_new_tokens_reopen_there\",\"symptom\":\"Crypto text admission differs\",\"commit\":\"dccccf2ad40820606b1c74f4de951cb163e0fe6a\",\"files\":{\"engine-rust/src/vault/mod.rs\":\"71150ac122b5f5c26199ff9fcac4c042db5fcd403beb89f1a809dd0da925de3b\"}}",
        )?;
        assert_eq!(registry.current[51], expected);
        let cases = registry.cases()?;
        assert_eq!(cases[cases.len() - 52 + 51], expected);
        Ok(())
    }
);
fn fixture(temp: &Temp) -> ci::Result<history::Case> {
    temp.write("engine-rust/src/models.rs", b"current source\n")?;
    let mut case = history::Registry::read()?.cases()?[0].clone();
    case.files = BTreeMap::from([(
        "engine-rust/src/models.rs".into(),
        ci::sha(b"historical source\n"),
    )]);
    case.companions.clear();
    Ok(case)
}
fn reuse(change: &str) -> TestResult {
    let temp = Temp::new("reused-input")?;
    let case = fixture(&temp)?;
    let source = temp.0.join("engine-rust/src/models.rs");
    let mut reads = 0;
    let mut proofs = 0;
    for invocation in 0..if change == "none" { 2 } else { 1 } {
        let mut inputs = BTreeMap::new();
        for index in 0..2 {
            let mut selected = case.clone();
            if index == 1 && change == "hash" {
                selected
                    .files
                    .values_mut()
                    .for_each(|hash| *hash = "0".repeat(64));
            }
            let result = history::SourceSwap::prepare_checked(
                &temp.0,
                &selected,
                &mut inputs,
                |_, _| {
                    reads += 1;
                    Ok(b"historical source\n".to_vec())
                },
                |path| {
                    if index == 1 && change == "linked" {
                        return Err(io::Error::other(
                            "Historical regression source is not a regular maintained file",
                        )
                        .into());
                    }
                    ci::regular(path, false)?;
                    Ok(())
                },
            );
            if index == 1 && change != "none" {
                assert!(result.is_err());
                break;
            }
            let mut swap = result?;
            assert_eq!(fs::read(&source)?, b"historical source\n");
            proofs += 1;
            swap.restore()?;
            assert_eq!(fs::read(&source)?, b"current source\n");
            if index == 0 && change == "missing" {
                fs::remove_file(&source)?;
            }
        }
        if change == "none" {
            assert_eq!(proofs, 2 * (invocation + 1));
            assert_eq!(reads, invocation + 1);
        }
    }
    if change != "none" {
        assert_eq!(proofs, 1);
        assert_eq!(reads, if change == "hash" { 2 } else { 1 });
    }
    if change != "missing" {
        assert_eq!(fs::read(source)?, b"current source\n");
    }
    Ok(())
}
source_cases!("tests/test_native_history_inputs.py::test_reused_inputs_revalidate_integrity_and_current_paths",reuse,[history_input_reuse=>("none","none"),history_input_hash=>("hash","hash"),history_input_missing=>("missing","missing"),history_input_linked=>("linked","linked")]);
fn platform(damage: &str) -> TestResult {
    let temp = Temp::new("explicit-history")?;
    let mut case = fixture(&temp)?;
    case.platform = Some(
        if damage == "platform" {
            "other"
        } else {
            "fixture-host"
        }
        .into(),
    );
    let cases = if damage == "empty" {
        Vec::new()
    } else {
        vec![case]
    };
    let admitted = history::explicit_cases(&cases, "fixture-host", damage != "dirty");
    if damage == "None" {
        admitted?;
        let mut reads = 0;
        let mut inputs = BTreeMap::new();
        let mut swap =
            history::SourceSwap::prepare_with(&temp.0, &cases[0], &mut inputs, |_, _| {
                reads += 1;
                Ok(b"historical source\n".to_vec())
            })?;
        assert_eq!(reads, 1);
        swap.restore()?;
    } else {
        assert!(admitted.is_err());
    }
    assert_eq!(
        fs::read(temp.0.join("engine-rust/src/models.rs"))?,
        b"current source\n"
    );
    Ok(())
}
source_cases!("tests/test_native_history_inputs.py::test_explicit_platform_proofs_keep_exact_admission_and_do_not_run_other_cases",platform,[history_explicit_good=>("None","None"),history_explicit_empty=>("empty","empty"),history_explicit_platform=>("platform","platform"),history_explicit_dirty=>("dirty","dirty")]);
fn timings(previous: &str) -> TestResult {
    let temp = Temp::new("timing-retention")?;
    let timing = temp.0.join("engine-rust/target/cargo-timings");
    let path = temp.write(
        "engine-rust/target/cargo-timings/cargo-timing.html",
        b"<html>previous build timing</html>",
    )?;
    let owned = temp.0.join("owned-temp");
    fs::create_dir(&owned)?;
    if previous == "absent" {
        fs::remove_file(&path)?;
        fs::remove_dir(&timing)?;
    }
    let retained = history::Timings::prepare_checked(
        &temp.0,
        &owned,
        |from, to| {
            if previous == "rename" {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Fixture timing retention denied",
                )
                .into());
            }
            fs::rename(from, to)?;
            Ok(())
        },
        |path| {
            if previous == "linked" {
                return Err(io::Error::other(
                    "Historical timing directory has uncertain ownership",
                )
                .into());
            }
            ci::regular(path, true)?;
            Ok(())
        },
    );
    let output = temp.0.join("step-output");
    if ["rename", "linked"].contains(&previous) {
        assert!(retained.is_err());
        assert_eq!(fs::read(path)?, b"<html>previous build timing</html>");
        assert!(!output.exists());
        return Ok(());
    }
    let retained = retained?;
    assert!(!timing.exists());
    fs::create_dir(&timing)?;
    fs::write(
        timing.join("cargo-timing.html"),
        b"<html>current build timing</html>",
    )?;
    retained.publish(&output)?;
    assert_eq!(fs::read_to_string(output)?, "timings_ready=true\n");
    assert_eq!(retained.previous.is_some(), previous == "present");
    if let Some(previous) = retained.previous {
        assert_eq!(
            fs::read(previous.join("cargo-timing.html"))?,
            b"<html>previous build timing</html>"
        );
    }
    Ok(())
}
source_cases!("tests/test_native_history_inputs.py::test_history_timings_preserve_previous_reports_and_refuse_uncertain_paths",timings,[history_timing_absent=>("absent","absent"),history_timing_present=>("present","present"),history_timing_rename=>("rename","rename"),history_timing_linked=>("linked","linked")]);
