"""Bound one migration verification command and settle its entire owned tree."""
from __future__ import annotations

import argparse
import math
import os
import queue
import shutil
import secrets
import stat
import subprocess
import sys
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'engine'))
from numera.proctree import spawn, end_tree  # noqa: E402
from run_logging import run_log  # noqa: E402


# Linux sun_path is 108 bytes including NUL. Chromium appends this fixed
# six-character-random directory and socket filename beneath TMPDIR.
CHROMIUM_SOCKET_SUFFIX = '/org.chromium.Chromium.XXXXXX/SingletonSocket'


def temporary_parent(root: Path, env: dict[str, str], platform: str) -> Path:
    parent = root / '.ci-work'
    if platform != 'linux':
        return parent
    fits = lambda path: len(str(path / 't000000').encode('utf-8')) + len(CHROMIUM_SOCKET_SUFFIX) <= 107
    if fits(parent):
        return parent
    # An overlong checkout cannot contain a valid Chromium AF_UNIX path.
    # Only an explicit hosted runner temp directory permits this exception.
    runner = env.get('RUNNER_TEMP', '')
    if env.get('GITHUB_ACTIONS') != 'true' or not runner or not Path(runner).is_absolute():
        raise ValueError('Linux checkout exceeds the browser socket path limit; a hosted RUNNER_TEMP is required')
    parent = Path(runner)
    if not fits(parent):
        raise ValueError('Hosted RUNNER_TEMP also exceeds the browser socket path limit')
    return parent


def create_temporary(root: Path, env: dict[str, str], platform: str) -> Path:
    parent = temporary_parent(root, env, platform)
    parent.mkdir(parents=True, exist_ok=True)
    for _ in range(32):
        temporary = parent / ('t' + secrets.token_hex(3))
        try:
            temporary.mkdir(mode=0o700)
            return temporary
        except FileExistsError:
            continue
    raise OSError('Cannot create a unique owned verification temp directory')


def settle_reported(path: Path, clean: bool, seconds: float) -> tuple[bool, int, bool]:
    from numera import unit_report

    deadline = time.monotonic() + seconds
    # Stop reported producers, then reread: they may publish an SDK unit
    # between a snapshot and termination. Unresolved intent stays unproved.
    while True:
        before = path.read_bytes()
        receipt = unit_report.read(path)
        for nested in receipt.trees:
            clean = end_tree(nested, ends_at=deadline) and clean
        if path.read_bytes() == before:
            return receipt.complete and clean, len(receipt.trees), receipt.complete
        if time.monotonic() >= deadline:
            return False, len(receipt.trees), receipt.complete


def run(command: list[str], wall: float, idle: float, logger, *, report_units: bool = False) -> int:
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
    temporary = create_temporary(ROOT, env, sys.platform)
    logger.info('Owned verification temp path=%s socket_limit_exception=%s', temporary,
                not temporary.is_relative_to(ROOT))
    receipt_path = None
    if report_units:
        from numera import unit_report
        jobs = temporary / 'jobs'
        jobs.mkdir()
        receipt_path = jobs / 'verification.units'
        receipt_path.touch(exist_ok=False)
        unit_report.initialize(receipt_path)
        env[unit_report.UNITS_ENV] = str(receipt_path)
    env['NUMERA_TEST_TEMP'] = str(temporary)
    for key in ('TEMP', 'TMP', 'TMPDIR'):
        env[key] = str(temporary)
    logger.info('Verification phase=spawn wall=%ss idle=%ss', wall, idle)
    try:
        proc, tree = spawn(command, env=env, stdin=subprocess.DEVNULL,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True, encoding='utf-8', errors='strict', bufsize=1)
    except Exception:
        logger.warning('Spawn failed; retain owned temp until child ownership is reconciled path=%s', temporary)
        raise
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
    logger.info('Owned child pid=%s wall=%ss idle=%ss workers=%s phase=monitor', proc.pid, wall, idle, budget)
    ended = False
    try:
        while True:
            now = time.monotonic()
            if now - started >= wall or now - progress >= idle:
                logger.error('Verification timeout wall_expired=%s idle_expired=%s '
                             'elapsed=%.3fs silence=%.3fs wall=%.3fs idle=%.3fs',
                             now - started >= wall, now - progress >= idle,
                             now - started, now - progress, wall, idle)
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
        logger.info('Verification phase=settle-tree pid=%s', proc.pid)
        clean = end_tree(tree, grace=2, kill_grace=3)
        if receipt_path is not None:
            clean, count, complete = settle_reported(receipt_path, clean, 10)
            logger.info('Verification reported units=%s complete=%s clean=%s', count, complete, clean)
        logger.info('Verification phase=reap-leader pid=%s clean=%s', proc.pid, clean)
        proc.wait(timeout=5)
        logger.info('Verification phase=join-reader pid=%s', proc.pid)
        reader.join(timeout=2)
        if reader.is_alive():
            raise RuntimeError('Command output reader did not settle')
        logger.info('Verification phase=close-output pid=%s', proc.pid)
        proc.stdout.close()
        logger.info('Owned child settled pid=%s exit=%s clean=%s', proc.pid, proc.returncode, clean)
        if not clean:
            raise RuntimeError('Command process tree could not be proved empty')
        logger.info('Verification phase=remove-temp pid=%s path=%s', proc.pid, temporary)

        def cleanup_error(function, path, exc_info):
            error = exc_info[1]
            operation = getattr(function, '__name__', '')
            if operation not in ('unlink', 'rmdir', 'scandir', 'islink', 'lstat', 'open', 'close'):
                operation = 'unknown'
            number = lambda value: value if type(value) is int else None
            kind, readonly, reparse = 'unknown', None, None
            try:
                details = os.lstat(path)
                kind = ('link' if stat.S_ISLNK(details.st_mode) else
                        'file' if stat.S_ISREG(details.st_mode) else
                        'directory' if stat.S_ISDIR(details.st_mode) else 'other')
                attributes = getattr(details, 'st_file_attributes', 0)
                readonly = bool(attributes & stat.FILE_ATTRIBUTE_READONLY)
                reparse = bool(attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT)
            except OSError:
                pass
            logger.error('Owned temp cleanup failed operation=%s errno=%s winerror=%s kind=%s readonly=%s reparse=%s',
                         operation, number(getattr(error, 'errno', None)),
                         number(getattr(error, 'winerror', None)), kind, readonly, reparse)
            raise error.with_traceback(exc_info[2])

        shutil.rmtree(temporary, onerror=cleanup_error)
        logger.info('Owned verification temp removed path=%s', temporary)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wall', type=float, required=True)
    parser.add_argument('--idle', type=float, required=True)
    parser.add_argument('--report-units', action='store_true',
                        help='Own versioned separately grouped child receipts through forced timeouts')
    parser.add_argument('command', nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ['--'] else args.command
    class CommandFailed(Exception):
        def __init__(self, code):
            self.code = code

    try:
        with run_log('migration-command') as logger:
            try:
                code = run(command, args.wall, args.idle, logger, report_units=args.report_units)
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
