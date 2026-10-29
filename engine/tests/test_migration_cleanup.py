"""Owned cleanup failures stay fatal and expose only fixed diagnostic metadata."""
from __future__ import annotations

import errno
import hashlib
import importlib.util
import logging
from pathlib import Path
import sys
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'scripts/migration'
pytestmark = pytest.mark.timeout(10)


def load(path: Path, monkeypatch):
    monkeypatch.syspath_prepend(str(SCRIPTS))
    spec = importlib.util.spec_from_file_location('owned_cleanup_fixture', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def assert_failed_cleanup(helper, tmp_path, monkeypatch, caplog, operation, winerror):
    temporary = tmp_path / 'private-name-must-not-be-logged'
    temporary.mkdir()
    retained = temporary / 'private-payload'
    retained.write_bytes(b'retained fixture bytes')
    outside = tmp_path / 'outside'
    outside.write_bytes(b'outside bytes')
    problem = PermissionError(errno.EACCES, 'private-message-must-not-be-logged', str(retained))
    problem.winerror = winerror
    calls = []
    stream = SimpleNamespace(close=lambda: calls.append('close-output'))
    process = SimpleNamespace(pid=123, returncode=7, stdout=stream,
                              poll=lambda: 7, wait=lambda timeout: calls.append('wait'))

    class Reader:
        def __init__(self, **kwargs):
            assert kwargs['name'] == 'migration-output'

        def start(self):
            pass

        def join(self, timeout):
            calls.append('join-reader')

        def is_alive(self):
            return False

    class Events:
        def get(self, timeout):
            return 'eof', None

    def remove(path, **kwargs):
        assert path == temporary
        calls.append('remove')
        handler = kwargs.get('onerror')
        if handler is None:
            raise problem
        function = SimpleNamespace(__name__=operation + '\nprivate-operation')
        if operation != 'unknown':
            function.__name__ = operation
        handler(function, str(retained), (PermissionError, problem, None))
        pytest.fail('Cleanup callback swallowed the original exception')

    with monkeypatch.context() as patch:
        patch.setattr(helper, 'create_temporary', lambda *args: temporary)
        patch.setattr(helper, 'spawn', lambda *args, **kwargs: (process, 'owned-tree'))
        patch.setattr(helper, 'end_tree', lambda tree, **kwargs: calls.append('settle-tree') or True)
        patch.setattr(helper, 'threading', SimpleNamespace(Thread=Reader))
        patch.setattr(helper, 'queue', SimpleNamespace(Queue=Events, Empty=helper.queue.Empty))
        patch.setattr(helper.shutil, 'rmtree', remove)
        logger = logging.getLogger('owned-cleanup-fixture')
        with caplog.at_level(logging.ERROR, logger=logger.name), pytest.raises(PermissionError) as caught:
            helper.run(['synthetic-command'], 10, 5, logger)
    assert caught.value is problem
    assert calls == ['settle-tree', 'wait', 'join-reader', 'close-output', 'remove']
    assert retained.read_bytes() == b'retained fixture bytes'
    assert outside.read_bytes() == b'outside bytes'
    messages = [record.getMessage() for record in caplog.records if record.levelno >= logging.ERROR]
    expected = 'unknown' if operation == 'unknown' else operation
    assert len(messages) == 1 and 'Owned temp cleanup failed operation=' + expected in messages[0], \
        'Owned cleanup failure diagnostic missing'
    assert 'errno=13' in messages[0] and f'winerror={winerror}' in messages[0]
    assert 'kind=file' in messages[0] and 'readonly=False' in messages[0] and 'reparse=False' in messages[0]
    assert all(value not in messages[0] for value in
               (str(temporary), str(retained), problem.strerror, 'private-operation', 'retained fixture bytes'))


@pytest.mark.parametrize('operation', ['unlink', 'rmdir', 'scandir', 'islink', 'unknown'])
@pytest.mark.parametrize('winerror', [5, 32, None])
def test_public_run_reports_failed_cleanup_without_swallowing_or_repeating(
        tmp_path, monkeypatch, caplog, operation, winerror):
    helper = load(SCRIPTS / 'run_owned.py', monkeypatch)
    assert_failed_cleanup(helper, tmp_path, monkeypatch, caplog, operation, winerror)


@pytest.mark.parametrize('wall,idle,wall_expired,idle_expired', [
    (5, 10, True, False), (10, 5, False, True), (5, 5, True, True),
], ids=['wall', 'idle', 'both'])
@pytest.mark.parametrize('reader_alive', [False, True], ids=['settled', 'reader-unsettled'])
def test_timeout_trigger_is_retained_before_fatal_cleanup(
        tmp_path, monkeypatch, caplog, wall, idle, wall_expired, idle_expired, reader_alive):
    helper = load(SCRIPTS / 'run_owned.py', monkeypatch)
    temporary = tmp_path / 'retained-input'
    temporary.mkdir()
    retained = temporary / 'payload'
    retained.write_bytes(b'unchanged diagnostic fixture')
    calls = []
    process = SimpleNamespace(pid=123, returncode=-1,
                              stdout=SimpleNamespace(close=lambda: calls.append('close-output')),
                              wait=lambda timeout: calls.append(('wait', timeout)))

    class Reader:
        def __init__(self, **kwargs):
            pass

        def start(self):
            pass

        def join(self, timeout):
            calls.append(('join', timeout))

        def is_alive(self):
            return reader_alive

    clock = iter((0.0, 6.0))
    monkeypatch.setattr(helper, 'create_temporary', lambda *args: temporary)
    monkeypatch.setattr(helper, 'spawn', lambda *args, **kwargs: (process, 'owned-tree'))
    monkeypatch.setattr(helper, 'time', SimpleNamespace(monotonic=lambda: next(clock)))
    monkeypatch.setattr(helper, 'threading', SimpleNamespace(Thread=Reader))
    monkeypatch.setattr(helper, 'end_tree', lambda tree, **kwargs: calls.append('settle') or True)
    logger = logging.getLogger('timeout-trigger-fixture')
    expected = RuntimeError if reader_alive else TimeoutError
    with caplog.at_level(logging.INFO, logger=logger.name), pytest.raises(expected) as caught:
        helper.run(['synthetic-command'], wall, idle, logger)
    assert str(caught.value) == ('Command output reader did not settle' if reader_alive else
                                 'Migration command exceeded wall/idle budget')
    if reader_alive:
        assert isinstance(caught.value.__context__, TimeoutError)
        assert calls == ['settle', ('wait', 5), ('join', 2)]
        assert retained.read_bytes() == b'unchanged diagnostic fixture'
    else:
        assert calls == ['settle', ('wait', 5), ('join', 2), 'close-output']
        assert not temporary.exists()
    messages = [record.getMessage() for record in caplog.records]
    triggers = [message for message in messages if message.startswith('Verification timeout ')]
    assert triggers == [f'Verification timeout wall_expired={wall_expired} idle_expired={idle_expired} '
                        f'elapsed=6.000s silence=6.000s wall={wall:.3f}s idle={idle:.3f}s'], \
        'Original timeout trigger was lost before cleanup'
    assert messages.index(triggers[0]) < next(index for index, message in enumerate(messages)
                                            if 'phase=settle-tree' in message)
    assert 'synthetic-command' not in triggers[0] and str(temporary) not in triggers[0]


def test_cleanup_diagnostic_is_load_bearing_against_exact_previous_runner(tmp_path, monkeypatch, caplog):
    fixture = ROOT / 'engine/tests/fixtures/migration-owned-before-cleanup-diagnostic.py.txt'
    source = fixture.read_bytes()
    assert hashlib.sha256(source).hexdigest() == '2510efada47b91c39dbfb8697d2836426009e9f9adf2c9c9270c13a1bbe443c3'
    path = tmp_path / 'historical/run_owned.py'
    path.parent.mkdir()
    path.write_bytes(source)
    monkeypatch.setattr(sys, 'dont_write_bytecode', True)
    helper = load(path, monkeypatch)
    with pytest.raises(AssertionError, match='Owned cleanup failure diagnostic missing'):
        assert_failed_cleanup(helper, tmp_path, monkeypatch, caplog, 'unlink', 32)
    assert path.read_bytes() == source
