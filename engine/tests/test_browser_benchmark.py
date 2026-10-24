"""Real-browser comparisons keep complete cookie results and failed synthetic inputs."""
from __future__ import annotations

import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    spec = importlib.util.spec_from_file_location(
        'browser_benchmark_fixture', ROOT / 'scripts/migration/benchmark_browser.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fixture(tmp_path):
    root = tmp_path / 'baseline'
    (root / 'profiles').mkdir(parents=True)
    (root / 'profiles/fixture.json').write_text('{"future":[true,7]}', encoding='utf-8')
    session = root / 'user-data/fixture'
    session.mkdir(parents=True)
    (session / 'sentinel.bin').write_bytes(b'\x00\xffretained session\r\n')
    return root


@pytest.mark.parametrize('mode', ['normal', 'seed-refusal', 'lost-cookie', 'metadata'])
def test_browser_baseline_uses_original_and_checks_actual_persistence(tmp_path, monkeypatch, mode):
    helper = load(monkeypatch)
    identifier = '0123456789ab'
    calls = []
    seeded = []

    def execute(argv, env, *, timeout):
        calls.append(argv)
        assert argv[:3] == [helper.sys.executable, '-m', 'numera']
        assert env == {'PYTHONPATH': 'immutable-original'}
        root = Path(argv[argv.index('--data-dir') + 1])
        operation = argv[5]
        if operation == 'create':
            assert timeout == 15 and argv[-3:] == ['--engine', 'camoufox', '--json']
            (root / 'user-data' / identifier).mkdir(parents=True)
            payload = {'profile': {'id': identifier}}
        elif argv[6] == 'import':
            assert timeout == 100 and argv[-2:] == ['--clear', '--json']
            seeded.extend(json.loads(Path(argv[8]).read_text(encoding='utf-8'))['cookies'])
            if mode == 'seed-refusal':
                return 1, '{}', ''
            payload = {'ok': True, 'imported': 2, 'cleared': True}
        else:
            assert timeout == 100 and argv[6] == 'export'
            destination = Path(argv[8])
            rows = seeded[:1] if mode == 'lost-cookie' else list(reversed(seeded))
            destination.write_text(json.dumps({'cookies': rows, 'origins': []}), encoding='utf-8')
            payload = {'ok': True, 'count': 1 if mode == 'metadata' else 2,
                       'file': str(destination), 'format': 'json'}
        return 0, json.dumps(payload), ''

    monkeypatch.setattr(helper, 'execute', execute)
    if mode == 'normal':
        root, actual, payload = helper.baseline(tmp_path, {'PYTHONPATH': 'immutable-original'}, 'camoufox')
        assert actual == identifier and payload['cookies'] == list(reversed(seeded))
        assert (root / 'user-data' / identifier / 'sentinel.bin').read_bytes() == helper.SENTINEL
        assert len(calls) == 3
    else:
        with pytest.raises(ValueError, match='baseline|persistence|metadata'):
            helper.baseline(tmp_path, {'PYTHONPATH': 'immutable-original'}, 'camoufox')


def test_browser_environment_binds_sdk_without_inheriting_real_application_state(tmp_path, monkeypatch):
    helper = load(monkeypatch)
    monkeypatch.setenv('NUMERA_PASSWORD', 'must-not-forward')
    monkeypatch.setenv('NUMERA_API_TOKEN', 'must-not-forward')
    monkeypatch.setenv('NUMERA_DATA_DIR', 'must-not-forward')
    monkeypatch.setenv('NUMERA_HELPER_UNITS', 'retained-parent')
    monkeypatch.setenv('XDG_CACHE_HOME', 'existing-sdk-cache')
    bound = []
    monkeypatch.setattr(helper, 'bind_sdk_cache', lambda env, engine: bound.append(engine))
    env = helper.environment(tmp_path / 'immutable', tmp_path, 'camoufox')
    assert bound == ['camoufox']
    assert env['HOME'] == env['USERPROFILE'] == str(tmp_path / 'synthetic home')
    assert env['PYTHONPATH'] == str(tmp_path / 'immutable/engine')
    assert env['NUMERA_BROWSER_ADAPTER'] == str(ROOT / 'scripts/browser-adapter/adapter.py')
    assert env['NUMERA_SDK_PYTHON'] == helper.sys.executable
    assert env['NUMERA_HELPER_UNITS'] == 'retained-parent' and env['XDG_CACHE_HOME'] == 'existing-sdk-cache'
    assert all(key not in env for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR'))


@pytest.mark.parametrize('mode', ['normal', 'cookie', 'order', 'type', 'profile', 'sentinel', 'metadata', 'exit'])
def test_browser_pair_checks_full_export_and_preserves_failed_inputs(tmp_path, monkeypatch, mode):
    helper = load(monkeypatch)
    root = fixture(tmp_path)
    cookies = [{'name': 'one', 'expires': 1900000000}, {'name': 'two', 'httpOnly': True}]
    calls = []

    def execute(argv, env, *, timeout):
        calls.append(argv)
        assert timeout == 100 and env == {'fixture': 'environment'}
        assert '--wall' in argv and argv[argv.index('--wall') + 1] == '90'
        assert '--nested' in argv and argv[-7:-4] == ['cookies', 'export', 'fixture']
        copied = Path(argv[argv.index('--data-dir') + 1])
        export = Path(argv[-4])
        assert copied != root and copied.name == 'data with spaces'
        assert (copied / 'profiles/fixture.json').read_bytes() == (root / 'profiles/fixture.json').read_bytes()
        rows = json.loads(json.dumps(cookies))
        if mode == 'cookie':
            rows[0]['name'] = 'changed'
        elif mode == 'order':
            rows.reverse()
        elif mode == 'type':
            rows[1]['httpOnly'] = 1
        elif mode == 'profile':
            (copied / 'profiles/fixture.json').write_text('{}', encoding='utf-8')
        elif mode == 'sentinel':
            (copied / 'user-data/fixture/sentinel.bin').write_bytes(b'changed')
        export.write_text(json.dumps({'cookies': rows}), encoding='utf-8')
        payload = {'ok': True, 'count': 2, 'file': str(export), 'format': 'json'}
        if mode == 'metadata':
            payload['future'] = True
        sample = {'elapsed_ns': 7, 'memory': {'metric': 'fixture', 'bytes': 1234}, 'payload': payload}
        return (1 if mode == 'exit' else 0), json.dumps(sample), ''

    monkeypatch.setattr(helper, 'execute', execute)
    if mode in ('profile', 'sentinel', 'metadata', 'exit'):
        with pytest.raises(ValueError, match='protected|sentinel|metadata|observation'):
            helper.observation(tmp_path / 'candidate', tmp_path, tmp_path, root,
                               {'fixture': 'environment'}, 'native', 'fixture')
    else:
        sample, payload = helper.observation(tmp_path / 'candidate', tmp_path, tmp_path, root,
                                             {'fixture': 'environment'}, 'native', 'fixture')
        assert sample == {'elapsed_ns': 7, 'memory': {'metric': 'fixture', 'bytes': 1234}}
        assert (payload == helper.exact_json({'cookies': cookies})) == (mode == 'normal')
    assert len(calls) == 1 and len(list(tmp_path.glob('browser-pair-*'))) == 1
    assert (root / 'profiles/fixture.json').read_text(encoding='utf-8') == '{"future":[true,7]}'


@pytest.mark.parametrize('mode', ['normal', 'parity', 'inconclusive', 'metric', 'write'])
def test_browser_run_requires_all_pairs_and_retains_unaccepted_evidence(tmp_path, monkeypatch, mode):
    helper = load(monkeypatch)
    temporary = tmp_path / '.ci-work/migration-temp'
    assert not temporary.exists(), 'Clean checkout must not rely on a precreated benchmark directory'
    candidate = tmp_path / 'candidate'
    candidate.write_bytes(b'inert identity-bound binary')
    destination = tmp_path / 'measurements.json'
    if mode == 'write':
        destination.write_text('prior evidence', encoding='utf-8')
    monkeypatch.setattr(helper, 'ROOT', tmp_path)
    monkeypatch.setattr(helper, 'artifact', lambda *args, **kwargs: candidate)
    monkeypatch.setattr(helper.sys, 'platform', 'linux')
    monkeypatch.setenv('GITHUB_ACTIONS', 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('GITHUB_SHA', 'a' * 40)
    monkeypatch.setenv('GITHUB_RUN_ID', '123')
    import baseline_source
    monkeypatch.setattr(baseline_source, 'source_root', lambda: tmp_path)
    identity = {'source_sha': 'a' * 40, 'generation': 123, 'application_runtime': 'native-candidate'}
    monkeypatch.setattr(helper, 'execute', lambda *args, **kwargs: (0, json.dumps(identity), ''))
    monkeypatch.setattr(helper, 'environment', lambda *args: {})
    baseline_payload = {'cookies': [{'name': 'one'}, {'name': 'two'}], 'origins': []}
    monkeypatch.setattr(helper, 'baseline', lambda *args: (tmp_path, 'fixture', baseline_payload))
    observed = []

    def observation(candidate, original, parent, template, env, runtime, identifier):
        with helper.fixture_directory('browser-pair-', parent, retain=True) as pair:
            (pair / 'input.json').write_text('{"synthetic":true}', encoding='utf-8')
            observed.append(runtime)
            value = 1 if runtime == 'native' and mode != 'inconclusive' else 100
            metric = runtime if mode == 'metric' else 'fixture'
            return {'elapsed_ns': value, 'memory': {'metric': metric, 'bytes': value}}, (
                runtime if mode == 'parity' else helper.exact_json(baseline_payload))

    monkeypatch.setattr(helper, 'observation', observation)
    logger = SimpleNamespace(info=lambda *args: None)
    if mode == 'normal':
        assert helper.run(tmp_path / 'report', destination, 'playwright', logger) == 0
        assert not list(temporary.iterdir())
    else:
        with pytest.raises((ValueError, FileExistsError), match='parity|inconclusive|metric|exists'):
            helper.run(tmp_path / 'report', destination, 'playwright', logger)
        retained = list(temporary.glob('browser-benchmark-*'))
        assert len(retained) == 1
        assert len(list(retained[0].glob('browser-pair-*/input.json'))) == len(observed)
    if mode != 'parity':
        assert observed == [runtime for index in range(-1, 7) for runtime in
                            (('original', 'native') if index % 2 == 0 else ('native', 'original'))]
    if mode == 'write':
        assert destination.read_text(encoding='utf-8') == 'prior evidence'
    elif mode in ('normal', 'inconclusive'):
        evidence = json.loads(destination.read_text(encoding='utf-8'))
        assert evidence['engine'] == 'playwright' and evidence['repetitions'] == 7
        assert evidence['warmup_pairs'] == 1 and len(evidence['results'][0]['samples']) == 7


@pytest.mark.parametrize('mode', [
    'normal', 'pending-self', 'absent', 'missing-self', 'wrong-self',
    'incomplete-before', 'incomplete', 'live', 'cleanup', 'wait',
])
def test_browser_sample_requires_natural_nested_settlement_before_peak(monkeypatch, mode):
    import io
    import sys
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    import benchmark_sample as sample
    from numera import unit_report

    prior = sample.proctree.Tree(41, 4100, 41, 'pgid')
    observer = sample.proctree.Tree(42, 4200, 42, 'pgid')
    direct = sample.proctree.Tree(43, 4300, 43, 'pgid')
    sdk = sample.proctree.Tree(44, 4400, 44, 'pgid')
    clock, calls, reads, sleeps = [0.0], [], [0], []

    def sleep(seconds):
        sleeps.append(('capture' if 'spawn' in calls else 'registration', seconds))
        clock[0] += seconds

    def wait(**kwargs):
        if mode == 'wait' and kwargs['timeout'] == 5:
            raise TimeoutError('Synthetic leader reap failure')
        return 0

    process = SimpleNamespace(stdout=io.BytesIO(b'{"ok":true}'), stderr=io.BytesIO(),
                              poll=lambda: 0, wait=wait)

    def receipt(path):
        reads[0] += 1
        if mode == 'missing-self':
            return SimpleNamespace(complete=True, trees=[prior])
        if mode == 'wrong-self':
            return SimpleNamespace(complete=True, trees=[prior, sample.proctree.Tree(42, 9999, 42, 'pgid')])
        if mode == 'incomplete-before' or (mode == 'pending-self' and reads[0] == 1):
            return SimpleNamespace(complete=False, trees=[prior])
        return SimpleNamespace(complete=mode != 'incomplete' or 'spawn' not in calls,
                               trees=[prior, observer, *([direct, sdk] if 'spawn' in calls else [])])

    def spawn(*args, **kwargs):
        calls.append('spawn')
        assert kwargs['stdin'] is sample.subprocess.DEVNULL
        return process, direct

    def prove(tree, *, deadline):
        assert 0 < deadline <= 90 and tree not in (prior, observer)
        calls.append('sdk-proof' if tree is sdk else 'direct-proof')
        return tree is direct or mode != 'live'

    def cleanup(tree, **kwargs):
        assert tree not in (prior, observer)
        calls.append('sdk-cleanup' if tree is sdk else 'direct-cleanup')
        return mode != 'cleanup' or tree is not sdk

    if mode == 'absent':
        monkeypatch.delenv(unit_report.UNITS_ENV, raising=False)
    else:
        monkeypatch.setenv(unit_report.UNITS_ENV, 'synthetic-receipt')
    monkeypatch.setattr(unit_report, 'read', receipt)
    monkeypatch.setattr(unit_report, 'begin_spawn', lambda: 'intent')
    monkeypatch.setattr(unit_report, 'record', lambda *args, **kwargs: None)
    monkeypatch.setattr(sample.sys, 'platform', 'linux')
    monkeypatch.setitem(sys.modules, 'resource', SimpleNamespace(
        RUSAGE_CHILDREN=-1, getrusage=lambda who: SimpleNamespace(ru_maxrss=0)))
    monkeypatch.setattr(sample.proctree, 'spawn', spawn)
    monkeypatch.setattr(sample.proctree, 'prove_empty', prove)
    monkeypatch.setattr(sample.proctree, 'end_tree', cleanup)
    monkeypatch.setattr(sample, 'pipe_reader', lambda pipe: lambda: pipe.read())
    monkeypatch.setattr(sample.os, 'getpid', lambda: 42)
    monkeypatch.setattr(sample.proctree, '_inherited_tree', lambda: observer)
    monkeypatch.setattr(sample.time, 'monotonic', lambda: clock[0])
    monkeypatch.setattr(sample.time, 'sleep', sleep)
    monkeypatch.setattr(sample.time, 'perf_counter_ns', lambda: 7)
    monkeypatch.setattr(sample, 'linux_peak', lambda: calls.append('peak') or
                        {'metric': 'fixture', 'bytes': 1234})
    logger = SimpleNamespace(error=lambda *args: None)
    if mode in ('normal', 'pending-self'):
        assert sample.observe(['inert'], {}, logger, wall=90, nested=True)['payload'] == {'ok': True}
        assert calls.index('sdk-proof') < calls.index('peak') < calls.index('sdk-cleanup')
        assert sleeps == ([('registration', .01)] if mode == 'pending-self' else []) + [('capture', .001)]
    else:
        with pytest.raises((ValueError, RuntimeError, TimeoutError), match='receipt|identity|nested|cleanup|reap'):
            sample.observe(['inert'], {}, logger, wall=90, nested=True)
        if mode not in ('cleanup', 'wait'):
            assert 'peak' not in calls
    if mode in ('absent', 'missing-self', 'wrong-self', 'incomplete-before'):
        assert 'spawn' not in calls
    else:
        assert process.stdout.closed and process.stderr.closed
        assert calls.count('direct-cleanup') == calls.count('sdk-cleanup') == 1


@pytest.mark.parametrize('damage', [None, 'missing', 'shallow', 'conditional', 'duplicate', 'late'])
def test_browser_workflow_requires_original_history_before_acceptance(monkeypatch, damage):
    import yaml

    spec = importlib.util.spec_from_file_location('browser_history_fixture', ROOT / 'scripts/ci/suites.py')
    coverage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(coverage)
    path = ROOT / '.github/workflows/ci.yml'
    document = yaml.safe_load(path.read_text(encoding='utf-8'))
    steps = document['jobs']['engine-live']['steps']
    checkout = next(step for step in steps if step.get('uses', '').startswith('actions/checkout@'))
    if damage == 'missing':
        steps.remove(checkout)
    elif damage == 'shallow':
        checkout['with']['fetch-depth'] = 1
    elif damage == 'conditional':
        checkout['if'] = 'false'
    elif damage == 'duplicate':
        steps.append(dict(checkout))
    elif damage == 'late':
        steps.remove(checkout)
        steps.append(checkout)
    changed = yaml.safe_dump(document)
    read_text = Path.read_text
    monkeypatch.setattr(Path, 'read_text', lambda selected, *args, **kwargs:
                        changed if selected == path else read_text(selected, *args, **kwargs))
    if damage:
        with pytest.raises(ValueError, match='original history'):
            coverage.check_workflows()
    else:
        assert coverage.check_workflows()['python'] > 0


@pytest.mark.parametrize('damage', ['unoptimized', 'missing', 'softened', 'skipped', 'artifact', 'order'])
def test_browser_workflow_guard_rejects_missing_or_softened_measurement(monkeypatch, damage):
    import yaml

    spec = importlib.util.spec_from_file_location('browser_workflow_fixture', ROOT / 'scripts/ci/suites.py')
    coverage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(coverage)
    action = ROOT / '.github/actions/native-candidate-acceptance/action.yml'
    text = action.read_text(encoding='utf-8')
    assert text.count('python scripts/migration/candidate_stage.py') == 1
    document = yaml.safe_load(text)
    steps = document['runs']['steps']
    if damage == 'unoptimized':
        steps[3]['run'] = steps[3]['run'].replace(' --release', '')
    elif damage == 'missing':
        steps.pop(5)
    elif damage == 'softened':
        steps[5]['continue-on-error'] = True
    elif damage == 'skipped':
        steps[5]['if'] = 'false'
    elif damage == 'artifact':
        steps[6]['with']['if-no-files-found'] = 'warn'
    else:
        steps[4], steps[5] = steps[5], steps[4]
    read_text = Path.read_text
    changed = yaml.safe_dump(document)
    monkeypatch.setattr(Path, 'read_text', lambda path, *args, **kwargs:
                        changed if path == action else read_text(path, *args, **kwargs))
    with pytest.raises(ValueError, match='acceptance|measurements'):
        coverage.check_workflows()
