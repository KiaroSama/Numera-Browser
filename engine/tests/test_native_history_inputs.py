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
        assert calls == [('diff', '--exit-code', 'HEAD', 'engine-rust/src/ownership/identity.rs'),
                         ('show', f"{case['commit']}:engine-rust/src/ownership/identity.rs")]
    assert path.read_bytes() == current and logger.handlers == []
