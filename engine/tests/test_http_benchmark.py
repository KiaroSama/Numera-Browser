"""HTTP performance observes the real boundary without accepting failed teardown."""
from __future__ import annotations

import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[2]
pytestmark = pytest.mark.timeout(10)


def load(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    spec = importlib.util.spec_from_file_location(
        'http_benchmark_fixture', ROOT / 'scripts/migration/benchmark_http_sample.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_http_observer_bootstraps_under_isolated_python_from_an_unrelated_directory(tmp_path):
    result = subprocess.run(
        [sys.executable, '-I', str(ROOT / 'scripts/migration/benchmark_http_sample.py'), '--help'],
        cwd=tmp_path, stdin=subprocess.DEVNULL, capture_output=True, encoding='utf-8', timeout=5,
    )
    assert result.returncode == 0, result.stderr
    assert '--runtime' in result.stdout and '--data-dir' in result.stdout
    assert not result.stderr


@pytest.mark.parametrize('mode', ['normal', 'request', 'early-exit', 'receipt', 'settlement', 'wait'])
def test_http_sample_measures_only_request_and_rejects_failed_ownership(monkeypatch, mode):
    helper = load(monkeypatch)
    clock, calls = [0], []
    process = SimpleNamespace(stdout=io.BytesIO(), stderr=io.BytesIO(), returncode=None)
    tree = SimpleNamespace(pid=42)

    def poll():
        return 1 if mode == 'early-exit' else None

    def wait(timeout):
        calls.append('wait')
        assert timeout == 5
        if mode == 'wait':
            raise TimeoutError('Synthetic reap failure')
        process.returncode = -15
        return -15

    process.poll, process.wait = poll, wait

    def spawn(argv, **options):
        calls.append('spawn')
        assert argv == ['inert-candidate', 'serve', '--host', '127.0.0.1', '--port', '12345', '--json']
        assert options['env'] == {'NUMERA_API_TOKEN': 'benchmark-only'}
        assert options['stdin'] is helper.subprocess.DEVNULL
        assert options['stdout'] is options['stderr'] is helper.subprocess.PIPE
        return process, tree

    def record(owned, **kwargs):
        assert owned is tree
        calls.append('record')
        if mode == 'receipt':
            raise RuntimeError('Synthetic receipt failure')

    def request(port, path, timeout):
        assert port == 12345 and 0 < timeout <= 2
        calls.append(path)
        if path == '/api/ready':
            clock[0] += 1000
            return {'service': 'numera-browser', 'ok': True}
        clock[0] += 7
        if mode == 'request':
            raise ValueError('Synthetic invalid HTTP response')
        return [{'id': 'fixture'}]

    def end_tree(owned, **kwargs):
        assert owned is tree and kwargs == {'grace': 1, 'kill_grace': 3}
        calls.append('settle')
        return mode != 'settlement'

    monkeypatch.setattr(helper.sys, 'platform', 'linux')
    monkeypatch.setattr(helper, 'require_fresh_observer', lambda: None)
    monkeypatch.setattr(helper, 'free_port', lambda: 12345)
    monkeypatch.setattr(helper.proctree, 'spawn', spawn)
    monkeypatch.setattr(helper.proctree, 'end_tree', end_tree)
    monkeypatch.setattr(helper.unit_report, 'begin_spawn', lambda: 'attempt')
    monkeypatch.setattr(helper.unit_report, 'record', record)
    monkeypatch.setattr(helper, 'request', request)
    monkeypatch.setattr(helper.time, 'monotonic', lambda: 0)
    monkeypatch.setattr(helper.time, 'perf_counter_ns', lambda: clock[0])
    monkeypatch.setattr(helper, 'linux_peak', lambda: calls.append('peak') or
                        {'metric': 'peak-child-rss-bytes', 'bytes': 1234})
    monkeypatch.setattr(helper, 'pipe_reader', lambda pipe: lambda: pipe.read())
    logger = SimpleNamespace(error=lambda *args: None)
    if mode == 'normal':
        result = helper.observe(['inert-candidate'], {}, logger)
        assert result == {'elapsed_ns': 7, 'memory': {'metric': 'peak-child-rss-bytes', 'bytes': 1234},
                          'payload': [{'id': 'fixture'}]}
        assert calls.index('peak') > calls.index('wait') > calls.index('/api/profiles')
    else:
        with pytest.raises((ValueError, RuntimeError, TimeoutError)):
            helper.observe(['inert-candidate'], {}, logger)
        assert 'peak' not in calls
    assert calls[:2] == ['spawn', 'record']
    assert calls.count('settle') == calls.count('wait') == 1
    assert process.stdout.closed and process.stderr.closed


@pytest.mark.parametrize('mode', [
    'normal', 'chunked', 'eof', 'status', 'content-type', 'size', 'utf8', 'json', 'truncated',
])
def test_http_request_requires_bounded_complete_json_and_closes_connection(monkeypatch, mode):
    helper = load(monkeypatch)
    closed = []
    payload = {'utf8': b'\xff', 'json': b'{', 'size': b'x' * 1_048_577}.get(mode, b'[{"id":"fixture"}]')
    status = 503 if mode == 'status' else 200
    content_type = 'text/plain' if mode == 'content-type' else 'application/json'
    framing = f'Content-Length: {len(payload) + (7 if mode == "truncated" else 0)}\r\n'
    body = payload
    if mode == 'chunked':
        framing = 'Transfer-Encoding: chunked\r\n'
        body = f'{len(payload):x}\r\n'.encode('ascii') + payload + b'\r\n0\r\n\r\n'
    elif mode == 'eof':
        framing = 'Connection: close\r\n'
    wire = f'HTTP/1.1 {status} Fixture\r\nContent-Type: {content_type}\r\n{framing}\r\n'.encode('ascii') + body

    class Socket:
        def makefile(self, mode):
            assert mode == 'rb'
            return io.BytesIO(wire)

    class Connection:
        def __init__(self, host, port, timeout):
            assert (host, port, timeout) == ('127.0.0.1', 12345, 2)
            self.response = helper.http.client.HTTPResponse(Socket())

        def request(self, method, path, headers):
            assert method == 'GET' and path == '/api/profiles'
            assert headers == {'Authorization': 'Bearer benchmark-only'}

        def getresponse(self):
            self.response.begin()
            return self.response

        def close(self):
            self.response.close()
            closed.append(True)

    monkeypatch.setattr(helper.http.client, 'HTTPConnection', Connection)
    if mode in ('normal', 'chunked', 'eof'):
        assert helper.request(12345, '/api/profiles', 2) == [{'id': 'fixture'}]
    else:
        with pytest.raises(ValueError, match='status|bound|declared|codec|Expecting'):
            helper.request(12345, '/api/profiles', 2)
    assert closed == [True]


@pytest.mark.parametrize('mode', ['normal', 'missing', 'duplicate', 'identity', 'changed-bytes', 'changed-field', 'reordered'])
def test_http_pair_preserves_complete_payload_and_input_bytes(tmp_path, monkeypatch, mode):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    spec = importlib.util.spec_from_file_location('benchmark_pair_fixture', ROOT / 'scripts/migration/benchmark.py')
    benchmark = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(benchmark)
    template = tmp_path / 'baseline'
    template.mkdir()
    (template / 'config.json').write_text('{"future":true}', encoding='utf-8')
    rows = [{'id': f'{index:012x}', 'name': f'Fixture {index}', 'future': [True, 7]} for index in range(32)]

    def execute(argv, environment, *, timeout):
        assert Path(argv[2]).name == 'benchmark_http_sample.py' and timeout == 25
        root = Path(argv[argv.index('--data-dir') + 1])
        assert (root / 'config.json').read_bytes() == b'{"future":true}'
        payload = list(rows)
        if mode == 'missing':
            payload.pop()
        elif mode == 'duplicate':
            payload[0] = payload[1]
        elif mode == 'identity':
            payload[0] = {**payload[0], 'id': []}
        elif mode == 'changed-bytes':
            (root / 'config.json').write_text('{"future":false}', encoding='utf-8')
        elif mode == 'changed-field':
            payload[0] = {**payload[0], 'future': [1, 7]}
        elif mode == 'reordered':
            payload.reverse()
        return 0, json.dumps({'elapsed_ns': 7, 'memory': {'metric': 'fixture', 'bytes': 1234},
                             'payload': payload}), ''

    monkeypatch.setattr(benchmark, 'execute', execute)
    if mode in ('normal', 'changed-field', 'reordered'):
        sample, payload = benchmark.observation(tmp_path / 'candidate', tmp_path, tmp_path,
                                                template, {}, 'native', 'http-list', '')
        assert sample == {'elapsed_ns': 7, 'memory': {'metric': 'fixture', 'bytes': 1234}}
        assert (payload == benchmark.exact_json(rows)) == (mode == 'normal')
        assert len(list(tmp_path.glob('pair-*'))) == 1, 'Pair was removed before cross-runtime comparison'
    else:
        with pytest.raises(ValueError, match='profiles|protected'):
            benchmark.observation(tmp_path / 'candidate', tmp_path, tmp_path,
                                  template, {}, 'native', 'http-list', '')
        assert len(list(tmp_path.glob('pair-*'))) == 1, 'Failed benchmark fixture was erased'


@pytest.mark.parametrize('mode', ['normal', 'parity', 'inconclusive', 'write'])
def test_complete_http_benchmark_retains_failed_pairs_until_acceptance(tmp_path, monkeypatch, mode):
    monkeypatch.syspath_prepend(str(ROOT / 'scripts/migration'))
    spec = importlib.util.spec_from_file_location('benchmark_run_fixture', ROOT / 'scripts/migration/benchmark.py')
    benchmark = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(benchmark)
    candidate = tmp_path / 'candidate'
    candidate.write_bytes(b'inert binary identity')
    temporary = tmp_path / '.ci-work/migration-temp'
    temporary.mkdir(parents=True)
    destination = tmp_path / 'measurements.json'
    if mode == 'write':
        destination.write_text('keep original evidence', encoding='utf-8')
    monkeypatch.setattr(benchmark, 'ROOT', tmp_path)
    monkeypatch.setattr(benchmark, 'SCENARIOS', ('http-list',))
    monkeypatch.setattr(benchmark, 'artifact', lambda *args, **kwargs: candidate)
    monkeypatch.setattr(benchmark, 'baseline', lambda parent, env: (parent, parent, ''))
    monkeypatch.setattr(benchmark.sys, 'platform', 'linux')
    monkeypatch.setenv('GITHUB_ACTIONS', 'true')
    monkeypatch.setenv('GITHUB_REPOSITORY', 'KiaroSama/Numera-Browser')
    monkeypatch.setenv('GITHUB_SHA', 'a' * 40)
    monkeypatch.setenv('GITHUB_RUN_ID', '123')
    import baseline_source
    monkeypatch.setattr(baseline_source, 'source_root', lambda: tmp_path)
    identity = {'source_sha': 'a' * 40, 'generation': 123, 'application_runtime': 'native-candidate'}
    monkeypatch.setattr(benchmark, 'execute', lambda *args, **kwargs: (0, json.dumps(identity), ''))
    observed = []

    def observation(candidate, original, parent, template, env, runtime, scenario, password):
        with benchmark.fixture_directory('pair-', parent, retain=True) as root:
            (root / 'input.json').write_text('{"synthetic":true}', encoding='utf-8')
            observed.append(runtime)
            value = 1 if runtime == 'native' and mode != 'inconclusive' else 100
            sample = {'elapsed_ns': value, 'memory': {'metric': 'fixture', 'bytes': value}}
            return sample, runtime if mode == 'parity' else 'same full ordered response'

    monkeypatch.setattr(benchmark, 'observation', observation)
    logger = SimpleNamespace(info=lambda *args: None)
    if mode == 'normal':
        assert benchmark.run(tmp_path / 'report', destination, logger) == 0
        assert not list(temporary.iterdir())
    else:
        with pytest.raises((ValueError, FileExistsError), match='parity|inconclusive|exists'):
            benchmark.run(tmp_path / 'report', destination, logger)
        roots = list(temporary.glob('benchmark-*'))
        assert len(roots) == 1
        assert len(list(roots[0].glob('pair-*/input.json'))) == len(observed)
    if mode != 'parity':
        assert len(observed) == 16, 'Warm-up or measured alternating pairs were omitted'
        assert observed == [runtime for index in range(-1, 7) for runtime in
                            (('original', 'native') if index % 2 == 0 else ('native', 'original'))]
    if mode == 'write':
        assert destination.read_text(encoding='utf-8') == 'keep original evidence'
    elif mode != 'parity':
        result = json.loads(destination.read_text(encoding='utf-8'))
        assert len(result['results'][0]['samples']) == 7 and result['warmup_pairs'] == 1
