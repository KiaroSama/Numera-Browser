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
from numera import proctree
from run_logging import run_log


def request(port, method, path, token=None):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=2)
    try:
        connection.request(method, path, headers={'Authorization': f'Bearer {token}'} if token else {})
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


def observe(executable, root, env, log):
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    process, tree = proctree.spawn([*executable, '--data-dir', str(root), 'serve', '--host', '127.0.0.1',
                                   '--port', str(port)], env=env, stdin=subprocess.DEVNULL,
                                  stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
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
        if not proctree.end_tree(tree, grace=1, kill_grace=3):
            raise RuntimeError('HTTP acceptance server tree did not settle')
        process.wait(timeout=5)
        for pipe in (process.stdout, process.stderr):
            pipe.close()
        end = time.monotonic() + 2
        while True:
            with socket.socket() as check:
                check.settimeout(0.2)
                if check.connect_ex(('127.0.0.1', port)) != 0:
                    break
            if time.monotonic() >= end:
                raise AssertionError('HTTP listener survived owned cleanup')
            time.sleep(min(0.02, max(0, end - time.monotonic())))


def main(candidate, log):
    with tempfile.TemporaryDirectory(prefix='native-http-', dir=ROOT / '.ci-work/migration-temp') as temporary:
        parent = Path(temporary)
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
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-http-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
