"""Frozen portable processes share a lifetime lease against file replacement."""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import secrets
import sys


class InstallationBusy(RuntimeError):
    """A portable update owns the executable installation."""


def _take_shared(fd):
    if os.name != 'nt':
        import fcntl
        fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
        return
    import ctypes
    import ctypes.wintypes as wt
    import msvcrt

    class Overlapped(ctypes.Structure):
        _fields_ = [('Internal', ctypes.c_size_t), ('InternalHigh', ctypes.c_size_t),
                    ('Offset', wt.DWORD), ('OffsetHigh', wt.DWORD), ('hEvent', wt.HANDLE)]
    function = ctypes.WinDLL('kernel32', use_last_error=True).LockFileEx
    function.argtypes = [wt.HANDLE, wt.DWORD, wt.DWORD, wt.DWORD, wt.DWORD,
                         ctypes.POINTER(Overlapped)]
    function.restype = wt.BOOL
    overlap = Overlapped()
    # First-byte shared lock conflicts with existing transaction-style exclusive
    # msvcrt and the worker's Rust exclusive file lock, without unlinking a domain.
    if not function(msvcrt.get_osfhandle(fd), 1, 0, 1, 0, ctypes.byref(overlap)):
        raise ctypes.WinError(ctypes.get_last_error())


def _check_activation(directory):
    path = directory / 'activation.json'
    if not path.exists() and not path.is_symlink():
        return
    try:
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 4096:
            raise ValueError('unsafe marker')
        record = json.loads(path.read_text(encoding='utf-8'))
        nonce = os.environ.get('NUMERA_UPDATE_ACTIVATION') or ''
        digest = hashlib.sha256(nonce.encode('utf-8')).hexdigest()
        if (not isinstance(record, dict) or record.get('schema') != 1
                or not nonce or not isinstance(record.get('nonce_sha256'), str)
                or not secrets.compare_digest(record['nonce_sha256'].encode('utf-8'),
                                               digest.encode('ascii'))):
            raise ValueError('activation identity differs')
    except (OSError, ValueError) as error:
        raise InstallationBusy('portable update recovery owns this installation; normal startup is deferred') from error


@contextmanager
def runtime_lease():
    if not getattr(sys, 'frozen', False):
        yield
        return
    executable = Path(sys.executable).resolve().parent
    root = executable.parent if executable.name == 'engine' else executable
    directory = root / '.updates'
    if directory.is_symlink() or (directory.exists() and not directory.is_dir()):
        raise InstallationBusy('portable update ownership directory is unsafe')
    directory.mkdir(exist_ok=True)
    path = directory / 'runtime.lock'
    if path.is_symlink():
        raise InstallationBusy('portable runtime lease is unsafe')
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        try:
            _take_shared(fd)
        except OSError as error:
            raise InstallationBusy('portable update activation is replacing application files; retry after recovery') from error
        _check_activation(directory)
        yield
    finally:
        os.close(fd)  # Native lock lifetime is exactly the process operation.
