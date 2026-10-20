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
    spec.loader.exec_module(module)
    return module


def test_option_reader_companions_preserve_selected_defects_and_prior_snapshots():
    module = load()
    current = {(case['target'], case['test'], case['commit']): case for case in module.REGRESSIONS}
    original_tree = ast.parse(
        (ROOT / 'scripts/ci/native_regressions.py').read_text(encoding='utf-8'))
    first_registry = next(node.value for node in original_tree.body
                          if isinstance(node, ast.Assign)
                          and any(isinstance(target, ast.Name)
                                  and target.id == 'REGRESSIONS' for target in node.targets))
    original = ast.literal_eval(first_registry)
    assert len(module.REGRESSIONS) == len(original) + 1
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
    added = module.REGRESSIONS[-1]
    assert added['test'] == 'settings_errors::storage_settings_errors_match_original_lazy_reads_and_preserve_bytes'
    assert added['files'] == module._STORAGE_READERS
    assert added['commit'] == module._STORAGE_READERS_COMMIT
    assert added['symptom'] == 'Storage settings admission differs'


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
