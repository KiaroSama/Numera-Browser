"""Unmeasured memory diagnostics never replace lifetime benchmark acceptance."""
from __future__ import annotations

import importlib.util
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    spec = importlib.util.spec_from_file_location(
        'browser_memory_fixture', ROOT / 'scripts/migration/benchmark_memory.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def stat(pid, group, birth=900):
    # Fields 3..22, including a command containing parentheses and spaces.
    fields = ['S', '1', str(group), *(['0'] * 16), str(birth)]
    return f'{pid} (private command (not a label)) {" ".join(fields)}\n'


def process(root, pid, group, rss=13, peak=23):
    directory = root / str(pid)
    directory.mkdir()
    (directory / 'stat').write_text(stat(pid, group), encoding='utf-8')
    (directory / 'status').write_text(
        f'Name:\tPRIVATE\nVmRSS:\t{rss} kB\nVmHWM:\t{peak} kB\n', encoding='utf-8')
    return directory


@pytest.mark.parametrize('damage', [None, 'missing', 'partial', 'negative', 'duplicate', 'reuse', 'limit'])
def test_linux_memory_reads_only_owned_groups_and_rejects_partial_evidence(tmp_path, monkeypatch, damage):
    module = load(monkeypatch)
    command = process(tmp_path, 41, 41)
    process(tmp_path, 42, 42, rss=17, peak=29)
    process(tmp_path, 43, 42, rss=31, peak=47)
    foreign = process(tmp_path, 99, 99)
    (foreign / 'status').write_text('must never be parsed', encoding='utf-8')
    if damage == 'missing':
        (command / 'status').unlink()
    elif damage == 'partial':
        (command / 'status').write_text('VmRSS: 1 kB\n', encoding='utf-8')
    elif damage == 'negative':
        (command / 'status').write_text('VmRSS: -1 kB\nVmHWM: 2 kB\n', encoding='utf-8')
    elif damage == 'duplicate':
        (command / 'status').write_text('VmRSS: 1 kB\nVmRSS: 1 kB\nVmHWM: 2 kB\n', encoding='utf-8')
    elif damage == 'limit':
        monkeypatch.setattr(module, 'MAX_PROCESSES', 1)
    elif damage == 'reuse':
        read = module.read
        reads = []

        def replaced(path):
            raw = read(path)
            if path == command / 'stat':
                reads.append(path)
                if len(reads) >= 2:
                    return stat(41, 99, birth=901).encode('utf-8')
            return raw

        monkeypatch.setattr(module, 'read', replaced)
    rows, missing = module.linux_snapshot({41: 900, 42: 900}, root=tmp_path)
    if damage is None:
        assert missing == 0
        assert rows == [
            (41, 900, {'rss_bytes': 13 * 1024, 'peak_rss_bytes': 23 * 1024}),
            (42, 900, {'rss_bytes': 17 * 1024, 'peak_rss_bytes': 29 * 1024}),
            (43, 900, {'rss_bytes': 31 * 1024, 'peak_rss_bytes': 47 * 1024}),
        ]
    else:
        assert missing > 0, 'Missing memory evidence was silently interpreted as zero'
        assert not any(row[0] == 41 for row in rows) or damage == 'limit'
    assert all(row[0] != 99 for row in rows)


@pytest.mark.parametrize('damage', [None, 'receipt', 'query', 'capacity', 'reused-sdk', 'reused-command'])
def test_recorder_preserves_roles_maxima_and_sampled_coverage_without_summing(monkeypatch, damage):
    module = load(monkeypatch)
    direct = SimpleNamespace(pid=41, token=900, group=41, kind='pgid')
    sdk = SimpleNamespace(pid=42, token=901, group=42, kind='pgid')
    prior = SimpleNamespace(pid=10, token=800, group=10, kind='pgid')
    clock = [0.0]
    monkeypatch.setattr(module.time, 'monotonic', lambda: clock[0])
    monkeypatch.setattr(module.sys, 'platform', 'linux')
    receipt = SimpleNamespace(complete=damage != 'receipt', trees=[prior, direct, sdk])
    monkeypatch.setattr(module, 'unit_report', SimpleNamespace(read=lambda path: receipt))
    queries = []

    def query(groups):
        queries.append(groups)
        if damage == 'query':
            raise PermissionError('private path and credentials must not be exposed')
        return [(41, 999 if damage == 'reused-command' else 900,
                 {'rss_bytes': 11, 'peak_rss_bytes': 21}),
                (42, 999 if damage == 'reused-sdk' else 901,
                 {'rss_bytes': 31, 'peak_rss_bytes': 41}),
                (43, 902, {'rss_bytes': 71, 'peak_rss_bytes': 91}),
                (44, 903, {'rss_bytes': 61, 'peak_rss_bytes': 81})], 0

    monkeypatch.setattr(module, 'linux_snapshot', query)
    if damage == 'capacity':
        monkeypatch.setattr(module, 'MAX_SNAPSHOTS', 1)
    recorder = module.Recorder(direct, Path('inert-receipt'), [prior], None)
    recorder.sample()
    recorder.sample()  # No repeated process-table scan inside the interval.
    clock[0] += .1
    recorder.sample()
    result = recorder.result()
    assert result['coverage'] == 'sampled-not-lifetime-attribution'
    assert set(result) == {'coverage', 'snapshots', 'missing', 'truncated', 'roles'}
    assert all(key in ('command', 'sdk-leader', 'owned-descendant') for key in result['roles'])
    if damage in ('receipt', 'query'):
        assert result['missing'] > 0 and not result['roles']
    else:
        assert queries and all(groups == {41: 900, 42: 901} for groups in queries)
        if damage == 'reused-command':
            assert 'command' not in result['roles'], 'Reused command PID retained its old role'
        else:
            assert result['roles']['command']['max_observed']['peak_rss_bytes'] == 21
        if damage == 'reused-sdk':
            assert 'sdk-leader' not in result['roles'], 'Reused SDK PID retained its old role'
        else:
            assert result['roles']['sdk-leader']['max_observed']['peak_rss_bytes'] == 41
        descendants = result['roles']['owned-descendant']
        assert descendants['identities'] == (3 if damage in ('reused-sdk', 'reused-command') else 2)
        assert descendants['max_observed']['rss_bytes'] == 71
        assert descendants['max_observed']['peak_rss_bytes'] == 91, 'Process peaks must not be summed'
        assert result['snapshots'] == (1 if damage == 'capacity' else 2)
        assert result['truncated'] == (damage == 'capacity')
    assert 'private' not in str(result) and 'credentials' not in str(result)


@pytest.mark.parametrize('damage', [None, 'reused-leader'])
def test_recorder_reaches_real_procfs_membership_and_counter_parser(tmp_path, monkeypatch, damage):
    module = load(monkeypatch)
    process(tmp_path, 41, 41)
    process(tmp_path, 42, 42, rss=17, peak=29)
    process(tmp_path, 43, 42, rss=31, peak=47)
    direct = SimpleNamespace(pid=41, token=900, group=41, kind='pgid')
    sdk = SimpleNamespace(pid=42, token=899 if damage else 900, group=42, kind='pgid')
    monkeypatch.setattr(module.sys, 'platform', 'linux')
    monkeypatch.setattr(module.time, 'monotonic', lambda: 1.0)
    monkeypatch.setattr(module, 'unit_report', SimpleNamespace(
        read=lambda path: SimpleNamespace(complete=True, trees=[direct, sdk])))
    query = module.linux_snapshot
    monkeypatch.setattr(module, 'linux_snapshot', lambda groups: query(groups, root=tmp_path))
    recorder = module.Recorder(direct, Path('inert-receipt'), [], None)
    recorder.sample()
    result = recorder.result()
    assert result['roles']['command']['max_observed']['rss_bytes'] == 13 * 1024
    if damage:
        assert result['missing'] > 0
        assert set(result['roles']) == {'command'}, 'Reused group leader admitted unrelated memory'
    else:
        assert result['missing'] == 0
        assert result['roles']['sdk-leader']['max_observed']['peak_rss_bytes'] == 29 * 1024
        assert result['roles']['owned-descendant']['max_observed']['peak_rss_bytes'] == 47 * 1024


@pytest.mark.parametrize('damage', [None, 'extra', 'role', 'counter', 'number', 'coverage'])
def test_diagnostic_projection_never_forwards_unknown_text(monkeypatch, damage):
    module = load(monkeypatch)
    value = {'coverage': 'sampled-not-lifetime-attribution', 'snapshots': 2, 'missing': 1,
             'truncated': False, 'roles': {
                 'command': {'identities': 1, 'max_observed': {'rss_bytes': 123}}}}
    if damage == 'extra':
        value['private'] = 'do-not-publish'
    elif damage == 'role':
        value['roles']['do-not-publish'] = value['roles'].pop('command')
    elif damage == 'counter':
        value['roles']['command']['max_observed']['private-path'] = 1
    elif damage == 'number':
        value['roles']['command']['max_observed']['rss_bytes'] = 'do-not-publish'
    elif damage == 'coverage':
        value['coverage'] = 'lifetime-peak'
    events = []
    logger = SimpleNamespace(warning=lambda message, *args: events.append(message % args))
    if damage:
        with pytest.raises(ValueError, match='diagnostic'):
            module.log_evidence(value, 'native', logger)
        assert not events
    else:
        module.log_evidence(value, 'native', logger)
        assert len(events) == 2
        assert 'runtime=native role=command identities=1 metric=rss_bytes max_observed=123' in events[1]
        assert 'sampled-not-lifetime-attribution' in events[0]
    assert 'do-not-publish' not in str(events)


@pytest.mark.parametrize('failed_close', [False, True])
def test_windows_memory_uses_retained_job_and_closes_process_handles(monkeypatch, failed_close):
    module = load(monkeypatch)
    spec = importlib.util.spec_from_file_location(
        'windows_memory_fixture', ROOT / 'scripts/migration/benchmark_memory_windows.py')
    native = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(native)
    seen = []

    class API:
        def OpenProcess(self, access, inherited, pid):
            assert access == 0x1000 and inherited is False
            seen.append(('open', pid))
            return pid + 100

        def IsProcessInJob(self, handle, job, output):
            assert job == 500
            output._obj.value = handle != 199
            return True

        def GetProcessTimes(self, handle, created, *other):
            created._obj.dwLowDateTime = handle
            return True

        def K32GetProcessMemoryInfo(self, handle, counters, size):
            assert size == native.ctypes.sizeof(counters._obj)
            counters._obj.WorkingSetSize = 11
            counters._obj.PeakWorkingSetSize = 17
            counters._obj.PrivateUsage = 23
            counters._obj.PeakPagefileUsage = 29
            return handle != 143

        def CloseHandle(self, handle):
            seen.append(('close', handle))
            return not failed_close

    monkeypatch.setattr(native, 'api', lambda: API())
    monkeypatch.setattr(native, 'members', lambda kernel, job: [41, 42, 43, 99])
    if failed_close:
        import sys
        monkeypatch.setitem(sys.modules, 'benchmark_memory_windows', native)
        monkeypatch.setattr(module.sys, 'platform', 'win32')
        monkeypatch.setattr(module, 'unit_report', SimpleNamespace(
            read=lambda path: SimpleNamespace(complete=True, trees=[])))
        direct = SimpleNamespace(pid=41, token=141, group='owned-job', kind='job')
        recorder = module.Recorder(direct, Path('inert-receipt'), [], 500)
        with pytest.raises(RuntimeError, match='handle did not close'):
            recorder.sample()
        assert seen == [('open', 41), ('close', 141)]
    else:
        rows, missing = native.snapshot(500)
        assert missing == 2  # Memory refusal and a PID no longer in the actual job.
        assert rows == [(pid, pid + 100, {'rss_bytes': 11, 'peak_rss_bytes': 17,
                                        'private_bytes': 23, 'peak_private_bytes': 29}) for pid in (41, 42)]
        assert [value for kind, value in seen if kind == 'close'] == [141, 142, 143, 199]


@pytest.mark.parametrize('damage', [None, 'short', 'truncated', 'duplicate', 'zero'])
def test_windows_job_membership_refuses_incomplete_or_invalid_lists(monkeypatch, damage):
    module = load(monkeypatch)
    import benchmark_memory_windows as native
    import struct

    class API:
        def QueryInformationJobObject(self, job, kind, buffer, size, returned):
            assert job == 500 and kind == 3
            ids = [41, 41 if damage == 'duplicate' else 0 if damage == 'zero' else 42]
            raw = struct.pack('<II', 3 if damage == 'truncated' else 2, 2)
            raw += b''.join(pid.to_bytes(native.ctypes.sizeof(native.ctypes.c_size_t), 'little') for pid in ids)
            native.ctypes.memmove(buffer, raw, len(raw))
            returned._obj.value = 4 if damage == 'short' else len(raw)
            return True

    if damage:
        with pytest.raises((OSError, ValueError)):
            native.members(API(), 500)
    else:
        assert native.members(API(), 500) == [41, 42]
