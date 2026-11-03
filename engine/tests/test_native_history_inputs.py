"""Immutable historical inputs are reused without reusing proof verdicts."""
import hashlib
import logging
import sys
from types import SimpleNamespace

import pytest

from test_native_regressions import load


@pytest.mark.parametrize('change', ['none', 'hash', 'missing', 'linked'])
def test_reused_inputs_revalidate_integrity_and_current_paths(tmp_path, monkeypatch, change):
    module = load()
    path = tmp_path / 'engine-rust/src/models.rs'
    path.parent.mkdir(parents=True)
    current, historical = b'current source\n', b'historical source\n'
    path.write_bytes(current)
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(historical).hexdigest()}, companions={})
    second = dict(case, files=dict(case['files']))
    if change == 'hash':
        second['files']['engine-rust/src/models.rs'] = '0' * 64
    monkeypatch.setattr(module, 'REGRESSIONS', (case, second))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setitem(sys.modules, 'baseline_source',
                        SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    reads, proofs = [], []

    def git(root, *arguments):
        reads.append(arguments)
        return historical

    def run(command, wall, idle, logger):
        proofs.append(command)
        assert path.read_bytes() == historical
        assert (wall, idle) == (180, 90)
        logger.info('running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 101

    class Logger(logging.Logger):
        def info(self, message, *args, **kwargs):
            super().info(message, *args, **kwargs)
            if message.startswith('Historical regression caught') and len(proofs) == 1:
                assert path.read_bytes() == current
                if change == 'missing':
                    path.unlink()
                elif change == 'linked':
                    original_link = type(path).is_symlink
                    monkeypatch.setattr(type(path), 'is_symlink', lambda item:
                                        item == path or original_link(item))

    monkeypatch.setattr(module, 'git', git)
    monkeypatch.setattr(module.subprocess, 'run', lambda *args, **kwargs: None)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    logger = Logger('historical-input-fixture', level=logging.INFO)
    if change == 'none':
        module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(proofs) == 2 and len(reads) == 1
        module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(proofs) == 4 and len(reads) == 2, 'Inputs leaked across prove calls'
    else:
        with pytest.raises(ValueError, match='integrity differs|regular maintained file'):
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(proofs) == 1
        assert len(reads) == (2 if change == 'hash' else 1)
    assert logger.handlers == []
    if change != 'missing':
        assert path.read_bytes() == current


@pytest.mark.timeout(10)
@pytest.mark.parametrize('damage', [None, 'empty', 'platform', 'dirty'])
def test_explicit_platform_proofs_keep_exact_admission_and_do_not_run_other_cases(
        tmp_path, monkeypatch, damage):
    module = load()
    path = tmp_path / 'engine-rust/src/ownership/identity.rs'
    path.parent.mkdir(parents=True)
    current, historical = b'current identity\n', b'previous unsupported identity\n'
    path.write_bytes(current)
    case = dict(module.REGRESSIONS[0], platform='fixture-host', files={
        'engine-rust/src/ownership/identity.rs': hashlib.sha256(historical).hexdigest()},
        companions={})
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform='fixture-host'))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setitem(sys.modules, 'baseline_source',
                        SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    monkeypatch.setattr(module.subprocess, 'run', lambda *args, **kwargs: None)
    calls = []

    def git(root, *arguments):
        calls.append(arguments)
        if arguments[0] == 'diff':
            if damage == 'dirty':
                raise module.subprocess.CalledProcessError(1, ['git', 'diff'])
            return b''
        assert arguments == ('show', f"{case['commit']}:engine-rust/src/ownership/identity.rs")
        return historical

    def run(command, wall, idle, logger):
        assert path.read_bytes() == historical and (wall, idle) == (180, 90)
        logger.info('running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 101

    monkeypatch.setattr(module, 'git', git)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    logger = logging.Logger('explicit-platform-proof', level=logging.INFO)
    selected = () if damage == 'empty' else (dict(case, platform='other') if damage == 'platform' else case,)
    if damage:
        with pytest.raises((ValueError, module.subprocess.CalledProcessError)):
            module.prove(tmp_path, tmp_path / 'original', logger, cases=selected)
        assert not any(call[0] == 'show' for call in calls)
    else:
        module.prove(tmp_path, tmp_path / 'original', logger, cases=selected)
        assert calls == [('diff', '--exit-code', 'HEAD', '--', 'engine-rust/src/ownership/identity.rs'),
                         ('show', f"{case['commit']}:engine-rust/src/ownership/identity.rs")]
    assert path.read_bytes() == current and logger.handlers == []


@pytest.mark.timeout(10)
@pytest.mark.parametrize('previous', ['absent', 'present', 'rename', 'linked'])
def test_history_timings_preserve_previous_reports_and_refuse_uncertain_paths(tmp_path, monkeypatch, previous):
    module = load()
    timing = tmp_path / 'engine-rust/target/cargo-timings'
    owned = tmp_path / 'owned-temp'
    owned.mkdir()
    monkeypatch.setenv('NUMERA_TEST_TEMP', str(owned))
    output = tmp_path / 'step-output'
    monkeypatch.setenv('GITHUB_OUTPUT', str(output))
    timing.mkdir(parents=True)
    old = b'<html>previous build timing</html>'
    (timing / 'cargo-timing.html').write_bytes(old)
    if previous == 'absent':
        (timing / 'cargo-timing.html').unlink()
        timing.rmdir()
    elif previous == 'rename':
        rename = type(timing).rename

        def denied(path, destination):
            if path == timing:
                raise PermissionError('Fixture timing retention denied')
            return rename(path, destination)

        monkeypatch.setattr(type(timing), 'rename', denied)
    elif previous == 'linked':
        is_symlink = type(timing).is_symlink
        monkeypatch.setattr(type(timing), 'is_symlink',
                            lambda path: path == timing or is_symlink(path))
    source = tmp_path / 'engine-rust/src/models.rs'
    source.parent.mkdir()
    source.write_bytes(b'current source')
    historical = b'historical source'
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(historical).hexdigest()}, companions={})
    monkeypatch.setattr(module, 'REGRESSIONS', (case,))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setattr(module, 'git', lambda *args: historical)
    monkeypatch.setitem(sys.modules, 'baseline_source', SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    calls = []

    def compile(command, **kwargs):
        calls.append(command)
        assert '--timings' in command and not timing.exists()
        timing.mkdir()
        (timing / 'cargo-timing.html').write_bytes(b'<html>current build timing</html>')
        raise module.subprocess.CalledProcessError(1, command)

    monkeypatch.setattr(module.subprocess, 'run', compile)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=lambda *args: pytest.fail('Unexpected proof')))
    logger = logging.Logger('historical-timing-fixture', level=logging.INFO)
    if previous in ('rename', 'linked'):
        with pytest.raises((ValueError, PermissionError), match='Historical timing directory|Fixture timing retention denied'):
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert calls == [] and (timing / 'cargo-timing.html').read_bytes() == old
        assert not output.exists(), 'Old timing reports were advertised as fresh'
    else:
        with pytest.raises(module.subprocess.CalledProcessError):
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert len(calls) == 1
        assert output.read_text(encoding='utf-8') == 'timings_ready=true\n'
        assert (timing / 'cargo-timing.html').read_bytes() == b'<html>current build timing</html>'
        retained = list(owned.glob('native-history-timings-*/previous/cargo-timing.html'))
        assert len(retained) == (1 if previous == 'present' else 0)
        if retained:
            assert retained[0].read_bytes() == old
    assert source.read_bytes() == b'current source' and logger.handlers == []


@pytest.mark.timeout(10)
def test_directory_metadata_history_retains_exact_prior_source_and_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[31]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'first_run_parity',
        'test': 'imported_new_directories_preserve_original_metadata_without_replacing_existing_modes',
        'symptom': 'First-run directory metadata differs',
        'commit': '1a46f71178be5c482ecad04f14666eadaa52dc0d',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '6a9afa7356a02b123d13ebf8d5742a67e4d6e2bf8abff0d1cb8f2459a94c5670'},
    }
    assert module.REGRESSIONS[-19] == case


@pytest.mark.timeout(10)
def test_cli_integer_history_retains_exact_prior_source_and_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[32]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'integer_options_preserve_original_digit_bound_before_zero_normalization',
        'symptom': 'CLI integer digit admission differs',
        'commit': 'ff56fc25f25a4ad9874e644b85295ccd622fd34f',
        'files': {'engine-rust/src/cli/definitions.rs':
                  '62577821c1120e9518a30277bb165d4e77c103ef5803c06157a2a4bc4922c4a7'},
    }
    assert module.REGRESSIONS[-18] == case


@pytest.mark.timeout(10)
def test_hint_peer_disconnect_history_retains_compile_compatible_source_and_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[33]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'browser_hints_parity',
        'test': 'hint_response_peer_disconnects_preserve_other_io_failures',
        'symptom': 'Hint response peer disconnect escaped',
        'commit': 'f173f3ae59c91fc9524507f91fe6d936b2c4a0d4',
        'files': {'engine-rust/tests/common/hint_site.rs':
                  '4dcfdaa91e4079186c5b7c9268ad50efa06c7f6f5782170c4af8fa2172a879ae'},
    }
    assert module.REGRESSIONS[-17] == case


@pytest.mark.timeout(10)
def test_cli_option_token_history_retains_prior_parser_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[34]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'separate_option_values_match_original_token_classification',
        'symptom': 'CLI option token admission differs',
        'commit': 'c299ef506c593623bfd0646b3d2efe1d83596532',
        'files': {'engine-rust/src/cli/mod.rs':
                  'c534342a6f7db08b99641bd52e02e729d3abb68a1e78257b0c502c077479fcd8'},
    }
    assert module.REGRESSIONS[-16] == case


@pytest.mark.timeout(10)
def test_fifo_import_history_retains_exact_blocking_source_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[35]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'first_run_failure',
        'test': 'fifo_import_matches_original_refusal_continuation_and_retry',
        'symptom': 'First-run FIFO refusal differs',
        'commit': 'f13c271b158663ca44091efba7539a11b6542c59',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  'ae6dcc19e630d1b110837c55f4726fe300309470772945a6904a634692f59b43'},
    }
    assert module.REGRESSIONS[-15] == case


@pytest.mark.timeout(10)
def test_import_attribute_history_retains_exact_prior_source_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[36]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'first_run_metadata_parity',
        'test': 'imported_attributes_match_original_without_replacing_destination_metadata',
        'symptom': 'First-run extended attributes differ',
        'commit': 'ce84984c7ea2da7cb78ca8a27917fa051c96ab5d',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  'fe024803ade4378889782ce206386660c92a790bde9d1d24ca0451080cb470d4'},
    }
    assert module.REGRESSIONS[-14] == case


@pytest.mark.timeout(10)
def test_cli_positional_history_retains_prior_parser_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[37]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'positional_values_preserve_original_admission_and_option_boundaries',
        'symptom': 'CLI positional token admission differs',
        'commit': 'f0bc556e932e75c6034c9282d7ba6b8c4b573bdf',
        'files': {'engine-rust/src/cli/mod.rs':
                  '43b939bcaeb5acef515b2b22cef85f85842a1667547c17811fca17102935aba5'},
    }
    assert module.REGRESSIONS[-13] == case


@pytest.mark.timeout(10)
def test_short_help_history_retains_prior_normalizer_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[38]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'short_help_preserves_original_explicit_tail_admission',
        'symptom': 'CLI short-help token admission differs',
        'commit': 'a104df072bf48ca200440d5f40298a9ce9b9f24d',
        'files': {'engine-rust/src/cli/option_tokens.rs':
                  'dd7ca91090c4cdbfb2e2c17f5f9108432595f3bd28e7356708b4284d938634da'},
    }
    assert module.REGRESSIONS[-12] == case


@pytest.mark.timeout(10)
def test_variadic_history_retains_coherent_prior_parser_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[39]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'variadic::variadic_positionals_preserve_original_option_separated_runs',
        'symptom': 'CLI variadic run admission differs',
        'commit': 'e382b5432e7a2bceab89849b037e2b3727fc76c8',
        'files': {'engine-rust/src/cli/mod.rs':
                  '7aa0a8e0303f0e674891d554bfcf38e394569ac6e1219b2c11ad85785be92a54',
                  'engine-rust/src/cli/positional_tokens.rs':
                  'cfe23726c81a6a2f04c827c6dc68f4da68bfcc00cffce2c84bec95847d63b093'},
    }
    assert module.REGRESSIONS[-11] == case


@pytest.mark.timeout(10)
def test_log_retention_history_retains_prior_sink_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[40]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'logging_parity',
        'test': 'retention::run_retention_matches_original_mtime_order_and_namespace',
        'symptom': 'Runtime log retention differs',
        'commit': '3be53623c6dbd55e22ab23dcbe28dc7c4852053d',
        'files': {'engine-rust/src/runtime_log.rs':
                  '78db62948c0519ffadbcf154574caac10078bee0577c6269a0161e4191743590'},
    }
    assert module.REGRESSIONS[-10] == case


@pytest.mark.timeout(10)
def test_nested_copy_history_retains_prior_importer_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[41]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'first_run_parity',
        'test': 'nested_copy::new_subtrees_preserve_original_nested_lock_entries',
        'symptom': 'First-run nested copy differs',
        'commit': 'c44a948d6f194368fc8316efb127b46cfd80d9d7',
        'files': {'engine-rust/src/storage/first_run.rs':
                  'a7e702675adb3f12e7495d625d0cb06aa400c594d4997526f8027b6dbb578f73'},
    }
    assert module.REGRESSIONS[-9] == case


@pytest.mark.timeout(10)
def test_unicode_representation_history_retains_prior_formatter_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[42]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'validation_parity',
        'test': 'unicode_representation_matches_original_public_validation_and_argv',
        'symptom': 'Unicode representation differs',
        'commit': '57f0d46506f11248640c2fa2095a05e087c20c65',
        'files': {'engine-rust/src/validation.rs':
                  '5397a1882c0a5cdf867e4d21a353322da76129db2d982fe6093c4eeb9c7d0591'},
    }
    assert module.REGRESSIONS[-8] == case


@pytest.mark.timeout(10)
def test_numeric_representation_history_retains_prior_formatter_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[43]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'validation_parity',
        'test': 'finite_numbers_match_original_validation_and_mcp_representation',
        'symptom': 'Numeric representation differs',
        'commit': '65e9dbd4b9a1b15ee76a27d7cf3947b18455c21d',
        'files': {'engine-rust/src/validation.rs':
                  '01a2a4718ff2792ffd80ab5f130261007409ed58e3aea1ffaf28d8eae3327158'},
    }
    assert module.REGRESSIONS[-7] == case


@pytest.mark.timeout(10)
def test_fraction_admission_history_retains_prior_comparison_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[44]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'validation_parity',
        'test': 'permitted_fraction_spellings_match_original_numeric_admission',
        'symptom': 'Fraction numeric admission differs',
        'commit': '75acb553e30728dd58472facc5acd09791de38a1',
        'files': {'engine-rust/src/validation.rs':
                  'd67e56a33198c01e40b4e2fe0ddcfbb4429dd83c4de45c4972e65ed21b486066'},
    }
    assert module.REGRESSIONS[-6] == case


@pytest.mark.timeout(10)
def test_import_access_time_history_retains_prior_snapshot_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[45]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'first_run_metadata_parity',
        'test': 'direct_copy_access_times_match_original_without_changing_copytree_snapshots',
        'symptom': 'First-run access-time copy policy differs',
        'commit': 'b6bab47c1f34872e11061586ae5c5291cfe1c056',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '7f07dcc91641071ebb5e5576c3e0bcf3186400fd9e0d199f227231503a2ea002'},
    }
    assert module.REGRESSIONS[-5] == case


@pytest.mark.timeout(10)
def test_container_conversion_history_retains_prior_formatter_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[46]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'conversion_parity',
        'test': 'container_text_conversion_matches_original_profiles_and_roundtrips',
        'symptom': 'Container text conversion differs',
        'commit': '12a76ec9f205fd6c2e760eaf28bdd64aa0105c8b',
        'files': {'engine-rust/src/conversion.rs':
                  'c52af49700c56939aa482ba1c155aed5b861258e7404cf4c5069df70e65ceef4'},
    }
    assert module.REGRESSIONS[-4] == case


@pytest.mark.timeout(10)
def test_copytree_error_history_retains_prior_importer_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[47]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'first_run_failure',
        'test': 'copytree_errors::new_subtree_errors_match_original_continuation_summaries_and_retry',
        'symptom': 'First-run copytree failure aggregation differs',
        'commit': 'da41e44368222813bb929cf579fd915baeeea7b1',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '6e865c0e82abea1c6cdb7ad793d5bc2eab39c05bedc37cfbd311d92854e5da1b'},
    }
    assert module.REGRESSIONS[-3] == case


@pytest.mark.timeout(10)
def test_import_profile_count_history_retains_prior_scan_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[48]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'first_run_failure',
        'test': 'profile_count::advisory_profile_count_matches_original_when_destination_scan_is_denied',
        'symptom': 'First-run advisory profile count differs',
        'commit': 'c2615a4e5e90afc60c943142818e37906a952a47',
        'platform': 'linux',
        'files': {'engine-rust/src/storage/first_run.rs':
                  '81ebc1b78e948ce3c23a275db4b69e15c2555c6c7f2938b5d1a80b447f8669f8'},
    }
    assert module.REGRESSIONS[-2] == case


@pytest.mark.timeout(10)
def test_version_flag_history_retains_exact_prior_registry_and_named_assertion():
    module = load()
    case = module.CURRENT_CALLER_REGRESSIONS[49]
    assert len(module.CURRENT_CALLER_REGRESSIONS) == 50
    assert case == {
        'target': 'cli_registry_parity',
        'test': 'version::version_flag_matches_original_root_scope_and_value_admission',
        'symptom': 'CLI version flag admission differs',
        'commit': 'a071ff7f2ceb55cd7ed9f05198692e9ac19bcb17',
        'files': {'engine-rust/src/cli/mod.rs':
                  '6d6cfccdbed4a0ebf60ed438d17f0ab5d5e688813a61162287e360d4feb4150a'},
    }
    assert module.REGRESSIONS[-1] == case
