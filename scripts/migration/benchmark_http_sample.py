"""One real HTTP request observation with an owned server and lifetime OS peak."""
from __future__ import annotations

import argparse
import http.client
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from numera import proctree, unit_report
from benchmark_sample import linux_peak, pipe_reader, windows_peak
from run_logging import run_log


def require_fresh_observer():
    if sys.platform not in ('linux', 'win32'):
        raise ValueError('HTTP benchmark requires Windows or Linux')
    if sys.platform == 'linux':
        import resource
        if resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss != 0:
            raise ValueError('Each Linux HTTP sample requires a fresh observer')


def free_port():
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        return reservation.getsockname()[1]


def request(port, path, timeout):
    connection = http.client.HTTPConnection('127.0.0.1', port, timeout=timeout)
    try:
        connection.request('GET', path, headers={'Authorization': 'Bearer benchmark-only'})
        response = connection.getresponse()
        if response.status != 200 or not response.getheader('Content-Type', '').startswith('application/json'):
            raise ValueError('HTTP benchmark response status or content type differs')
        data = response.read(1_048_577)
        if len(data) > 1_048_576:
            raise ValueError('HTTP benchmark response exceeds its bound')
        if response.length not in (None, 0):
            raise ValueError('HTTP benchmark response ended before its declared length')
        return json.loads(data.decode('utf-8', errors='strict'))
    finally:
        connection.close()


def observe(argv, environment, logger):
    require_fresh_observer()
    port = free_port()
    command = [*argv, 'serve', '--host', '127.0.0.1', '--port', str(port), '--json']
    environment = {**environment, 'NUMERA_API_TOKEN': 'benchmark-only'}
    attempt = unit_report.begin_spawn()
    process = query = None
    try:
        if sys.platform == 'win32':
            from benchmark_windows import spawn
            process, tree, query = spawn(command, environment)
        else:
            process, tree = proctree.spawn(command, env=environment, stdin=subprocess.DEVNULL,
                                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            unit_report.record(tree, attempt=attempt)
            readers = [pipe_reader(process.stdout), pipe_reader(process.stderr)]
            ended, output_bytes = set(), [0, 0]

            def drain():
                for index, read in enumerate(readers):
                    if index in ended:
                        continue
                    while True:
                        chunk = read()
                        if chunk == b'':
                            ended.add(index)
                        if not chunk:
                            break
                        output_bytes[index] += len(chunk)
                        if output_bytes[index] > 65536:
                            raise ValueError('HTTP benchmark server output exceeds its bound')

            deadline = time.monotonic() + 10
            while True:
                if process.poll() is not None:
                    raise ValueError('HTTP benchmark server exited before readiness')
                drain()
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('HTTP benchmark server readiness exceeded its deadline')
                try:
                    ready = request(port, '/api/ready', min(2, remaining))
                    if ready.get('ok') is True and ready.get('service') == 'numera-browser':
                        break
                except (OSError, http.client.HTTPException):
                    pass
                time.sleep(min(.01, max(0, deadline - time.monotonic())))
            started = time.perf_counter_ns()
            payload = request(port, '/api/profiles', 2)
            elapsed = time.perf_counter_ns() - started
            if process.poll() is not None:
                raise ValueError('HTTP benchmark server exited during its request')
            drain()
        finally:
            try:
                clean = proctree.end_tree(tree, grace=1, kill_grace=3)
                process.wait(timeout=5)
                if not clean:
                    raise RuntimeError('HTTP benchmark server tree did not settle')
                if 'readers' in locals():
                    drain()
                    if len(ended) != 2:
                        raise RuntimeError('HTTP benchmark server pipes did not reach EOF')
            finally:
                process.stdout.close()
                process.stderr.close()
        memory = windows_peak(query) if query is not None else linux_peak()
        return {'elapsed_ns': elapsed, 'memory': memory, 'payload': payload}
    except Exception as error:
        logger.error('HTTP observation failed type=%s', type(error).__name__)
        raise
    finally:
        if query is not None:
            from numera.winjob import _kernel32
            process.close()
            _kernel32().CloseHandle(query)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--original', type=Path, required=True)
    parser.add_argument('--runtime', choices=('original', 'native'), required=True)
    parser.add_argument('--data-dir', type=Path, required=True)
    args = parser.parse_args()
    executable = ([sys.executable, '-m', 'numera'] if args.runtime == 'original'
                  else [str(args.candidate.resolve(strict=True))])
    environment = dict(os.environ, PYTHONPATH=str(args.original / 'engine'))
    with run_log('benchmark-http-sample', Path(os.environ['NUMERA_LOG_DIR'])) as logger:
        logger.info('Observing HTTP runtime=%s', args.runtime)
        result = observe([*executable, '--data-dir', str(args.data_dir)], environment, logger)
        print(json.dumps(result, allow_nan=False), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
