"""One foreground CLI observation per process, with OS lifetime peak accounting."""
from __future__ import annotations

import argparse
import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
sys.path.insert(0, str(ROOT / 'scripts/migration'))
from numera import proctree, unit_report
from run_logging import run_log


def pipe_reader(pipe):
    # Loading the HTTP package would inflate this observer's pre-exec RSS floor.
    import importlib.util
    spec = importlib.util.spec_from_file_location(
        'benchmark_pipe', ROOT / 'engine/numera/server/helper_streams.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module._nonblocking_read(pipe)


def linux_peak():
    if sys.platform != 'linux':
        raise ValueError('Child resident peak requires Linux resource accounting')
    import resource
    peak = resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss
    if not isinstance(peak, int) or isinstance(peak, bool) or peak <= 0:
        raise ValueError('OS peak memory must be a positive integer')
    return {'metric': 'peak-child-rss-bytes', 'bytes': peak * 1024}


def windows_peak(query):
    from numera.winjob import _job_structs, _kernel32

    _, structure = _job_structs()
    counters = structure()
    if not _kernel32().QueryInformationJobObject(
            query, 9, ctypes.byref(counters), ctypes.sizeof(counters), None):
        raise OSError('Windows benchmark job peak could not be queried')
    peak = counters.PeakJobMemoryUsed
    if peak <= 0:
        raise ValueError('OS peak memory must be a positive integer')
    return {'metric': 'peak-job-committed-bytes', 'bytes': peak}


def observe(argv, environment, logger):
    if sys.platform not in ('linux', 'win32'):
        raise ValueError('Benchmark observation requires Windows or Linux')
    if sys.platform == 'linux':
        import resource
        if resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss != 0:
            raise ValueError('Each Linux sample requires a fresh observer process')
    attempt = unit_report.begin_spawn()
    query = None
    process = None
    phase = 'spawn'
    started = time.perf_counter_ns()
    try:
        if sys.platform == 'win32':
            from benchmark_windows import spawn
            process, tree, query = spawn(argv, environment)
        else:
            process, tree = proctree.spawn(argv, env=environment, stdin=subprocess.DEVNULL,
                                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        unit_report.record(tree, attempt=attempt)
        phase = 'pipe-setup'
        readers = [pipe_reader(process.stdout), pipe_reader(process.stderr)]
        buffers = [bytearray(), bytearray()]
        ended = set()
        deadline = time.monotonic() + 15
        phase = 'capture'
        while len(ended) != 2 or process.poll() is None:
            if time.monotonic() >= deadline:
                raise TimeoutError('Benchmark child exceeded its 15-second deadline')
            progress = False
            for index, read in enumerate(readers):
                if index in ended:
                    continue
                chunk = read()
                if chunk == b'':
                    ended.add(index)
                elif chunk is not None:
                    progress = True
                    buffers[index].extend(chunk)
                    if len(buffers[index]) > 131072:
                        raise ValueError('Benchmark child output exceeded its bound')
            if not progress:
                time.sleep(min(0.001, max(0, deadline - time.monotonic())))
        elapsed = time.perf_counter_ns() - started
        phase = 'exit'
        code = process.wait(timeout=1)
        if code != 0 or buffers[1]:
            logger.error('Target refused exit=%s stdout_bytes=%s stderr_bytes=%s',
                         code, len(buffers[0]), len(buffers[1]))
            raise ValueError('Benchmark child failed or wrote unexpected stderr')
        phase = 'tree-empty'
        if proctree.alive_tree(tree):
            raise ValueError('Foreground benchmark child left an active owned tree')
        phase = 'memory'
        memory = windows_peak(query) if query is not None else linux_peak()
        phase = 'json'
        return {'elapsed_ns': elapsed, 'memory': memory,
                'payload': json.loads(buffers[0].decode('utf-8'))}
    except Exception as error:
        logger.error('Observation failed phase=%s type=%s winerror=%s', phase, type(error).__name__,
                     getattr(error, 'winerror', None) if isinstance(getattr(error, 'winerror', None), int) else None)
        raise
    finally:
        if process is not None:
            try:
                if not proctree.end_tree(tree, grace=1, kill_grace=3):
                    raise RuntimeError('Benchmark child cleanup could not be proved')
                process.wait(timeout=5)
            finally:
                for pipe in (process.stdout, process.stderr):
                    pipe.close()
                if query is not None:
                    from numera.winjob import _kernel32
                    process.close()
                    _kernel32().CloseHandle(query)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--original', type=Path, required=True)
    parser.add_argument('--runtime', choices=('original', 'native'), required=True)
    parser.add_argument('arguments', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    arguments = args.arguments[1:] if args.arguments[:1] == ['--'] else args.arguments
    executable = ([sys.executable, '-m', 'numera'] if args.runtime == 'original'
                  else [str(args.candidate.resolve(strict=True))])
    environment = dict(os.environ, PYTHONPATH=str(args.original / 'engine'))
    with run_log('benchmark-sample', Path(os.environ['NUMERA_LOG_DIR'])) as logger:
        logger.info('Observing runtime=%s', args.runtime)
        result = observe([*executable, *arguments], environment, logger)
        print(json.dumps(result, allow_nan=False), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
