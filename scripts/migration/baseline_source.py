"""Materialize and activate the immutable original application for migration observers."""
from __future__ import annotations

import atexit
from contextlib import contextmanager
from functools import lru_cache
import hashlib
import importlib
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[2]
BASELINE = '13b6d9c6973281fe123353eaec05b29d65781a2d'
BASELINE_TREE = '760f9873ded13c5ebfecae1f16b385e34f695124'
SOURCE_TREES = {'engine/numera': '25cd7f34689e4a1688c426fdb7d4a6c4b898998e',
                'engine/tests': 'ec1a33ab13ca6ee1dd6e7c629de4118ab848c82a'}
ENVIRONMENT = 'NUMERA_ORACLE_SOURCE_ROOT'
MAX_BYTES = 8 * 1024 * 1024


def git(*arguments: str, data: bytes | None = None) -> bytes:
    return subprocess.run(['git', '-C', str(ROOT), *arguments], input=data,
                          env={**os.environ, 'GIT_NO_REPLACE_OBJECTS': '1'},
                          check=True, capture_output=True, timeout=10).stdout


@lru_cache(maxsize=1)
def entries() -> tuple:
    for prefix, expected in SOURCE_TREES.items():
        if git('rev-parse', f'{BASELINE_TREE}:{prefix}').decode('ascii').strip() != expected:
            raise ValueError('Original source tree identity differs from the reviewed baseline')
    rows = []
    total = 0
    for raw in git('ls-tree', '-rlz', BASELINE_TREE, '--', *SOURCE_TREES).split(b'\0'):
        if not raw:
            continue
        metadata, name = raw.split(b'\t', 1)
        mode, kind, oid, size = metadata.split()
        relative = PurePosixPath(name.decode('utf-8', errors='strict'))
        if (mode not in (b'100644', b'100755') or kind != b'blob'
                or relative.is_absolute() or '..' in relative.parts or '\\' in str(relative)
                or len(relative.parts) < 3 or '/'.join(relative.parts[:2]) not in SOURCE_TREES
                or any(ord(character) < 32 for character in str(relative))):
            raise ValueError('Original source contains an unsafe or nonregular entry')
        length = int(size)
        total += length
        if length < 0 or total > MAX_BYTES or len(rows) >= 1024:
            raise ValueError('Original source exceeds its bounded reference scope')
        rows.append((relative.as_posix(), oid.decode('ascii'), length, mode == b'100755'))
    if not rows or len({row[0] for row in rows}) != len(rows):
        raise ValueError('Original source file inventory is empty or duplicated')
    return tuple(rows)


def blob_oid(raw: bytes) -> str:
    return hashlib.sha1(b'blob ' + str(len(raw)).encode('ascii') + b'\0' + raw,
                        usedforsecurity=False).hexdigest()


def materialize(destination: Path) -> None:
    """Read exact blobs in one bounded Git process; no checkout filters or archive attributes."""
    rows = entries()
    packed = git('cat-file', '--batch', data=''.join(oid + '\n' for _, oid, _, _ in rows).encode('ascii'))
    offset = 0
    for name, oid, size, executable in rows:
        end = packed.find(b'\n', offset)
        if end < 0 or packed[offset:end] != f'{oid} blob {size}'.encode('ascii'):
            raise ValueError('Original source Git batch header differs')
        offset = end + 1
        raw = packed[offset:offset + size]
        if len(raw) != size or blob_oid(raw) != oid or packed[offset + size:offset + size + 1] != b'\n':
            raise ValueError('Original source Git blob integrity differs')
        offset += size + 1
        path = destination / name
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open('xb') as output:
            output.write(raw)
        path.chmod(0o755 if executable else 0o644)
    if offset != len(packed):
        raise ValueError('Original source Git batch contains unexpected trailing data')


def verify(directory: Path) -> Path:
    parent = ROOT / '.ci-work'
    root = directory.resolve(strict=True)
    if directory.is_symlink() or not root.is_relative_to(parent.resolve()) or root == parent.resolve():
        raise ValueError('Original source reference is outside its owned project workspace')
    expected = {name for name, _, _, _ in entries()}
    actual = set()
    for path in root.rglob('*'):
        if path.is_symlink() or not path.resolve().is_relative_to(root):
            raise ValueError('Original source reference contains a symbolic link or escaping path')
        if path.is_file():
            actual.add(path.relative_to(root).as_posix())
    if actual != expected:
        raise ValueError('Original source reference file inventory differs')
    for name, oid, size, _ in entries():
        path = root / name
        if path.stat().st_size != size:
            raise ValueError('Original source reference bytes differ from the frozen baseline')
        raw = path.read_bytes()
        if blob_oid(raw) != oid:
            raise ValueError('Original source reference bytes differ from the frozen baseline')
    return root


@contextmanager
def reference():
    """One parent-owned extraction shared by its explicitly configured observer children."""
    configured = os.environ.get(ENVIRONMENT)
    if configured:
        yield verify(Path(configured))
        return
    parent = ROOT / '.ci-work'
    parent.mkdir(parents=True, exist_ok=True)
    with TemporaryDirectory(prefix='original-source-', dir=parent) as temporary:
        root = Path(temporary)
        materialize(root)
        yield root


@lru_cache(maxsize=1)
def source_root() -> Path:
    configured = os.environ.get(ENVIRONMENT)
    if configured:
        return verify(Path(configured))
    parent = ROOT / '.ci-work'
    parent.mkdir(parents=True, exist_ok=True)
    owned = TemporaryDirectory(prefix='original-source-', dir=parent)
    atexit.register(owned.cleanup)
    root = Path(owned.name)
    try:
        materialize(root)
    except BaseException:
        owned.cleanup()
        raise
    return root


def tracked_paths(prefix: str) -> list[Path]:
    if prefix not in SOURCE_TREES:
        raise ValueError('Original source prefix is outside the reviewed scope')
    root = source_root()
    return sorted(root / name for name, _, _, _ in entries() if name.startswith(prefix + '/'))


def activate() -> Path:
    """Refuse mixed current/original modules instead of silently changing loaded state."""
    root = source_root()
    engine = root / 'engine'
    package = engine / 'numera'
    for name, module in tuple(sys.modules.items()):
        if name == 'numera' or name.startswith('numera.'):
            filename = getattr(module, '__file__', None)
            if filename is None or not Path(filename).resolve().is_relative_to(package):
                raise RuntimeError('Original source observer already imported a nonbaseline Numera module')
    if not sys.path or sys.path[0] != str(engine):
        sys.path.insert(0, str(engine))
    sys.dont_write_bytecode = True
    importlib.import_module('numera')
    return root
