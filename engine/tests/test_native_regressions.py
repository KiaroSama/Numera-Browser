"""Historical failure proof rejects unrelated failures and restores fixed source."""
import ast
import hashlib
import importlib.util
import logging
from pathlib import Path
from types import SimpleNamespace
import sys

import pytest

ROOT = Path(__file__).resolve().parents[2]


def load():
    spec = importlib.util.spec_from_file_location(
        'native_regressions_fixture', ROOT / 'scripts/ci/native_regressions.py')
    module = importlib.util.module_from_spec(spec)
    helper_spec = importlib.util.spec_from_file_location(
        'native_settings_history', ROOT / 'scripts/ci/native_settings_history.py')
    helper = importlib.util.module_from_spec(helper_spec)
    helper_spec.loader.exec_module(helper)
    from unittest.mock import patch

    cases_spec = importlib.util.spec_from_file_location(
        'native_regression_cases', ROOT / 'scripts/ci/native_regression_cases.py')
    cases = importlib.util.module_from_spec(cases_spec)
    cases_spec.loader.exec_module(cases)
    with patch.dict(sys.modules, {'native_settings_history': helper,
                                 'native_regression_cases': cases}):
        spec.loader.exec_module(module)
    return module


def test_option_reader_companions_preserve_selected_defects_and_prior_snapshots():
    module = load()
    current = {(case['target'], case['test'], case['commit']): case for case in module._STORAGE_REGRESSIONS}
    original_tree = ast.parse(
        (ROOT / 'scripts/ci/native_regression_cases.py').read_text(encoding='utf-8'))
    first_registry = next(node.value for node in original_tree.body
                          if isinstance(node, ast.Assign)
                          and any(isinstance(target, ast.Name)
                                  and target.id == 'REGRESSIONS' for target in node.targets))
    original = ast.literal_eval(first_registry)
    assert len(module._STORAGE_REGRESSIONS) == len(original) + 1
    grouped = 0
    for before in original:
        after = current[(before['target'], before['test'], before['commit'])]
        assert {key: after[key] for key in before if key != 'companions'} == {
            key: value for key, value in before.items() if key != 'companions'}
        for name, source in before.get('companions', {}).items():
            assert after['companions'][name] == source
        touched = set(before['files']) | set(before.get('companions', {}))
        if touched & module._STORAGE_READERS.keys():
            grouped += 1
            assert set(module._STORAGE_READERS) <= set(after['files']) | set(after['companions'])
            assert not set(after['files']) & set(after['companions'])
        else:
            assert after == before
    assert grouped == 9
    added = module._STORAGE_REGRESSIONS[-1]
    assert added['test'] == 'settings_errors::storage_settings_errors_match_original_lazy_reads_and_preserve_bytes'
    assert added['files'] == module._STORAGE_READERS
    assert added['commit'] == module._STORAGE_READERS_COMMIT
    assert added['symptom'] == 'Storage settings admission differs'


def test_settings_signature_companions_preserve_all_existing_proofs():
    module = load()
    history = module.native_settings_history
    previous = module._STORAGE_REGRESSIONS
    assert len(module._SETTINGS_REGRESSIONS) == len(previous) + 2
    for before, after in zip(previous, module._SETTINGS_REGRESSIONS):
        assert {key: after[key] for key in before if key != 'companions'} == {
            key: value for key, value in before.items() if key != 'companions'}
        for name, source in before.get('companions', {}).items():
            assert after['companions'][name] == source
        touched = before['files'].keys() | before.get('companions', {}).keys()
        if touched & (history.SOURCES.keys() | module._STORAGE_READERS.keys()):
            assert history.SOURCES.keys() <= after['files'].keys() | after['companions'].keys()
            assert not after['files'].keys() & after['companions'].keys()
            for name, expected in history.SOURCES.items():
                if name not in touched:
                    assert after['companions'][name] == {'commit': history.COMMIT, 'sha256': expected}
        else:
            assert after == before
    added = module._SETTINGS_REGRESSIONS[-2]
    assert added['target'] == 'cli_settings_parity'
    assert added['test'] == 'strict_settings::settings_commands_preserve_original_decoding_refusals_and_early_errors'
    assert added['symptom'] == 'Strict settings CLI admission differs'
    assert added['commit'] == history.COMMIT
    assert added['files'] == history.SOURCES
    extension = module._SETTINGS_REGRESSIONS[-1]
    assert extension == {
        'target': 'config_parity',
        'test': 'extension_admission::extension_setter_preserves_original_stored_set_admission_and_cache',
        'symptom': 'Stored extension set admission differs',
        'commit': 'b8e589f28e57e5734be2bacbd000bb11cc454ba2',
        'files': {'engine-rust/src/storage/config.rs':
                  '2c037f3fcb63a1d210c84437a72285a371a9c1fe89d85d510fe3c649142c406f'},
    }
    assert not extension['files'].keys() & extension.get('companions', {}).keys()


def test_schedule_history_preserves_text_failure_without_later_api_callers():
    module = load()
    cases = [case for case in module.REGRESSIONS if case['target'] == 'schedule_store_parity']
    assert len(cases) == 1
    case = cases[0]
    assert case['test'] == 'schedule_text_admission_matches_original_without_replacing_bytes'
    assert case['symptom'] == 'Schedule text admission differs'
    assert case['commit'] == 'b9d7399732c140d6141f37633486fffd90a2e30a'
    assert case['files'] == {
        'engine-rust/src/storage/schedules.rs':
            '749856bedd3df0be79cbd3853014fc75d24bb8b9320dcc9a09c32747ccf47527',
    }
    assert case['companions'] == {
        'engine-rust/tests/schedule_store_parity.rs': {
            'commit': '93db8f8b93d8b8e961bc548a3174da560c16c0b3',
            'sha256': '0419aa623eb9ec63e9d442af7e683756e7d4e04436c56ef0537691c6386504a4',
        },
        'engine-rust/src/cli/schedule_commands.rs': {
            'commit': '93db8f8b93d8b8e961bc548a3174da560c16c0b3',
            'sha256': '8ecf3e0a949f9eedef112c9c49b7661468d167072dc7230fdeb6ff9ae7f72b52',
        },
    }
    assert not case['files'].keys() & case['companions'].keys()


def test_pool_history_keeps_a_compatible_importer_without_replacing_the_selected_defect():
    module = load()
    cases = [case for case in module.REGRESSIONS if case['target'] == 'pool_parse_parity']
    assert len(cases) == 1
    case = cases[0]
    assert case['test'] == 'pooled_proxy_text_boundaries_match_original_full_results'
    assert case['symptom'] == 'Pooled proxy text boundaries differ'
    assert case['commit'] == 'ef6ebe26ba183fbbd9892d077d70fdcd8f7f7c64'
    assert case['files']['engine-rust/src/pool.rs'] == '5381266449dafc5eca9009fe1d900c3aed760da00a3855b8d122dc58772bb5ab'
    assert case['files']['engine-rust/src/pool_normalize.rs'] == 'f97331c9da3246f9ad993df1cacfe80f9d59d5494fd6db4c9c8cd26b05cd17e7'
    assert case['companions']['engine-rust/src/importers.rs'] == {
        'commit': '4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1',
        'sha256': 'e04030f2743e3f77625e0428c28c5727aabf39091e90aa01351775d7a0d16572',
    }
    assert not case['files'].keys() & case['companions'].keys()


def test_scheduler_row_history_requires_the_original_isolation_failure():
    module = load()
    cases = [case for case in module.REGRESSIONS
             if case['test'] == 'scheduler_rows::skipped_malformed_schedule_ids_do_not_abort_later_occurrences']
    assert len(cases) == 1
    assert cases[0] == {
        'target': 'launch_http_parity',
        'test': 'scheduler_rows::skipped_malformed_schedule_ids_do_not_abort_later_occurrences',
        'symptom': 'Scheduler row isolation differs',
        'commit': '7d8108a31551330c34a7d183bb04335422b0bcb0',
        'files': {'engine-rust/src/http/scheduler.rs':
                  '3e0e0e7fc208acc286c5006d15d172658df9e08078192a19c7c42e62c39d4a0c'},
    }


def test_ascii_refusal_history_requires_the_shared_representation_failure():
    module = load()
    cases = [case for case in module.REGRESSIONS
             if case['test'] == 'ascii_refusal_representation_matches_original_without_exposing_secret_values']
    assert cases == [{
        'target': 'validation_parity',
        'test': 'ascii_refusal_representation_matches_original_without_exposing_secret_values',
        'symptom': 'ASCII refusal representation differs',
        'commit': '65f5dfd15f2a4b6cc7fd375e858a76b153c83862',
        'files': {'engine-rust/src/validation.rs':
                  'd505c76357437baee6a8ef76d7626f099adbf59b8f2829cd8b4dfc73c1e8d4e6'},
    }]


def test_legacy_read_history_requires_the_existing_http_assertion():
    module = load()
    cases = [case for case in module.REGRESSIONS
             if case['test'] == 'read_view::legacy_read_views_match_original_core_http_and_name_admission']
    assert cases == [{
        'target': 'profile_read_parity',
        'test': 'read_view::legacy_read_views_match_original_core_http_and_name_admission',
        'symptom': 'Legacy HTTP read view differs',
        'commit': '65f5dfd15f2a4b6cc7fd375e858a76b153c83862',
        'files': {'engine-rust/src/http/profile_routes.rs':
                  '9c3fcf9caf0bc6fa97ba48b62341b527fb1bc6f017e3859720a257b8d88bd202'},
    }]


def test_current_caller_history_is_appended_after_compatibility_overlays():
    module = load()
    expected = ({
        'target': 'repo_batch_parity',
        'test': 'complete_batch_refusals_match_original_index_order_and_preserved_bytes',
        'symptom': 'Complete indexed batch refusal differs',
        'commit': 'd0d81cc6c173c302520f107061f53b57ff0ce885',
        'files': {'engine-rust/src/storage/repo_write.rs':
                  'ce4d904503fd52f828896f9abd840e76aee1e0131f292064fd99a875a864c53f'},
    },)
    claim = module.CURRENT_CALLER_REGRESSIONS[0]
    assert claim == {
        'target': 'claim_parity',
        'test': 'diagnostics::claim_diagnostics_match_original_truth_and_display_without_changing_records',
        'symptom': 'Claim diagnostic display differs',
        'commit': 'd25e088d8adb4c70e2dd401ffbe100331cfb35f4',
        'files': {'engine-rust/src/ownership/claims.rs':
                  '6af8a9ffd4a29f9ce29da25da9ecf8e4b4fc2031b9228424be8c578650eccbb8'},
    }
    added = module.CURRENT_CALLER_REGRESSIONS[1:-1]
    assert [(case['target'], case['test'], case['symptom']) for case in added] == [
        ('profile_http_import_parity', 'lexical::importer_lexical_profiles_match_original_without_changing_inputs', 'Imported lexical dashboard differs'),
        ('profile_http_import_parity', 'lexical::native_envelope_versions_match_original_exact_numeric_guidance', 'Native export version guidance differs'),
    ]
    assert all(case['commit'] == '4297b1f8e06f2ca0ae7617f2602bd5ed43d450d1' for case in added)
    assert added[0]['files'] == {'engine-rust/src/importers.rs': 'e04030f2743e3f77625e0428c28c5727aabf39091e90aa01351775d7a0d16572'}
    assert added[1]['files'] == {'engine-rust/src/import_format.rs': 'fb386692f47d6e7d0281b302112001f6af120418d0bbf5d765c231243de51233'}
    assert module.CURRENT_CALLER_REGRESSIONS[-1:] == expected
    assert module.REGRESSIONS == module._SETTINGS_REGRESSIONS + module.CURRENT_CALLER_REGRESSIONS
    assert not any(case.get('companions') for case in module.CURRENT_CALLER_REGRESSIONS)


@pytest.mark.parametrize('damage', [None, 'exit', 'empty', 'name', 'symptom', 'summary'])
def test_failure_proof_requires_the_exact_production_assertion(damage):
    module = load()
    case = module.REGRESSIONS[0]
    evidence = module.FailureEvidence(case)
    messages = {
        'empty': 'running 1 test',
        'name': f"test {case['test']} ... FAILED",
        'symptom': f"assertion `left == right` failed: {case['symptom']}",
        'summary': 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                   '1 filtered out; finished in 0.01s',
    }
    try:
        for key, text in messages.items():
            if key != damage:
                evidence.emit(logging.makeLogRecord({'msg': text}))
        if damage:
            with pytest.raises(ValueError, match='required production assertion'):
                evidence.verify(0 if damage == 'exit' else 101)
        else:
            evidence.verify(101)
    finally:
        evidence.close()


@pytest.mark.parametrize('damage', [None, 'local', 'repository', 'sha', 'head', 'dirty'])
def test_historical_execution_requires_the_exact_clean_hosted_source(monkeypatch, damage):
    import subprocess

    module = load()
    sha = 'a' * 40
    monkeypatch.setenv('GITHUB_ACTIONS', 'false' if damage == 'local' else 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'other/repository' if damage == 'repository'
                       else 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('GITHUB_SHA', 'invalid' if damage == 'sha' else sha)
    calls = []

    def git(root, *arguments):
        calls.append(arguments)
        if arguments == ('rev-parse', 'HEAD'):
            return (('b' * 40 if damage == 'head' else sha) + '\n').encode('ascii')
        assert arguments[:3] == ('diff', '--exit-code', 'HEAD')
        if damage == 'dirty':
            raise subprocess.CalledProcessError(1, ['git', 'diff'])
        return b''

    monkeypatch.setattr(module, 'git', git)
    if damage:
        with pytest.raises((ValueError, subprocess.CalledProcessError)):
            module.require_hosted_source(ROOT)
    else:
        assert module.require_hosted_source(ROOT) == sha
        assert len(calls) == 2


@pytest.mark.parametrize('platform', ['win32', 'linux'])
@pytest.mark.parametrize('damage', [None, 'warm', 'launch', 'assertion', 'integrity', 'companion'])
@pytest.mark.parametrize('incremental', [None, '0', '1'])
def test_historical_runner_restores_source_and_environment(
        tmp_path, monkeypatch, damage, platform, incremental):
    module = load()
    source = tmp_path / 'engine-rust/src/models.rs'
    source.parent.mkdir(parents=True)
    current, historical = b'fixed production bytes\n', b'historical production bytes\n'
    source.write_bytes(current)
    companion = tmp_path / 'engine-rust/tests/config_parity.rs'
    companion.parent.mkdir(parents=True)
    fixed_test, old_test = b'fixed test bytes\n', b'original assertion bytes\n'
    companion.write_bytes(fixed_test)
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(historical).hexdigest()},
        companions={'engine-rust/tests/config_parity.rs': {
            'commit': 'companion-commit', 'sha256': hashlib.sha256(old_test).hexdigest()}})
    excluded = dict(case, platform='linux' if platform == 'win32' else 'win32',
                    commit='excluded-source-must-not-be-read')
    selected = dict(case, platform=platform)
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform=platform))
    monkeypatch.setattr(module, 'REGRESSIONS',
                        (excluded, selected, case) if damage is None else (excluded, selected))
    sha = 'a' * 40
    monkeypatch.setenv('GITHUB_ACTIONS', 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('GITHUB_SHA', sha)
    monkeypatch.setenv('NUMERA_ORACLE_SOURCE_ROOT', 'prior-original-root')
    monkeypatch.delenv('PYTHONDONTWRITEBYTECODE', raising=False)
    if incremental is None:
        monkeypatch.delenv('CARGO_INCREMENTAL', raising=False)
    else:
        monkeypatch.setenv('CARGO_INCREMENTAL', incremental)
    calls, warmups = [], []

    def git(root, *arguments):
        if arguments == ('rev-parse', 'HEAD'):
            return sha.encode('ascii')
        if arguments[0] == 'diff':
            return b''
        if arguments == ('show', 'companion-commit:engine-rust/tests/config_parity.rs'):
            return old_test + (b'changed' if damage == 'companion' else b'')
        assert arguments == ('show', f"{case['commit']}:engine-rust/src/models.rs")
        return historical + (b'changed' if damage == 'integrity' else b'')

    def run(command, wall, idle, logger):
        import os

        calls.append(command)
        assert source.read_bytes() == historical
        assert companion.read_bytes() == old_test
        assert (wall, idle) == (180, 90)
        assert command[-4:] == [case['test'], '--exact', '--test-threads=1', '--color=never']
        assert '--locked' in command and '--offline' in command
        assert os.environ['NUMERA_ORACLE_SOURCE_ROOT'] == str(tmp_path / 'original')
        assert os.environ['PYTHONDONTWRITEBYTECODE'] == '1'
        assert os.environ['CARGO_INCREMENTAL'] == '1'
        if damage == 'launch':
            raise TimeoutError('fixture runner timeout')
        logger.info('running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', 'unrelated failure' if damage == 'assertion' else case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 101

    def warmup(command, **options):
        import os

        assert os.environ['CARGO_INCREMENTAL'] == '1'
        assert source.read_bytes() == historical
        assert companion.read_bytes() == old_test
        assert command == ['cargo', 'test', '--manifest-path', str(tmp_path / 'engine-rust/Cargo.toml'),
                           '--locked', '--offline', '--test', case['target'], '--no-run']
        assert options == {'stdin': module.subprocess.DEVNULL, 'check': True}
        warmups.append(command)
        if damage == 'warm':
            raise module.subprocess.CalledProcessError(1, command)
        return SimpleNamespace(returncode=0)

    monkeypatch.setattr(module, 'git', git)
    monkeypatch.setattr(module.subprocess, 'run', warmup)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    monkeypatch.setitem(sys.modules, 'baseline_source',
                        SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    logger = logging.Logger('native-regression-fixture', level=logging.INFO)
    if damage:
        with pytest.raises((ValueError, TimeoutError, module.subprocess.CalledProcessError)):
            module.prove(tmp_path, tmp_path / 'original', logger)
    else:
        module.prove(tmp_path, tmp_path / 'original', logger)
    import os

    assert source.read_bytes() == current
    assert companion.read_bytes() == fixed_test
    assert os.environ['NUMERA_ORACLE_SOURCE_ROOT'] == 'prior-original-root'
    assert 'PYTHONDONTWRITEBYTECODE' not in os.environ
    assert os.environ.get('CARGO_INCREMENTAL') == incremental
    assert logger.handlers == []
    assert len(calls) == (2 if damage is None else 0 if damage in ('integrity', 'companion', 'warm') else 1)
    assert len(warmups) == (0 if damage in ('integrity', 'companion') else 1)
