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
