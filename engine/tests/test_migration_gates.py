"""Migration readiness must not be inferred from a small passing Rust slice."""
import importlib.util
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'scripts' / 'migration'


def load(name):
    sys.path.insert(0, str(SCRIPTS))
    try:
        spec = importlib.util.spec_from_file_location(f'migration_{name}', SCRIPTS / f'{name}.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    finally:
        sys.path.remove(str(SCRIPTS))


def test_repaired_baseline_is_reachable_without_changing_fixture_provenance(monkeypatch):
    inventory = load('inventory')

    def original_paths(prefix):
        assert prefix == 'engine/numera'
        return [ROOT / 'engine/numera/models.py']

    monkeypatch.setattr(inventory, 'baseline_paths', original_paths)
    assert inventory.tracked_paths('engine/numera') == [ROOT / 'engine/numera/models.py']
    assert inventory.BASELINE_TREE == '760f9873ded13c5ebfecae1f16b385e34f695124'
    assert inventory.BASELINE == '13b6d9c6973281fe123353eaec05b29d65781a2d'


def test_original_inventory_reads_frozen_bytes_instead_of_working_source(tmp_path, monkeypatch):
    import hashlib
    inventory = load('inventory')
    source = tmp_path / 'original/engine/numera/models.py'
    source.parent.mkdir(parents=True)
    original = b'def original_contract():\n    return 1\n'
    source.write_bytes(original)
    monkeypatch.setattr(inventory, 'source_root', lambda: tmp_path / 'original')
    row = inventory.describe(source)
    assert row['path'] == 'engine/numera/models.py'
    assert row['sha256'] == hashlib.sha256(original).hexdigest()
    assert row['symbols'] == ['original_contract']


def test_inventory_keeps_alternative_definitions_independently_addressable(tmp_path, monkeypatch):
    inventory = load('inventory')
    source = tmp_path / 'engine/numera/conditional.py'
    source.parent.mkdir(parents=True)
    source.write_text('''if WINDOWS:
    def take(): pass
else:
    def take(): pass
if OTHER:
    def take(): pass

def read():
    if WINDOWS:
        def pipe(): pass
    else:
        def pipe(): pass
''', encoding='utf-8')
    monkeypatch.setattr(inventory, 'source_root', lambda: tmp_path)
    monkeypatch.setattr(inventory, 'tracked_paths',
                        lambda prefix: [source] if prefix == 'engine/numera' else [])
    def check(module):
        result = module.discover()
        assert result['files'][0]['symbols'] == ['take', 'take', 'take', 'read', 'read.pipe', 'read.pipe']
        assert [row['id'] for row in result['capabilities']] == [
            'engine/numera/conditional.py::take', 'engine/numera/conditional.py::take#2',
            'engine/numera/conditional.py::take#3',
            'engine/numera/conditional.py::read', 'engine/numera/conditional.py::read.pipe',
            'engine/numera/conditional.py::read.pipe#2',
        ], 'Alternative source definitions lost their distinct capability identities'
        assert all(row['status'] == 'pending' and row['target'] is None and row['test'] is None
                   for row in result['capabilities'])
        assert module.discover() == result

    import hashlib
    import pytest

    historical = (ROOT / 'engine/tests/fixtures/migration-inventory-before-occurrences.py.txt').read_bytes()
    assert hashlib.sha256(historical).hexdigest() == \
        '22de18dea172c6f67a991fa53ed47dfeaae220f1a139a39f93f5f9f76c274314'
    # The exact reviewed historical implementation must fail this same assertion.
    old_source = tmp_path / 'historical/inventory.py'
    old_source.parent.mkdir()
    old_source.write_bytes(historical)
    with monkeypatch.context() as prior:
        prior.setattr(sys, 'dont_write_bytecode', True)
        prior.setattr(sys, 'path', [str(SCRIPTS), *sys.path])
        spec = importlib.util.spec_from_file_location('historical_inventory', old_source)
        old = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(old)
        prior.setattr(old, 'source_root', lambda: tmp_path)
        prior.setattr(old, 'tracked_paths', inventory.tracked_paths)
        with pytest.raises(AssertionError, match='Alternative source definitions lost'):
            check(old)
    check(inventory)


def test_capability_gate_accepts_distinct_occurrences_but_refuses_reused_ids(tmp_path, monkeypatch):
    gate = load('check_coverage')
    first = {'id': 'source.py::take', 'status': 'verified',
             'target': 'scripts/migration/inventory.py', 'test': 'engine/tests/test_migration_gates.py'}
    second = {**first, 'id': 'source.py::take#2'}
    source = {'source_commit': 'frozen-fixture', 'files': [{'path': 'source.py'}],
              'capabilities': [first, second]}
    monkeypatch.setattr(gate, 'discover', lambda: source)
    path = tmp_path / 'coverage.json'
    path.write_text(json.dumps(source), encoding='utf-8')
    assert gate.check(path) == []
    for rows in ([first], [first, first], [first, {**second, 'status': 'pending'}]):
        path.write_text(json.dumps({**source, 'capabilities': rows}), encoding='utf-8')
        assert gate.check(path), 'A missing, reused or pending occurrence allowed selection'
    path.write_text(json.dumps({**source, 'capabilities': [first, second, second]}), encoding='utf-8')
    assert gate.check(path) == ['Duplicate capability: source.py::take#2']


def test_replay_comparison_preserves_identity_secret_and_creation_time():
    import pytest

    replay = load('replay_rollback')
    expected = {'id': 'fixture', 'created_at': 1000.0, 'updated_at': 1001.0,
                'notes': 'original', 'fingerprint': {'seed': 42},
                'proxy': {'password': 'synthetic'}, 'future': [42, True]}
    observed = {**expected, 'updated_at': 1002.0}
    replay.require_profile(observed, expected)
    assert expected['updated_at'] == 1001.0 and observed['updated_at'] == 1002.0
    for changed in (
        {**observed, 'id': 'other'}, {**observed, 'created_at': 1002.0},
        {**observed, 'notes': 'lost'}, {**observed, 'fingerprint': {'seed': 43}},
        {**observed, 'proxy': {'password': None}},
        {**observed, 'future': [42, 1]}, {**observed, 'future': [42.0, True]},
    ):
        with pytest.raises(AssertionError, match='Cross-runtime profile changed'):
            replay.require_profile(changed, expected)
    for timestamp in (None, True, '1002', float('nan'), float('inf'), -1.0):
        with pytest.raises(AssertionError, match='timestamp'):
            replay.require_profile({**observed, 'updated_at': timestamp}, expected)


def test_replay_snapshot_tracks_exact_store_and_session_bytes(tmp_path):
    replay = load('replay_rollback')
    (tmp_path / 'profiles').mkdir()
    (tmp_path / 'user-data/fixture/Default').mkdir(parents=True)
    (tmp_path / 'logs').mkdir()
    profile = tmp_path / 'profiles/fixture.json'
    profile.write_bytes(b'{"id":"fixture"}\n')
    session = tmp_path / 'user-data/fixture/Default/Cookies'
    session.write_bytes(b'\x00\xffsynthetic session')
    (tmp_path / 'config.json').write_bytes(b'{"future":true}')
    (tmp_path / 'dashboard-config.json').write_bytes(b'{"folders":[]}')
    before = replay.protected_bytes(tmp_path)
    assert before == {'profiles/fixture.json': b'{"id":"fixture"}\n',
                      'user-data/fixture/Default/Cookies': b'\x00\xffsynthetic session',
                      'config.json': b'{"future":true}',
                      'dashboard-config.json': b'{"folders":[]}'}
    (tmp_path / 'logs/run.log').write_text('ignored operational log', encoding='utf-8')
    assert replay.protected_bytes(tmp_path) == before
    session.write_bytes(b'changed')
    assert replay.protected_bytes(tmp_path) != before


def test_benchmark_range_verdict_refuses_noise_regression_and_invalid_samples():
    import pytest

    benchmark = load('benchmark')
    assert benchmark.verdict([1, 2, 3], [4, 5, 6]) == 'no-regression'
    assert benchmark.verdict([3, 3, 3], [3, 3, 3]) == 'inconclusive'
    assert benchmark.verdict([7, 8, 9], [4, 5, 6]) == 'regression'
    assert benchmark.verdict([3, 5, 7], [4, 5, 6]) == 'inconclusive'
    for sample in ([], [0], [-1], [True], [1.0], ['1']):
        with pytest.raises(ValueError, match='positive integer'):
            benchmark.verdict(sample, [1, 2, 3])
        with pytest.raises(ValueError, match='positive integer'):
            benchmark.verdict([1, 2, 3], sample)


def test_benchmark_failure_projection_retains_only_known_stage_type_and_error_code():
    benchmark = load('benchmark')
    safe = ('[2026-10-18 00:00:00 UTC] [ERROR] [numera.migration-benchmark-sample] '
            'Observation failed phase=memory type=OSError winerror=5')
    assert benchmark.observation_failure(safe) == 'phase=memory type=OSError winerror=5'
    assert benchmark.observation_failure(safe.replace('memory', 'spawn')) == 'phase=spawn type=OSError winerror=5'
    for value in (safe + ' password=synthetic-private', safe.replace('memory', 'private-path'),
                  safe.replace('OSError', 'SyntheticSensitiveError'), 'raw synthetic-private traceback'):
        assert benchmark.observation_failure(value) == 'phase=unknown type=unknown winerror=None'


def test_benchmark_spawn_failure_reports_phase_without_touching_uncreated_process(monkeypatch):
    import pytest
    from types import SimpleNamespace

    sample = load('benchmark_sample')
    error = OSError('synthetic-private spawn failure')
    error.winerror = 5
    messages = []
    events = []

    def refuse(*args):
        events.append('spawn')
        raise error

    monkeypatch.setattr(sys, 'platform', 'win32')
    monkeypatch.setitem(sys.modules, 'benchmark_windows', SimpleNamespace(spawn=refuse))
    monkeypatch.setattr(sample.unit_report, 'begin_spawn', lambda: events.append('intent') or 'fixture')
    monkeypatch.setattr(sample.unit_report, 'cancel_spawn', lambda attempt: pytest.fail('Uncertain intent cancelled'))
    monkeypatch.setattr(sample.proctree, 'end_tree', lambda *args, **kwargs: pytest.fail('No returned tree exists'))
    logger = SimpleNamespace(error=lambda message, *args: messages.append(message % args))
    with pytest.raises(OSError) as observed:
        sample.observe(['fixture.exe'], {}, logger)
    assert observed.value is error and events == ['intent', 'spawn']
    assert messages == ['Observation failed phase=spawn type=OSError winerror=5']


def test_benchmark_requires_natural_settlement_inside_the_original_deadline(monkeypatch):
    import hashlib
    import io
    import pytest
    from importlib.machinery import SourceFileLoader
    from types import SimpleNamespace
    from numera import winjob

    sample = load('benchmark_sample')
    structures = winjob._job_structs

    def check(module, mode):
        clock, events, queries = [0.0], [], []
        closed = {'process': False, 'job': False}
        forced = [False]
        tree = module.proctree.Tree(123, None, 'synthetic-job', 'job')

        def sleep(seconds):
            assert 0 <= seconds <= .05
            clock[0] += seconds

        def wait(timeout):
            events.append(('wait', timeout))
            if timeout == 1:
                clock[0] += 15 if mode == 'expired' else .2
            return 0

        def close():
            closed['process'] = True
            events.append('process-close')

        process = SimpleNamespace(stdout=io.BytesIO(b'{"ok":true}'), stderr=io.BytesIO(),
                                  poll=lambda: 0, wait=wait, close=close)

        def active(name):
            assert name == tree.group and not any(closed.values()) and not forced[0]
            queries.append(clock[0])
            if mode == 'late':
                clock[0] = 15.1
                return True, 0
            if mode == 'unknown':
                return False, 0
            return True, int(mode == 'active' or clock[0] < .25)

        def cleanup(*args, **kwargs):
            events.append('cleanup')
            forced[0] = True
            return True

        class Kernel:
            def QueryInformationJobObject(self, handle, kind, counters, size, unused):
                assert handle == 99 and kind == 9 and not any(closed.values()) and not forced[0]
                events.append(('peak', clock[0]))
                counters._obj.PeakJobMemoryUsed = 1234
                return True

            def CloseHandle(self, handle):
                assert handle == 99
                closed['job'] = True
                events.append('job-close')

        fake_time = SimpleNamespace(monotonic=lambda: clock[0], sleep=sleep,
                                    perf_counter_ns=lambda: int(clock[0] * 1_000_000_000))
        logger = SimpleNamespace(error=lambda *args: events.append('error'))
        with monkeypatch.context() as patch:
            patch.setattr(sys, 'platform', 'win32')
            patch.setitem(sys.modules, 'benchmark_windows', SimpleNamespace(
                spawn=lambda *args: (process, tree, 99)))
            patch.setitem(sys.modules, 'numera.winjob', SimpleNamespace(
                _kernel32=Kernel, _job_structs=structures))
            patch.setattr(module, 'time', fake_time)
            patch.setattr(module.proctree, 'time', fake_time)
            patch.setattr(module.proctree, '_job_active', active)
            patch.setattr(module.proctree, 'end_tree', cleanup)
            patch.setattr(module, 'pipe_reader', lambda pipe: lambda: pipe.read())
            patch.setattr(module.unit_report, 'begin_spawn', lambda: 'synthetic-intent')
            patch.setattr(module.unit_report, 'record', lambda *args, **kwargs: None)
            if mode == 'transient':
                try:
                    result = module.observe(['synthetic.exe'], {}, logger)
                except ValueError as error:
                    raise AssertionError('Natural benchmark settlement was refused') from error
                assert result['payload'] == {'ok': True}
                assert result['memory'] == {'metric': 'peak-job-committed-bytes', 'bytes': 1234}
                peak = next(event[1] for event in events if isinstance(event, tuple) and event[0] == 'peak')
                assert result['elapsed_ns'] == int(peak * 1_000_000_000) >= 250_000_000
                assert len(queries) == 2 and events.index(('peak', peak)) < events.index('cleanup')
            else:
                with pytest.raises(ValueError, match='benchmark'):
                    module.observe(['synthetic.exe'], {}, logger)
                assert not any(isinstance(event, tuple) and event[0] == 'peak' for event in events)
                if mode in ('active', 'unknown'):
                    assert 15 <= clock[0] < 15.05, 'Settlement reset or exceeded the original budget'
                elif mode == 'expired':
                    assert not queries, 'An exhausted deadline still attempted natural acceptance'
                else:
                    assert len(queries) == 1, 'A late empty observation was accepted or retried'
        assert process.stdout.closed and process.stderr.closed and all(closed.values())
        assert events[-2:] == ['process-close', 'job-close'] and events.count('cleanup') == 1

    # Exact historical bytes must fail the same transient-settlement contract.
    path = ROOT / 'engine/tests/fixtures/benchmark-sample-before-settlement.py.txt'
    assert hashlib.sha256(path.read_bytes()).hexdigest() == \
        'e9e65689ecff7dd714e7e1f349c442c02f4ad4992c77532d58c45a7bd2f2aaa1'
    loader = SourceFileLoader('historical_benchmark_sample', str(path))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    old = importlib.util.module_from_spec(spec)
    with monkeypatch.context() as patch:
        patch.setattr(sys, 'dont_write_bytecode', True)
        patch.setattr(sys, 'path', [str(SCRIPTS), *sys.path])
        loader.exec_module(old)
        with pytest.raises(AssertionError, match='Natural benchmark settlement was refused'):
            check(old, 'transient')
    for mode in ('transient', 'active', 'unknown', 'expired', 'late'):
        check(sample, mode)


def test_benchmark_peak_requires_supported_platform_and_positive_os_observation(monkeypatch):
    import pytest
    from types import SimpleNamespace

    sample = load('benchmark_sample')
    calls = []
    monkeypatch.setitem(sys.modules, 'resource', SimpleNamespace(
        RUSAGE_CHILDREN=-1,
        getrusage=lambda who: calls.append(who) or SimpleNamespace(ru_maxrss=1234)))
    monkeypatch.setattr(sys, 'platform', 'linux')
    assert sample.linux_peak() == {'metric': 'peak-child-rss-bytes', 'bytes': 1234 * 1024}
    assert calls == [-1]
    for invalid in (0, -1, True, 1.5):
        monkeypatch.setitem(sys.modules, 'resource', SimpleNamespace(
            RUSAGE_CHILDREN=-1, getrusage=lambda who, value=invalid: SimpleNamespace(ru_maxrss=value)))
        with pytest.raises(ValueError, match='positive integer'):
            sample.linux_peak()
    monkeypatch.setattr(sys, 'platform', 'darwin')
    with pytest.raises(ValueError, match='Linux'):
        sample.linux_peak()


def test_benchmark_windows_assigns_job_at_birth_and_closes_failed_creation(monkeypatch):
    import pytest
    from types import SimpleNamespace

    windows = load('benchmark_windows')
    events = []
    fail = [False]

    class Kernel:
        def InitializeProcThreadAttributeList(self, buffer, count, flags, size):
            assert count == 2
            size._obj.value = 128
            return True
        def UpdateProcThreadAttribute(self, buffer, flags, attribute, values, size, unused, previous):
            events.append(attribute)
            return True
        def DeleteProcThreadAttributeList(self, buffer):
            events.append('delete-attributes')
        def CreateProcessW(self, app, command, security, thread_security, inherit, flags,
                           environment, cwd, startup, info):
            assert events == [0x2000D, 0x20002]
            assert flags & 0x80000 and flags & 0x08000000 and inherit
            assert startup._obj.flags == 0x100
            events.append('create')
            if fail[0]:
                raise OSError('synthetic creation refusal')
            info._obj.process, info._obj.thread, info._obj.pid = 123, 456, 789
            return True
        def CloseHandle(self, handle):
            events.append(('close', handle))
            return True

    kernel = Kernel()
    monkeypatch.setattr(windows, 'api', lambda: kernel)
    monkeypatch.setitem(sys.modules, 'msvcrt', SimpleNamespace(get_osfhandle=lambda fd: fd))
    monkeypatch.setitem(sys.modules, 'numera.winjob', SimpleNamespace(_job_open=lambda name: 99))
    process, tree, job = windows.spawn(['fixture.exe', '--json'], {'PYTHONUTF8': '1'})
    assert tree.pid == 789 and tree.kind == 'job' and job == 99
    assert events[-1] == ('close', 456) and ('close', 99) not in events
    process.stdout.close()
    process.stderr.close()
    process.close()
    kernel.CloseHandle(job)
    assert ('close', 123) in events
    events.clear()
    fail[0] = True
    with pytest.raises(OSError, match='synthetic creation refusal'):
        windows.spawn(['fixture.exe'], {})
    assert events[-1] == ('close', 99)


def test_benchmark_creation_compares_new_dashboard_content_not_just_existence(tmp_path, monkeypatch):
    import pytest

    benchmark = load('benchmark')
    template = tmp_path / 'template'
    template.mkdir()
    config = {'default_engine': 'playwright', 'future': {'items': [42, True]}}
    (template / 'config.json').write_bytes(json.dumps(config, indent=2).replace('\n', '\r\n').encode('utf-8'))
    settings = [config]
    newline = ['\n']
    dashboard = [{'id': 'new', 'name': 'Measured creation', '_rev': 1, '_created': 1000.0}]
    lock_content = [b'']
    extra_lock = [False]

    def observe(argv, env, **kwargs):
        root = Path(argv[argv.index('--data-dir') + 1])
        (root / 'config.json').write_bytes(json.dumps(settings[0], indent=2).replace('\n', newline[0]).encode('utf-8'))
        profile = {'id': 'new', 'name': 'Measured creation', 'created_at': 1000.0, 'updated_at': 1001.0}
        for directory, record in (('profiles', profile), ('dashboard', dashboard[0])):
            (root / directory).mkdir()
            path = root / directory / 'new.json'
            path.write_text(json.dumps(record), encoding='utf-8')
            import hashlib
            lock = path.parent / '.locks' / f'new.json.{hashlib.sha256(str(path.resolve()).encode("utf-8")).hexdigest()[:16]}.lock'
            lock.parent.mkdir()
            lock.write_bytes(lock_content[0])
            if extra_lock[0]:
                (lock.parent / 'unrelated.lock').write_bytes(b'')
        return 0, json.dumps({'elapsed_ns': 1, 'memory': {'metric': 'fixture', 'bytes': 1},
                             'payload': {'ok': True, 'profile': profile}}), ''

    monkeypatch.setattr(benchmark, 'execute', observe)
    args = (tmp_path / 'candidate', tmp_path, tmp_path, template, {}, 'native', 'create', '')
    _, expected = benchmark.observation(*args)
    newline[0] = '\r\n'
    assert benchmark.observation(*args)[1] == expected
    for changed in ({**config, 'default_engine': 'camoufox'},
                    {**config, 'future': {'items': [42, 1]}},
                    {'default_engine': 'playwright'}, {**config, 'unexpected': True}, None):
        settings[0] = changed
        with pytest.raises(ValueError, match='settings beyond its requested engine'):
            benchmark.observation(*args)
    settings[0] = config
    dashboard[0] = {**dashboard[0], 'name': 'lost identity'}
    _, changed = benchmark.observation(*args)
    assert changed != expected, 'Benchmark discarded the new dashboard fields before comparison'
    dashboard[0] = {}
    with pytest.raises(ValueError, match='dashboard identity'):
        benchmark.observation(*args)
    dashboard[0] = {'id': 'new', 'name': 'Measured creation', '_rev': 1, '_created': 1000.0}
    lock_content[0] = b'not bookkeeping'
    with pytest.raises(ValueError, match='unrelated protected record'):
        benchmark.observation(*args)
    lock_content[0] = b''
    extra_lock[0] = True
    with pytest.raises(ValueError, match='unrelated protected record'):
        benchmark.observation(*args)


def test_frozen_reference_rejects_drift_and_keeps_current_python_modules(tmp_path, monkeypatch):
    import pytest
    baseline = load('baseline_source')
    monkeypatch.setattr(baseline, 'ROOT', tmp_path)
    monkeypatch.delenv(baseline.ENVIRONMENT, raising=False)
    names = ('engine/numera/__init__.py', 'engine/tests/test_original.py')
    contents = (b'VALUE = "original"\n', b'def test_original(): pass\n')
    rows = tuple((name, baseline.blob_oid(raw), len(raw), False) for name, raw in zip(names, contents))
    monkeypatch.setattr(baseline, 'entries', lambda: rows)
    calls = []
    def batch(*arguments, data):
        assert arguments == ('cat-file', '--batch')
        calls.append(data)
        return b''.join(f'{oid} blob {size}\n'.encode('ascii') + raw + b'\n'
                        for (_, oid, size, _), raw in zip(rows, contents))
    monkeypatch.setattr(baseline, 'git', batch)
    with baseline.reference() as root:
        assert baseline.verify(root) == root
        assert (root / names[0]).read_bytes() == contents[0]
        (root / names[0]).write_bytes(b'VALUE = "changed"\n')
        with pytest.raises(ValueError, match='bytes differ'):
            baseline.verify(root)
        (root / names[0]).write_bytes(contents[0])
        (root / 'unexpected.py').write_bytes(b'raise RuntimeError("unreviewed")\n')
        with pytest.raises(ValueError, match='inventory differs'):
            baseline.verify(root)
    assert not root.exists() and len(calls) == 1
    from types import SimpleNamespace
    monkeypatch.setattr(baseline, 'source_root', lambda: tmp_path / 'original')
    monkeypatch.setitem(sys.modules, 'numera', SimpleNamespace(__file__=str(tmp_path / 'current/numera/__init__.py')))
    with pytest.raises(RuntimeError, match='nonbaseline'):
        baseline.activate()


def test_original_observer_imports_reference_not_current_path(tmp_path):
    import os
    import subprocess
    fake = tmp_path / 'current/numera'
    fake.mkdir(parents=True)
    (fake / '__init__.py').write_text('raise RuntimeError("current source leaked")\n', encoding='utf-8')
    program = '''import json, os, pathlib, sys
import run_logging
assert "numera" not in sys.modules, "Logging utility pre-imported current source"
import baseline_source as baseline
baseline.ROOT = pathlib.Path(os.environ["FIXTURE_ROOT"])
raw = b'VALUE = "original"\\n'
oid = baseline.blob_oid(raw)
baseline.entries = lambda: (("engine/numera/__init__.py", oid, len(raw), False),)
calls = []
def git(*arguments, data):
    calls.append(data)
    return (oid + " blob " + str(len(raw)) + "\\n").encode("ascii") + raw + b"\\n"
baseline.git = git
root = baseline.activate()
import numera
assert numera.VALUE == "original"
assert pathlib.Path(numera.__file__).is_relative_to(root / "engine")
assert baseline.activate() == root and len(calls) == 1
print(json.dumps({"root": str(root), "baseline": baseline.BASELINE_TREE}))
'''
    environment = {**os.environ, 'PYTHONPATH': os.pathsep.join((str(SCRIPTS), str(fake.parent))),
                   'PYTHONUTF8': '1', 'FIXTURE_ROOT': str(tmp_path)}
    environment.pop('NUMERA_ORACLE_SOURCE_ROOT', None)
    completed = subprocess.run([sys.executable, '-c', program], env=environment, stdin=subprocess.DEVNULL,
                               capture_output=True, encoding='utf-8', errors='strict', timeout=10, check=True)
    result = json.loads(completed.stdout)
    assert result['baseline'] == '760f9873ded13c5ebfecae1f16b385e34f695124'
    assert not Path(result['root']).exists(), 'Owned original-source reference survived observer exit'


def test_empty_missing_and_forged_ledgers_cannot_allow_switch(tmp_path, monkeypatch):
    gate = load('check_coverage')
    source = {'files':[{'path':'source.py'}], 'capabilities':[{'id':'source.py::required'}]}
    monkeypatch.setattr(gate, 'discover', lambda: source)
    path = tmp_path / 'coverage.json'
    for document in ([], {}, {'capabilities':[]},
                     {'files':source['files'], 'capabilities':[{'id':'source.py::other','status':'verified'}]},
                     {'files':source['files'], 'capabilities':[{'id':'source.py::required','status':'pending'}]},
                     {'files':source['files'], 'capabilities':[{'id':'source.py::required','status':'verified','target':'../outside','test':'../outside'}]}):
        path.write_text(json.dumps(document), encoding='utf-8')
        assert gate.check(path), 'Unproved coverage was accepted'


def test_owned_migration_command_propagates_exit_and_bounds_idle(tmp_path):
    helper = load('run_owned')
    with helper.run_log('owned-command-fixture', tmp_path) as logger:
        assert helper.run([sys.executable, '-c', 'print("ready"); raise SystemExit(7)'], 10, 5, logger) == 7
        # An Event wait models a silent blocked child; the outer owner must end it.
        import pytest
        with pytest.raises(TimeoutError):
            helper.run([sys.executable, '-c', 'import threading; threading.Event().wait()'], 5, 0.5, logger)
    assert not logger.handlers
    text = '\n'.join(path.read_text(encoding='utf-8') for path in tmp_path.glob('owned-command-fixture_*_UTC*.log'))
    for phase in ('spawn', 'monitor', 'settle-tree', 'reap-leader', 'join-reader', 'close-output', 'remove-temp'):
        assert 'phase=' + phase in text


def test_linux_temp_root_fits_chromium_socket_and_keeps_windows_contained(monkeypatch):
    from pathlib import PurePosixPath
    import pytest

    helper = load('run_owned')
    monkeypatch.setattr(helper, 'Path', PurePosixPath)
    hosted = {'GITHUB_ACTIONS': 'true', 'RUNNER_TEMP': '/home/runner/work/_temp'}
    root = PurePosixPath('/srv/numera')
    selected = helper.temporary_parent(root, hosted, 'linux')
    assert selected == root / '.ci-work'
    assert len(str(selected / 't000000').encode('utf-8')) + len(helper.CHROMIUM_SOCKET_SUFFIX) <= 107
    checkout = PurePosixPath('/home/runner/work/Numera-Browser/Numera-Browser')
    assert helper.temporary_parent(checkout, hosted, 'linux') == PurePosixPath(hosted['RUNNER_TEMP'])
    boundary = PurePosixPath('/' + 'a' * (107 - len('/.ci-work/t000000') - len(helper.CHROMIUM_SOCKET_SUFFIX) - 1))
    assert helper.temporary_parent(boundary, hosted, 'linux') == boundary / '.ci-work'
    assert helper.temporary_parent(PurePosixPath(str(boundary) + 'a'), hosted, 'linux') == PurePosixPath(hosted['RUNNER_TEMP'])
    long_root = PurePosixPath('/' + 'a' * 100)
    assert helper.temporary_parent(long_root, hosted, 'linux') == PurePosixPath(hosted['RUNNER_TEMP'])
    assert helper.temporary_parent(long_root, hosted, 'win32') == long_root / '.ci-work'
    unicode_root = PurePosixPath('/' + 'Ω' * 40)
    assert helper.temporary_parent(unicode_root, hosted, 'linux') == PurePosixPath(hosted['RUNNER_TEMP'])
    with pytest.raises(ValueError, match='hosted RUNNER_TEMP'):
        helper.temporary_parent(long_root, {}, 'linux')
    with pytest.raises(ValueError, match='also exceeds'):
        helper.temporary_parent(long_root, {**hosted, 'RUNNER_TEMP': '/' + 'x' * 100}, 'linux')


def test_owned_temp_is_unique_and_cleanup_removes_only_its_run(tmp_path, monkeypatch):
    helper = load('run_owned')
    root = tmp_path / 'project with spaces'
    monkeypatch.setattr(helper, 'ROOT', root)
    created = []
    create = helper.create_temporary

    def record(*args):
        path = create(*args)
        created.append(path)
        return path

    monkeypatch.setattr(helper, 'create_temporary', record)
    program = '''import os,socket,sys
from pathlib import Path
if sys.platform == "linux":
    directory=Path(os.environ["TMPDIR"])/"org.chromium.Chromium.123456"
    directory.mkdir()
    path=directory/"SingletonSocket"
    assert len(str(path).encode("utf-8")) <= 107
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as listener:
        listener.bind(str(path))
        print("actual singleton socket bound",flush=True)
else:
    root=Path(os.environ["TMPDIR"])
    assert root.is_dir() and root.name.startswith("t")
    print("owned platform temp resolved",flush=True)
'''
    with helper.run_log('temp-fixture', tmp_path) as logger:
        assert helper.run([sys.executable, '-c', program], 10, 5, logger) == 0
    assert len(created) == 1 and not created[0].exists()
    assert created[0].parent == helper.temporary_parent(root, __import__('os').environ, sys.platform)
    first = helper.create_temporary(root, __import__('os').environ, sys.platform)
    second = helper.create_temporary(root, __import__('os').environ, sys.platform)
    try:
        assert first != second and first.is_dir() and second.is_dir()
    finally:
        first.rmdir()
        second.rmdir()


def test_migration_logs_are_unique_sanitized_closed_and_fallback(tmp_path, capsys):
    helper = load('run_logging')
    directory = tmp_path / 'path with spaces'
    for _ in range(2):
        with helper.run_log('fixture', directory) as logger:
            logger.warning('password=synthetic-fixture-value')
    files = list(directory.glob('fixture_*_UTC*.log'))
    assert len(files) == 2
    text = ''.join(path.read_text(encoding='utf-8') for path in files)
    assert 'synthetic-fixture-value' not in text + capsys.readouterr().err
    assert '[WARNING]' in text and 'UTC]' in text and 'exit=0' in text
    for path in files:
        path.unlink()  # Windows cannot remove an open file handler.
    blocked = tmp_path / 'not-a-directory'
    blocked.write_text('fixture', encoding='utf-8')
    with helper.run_log('fallback-fixture', blocked) as logger:
        logger.error('token=synthetic-fixture-value')
    console = capsys.readouterr().err
    assert 'console only' in console and 'synthetic-fixture-value' not in console
