"""Bound one migration verification command and settle its entire owned tree."""
from __future__ import annotations

import argparse
import math
import os
import queue
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera.proctree import spawn, end_tree  # noqa: E402
from run_logging import run_log  # noqa: E402


def run(command: list[str], wall: float, idle: float, logger) -> int:
    if not command or wall <= 0 or idle <= 0 or not all(map(math.isfinite, (wall, idle))):
        raise ValueError('A command and finite positive wall/idle limits are required')
    env = os.environ.copy()
    # This wrapper is a verification boundary, not an application launch. A test
    # must not inherit a real store unlock or token from the developer's shell.
    for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR'):
        env.pop(key, None)
    env['PYTHONUTF8'] = '1'
    env['PYTHONIOENCODING'] = 'utf-8'
    env['NUMERA_ORACLE_PYTHON'] = sys.executable
    budget = max(1, min(2, int(env.get('HOOKMAKER_MAX_TEST_WORKERS', '2'))))
    env['CARGO_BUILD_JOBS'] = str(budget)
    env['NUMERA_TEST_TEMP'] = str(ROOT / '.ci-work' / 'migration-temp')
    proc, tree = spawn(command, env=env, stdin=subprocess.DEVNULL,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                       text=True, encoding='utf-8', errors='strict', bufsize=1)
    started = progress = time.monotonic()
    events: queue.Queue = queue.Queue()

    def pump():
        try:
            for line in proc.stdout:
                events.put(('line', line))
        except Exception as error:
            events.put(('error', type(error).__name__))
        finally:
            events.put(('eof', None))

    reader = threading.Thread(target=pump, name='migration-output', daemon=True)
    reader.start()
    logger.info('Owned child pid=%s wall=%ss idle=%ss workers=%s', proc.pid, wall, idle, budget)
    ended = False
    try:
        while True:
            now = time.monotonic()
            if now - started >= wall or now - progress >= idle:
                raise TimeoutError('Migration command exceeded wall/idle budget')
            try:
                kind, value = events.get(timeout=min(0.1, wall - (now - started), idle - (now - progress)))
            except queue.Empty:
                if proc.poll() is not None and ended:
                    return proc.returncode
                continue
            if kind == 'line':
                progress = time.monotonic()
                # Output goes through the same redaction filter as file logging.
                logger.info('%s', value.rstrip('\r\n'))
            elif kind == 'error':
                raise RuntimeError(f'Command output could not be decoded ({value})')
            else:
                ended = True
                if proc.poll() is not None:
                    return proc.returncode
    finally:
        clean = end_tree(tree, grace=2, kill_grace=3)
        proc.wait(timeout=5)
        reader.join(timeout=2)
        if reader.is_alive():
            raise RuntimeError('Command output reader did not settle')
        proc.stdout.close()
        logger.info('Owned child settled pid=%s exit=%s clean=%s', proc.pid, proc.returncode, clean)
        if not clean:
            raise RuntimeError('Command process tree could not be proved empty')


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wall', type=float, required=True)
    parser.add_argument('--idle', type=float, required=True)
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    class CommandFailed(Exception):
        def __init__(self, code):
            self.code = code

    try:
        with run_log('migration-command') as logger:
            try:
                code = run(command, args.wall, args.idle, logger)
            except TimeoutError:
                logger.error('Command timeout exit=124')
                raise CommandFailed(124) from None
            except Exception as error:
                logger.error('Command failed type=%s exit=1', type(error).__name__)
                raise CommandFailed(1) from None
            logger.info('Command result exit=%s', code)
            if code:
                raise CommandFailed(code)
        return 0
    except CommandFailed as error:
        return error.code


if __name__ == '__main__':
    raise SystemExit(main())
