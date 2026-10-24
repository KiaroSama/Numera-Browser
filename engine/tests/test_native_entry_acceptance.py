"""Entry acceptance delegates ownership and retains every failed fixture."""
from __future__ import annotations

from contextlib import contextmanager
import importlib.util
import logging
from pathlib import Path
import subprocess
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    import baseline_source
    monkeypatch.setattr(baseline_source, 'source_root', lambda: ROOT)
    spec = importlib.util.spec_from_file_location(
        'entry_acceptance_fixture', ROOT / 'scripts/migration/native_entry_acceptance.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize('mode', ['normal', 'input', 'communicate', 'decode', 'settlement'])
def test_execute_delegates_to_receipt_owner_and_never_accepts_failed_settlement(monkeypatch, mode):
    helper = load(monkeypatch)
    import candidate_acceptance

    calls = []
    argv, environment = ['inert-candidate', 'list'], {'fixture': 'value'}
    data = b'y\n' if mode == 'input' else None

    class Process:
        returncode = 7

        def communicate(self, supplied, timeout):
            assert supplied == data and timeout == 15
            calls.append('communicate')
            if mode == 'communicate':
                raise subprocess.TimeoutExpired(argv, timeout)
            return (b'\xff' if mode == 'decode' else 'Fixture α'.encode('utf-8')), b''

    @contextmanager
    def owned(arguments, env, cwd, stdin):
        assert arguments is argv and env is environment and cwd == helper.ROOT
        assert stdin == (subprocess.PIPE if data is not None else subprocess.DEVNULL)
        calls.append('owned')
        try:
            yield Process()
        finally:
            calls.append('settled')
            if mode == 'settlement':
                raise RuntimeError('Synthetic incomplete ownership')

    monkeypatch.setattr(candidate_acceptance, 'owned', owned)
    if mode in ('communicate', 'decode', 'settlement'):
        error = {'communicate': subprocess.TimeoutExpired, 'decode': UnicodeDecodeError,
                 'settlement': RuntimeError}[mode]
        with pytest.raises(error):
            helper.execute(argv, environment, data)
    else:
        assert helper.execute(argv, environment, data) == (7, 'Fixture α', '')
    assert calls == ['owned', 'communicate', 'settled']


@pytest.mark.parametrize('failure', [False, True])
def test_main_removes_only_successful_entry_fixtures(tmp_path, monkeypatch, failure):
    helper = load(monkeypatch)
    temporary = tmp_path / 'owned-entry'
    temporary.mkdir()
    marker = temporary / 'owner.json'
    marker.write_bytes(b'owned evidence')
    monkeypatch.setattr(helper.tempfile, 'mkdtemp', lambda **kwargs: str(temporary))

    class Store:
        def __init__(self, root):
            self.root = root

        def save(self, profile):
            directory = self.root / 'profiles'
            directory.mkdir(parents=True)
            (directory / 'fixture.json').write_text('{"updated_at":1000}', encoding='utf-8')

    monkeypatch.setattr(helper, 'ProfileStore', Store)
    monkeypatch.setattr(helper, 'Profile', lambda **kwargs: SimpleNamespace(**kwargs))
    monkeypatch.setattr(helper, 'generate', lambda **kwargs: {})

    def execute(*args, **kwargs):
        if failure:
            raise RuntimeError('Synthetic unsettled entry')
        return 0, '{}', ''

    monkeypatch.setattr(helper, 'execute', execute)
    for name in ('confirmation_acceptance', 'replay_rollback', 'configured_lock_acceptance'):
        monkeypatch.setattr(helper, name, lambda *args: None)
    logger = logging.getLogger('entry-retention-fixture')
    if failure:
        with pytest.raises(RuntimeError, match='Synthetic unsettled entry'):
            helper.main(tmp_path / 'inert-candidate', logger)
        assert marker.read_bytes() == b'owned evidence'
    else:
        assert helper.main(tmp_path / 'inert-candidate', logger) == 0
        assert not temporary.exists()
