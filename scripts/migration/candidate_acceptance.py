"""Exercise a relocated native application with an explicit existing SDK interpreter."""
from __future__ import annotations

import argparse
from contextlib import ExitStack, contextmanager
import http.client
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera import proctree, unit_report
from numera.server.helper_streams import _nonblocking_read
from candidate_stage import artifact, executable_name, stage, verify
from run_logging import run_log


@contextmanager
def owned(argv, env, cwd, stdin=subprocess.DEVNULL):
    report = Path(os.environ[unit_report.UNITS_ENV])
    before = unit_report.read(report)
    if not before.complete:
        raise RuntimeError('Outer acceptance owner has an incomplete ownership receipt')
    attempt = unit_report.begin_spawn()
    process, tree = proctree.spawn(argv, env={**env, unit_report.UNITS_ENV: str(report)}, cwd=cwd,
                                   stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        unit_report.record(tree, attempt=attempt)
        yield process
    finally:
        try:
            clean = proctree.end_tree(tree, grace=1, kill_grace=3)
            receipt = unit_report.read(report)
            deadline = time.monotonic() + 10
            for nested in receipt.trees:
                if nested not in before.trees:
                    clean = proctree.end_tree(nested, ends_at=deadline) and clean
            process.wait(timeout=5)
            if not clean or not receipt.complete:
                raise RuntimeError('Relocated application or SDK ownership did not settle')
        finally:
            for pipe in (process.stdin, process.stdout, process.stderr):
                if pipe is not None:
                    pipe.close()


def execute(argv, env, cwd, data=None):
    with owned(argv, env, cwd, subprocess.PIPE if data is not None else subprocess.DEVNULL) as process:
        if data is not None:
            if len(data) > 4096:
                raise ValueError('Candidate input exceeds its pipe-safe fixture bound')
            process.stdin.write(data)
            process.stdin.close()
        readers = [_nonblocking_read(process.stdout), _nonblocking_read(process.stderr)]
        output = [bytearray(), bytearray()]
        ended = set()
        deadline = time.monotonic() + 90
        while len(ended) != 2 or process.poll() is None:
            if time.monotonic() >= deadline:
                raise TimeoutError('Candidate command exceeded its wall deadline')
            progress = False
            for index, read in enumerate(readers):
                if index in ended:
                    continue
                chunk = read()
                if chunk == b'':
                    ended.add(index)
                elif chunk is not None:
                    output[index].extend(chunk)
                    progress = True
                    if len(output[index]) > (1_048_576 if index == 0 else 65536):
                        raise ValueError('Candidate command output exceeds acceptance bounds')
            if not progress:
                time.sleep(min(0.01, max(0, deadline - time.monotonic())))
        receipt = unit_report.read(Path(os.environ[unit_report.UNITS_ENV]))
        if not receipt.complete or any(proctree.alive_tree(tree) for tree in receipt.trees):
            raise AssertionError('Finite native command left live or uncertain SDK ownership before fallback cleanup')
        return process.returncode, bytes(output[0]), bytes(output[1])


def identity(binary, env, cwd):
    code, stdout, stderr = execute([str(binary), '--build-info'], env, cwd)
    if code or stderr:
        raise AssertionError('Candidate embedded identity command failed')
    expected = {'schema': 1, 'owner': 'KiaroSama', 'product': 'Numera Browser',
                'version': json.loads((ROOT / 'dashboard/package.json').read_text(encoding='utf-8'))['version'],
                'source_sha': os.environ['GITHUB_SHA'], 'generation': int(os.environ['GITHUB_RUN_ID']),
                'application_runtime': 'native-candidate', 'acceptance': True}
    if json.loads(stdout) != expected:
        raise AssertionError('Candidate identity differs from the exact hosted build')
    return expected


def request(port, method, path, body=None, token='acceptance-only'):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=90)
    headers = {'Authorization': 'Bearer ' + token} if token else {}
    if body is not None:
        headers['Content-Type'] = 'application/json'
        body = json.dumps(body).encode('utf-8')
    try:
        connection.request(method, path, body=body, headers=headers)
        response = connection.getresponse()
        data = response.read(1_048_577)
        if len(data) > 1_048_576:
            raise ValueError('Candidate HTTP response exceeds its bound')
        return response.status, data
    finally:
        connection.close()


def wait_ready(port, process, seconds=15):
    end = time.monotonic() + seconds
    while True:
        try:
            # Readiness has a short socket budget; browser operations use 90s.
            connection = http.client.HTTPConnection('127.0.0.1', port, timeout=0.5)
            try:
                connection.request('GET', '/api/ready')
                response = connection.getresponse()
                payload = json.loads(response.read(4096))
                if response.status == 200 and payload.get('service') == 'numera-browser':
                    return
            finally:
                connection.close()
        except (OSError, ValueError, http.client.HTTPException):
            pass
        if process.poll() is not None:
            raise AssertionError('Candidate server exited before readiness')
        if time.monotonic() >= end:
            raise TimeoutError('Candidate server readiness deadline exceeded')
        time.sleep(min(0.025, max(0, end - time.monotonic())))


def claim_tree(root, identifier):
    record = json.loads((root / 'sessions' / (identifier + '.json')).read_text(encoding='utf-8'))
    tree = proctree.Tree(pid=record['pid'], token=record['token'],
                         kind=record['tree']['kind'], group=record['tree']['group'])
    if not tree.verified or not proctree.alive_tree(tree):
        raise AssertionError('Candidate running profile has no observable owned tree')
    return tree


def settled(root, identifier, tree=None):
    if tree is not None and proctree.alive_tree(tree):
        raise AssertionError('Browser tree survived application stop')
    if (root / 'sessions' / (identifier + '.json')).exists():
        raise AssertionError('Candidate retained a settled browser claim')


def failure_diagnostics(directory, logger):
    from itertools import islice

    safe = {'Camoufox SDK import starting', 'Camoufox SDK import complete',
            'Camoufox persistent context entering', 'Camoufox persistent context entered', 'Startup'}
    types = {'FileNotFoundError', 'PermissionError', 'OSError', 'ValueError', 'TypeError',
             'RuntimeError', 'TimeoutError', 'Error', 'TargetClosedError', 'CamoufoxNotInstalled',
             'UnsupportedVersion', 'InvalidAddonPath', 'InvalidOS', 'InvalidPropertyType',
             'ProfileDirectoryError', 'NonFirefoxFingerprint', 'ImportError', 'ModuleNotFoundError'}
    emitted = omitted = 0
    logger.warning('Candidate diagnostics start')
    try:
        if directory.is_symlink() or getattr(directory.lstat(), 'st_file_attributes', 0) & 0x400:
            raise ValueError('Linked diagnostic directory')
        for index, path in enumerate(islice(directory.iterdir(), 17)):
            if index == 16:
                omitted += 1
                break
            if (not path.name.startswith(('browser-adapter_', 'native-cli_')) or path.suffix != '.log'
                    or path.is_symlink() or getattr(path.lstat(), 'st_file_attributes', 0) & 0x400
                    or not path.is_file() or path.stat().st_size > 65536):
                omitted += 1
                continue
            with path.open('rb') as stream:
                raw = stream.read(65537)
            if len(raw) > 65536:
                omitted += 1
                continue
            for line in raw.decode('utf-8').splitlines():
                match = re.fullmatch(
                    r'\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC\] '
                    r'\[(?:DEBUG|INFO|WARNING|ERROR|CRITICAL)\] '
                    r'\[(?:numera\.browser-adapter|entry)\] (.+)', line)
                if match is None:
                    continue
                event = match[1]
                failure = re.fullmatch(
                    r'Operation failed operation=(hello|open|close|inspect|navigate|mouse|route|evaluate|'
                    r'cookies-read|cookies-clear|cookies-add|invalid) type=([A-Za-z_][A-Za-z_0-9]*)', event)
                if failure:
                    kind = failure[2] if failure[2] in types else 'unknown'
                    event = f'Operation failed operation={failure[1]} type={kind}'
                else:
                    failure = re.fullmatch(r'(Protocol failed|SDK cleanup failed) type=([A-Za-z_][A-Za-z_0-9]*)', event)
                    if failure:
                        kind = failure[2] if failure[2] in types else 'unknown'
                        event = f'{failure[1]} type={kind}'
                    elif event not in safe and re.fullmatch(
                            r'(?:SDK timing operation=(?:hello|open|close|inspect|navigate|mouse|route|evaluate|'
                            r'cookies-read|cookies-clear|cookies-add|cleanup|invalid)|'
                            r'Native timing phase=(?:startup|prepare|metadata|execute|shutdown)) elapsed_ns=[0-9]{1,20}', event) is None:
                        continue
                if emitted >= 63:
                    omitted += 1
                    break
                logger.warning('Candidate event %s', event)
                emitted += 1
    except (OSError, UnicodeError, ValueError):
        omitted += 1
    logger.warning('Candidate diagnostics omitted=%s', omitted)


def exercise(binary, web, parent, engine, env, logger):
    cwd = parent / 'unrelated working directory'
    cwd.mkdir()
    root = parent / 'isolated data'
    root.mkdir()
    env = dict(env, NUMERA_API_TOKEN='acceptance-only')
    expected = identity(binary, env, cwd)
    original = parent / 'staged candidate'
    stage(binary, web, original, expected)
    relocated = parent / 'relocated application with spaces'
    original.rename(relocated)
    candidate = relocated / executable_name()
    assert verify(relocated) == expected
    assert identity(candidate, {**env, 'GITHUB_SHA': 'b' * 40, 'GITHUB_RUN_ID': '1'}, cwd) == expected

    def cli(*arguments, expected_exit=0):
        code, stdout, stderr = execute([str(candidate), '--data-dir', str(root), *arguments, '--json'], env, cwd)
        if code != expected_exit or stderr:
            logger.error('Candidate CLI failed operation=%s exit=%s stdout_bytes=%s stderr_bytes=%s',
                         arguments[0], code, len(stdout), len(stderr))
            raise AssertionError(f'Candidate CLI failed operation={arguments[0]} exit={code}')
        return json.loads(stdout)

    created = cli('create', 'Relocated fixture', '--seed', '42', '--engine', engine)
    identifier = created['profile']['id']
    assert cli('show', identifier)['engine'] == engine
    assert cli('show', 'missing', expected_exit=1)['ok'] is False
    # This also tests current_exe-based child execution from the relocated MCP.
    messages = b'{"id":1,"method":"ping"}\n{"id":2,"method":"tools/call","params":{"name":"list"}}\n'
    code, stdout, stderr = execute([str(candidate), '--data-dir', str(root), 'mcp'], env, cwd, messages)
    replies = [json.loads(line) for line in stdout.splitlines()]
    assert code == 0 and not stderr and len(replies) == 2
    assert replies[0] == {'jsonrpc': '2.0', 'id': 1, 'result': {}}
    assert replies[1]['result']['structuredContent']['profiles'][0]['id'] == identifier

    expires = int(time.time()) + 86_400
    cookie = {'name': 'persistent-fixture', 'value': 'synthetic', 'domain': '.fixture.test',
              'path': '/', 'expires': expires, 'secure': False, 'httpOnly': True, 'sameSite': 'Lax'}
    source = parent / 'cookie input.json'
    source.write_text(json.dumps({'cookies': [cookie]}), encoding='utf-8')
    assert cli('cookies', 'import', identifier, str(source), '--clear')['imported'] == 1
    settled(root, identifier)
    assert cli('cookies', 'list', identifier) == {'total': 1, 'domains': {'.fixture.test': 1}}
    exported = parent / 'cookie output.json'
    assert cli('cookies', 'export', identifier, str(exported))['count'] == 1
    assert json.loads(exported.read_text(encoding='utf-8'))['cookies'] == [cookie]
    logger.info('Relocated CLI/MCP and persistent cookie import/list/export passed engine=%s', engine)

    with socket.socket() as reserve:
        reserve.bind(('127.0.0.1', 0))
        port = reserve.getsockname()[1]
    observed = None
    cleaned_by_owner = False
    try:
        with ExitStack() as owners:
            parent_process = owners.enter_context(owned(
                [sys.executable, '-I', '-S', '-c', 'import threading; threading.Event().wait(600)'], env, cwd))
            server_env = {**env, 'NUMERA_PARENT_PID': str(parent_process.pid)}
            process = owners.enter_context(owned(
                [str(candidate), '--data-dir', str(root), 'serve', '--port', str(port)], server_env, cwd))
            wait_ready(port, process)
            assert request(port, 'GET', '/api/health', token=None)[0] == 401
            status, page = request(port, 'GET', '/', token=None)
            assert status == 200 and page == (web / 'index.html').read_bytes()
            asset = next(path for path in sorted((web / 'assets').rglob('*')) if path.is_file())
            status, data = request(port, 'GET', '/assets/' + asset.relative_to(web / 'assets').as_posix(), token=None)
            assert status == 200 and data == asset.read_bytes()
            base = '/api/profiles/' + identifier
            status, data = request(port, 'GET', base + '/cookies')
            assert status == 200 and json.loads(data) == {'cookies': [cookie]}
            for _ in range(2):
                status, data = request(port, 'POST', base + '/launch', {'headless': True})
                assert status == 200 and json.loads(data)['status'] == 'running'
                current = claim_tree(root, identifier)
                if observed is not None:
                    assert current == observed, 'Idempotent launch replaced the active owned tree'
                observed = current
            assert request(port, 'GET', base + '/cookies')[0] == 409
            status, data = request(port, 'POST', base + '/stop')
            assert status == 200 and json.loads(data)['status'] == 'stopped'
            settled(root, identifier, observed)
            observed = None
            assert request(port, 'POST', base + '/cookies', {'cookies': [], 'clear': True})[0] == 500
            status, data = request(port, 'GET', base + '/cookies')
            assert status == 200 and json.loads(data) == {'cookies': [cookie]}
            # Leave an actual browser open: server-tree cleanup must reach its SDK descendants.
            assert request(port, 'POST', base + '/launch', {'headless': True})[0] == 200
            observed = claim_tree(root, identifier)
            # Parent liveness is the public cross-platform shutdown contract.
            parent_process.terminate()
            parent_process.wait(timeout=5)
            process.wait(timeout=15)
            assert process.returncode == 0, 'Candidate parent-death shutdown did not succeed'
            assert proctree.prove_empty(observed, deadline=5), 'Browser survived native server shutdown'
            settled(root, identifier, observed)
            cleaned_by_owner = True
    finally:
        if observed is not None and proctree.alive_tree(observed):
            if not proctree.end_tree(observed, grace=1, kill_grace=3):
                raise RuntimeError('Candidate browser cleanup remains unresolved')
    assert cleaned_by_owner, 'Candidate browser survived its application owner; emergency cleanup is not acceptance'
    with socket.socket() as listener:
        listener.settimeout(0.5)
        assert listener.connect_ex(('127.0.0.1', port)) != 0, 'Candidate HTTP listener survived'
    assert verify(relocated) == expected, 'Candidate changed its packaged runtime inputs'
    logger.info('Relocated HTTP/auth/dashboard/browser restart/idempotence/stop passed engine=%s', engine)


def bind_sdk_cache(env, engine):
    if sys.platform != 'win32' or engine != 'camoufox':
        return
    from platformdirs.windows import get_win_folder

    # Native known-folder lookup can follow USERPROFILE even with LOCALAPPDATA set.
    # Resolve in the parent environment, before synthetic HOME reaches SDK children.
    root = Path(get_win_folder('CSIDL_LOCAL_APPDATA'))
    if not root.is_absolute() or not root.is_dir():
        raise ValueError('Existing absolute Windows Camoufox cache root is required')
    env['WIN_PD_OVERRIDE_LOCAL_APPDATA'] = str(root)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--dashboard', type=Path, required=True)
    parser.add_argument('--engine', choices=('playwright', 'patchright', 'camoufox', 'cloak'), required=True)
    args = parser.parse_args()
    if os.environ.get('GITHUB_ACTIONS') != 'true' or os.environ.get('GITHUB_REPOSITORY') != 'KiaroSama/Numera-Browser':
        raise ValueError('Candidate application acceptance requires its owning hosted repository')
    base = Path(os.environ['RUNNER_TEMP']).resolve(strict=True)
    if base.is_relative_to(ROOT.resolve()):
        raise ValueError('Relocation acceptance requires a path outside checkout ancestry')
    with run_log('candidate-application-acceptance') as logger:
        with tempfile.TemporaryDirectory(prefix='numera-candidate-', dir=base) as temporary:
            parent = Path(temporary)
            home = parent / 'synthetic home'
            home.mkdir()
            env = os.environ.copy()
            for key in tuple(env):
                if key.startswith(('NUMERA_', 'CARGO_', 'RUST', 'GITHUB_', 'ACTIONS_')) or key in (
                        'PYTHONPATH', 'PYTHONHOME', 'LD_LIBRARY_PATH', 'DYLD_LIBRARY_PATH'):
                    env.pop(key, None)
            # Existing SDK/browser caches are explicit external prerequisites,
            # while application home/data stay synthetic. No browser is copied or downloaded.
            env['XDG_CACHE_HOME'] = os.environ.get('XDG_CACHE_HOME') or str(Path.home() / '.cache')
            bind_sdk_cache(env, args.engine)
            env.update(NUMERA_SDK_PYTHON=sys.executable, NUMERA_LOG_DIR=str(parent / 'logs'),
                       PYTHONUTF8='1', PYTHONIOENCODING='utf-8', PYTHONDONTWRITEBYTECODE='1',
                       HOME=str(home), USERPROFILE=str(home))
            try:
                exercise(artifact(args.report), args.dashboard.resolve(strict=True), parent, args.engine, env, logger)
            except BaseException:
                failure_diagnostics(parent / 'logs', logger)
                raise
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
