"""Project pinned compiler phase receipts without publishing raw compiler output."""
from __future__ import annotations

from contextlib import contextmanager
import json
import logging
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time

LINE_BYTES = 65536
CACHE_BYTES = 2 * 1024 * 1024
TARGET = 'rustc_codegen_ssa::back::link'


def decode(text):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError('Duplicate compiler field')
            result[key] = value
        return result

    def constant(value):
        raise ValueError('Invalid compiler number')

    return json.loads(text, object_pairs_hook=pairs, parse_constant=constant)


def regular(path, *, directory=False):
    details = path.lstat()
    expected = stat.S_ISDIR if directory else stat.S_ISREG
    if (not expected(details.st_mode)
            or getattr(details, 'st_file_attributes', 0) & getattr(stat, 'FILE_ATTRIBUTE_REPARSE_POINT', 0)
            or (not directory and details.st_nlink != 1)):
        raise ValueError('Compiler cache ownership is uncertain')
    return details


def cache_identity(details):
    # Windows path stat retains creation-time ctime, while fstat reports change
    # time. Both expose the same birthtime; Unix retains its change-time guard.
    timestamp = (getattr(details, 'st_birthtime_ns', details.st_ctime_ns)
                 if sys.platform == 'win32' else details.st_ctime_ns)
    return (details.st_dev, details.st_ino, details.st_size, details.st_mtime_ns,
            timestamp, details.st_nlink)


def replace_cache(path, raw, before):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(dir=path.parent, prefix='compiler-phase-', delete=False) as output:
            temporary = Path(output.name)
            output.write(raw)
            output.flush()
            os.fsync(output.fileno())
            output.seek(0)
            if output.read(CACHE_BYTES + 1) != raw:
                raise ValueError('Compiler cache readback differs')
        if cache_identity(regular(path)) != cache_identity(before):
            raise ValueError('Compiler cache identity changed')
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def sanitize_cache(root, seen):
    # Cargo caches raw rustc JSON independently of the proof logger. Preserve
    # diagnostics and fingerprints; remove only this scoped INFO tracing target.
    parent = root
    for part in ('engine-rust', 'target', 'debug', '.fingerprint'):
        parent = parent / part
        try:
            regular(parent, directory=True)
        except FileNotFoundError:
            return 0
    removed = files = total = 0
    with os.scandir(parent) as entries:
        for number, entry in enumerate(entries):
            if number >= 4096:
                raise ValueError('Compiler cache directory bound exceeded')
            if not re.fullmatch(r'[A-Za-z0-9_-]+-[0-9a-f]{16}', entry.name):
                continue
            directory = Path(entry.path)
            regular(directory, directory=True)
            with os.scandir(directory) as outputs:
                for member_index, member in enumerate(outputs):
                    if member_index >= 512:
                        raise ValueError('Compiler cache member bound exceeded')
                    if not re.fullmatch(r'output-[A-Za-z0-9_-]{1,160}', member.name):
                        continue
                    files += 1
                    if files > 512:
                        raise ValueError('Compiler cache file bound exceeded')
                    path = Path(member.path)
                    before = regular(path)
                    if before.st_size > CACHE_BYTES or total + before.st_size > 32 * 1024 * 1024:
                        raise ValueError('Compiler cache byte bound exceeded')
                    if seen.get(path) == cache_identity(before):
                        continue
                    total += before.st_size
                    with path.open('rb') as output:
                        if cache_identity(os.fstat(output.fileno())) != cache_identity(before):
                            raise ValueError('Compiler cache identity changed')
                        raw = output.read(CACHE_BYTES + 1)
                    if len(raw) != before.st_size:
                        raise ValueError('Compiler cache size changed')
                    kept, count = [], 0
                    for line in raw.splitlines(keepends=True):
                        if len(line) > LINE_BYTES:
                            raise ValueError('Compiler cache line bound exceeded')
                        text = line.decode('utf-8', errors='strict')
                        event = decode(text) if text.lstrip().startswith('{') else None
                        if (isinstance(event, dict) and event.get('target') == TARGET
                                and event.get('level') == 'INFO'):
                            count += 1
                        else:
                            kept.append(line)
                    if count:
                        replace_cache(path, b''.join(kept), before)
                        removed += count
                    current = regular(path)
                    if not count and cache_identity(current) != cache_identity(before):
                        raise ValueError('Compiler cache identity changed')
                    seen[path] = cache_identity(current)
    return removed


def rust_path(text):
    if not re.fullmatch(r'"(?:[^"\\]|\\(?:["\\ntr0]|u\{[0-9a-fA-F]{1,6}\}))*"', text):
        raise ValueError('Invalid Rust path representation')

    def escape(match):
        value = match[0][1:]
        if value.startswith('u{'):
            number = int(value[2:-1], 16)
            if number > 0x10ffff or 0xd800 <= number <= 0xdfff:
                raise ValueError('Invalid Rust scalar')
            return chr(number)
        return {'"': '"', '\\': '\\', 'n': '\n', 't': '\t', 'r': '\r', '0': '\0'}[value]

    return re.sub(r'\\(?:u\{[0-9a-fA-F]{1,6}\}|.)', escape, text[1:-1])


class Projection(logging.Filter):
    def __init__(self, root, index):
        super().__init__()
        self.manifest = str(root / 'engine-rust/Cargo.toml')
        self.archives = root / 'engine-rust/target/debug/deps'
        self.index = index
        self.started = time.monotonic_ns()
        self.archive = self.start = self.end = None
        self.markers = self.artifacts = 0
        self.fresh = self.invalid = self.finished = self.testing = False

    def filter(self, record):
        text = record.getMessage()
        if self.finished or self.testing:
            return True
        if text == 'running 1 test':
            # Missing build-finished is unavailable evidence, not permission to
            # discard the original libtest assertion or change its verdict.
            self.testing = True
            return True
        if not text.lstrip().startswith('{'):
            return str(record.msg).startswith(('Owned ', 'Verification '))
        try:
            if len(text) > LINE_BYTES:
                raise ValueError('Compiler line bound exceeded')
            event = decode(text)
            if not isinstance(event, dict):
                raise ValueError('Invalid compiler record')
            if event.get('reason') == 'build-finished':
                if type(event.get('success')) is not bool:
                    raise ValueError('Invalid build completion')
                self.finished = True
                self.invalid |= not event['success']
                return False
            target = event.get('target')
            if (event.get('manifest_path') != self.manifest or not isinstance(target, dict)
                    or target.get('name') != 'numera_core' or target.get('kind') != ['lib']):
                return False
            if event.get('reason') == 'compiler-message':
                message = event.get('message')
                if not isinstance(message, dict) or message.get('target') != TARGET or message.get('level') != 'INFO':
                    return False
                fields = message.get('fields')
                value = fields.get('message') if isinstance(fields, dict) else None
                if not isinstance(value, str) or not value.startswith('preparing rlib to '):
                    return False
                archive = rust_path(value[len('preparing rlib to '):])
                if not isinstance(archive, str):
                    raise ValueError('Invalid archive path')
                path = Path(archive)
                if (path.parent != self.archives
                        or not re.fullmatch(r'libnumera_core-[0-9a-f]{16}\.rlib', path.name)):
                    raise ValueError('Unexpected archive path')
                self.markers += 1
                if self.markers != 1 or self.artifacts:
                    raise ValueError('Archive event order differs')
                self.archive = str(path)
                self.start = time.monotonic_ns() - self.started
                message = f'event=archive-start elapsed_ns={self.start}'
            elif event.get('reason') == 'compiler-artifact':
                profile = event.get('profile')
                if not isinstance(profile, dict) or profile.get('test') is not False:
                    return False
                self.artifacts += 1
                if self.artifacts != 1 or type(event.get('fresh')) is not bool:
                    raise ValueError('Artifact evidence differs')
                self.fresh = event['fresh']
                if self.fresh:
                    return False
                filenames = event.get('filenames')
                if (self.start is None or not isinstance(filenames, list)
                        or filenames.count(self.archive) != 1):
                    raise ValueError('Archive artifact differs')
                self.end = time.monotonic_ns() - self.started
                if not 0 <= self.start <= self.end <= 2**64 - 1:
                    raise ValueError('Compiler clock differs')
                message = (f'event=archive-complete elapsed_ns={self.end} '
                           f'archive_to_artifact_ns={self.end - self.start}')
            else:
                return False
            record.msg, record.args = f'Historical compiler index={self.index} {message}', ()
            return True
        except (ValueError, TypeError, RecursionError):
            self.invalid = True
            return False

    def status(self):
        if self.invalid or not self.finished or self.artifacts != 1:
            return 'unavailable'
        if self.fresh:
            return 'fresh'
        return 'complete' if self.markers == 1 and self.end is not None else 'unavailable'


@contextmanager
def compiler_phase(root, index, logger, *, enabled, seen=None):
    if not enabled:
        yield []
        return
    previous = {key: value for key, value in os.environ.items() if key.startswith('RUSTC_LOG')}
    projection = Projection(root, index)
    logger.filters.insert(0, projection)
    try:
        for key in previous:
            os.environ.pop(key)
        os.environ.update({'RUSTC_LOG': TARGET + '=info', 'RUSTC_LOG_FORMAT_JSON': '1',
                           'RUSTC_LOG_COLOR': 'never'})
        yield ['--message-format=json']
    finally:
        failed = sys.exc_info()[0] is not None
        cleanup_failed, removed = False, 0
        try:
            removed = sanitize_cache(root, {} if seen is None else seen)
        except (OSError, ValueError, RecursionError):
            cleanup_failed = True
        finally:
            logger.removeFilter(projection)
            for key in list(os.environ):
                if key.startswith('RUSTC_LOG'):
                    os.environ.pop(key)
            os.environ.update(previous)
        if cleanup_failed:
            logger.error('Historical compiler index=%s status=cache-cleanup-failed', index)
        logger.info('Historical compiler index=%s status=%s '
                    'coverage=receipt-interval-not-exclusive-compiler cache_records_removed=%s',
                    index, projection.status(), removed)
        if cleanup_failed and not failed:
            raise RuntimeError('Compiler diagnostic cache could not be sanitized') from None
