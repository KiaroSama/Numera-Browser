"""Owned proof accounting is diagnostic only and never changes proof verdicts."""
from __future__ import annotations

from contextlib import contextmanager
import ctypes
import hashlib
import importlib.util
import json
import logging
from pathlib import Path
import sys
from types import SimpleNamespace

import pytest

from test_native_regressions import load as load_runner

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load():
    spec = importlib.util.spec_from_file_location(
        'history_resources_fixture', ROOT / 'scripts/ci/native_history_resources.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class API:
    def __init__(self, damage=None):
        self.damage, self.calls = damage, []
        self.observations = 0

    def OpenJobObjectW(self, access, inherit, name):
        assert access == 4 and inherit is False and name == 'private-job-name'
        self.calls.append('open')
        return None if self.damage == 'open' else 123

    def GetCurrentProcess(self):
        return -1

    def IsProcessInJob(self, process, job, result):
        assert process == -1 and job == 123
        self.calls.append('membership')
        result._obj.value = self.damage != 'foreign'
        return self.damage != 'membership'

    def QueryInformationJobObject(self, job, kind, output, size, returned):
        assert job == 123 and kind == 8 and size == 96
        self.calls.append('query')
        self.observations += 1
        row = output._obj
        value = self.observations * 10
        if self.damage == 'decreased' and self.observations == 2:
            value = 1
        row.BasicInfo.TotalUserTime = value
        row.BasicInfo.TotalKernelTime = value + 1
        row.BasicInfo.TotalPageFaultCount = value + 2
        row.BasicInfo.TotalProcesses = value + 3
        row.BasicInfo.ActiveProcesses = 1
        row.BasicInfo.TotalTerminatedProcesses = 0
        for index, (name, _) in enumerate(row.IoInfo._fields_):
            setattr(row.IoInfo, name, value + index)
        if self.damage == 'negative':
            row.BasicInfo.TotalUserTime = -1
        elif self.damage == 'active':
            row.BasicInfo.ActiveProcesses = row.BasicInfo.TotalProcesses + 1
        returned._obj.value = 95 if self.damage == 'short' else size
        return self.damage != 'query' and not (
            self.damage == 'after-query' and self.observations == 2)

    def CloseHandle(self, handle):
        assert handle == 123
        self.calls.append('close')
        return self.damage != 'close'


def bind(module, monkeypatch, api, *, platform='win32', inherited=True):
    monkeypatch.setattr(module, 'sys', SimpleNamespace(platform=platform, exc_info=sys.exc_info))
    monkeypatch.setattr(module, 'kernel', lambda: api)
    monkeypatch.setattr(module, 'inherited_tree', lambda: SimpleNamespace(
        kind='job', group='private-job-name') if inherited else None)


@pytest.mark.parametrize('damage', [None, 'open', 'foreign', 'membership', 'query',
                                   'short', 'negative', 'active', 'after-query', 'decreased'])
def test_accounting_checks_membership_layout_and_monotonicity_without_payloads(
        monkeypatch, caplog, damage):
    module = load()
    assert ctypes.sizeof(module.Basic) == 48
    assert ctypes.sizeof(module.Io) == 48
    assert ctypes.sizeof(module.Accounting) == 96
    api = API(damage)
    bind(module, monkeypatch, api)
    clock = iter((100, 210))
    monkeypatch.setattr(module, 'time', SimpleNamespace(monotonic_ns=lambda: next(clock)))
    logger = logging.getLogger('history-accounting-fixture')
    reached = []
    with caplog.at_level(logging.INFO, logger=logger.name):
        with module.measure(7, logger):
            reached.append('unchanged-proof')
    assert reached == ['unchanged-proof']
    events = [record.getMessage() for record in caplog.records]
    assert len(events) == 1
    if damage is None:
        assert 'index=7 coverage=owned-proof-scope-not-compiler-attribution wall_ns=110' in events[0]
        counters = json.loads(events[0].split(' counters=', 1)[1])
        assert counters == {name: (0 if name == 'terminated_processes' else 10)
                            for name in module.CUMULATIVE}
        assert 'active_before=1 active_after=1' in events[0]
        assert api.calls == ['open', 'membership', 'query', 'membership', 'query', 'close']
    else:
        assert events == ['Historical resources index=7 status=unavailable']
        assert 'counters=' not in events[0]
    assert api.calls.count('close') == int(damage != 'open')
    if damage in ('foreign', 'membership'):
        assert 'query' not in api.calls
    assert all('private' not in event for event in events)


@pytest.mark.parametrize('platform,inherited', [('linux', True), ('win32', False)])
def test_unavailable_scope_never_opens_or_adopts_another_job(monkeypatch, caplog, platform, inherited):
    module = load()
    api = API()
    bind(module, monkeypatch, api, platform=platform, inherited=inherited)
    logger = logging.getLogger('history-scope-fixture')
    with caplog.at_level(logging.INFO, logger=logger.name), module.measure(0, logger):
        pass
    assert api.calls == []
    assert [record.getMessage() for record in caplog.records] == (
        [] if platform == 'linux' else ['Historical resources index=0 status=unavailable'])


@pytest.mark.parametrize('failure', [False, True])
def test_failed_query_handle_close_is_fatal_without_replacing_an_existing_proof_error(
        monkeypatch, caplog, failure):
    module = load()
    api = API('close')
    bind(module, monkeypatch, api)
    logger = logging.getLogger('history-close-fixture')
    original = TimeoutError('fixture proof deadline')
    expected = TimeoutError if failure else RuntimeError
    with caplog.at_level(logging.INFO, logger=logger.name), pytest.raises(expected) as caught:
        with module.measure(1, logger):
            if failure:
                raise original
    if failure:
        assert caught.value is original
    else:
        assert str(caught.value) == 'Historical resource query handle did not close'
    assert api.calls[-1] == 'close'
    assert any('status=handle-close-failed' in record.getMessage() for record in caplog.records)


@pytest.mark.parametrize('failure', [False, True])
def test_public_historical_runner_keeps_exact_commands_restoration_and_original_errors(
        tmp_path, monkeypatch, failure):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/ci'))
    module = load_runner()
    path = tmp_path / 'engine-rust/src/models.rs'
    path.parent.mkdir(parents=True)
    path.write_bytes(b'fixed')
    old = b'previous defect'
    case = dict(module.REGRESSIONS[0], files={
        'engine-rust/src/models.rs': hashlib.sha256(old).hexdigest()}, companions={})
    monkeypatch.setattr(module, 'REGRESSIONS', (case,))
    monkeypatch.setattr(module, 'require_hosted_source', lambda root: 'a' * 40)
    monkeypatch.setattr(module, 'git', lambda *args: old)
    monkeypatch.setitem(sys.modules, 'baseline_source', SimpleNamespace(ENVIRONMENT='NUMERA_ORACLE_SOURCE_ROOT'))
    events = []

    @contextmanager
    def measure(index, logger):
        assert index == 0 and path.read_bytes() == old
        events.append('before')
        try:
            yield
        finally:
            assert path.read_bytes() == old
            events.append('after')

    monkeypatch.setattr(module, 'measure', measure)

    def warmup(command, **options):
        assert command[-1] == '--no-run' and options['check'] is True
        events.append('compile')

    original = TimeoutError('fixture proof failure')

    def run(command, wall, idle, logger):
        assert (wall, idle) == (180, 90)
        assert command[-4:] == [case['test'], '--exact', '--test-threads=1', '--color=never']
        assert '--locked' in command and '--offline' in command and '--timings' in command
        events.append('proof')
        if failure:
            raise original
        for message in ('running 1 test', f"test {case['test']} ... FAILED", case['symptom'],
                        'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; '
                        '1 filtered out; finished in 0.01s'):
            logger.info('%s', message)
        return 101

    monkeypatch.setattr(module.subprocess, 'run', warmup)
    monkeypatch.setitem(sys.modules, 'run_owned', SimpleNamespace(run=run))
    logger = logging.Logger('history-accounting-runner', level=logging.INFO)
    if failure:
        with pytest.raises(TimeoutError) as caught:
            module.prove(tmp_path, tmp_path / 'original', logger)
        assert caught.value is original
    else:
        module.prove(tmp_path, tmp_path / 'original', logger)
    assert events == ['before', 'compile', 'proof', 'after']
    assert path.read_bytes() == b'fixed' and logger.handlers == []


@pytest.mark.skipif(sys.platform != 'win32', reason='job objects are Windows-only')
def test_real_windows_inherited_job_reports_numeric_lifetime_accounting(tmp_path, caplog):
    module = load()
    logger = logging.getLogger('history-accounting-os')
    with caplog.at_level(logging.INFO, logger=logger.name), module.measure(0, logger):
        path = tmp_path / 'accounting-fixture'
        path.write_bytes(bytes(range(256)))
        assert path.read_bytes() == bytes(range(256))
    events = [record.getMessage() for record in caplog.records]
    assert len(events) == 1 and 'coverage=owned-proof-scope-not-compiler-attribution' in events[0]
    values = json.loads(events[0].split(' counters=', 1)[1])
    assert set(values) == set(module.CUMULATIVE)
    assert all(type(value) is int and value >= 0 for value in values.values())
    assert str(tmp_path) not in events[0]
