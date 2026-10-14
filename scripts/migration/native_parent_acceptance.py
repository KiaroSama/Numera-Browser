"""Compare original/native server shutdown when their captured parent disappears."""
from __future__ import annotations
import argparse
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera import proctree
from native_http_acceptance import request
from run_logging import run_log


def observe(executable, root, env):
    parent, parent_tree = proctree.spawn([sys.executable, '-I', '-c',
                                        'import threading; threading.Event().wait(30)'],
                                       stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                                       stderr=subprocess.DEVNULL)
    server = tree = None
    try:
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            port = reservation.getsockname()[1]
        child_env = {**env, 'NUMERA_PARENT_PID': str(parent.pid)}
        server, tree = proctree.spawn([*executable, '--data-dir', str(root), 'serve', '--port', str(port)],
                                      env=child_env, stdin=subprocess.DEVNULL,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        end = time.monotonic() + 10
        while True:
            try:
                if request(port, 'GET', '/api/ready')[0] == 200:
                    break
            except (OSError, ConnectionError):
                pass
            if server.poll() is not None:
                raise AssertionError('Server exited before parent-death fixture became ready')
            if time.monotonic() >= end:
                raise TimeoutError('Parent-death server did not become ready')
            time.sleep(min(0.02, max(0, end - time.monotonic())))
        assert proctree.end_tree(parent_tree, grace=1, kill_grace=3)
        parent.wait(timeout=5)
        server.wait(timeout=5)
        assert server.returncode == 0, server.returncode
        assert not proctree.alive_tree(tree), 'Parent-death left a live server tree'
        end = time.monotonic() + 2
        while True:
            with socket.socket() as check:
                check.settimeout(0.2)
                if check.connect_ex(('127.0.0.1', port)) != 0:
                    break
            if time.monotonic() >= end:
                raise AssertionError('Parent-death left a live HTTP listener')
            time.sleep(min(0.02, max(0, end - time.monotonic())))
    finally:
        if tree is not None and not proctree.end_tree(tree, grace=1, kill_grace=3):
            raise RuntimeError('Parent-death server cleanup failed')
        if server is not None:
            server.wait(timeout=5)
            server.stdout.close()
            server.stderr.close()
        if not proctree.end_tree(parent_tree, grace=1, kill_grace=3):
            raise RuntimeError('Fixture parent cleanup failed')
        parent.wait(timeout=5)


def main(candidate, log):
    with tempfile.TemporaryDirectory(prefix='native-parent-', dir=ROOT / '.ci-work/migration-temp') as temporary:
        parent = Path(temporary)
        home = parent / 'home'
        home.mkdir()
        env = os.environ.copy()
        for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR', 'NUMERA_NATIVE_SUPERVISE',
                    'NUMERA_NATIVE_JOB_GATE', 'NUMERA_HELPER_UNITS', 'NUMERA_HELPER_SCOPE', 'NUMERA_PARENT_PID'):
            env.pop(key, None)
        env.update(PYTHONUTF8='1', PYTHONPATH=str(ROOT / 'engine'), HOME=str(home), USERPROFILE=str(home),
                   NUMERA_APP_DIR=str(parent), NUMERA_LOG_DIR=str(parent / 'logs'),
                   NUMERA_BROWSER_ADAPTER=str(parent / 'absent.py'), NUMERA_SDK_PYTHON=sys.executable)
        runtimes = [('native', [str(candidate)])]
        if os.name == 'posix':
            runtimes.insert(0, ('source', [sys.executable, '-m', 'numera']))
        else:
            log.info('Original Windows parent watcher relies on desktop job ownership, not PID polling; source PID-only case is inapplicable')
        for label, executable in runtimes:
            observe(executable, parent / label, env)
            log.info('Server parent-death exit0/listener and owned tree empty passed runtime=%s', label)
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-parent-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
