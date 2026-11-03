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


def registered_receipt(report, deadline):
    # Parent registration follows spawn; never interpret its pending self-transfer as a target unit.
    while True:
        receipt = unit_report.read(report)
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ValueError('Browser benchmark parent ownership receipt did not register its observer')
        if receipt.complete and any(tree.pid == os.getpid() for tree in receipt.trees):
            # Registration proves the parent completed job assignment; this query never adopts a job.
            observer = proctree._inherited_tree()
            if observer is None or not observer.verified or observer not in receipt.trees:
                raise ValueError('Browser benchmark observer ownership identity differs from its receipt')
            return receipt
        time.sleep(min(.01, remaining))


def observe(argv, environment, logger, *, wall=15, nested=False, diagnostics=False):
    if diagnostics and not nested:
        raise ValueError('Memory diagnostics require nested browser ownership')
    if not 0 < wall <= 90:
        raise ValueError('Benchmark deadline must be positive and at most 90 seconds')
    deadline = time.monotonic() + wall if nested else None
    report_value = os.environ.get(unit_report.UNITS_ENV)
    if nested and not report_value:
        raise ValueError('Browser benchmark requires a parent ownership receipt')
    report = Path(report_value) if nested else None
    before = registered_receipt(report, min(deadline, time.monotonic() + 2)) if nested else None
    if sys.platform not in ('linux', 'win32'):
        raise ValueError('Benchmark observation requires Windows or Linux')
    if sys.platform == 'linux':
        import resource
        if resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss != 0:
            raise ValueError('Each Linux sample requires a fresh observer process')
    attempt = unit_report.begin_spawn()
    query = None
    process = None
    recorder = None
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
        if diagnostics:
            from benchmark_memory import Recorder
            recorder = Recorder(tree, report, before.trees, query)
        phase = 'pipe-setup'
        readers = [pipe_reader(process.stdout), pipe_reader(process.stderr)]
        buffers = [bytearray(), bytearray()]
        ended = set()
        if deadline is None:
            deadline = time.monotonic() + wall
        phase = 'capture'
        while len(ended) != 2 or process.poll() is None:
            if recorder is not None:
                recorder.sample()
            if time.monotonic() >= deadline:
                raise TimeoutError(f'Benchmark child exceeded its {wall}-second deadline')
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
        phase = 'exit'
        code = process.wait(timeout=1)
        if code != 0 or buffers[1]:
            logger.error('Target refused exit=%s stdout_bytes=%s stderr_bytes=%s',
                         code, len(buffers[0]), len(buffers[1]))
            raise ValueError('Benchmark child failed or wrote unexpected stderr')
        phase = 'tree-empty'
        remaining = deadline - time.monotonic()
        if (remaining <= 0 or not proctree.prove_empty(tree, deadline=remaining)
                or time.monotonic() >= deadline):
            raise ValueError('Foreground benchmark tree did not settle within its deadline')
        if nested:
            receipt = unit_report.read(report)
            if not receipt.complete:
                raise ValueError('Browser benchmark ownership receipt is incomplete')
            for child in receipt.trees:
                if child not in before.trees and child != tree:
                    remaining = deadline - time.monotonic()
                    if (remaining <= 0 or not proctree.prove_empty(child, deadline=remaining)
                            or time.monotonic() >= deadline):
                        raise ValueError('Browser benchmark nested tree did not settle within its deadline')
        elapsed = time.perf_counter_ns() - started
        phase = 'memory'
        memory = windows_peak(query) if query is not None else linux_peak()
        phase = 'json'
        result = {'elapsed_ns': elapsed, 'memory': memory,
                  'payload': json.loads(buffers[0].decode('utf-8'))}
        if recorder is not None:
            result['diagnostics'] = recorder.result()
        return result
    except Exception as error:
        logger.error('Observation failed phase=%s type=%s winerror=%s', phase, type(error).__name__,
                     getattr(error, 'winerror', None) if isinstance(getattr(error, 'winerror', None), int) else None)
        raise
    finally:
        if process is not None:
            try:
                clean = proctree.end_tree(tree, grace=1, kill_grace=3)
                try:
                    process.wait(timeout=5)
                finally:
                    if nested:
                        receipt = unit_report.read(report)
                        end = time.monotonic() + 5
                        for child in receipt.trees:
                            if child not in before.trees and child != tree:
                                clean = proctree.end_tree(child, ends_at=end) and clean
                        clean = receipt.complete and clean
                    if not clean:
                        raise RuntimeError('Benchmark child cleanup could not be proved')
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
    parser.add_argument('--wall', type=int, default=15, help='Command and natural-settlement ceiling, at most 90s')
    parser.add_argument('--nested', action='store_true', help='Require all reported SDK trees to settle naturally')
    parser.add_argument('--memory-diagnostics', action='store_true',
                        help='Unmeasured numeric snapshots; never performance acceptance')
    parser.add_argument('arguments', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    arguments = args.arguments[1:] if args.arguments[:1] == ['--'] else args.arguments
    executable = ([sys.executable, '-m', 'numera'] if args.runtime == 'original'
                  else [str(args.candidate.resolve(strict=True))])
    environment = dict(os.environ, PYTHONPATH=str(args.original / 'engine'))
    with run_log('benchmark-sample', Path(os.environ['NUMERA_LOG_DIR'])) as logger:
        logger.info('Observing runtime=%s', args.runtime)
        result = observe([*executable, *arguments], environment, logger, wall=args.wall,
                         nested=args.nested, diagnostics=args.memory_diagnostics)
        print(json.dumps(result, allow_nan=False), flush=True)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
