"""Compare actual source/native HTTP servers with isolated stores and owned listeners."""
from __future__ import annotations
import argparse
import http.client
import json
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
from baseline_source import source_root
ORIGINAL = source_root()
sys.path.insert(0, str(ROOT / 'engine'))
from numera import proctree, unit_report
from run_logging import run_log


def request(port, method, path, token=None, *, body=None):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=2)
    try:
        headers = {'Authorization': f'Bearer {token}'} if token else {}
        if body is not None:
            headers['Content-Type'] = 'application/json'
            body = json.dumps(body, ensure_ascii=False, allow_nan=False).encode('utf-8')
        connection.request(method, path, body=body, headers=headers)
        response = connection.getresponse()
        body = response.read(65537)
        if len(body) > 65536:
            raise ValueError('HTTP acceptance response exceeds bound')
        headers = {key.lower(): value for key, value in response.getheaders()
                   if key.lower() in ('content-type', 'access-control-allow-origin', 'www-authenticate')
                   or (path in ('/openapi.json', '/docs', '/docs/oauth2-redirect', '/redoc')
                       and key.lower() in ('allow', 'content-length'))}
        if 'allow' in headers:
            headers['allow'] = sorted(method.strip() for method in headers['allow'].split(','))
        content = None
        if body:
            content = json.loads(body) if headers.get('content-type', '').startswith('application/json') else body.decode('utf-8')
        return response.status, headers, content
    finally:
        connection.close()


def observe(executable, root, env, log, *, exercise=None):
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    attempt = unit_report.begin_spawn()
    process, tree = proctree.spawn([*executable, '--data-dir', str(root), 'serve', '--host', '127.0.0.1',
                                   '--port', str(port)], env=env, stdin=subprocess.DEVNULL,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        unit_report.record(tree, attempt=attempt)
        end = time.monotonic() + 10
        while True:
            try:
                ready = request(port, 'GET', '/api/ready')
                if ready[0] == 200:
                    break
            except (OSError, http.client.HTTPException):
                pass
            if process.poll() is not None:
                stdout, stderr = process.communicate(timeout=2)
                raise RuntimeError(f'HTTP entry exited {process.returncode}: {stderr.decode("utf-8")!r}')
            if time.monotonic() >= end:
                if not proctree.end_tree(tree, grace=1, kill_grace=3):
                    raise RuntimeError('Unready HTTP entry cleanup did not settle')
                stdout, stderr = process.communicate(timeout=2)
                log.error('HTTP startup diagnostic executable=%s stdout=%r stderr=%r',
                          Path(executable[0]).name, stdout[:8192].decode('utf-8'), stderr[:8192].decode('utf-8'))
                raise TimeoutError('HTTP entry did not become ready')
            time.sleep(min(0.02, max(0, end - time.monotonic())))
        if exercise is not None:
            return exercise(port)
        cases = [('GET', '/api/ready', None), ('GET', '/api/health', None),
                 ('GET', '/api/health', 'acceptance-only'), ('GET', '/api/profiles', 'acceptance-only'),
                 ('GET', '/api/config', 'acceptance-only'), ('GET', '/api/engines', 'acceptance-only'),
                 ('HEAD', '/api/ready', None), ('GET', '/api/not-a-route', None),
                 ('GET', '/api/not-a-route', 'acceptance-only')]
        cases.extend((method, path, None) for path in ('/openapi.json', '/docs', '/docs/oauth2-redirect', '/redoc')
                     for method in ('GET', 'HEAD', 'POST'))
        results = []
        for method, path, token in cases:
            result = request(port, method, path, token)
            # Each empty isolated store has a distinct path, not a different contract.
            text = json.dumps(result, ensure_ascii=False).replace(str(root).replace('\\', '\\\\'), '<root>')
            results.append(json.loads(text))
            log.info('HTTP observed method=%s path=%s status=%s', method, path, result[0])
        return cases, results
    finally:
        try:
            clean = proctree.end_tree(tree, grace=1, kill_grace=3)
            process.wait(timeout=5)
        finally:
            for pipe in (process.stdout, process.stderr):
                pipe.close()
        if not clean:
            raise RuntimeError('HTTP acceptance server tree did not settle')
        end = time.monotonic() + 2
        while True:
            with socket.socket() as check:
                check.settimeout(0.2)
                if check.connect_ex(('127.0.0.1', port)) != 0:
                    break
            if time.monotonic() >= end:
                raise AssertionError('HTTP listener survived owned cleanup')
            time.sleep(min(0.02, max(0, end - time.monotonic())))


def require_http_profile(observed, expected, started, finished):
    import math
    from replay_rollback import require_equal

    for key in ('created_at', 'updated_at'):
        value = observed.get(key)
        if (isinstance(value, bool) or not isinstance(value, (int, float))
                or not math.isfinite(value) or not started <= value <= finished):
            raise AssertionError('HTTP replay derived timestamp is invalid')
    # The original dashboard converter constructs both timestamps on each write.
    require_equal({key: value for key, value in observed.items() if key not in ('created_at', 'updated_at')},
                  {key: value for key, value in expected.items() if key not in ('created_at', 'updated_at')},
                  'HTTP-derived complete profile')


def shared_store_replay(candidate, parent, env, log):
    from candidate_acceptance import owned
    from replay_rollback import protected_bytes, require_equal, require_profile

    root = parent / 'shared HTTP CLI data with spaces'
    root.mkdir()
    executables = {'original': [sys.executable, '-m', 'numera'], 'native': [str(candidate)]}

    def cli(runtime, *arguments):
        with owned([*executables[runtime], '--data-dir', str(root), *arguments, '--json'], env, ROOT) as process:
            stdout, stderr = process.communicate(timeout=15)
            code = process.returncode
        if code or stderr or len(stdout) > 65536:
            raise AssertionError(f'HTTP replay CLI failed runtime={runtime} operation={arguments[0]} exit={code}')
        log.info('HTTP replay CLI runtime=%s operation=%s exit=%s', runtime, arguments[0], code)
        return json.loads(stdout.decode('utf-8', errors='strict'))

    created = cli('original', 'create', 'HTTP replay α', '--seed', '42', '--engine', 'playwright',
                  '--notes', 'source created',
                  '--proxy', 'http://fixture:replay-http-secret@127.0.0.1:8080')['profile']
    identifier = created['id']
    path = '/api/profiles/' + identifier
    dashboard_path = root / 'dashboard' / (identifier + '.json')
    engine_path = root / 'profiles' / (identifier + '.json')
    dashboard = json.loads(dashboard_path.read_text(encoding='utf-8'))
    sentinel = {'text': 'retained α 日本語', 'items': [42, True]}
    dashboard['_futureReplay'] = sentinel
    dashboard_path.write_text(json.dumps(dashboard, ensure_ascii=False), encoding='utf-8')
    config_path = root / 'config.json'
    config = json.loads(config_path.read_text(encoding='utf-8'))
    config['futureReplay'] = sentinel
    config_path.write_text(json.dumps(config, ensure_ascii=False), encoding='utf-8')
    session = root / 'user-data' / identifier / 'Default' / 'replay.bin'
    session.parent.mkdir(parents=True)
    session.write_bytes(b'\x00\xffshared HTTP replay\r\n')
    created_at = dashboard['_created']
    expected = created
    canonical = None
    reference_failures = None

    def reopened():
        before = protected_bytes(root)
        for runtime in executables:
            require_profile(cli(runtime, 'show', identifier), expected)
        assert protected_bytes(root) == before, 'HTTP replay CLI read changed protected bytes'
        current = json.loads(dashboard_path.read_text(encoding='utf-8'))
        assert current['_futureReplay'] == sentinel and current['_created'] == created_at
        assert current['notes'] == expected['notes'] and current['tags'] == expected['tags']
        assert current['proxy']['pass'] == expected['proxy']['password'] == 'replay-http-secret'
        assert json.loads(engine_path.read_text(encoding='utf-8')) == expected
        assert json.loads(config_path.read_text(encoding='utf-8'))['futureReplay'] == sentinel
        assert session.read_bytes() == b'\x00\xffshared HTTP replay\r\n'

    for runtime in executables:
        def exercise(port):
            nonlocal expected, canonical, reference_failures
            before = protected_bytes(root)
            status, _, rows = request(port, 'GET', '/api/profiles', 'acceptance-only')
            assert status == 200 and isinstance(rows, list) and len(rows) == 1
            view = rows[0]
            assert view['id'] == identifier and view['notes'] == expected['notes']
            assert view['_futureReplay'] == sentinel and view['proxy']['pass'] == 'replay-http-secret'
            assert protected_bytes(root) == before, 'HTTP replay listing changed protected bytes'
            revision = view['_rev']
            assert type(revision) is int and revision >= 1
            notes = 'HTTP original α' if runtime == 'original' else 'HTTP native 日本語'
            started = time.time()
            status, _, saved = request(port, 'PUT', path, 'acceptance-only',
                                       body={'notes': notes, '_rev': revision})
            finished = time.time()
            assert status == 200 and saved['_rev'] == revision + 1
            require_equal(saved, {**view, 'notes': notes, '_rev': revision + 1}, 'HTTP patch response')
            # Read the writer's bytes before either CLI can reconcile a broken mirror.
            written = json.loads(engine_path.read_text(encoding='utf-8'))
            if canonical is None:
                canonical = written
                assert written['notes'] == notes and written['proxy'] == expected['proxy']
                assert written['id'] == identifier and written['fingerprint']['seed'] == 42
                source_expected = written
            else:
                source_expected = {**expected, 'notes': notes}
            require_http_profile(written, source_expected, started, finished)
            expected = written
            reopened()
            failures = []
            for token, body in ((None, {'notes': 'unauthorized'}),
                                ('acceptance-only', {'cores': 1.5}),
                                ('acceptance-only', {'notes': 'stale', '_rev': revision})):
                before = protected_bytes(root)
                result = request(port, 'PUT', path, token, body=body)
                assert protected_bytes(root) == before, 'HTTP replay refusal changed protected bytes'
                failures.append(result)
            assert [row[0] for row in failures] == [401, 422, 409]
            stale = failures[2][2]
            require_equal(stale, {'kind': 'StaleProfile', 'detail':
                f'profile {identifier} changed since you loaded it (revision {revision + 1}, you sent {revision})'},
                'HTTP stale refusal')
            if reference_failures is None:
                reference_failures = failures[:2]
            else:
                require_equal(failures[:2], reference_failures, 'HTTP unauthorized/invalid refusals')
            expected = {**expected, 'notes': 'CLI after ' + runtime, 'tags': [*expected['tags'], runtime]}
            returned = cli(runtime, 'edit', identifier, '--notes', expected['notes'],
                           '--add-tag', runtime)['profile']
            require_profile(returned, expected)
            written = json.loads(engine_path.read_text(encoding='utf-8'))
            require_equal(written, returned, 'CLI writer persisted profile')
            expected = written
            reopened()
            log.info('Shared HTTP write, complete CLI reopen, three refusals and byte preservation passed runtime=%s', runtime)

        observe(executables[runtime], root, env, log, exercise=exercise)

    def rollback(port):
        before = protected_bytes(root)
        status, _, rows = request(port, 'GET', '/api/profiles', 'acceptance-only')
        current = json.loads(dashboard_path.read_text(encoding='utf-8'))
        assert status == 200 and len(rows) == 1
        require_equal(rows[0], {**current, 'status': 'stopped', 'lastActive': 'never'}, 'HTTP source restart')
        assert protected_bytes(root) == before, 'HTTP source restart read changed protected bytes'
        reopened()
    observe(executables['original'], root, env, log, exercise=rollback)
    assert not list((root / 'sessions').glob('*.json')) and not (root / 'vault-migration.json').exists()
    log.info('Actual shared-store original/native HTTP/CLI replay and original server restart passed')


def main(candidate, log):
    parent = Path(tempfile.mkdtemp(prefix='native-http-', dir=ROOT / '.ci-work/migration-temp'))
    try:
        home = parent / 'home'
        home.mkdir()
        env = os.environ.copy()
        for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR', 'NUMERA_NATIVE_SUPERVISE',
                    'NUMERA_NATIVE_JOB_GATE', 'NUMERA_HELPER_UNITS', 'NUMERA_HELPER_SCOPE', 'NUMERA_PARENT_PID'):
            env.pop(key, None)
        env.update(PYTHONUTF8='1', PYTHONDONTWRITEBYTECODE='1', PYTHONPATH=str(ORIGINAL / 'engine'), HOME=str(home), USERPROFILE=str(home),
                   NUMERA_APP_DIR=str(parent), NUMERA_LOG_DIR=str(parent / 'logs'),
                   NUMERA_BROWSER_ADAPTER=str(parent / 'absent.py'), NUMERA_SDK_PYTHON=sys.executable,
                   NUMERA_API_TOKEN='acceptance-only')
        cases, source = observe([sys.executable, '-m', 'numera'], parent / 'source', env, log)
        _, native = observe([str(candidate)], parent / 'native', env, log)
        for case, expected, actual in zip(cases, source, native):
            assert actual == expected, f'{case[:2]} source={expected!r} native={actual!r}'
        log.info('Actual source/native HTTP status/body/selected headers matched; listeners and trees settled')
        shared_store_replay(candidate, parent, env, log)
    except BaseException:
        log.error('HTTP acceptance failed; fixture retained path=%s', parent)
        raise
    else:
        import shutil
        shutil.rmtree(parent)
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-http-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
