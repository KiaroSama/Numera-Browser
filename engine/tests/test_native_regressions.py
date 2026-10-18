"""Historical failure proof rejects unrelated failures and restores fixed source."""
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


@pytest.mark.parametrize('damage', [None, 'warm', 'launch', 'assertion', 'integrity'])
def test_historical_runner_restores_source_and_environment(tmp_path, monkeypatch, damage):
    module = load()
    source = tmp_path / 'engine-rust/src/models.rs'
    source.parent.mkdir(parents=True)
    current, historical = b'fixed production bytes\n', b'historical production bytes\n'
    source.write_bytes(current)
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(historical).hexdigest()})
    monkeypatch.setattr(module, 'REGRESSIONS', (case, case) if damage is None else (case,))
    sha = 'a' * 40
    monkeypatch.setenv('GITHUB_ACTIONS', 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('GITHUB_SHA', sha)
    monkeypatch.setenv('NUMERA_ORACLE_SOURCE_ROOT', 'prior-original-root')
    monkeypatch.delenv('PYTHONDONTWRITEBYTECODE', raising=False)
    calls, warmups = [], []

    def git(root, *arguments):
        if arguments == ('rev-parse', 'HEAD'):
            return sha.encode('ascii')
        if arguments[0] == 'diff':
            return b''
        assert arguments == ('show', f"{case['commit']}:engine-rust/src/models.rs")
        return historical + (b'changed' if damage == 'integrity' else b'')

    def run(command, wall, idle, logger):
        import os

        calls.append(command)
        assert source.read_bytes() == historical
        assert (wall, idle) == (180, 90)
        assert command[-4:] == [case['test'], '--exact', '--test-threads=1', '--color=never']
        assert '--locked' in command and '--offline' in command
        assert os.environ['NUMERA_ORACLE_SOURCE_ROOT'] == str(tmp_path / 'original')
        assert os.environ['PYTHONDONTWRITEBYTECODE'] == '1'
        if damage == 'launch':
            raise TimeoutError('fixture runner timeout')
        logger.info('running 1 test')
        logger.info('test %s ... FAILED', case['test'])
        logger.info('%s', 'unrelated failure' if damage == 'assertion' else case['symptom'])
        logger.info('test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                    '1 filtered out; finished in 0.01s')
        return 101

    def warmup(command, **options):
        assert source.read_bytes() == historical
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
    assert os.environ['NUMERA_ORACLE_SOURCE_ROOT'] == 'prior-original-root'
    assert 'PYTHONDONTWRITEBYTECODE' not in os.environ
    assert logger.handlers == []
    assert len(calls) == (2 if damage is None else 0 if damage in ('integrity', 'warm') else 1)
    assert len(warmups) == (0 if damage == 'integrity' else 1)
