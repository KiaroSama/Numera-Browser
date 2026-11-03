"""Read-only lifetime accounting for the existing owned historical proof job."""
from __future__ import annotations

from contextlib import contextmanager
import ctypes
import json
import sys
import time


class Basic(ctypes.Structure):
    _fields_ = [(name, ctypes.c_int64) for name in (
        'TotalUserTime', 'TotalKernelTime', 'ThisPeriodTotalUserTime', 'ThisPeriodTotalKernelTime')]
    _fields_ += [(name, ctypes.c_uint32) for name in (
        'TotalPageFaultCount', 'TotalProcesses', 'ActiveProcesses', 'TotalTerminatedProcesses')]


class Io(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in (
        'ReadOperationCount', 'WriteOperationCount', 'OtherOperationCount',
        'ReadTransferCount', 'WriteTransferCount', 'OtherTransferCount')]


class Accounting(ctypes.Structure):
    _fields_ = [('BasicInfo', Basic), ('IoInfo', Io)]


CUMULATIVE = ('user_100ns', 'kernel_100ns', 'page_faults', 'processes', 'terminated_processes',
              'read_operations', 'write_operations', 'other_operations',
              'read_bytes', 'write_bytes', 'other_bytes')


def kernel():
    from numera.winjob import _kernel32

    api = _kernel32()
    for name, result, arguments in (
        ('GetCurrentProcess', ctypes.c_void_p, ()),
        ('IsProcessInJob', ctypes.c_int,
         (ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_int))),
    ):
        function = getattr(api, name)
        function.restype, function.argtypes = result, arguments
    return api


def inherited_tree():
    from numera.proctree import _inherited_tree

    return _inherited_tree()


def snapshot(api, job):
    contained = ctypes.c_int()
    if (not api.IsProcessInJob(api.GetCurrentProcess(), job, ctypes.byref(contained))
            or not contained.value):
        raise ValueError('Historical accounting scope is not the current owned job')
    row = Accounting()
    returned = ctypes.c_uint32()
    size = ctypes.sizeof(row)
    if not api.QueryInformationJobObject(job, 8, ctypes.byref(row), size, ctypes.byref(returned)):
        raise OSError('Historical accounting query failed')
    if returned.value != size:
        raise ValueError('Historical accounting query is incomplete')
    basic = row.BasicInfo
    values = (basic.TotalUserTime, basic.TotalKernelTime, basic.TotalPageFaultCount,
              basic.TotalProcesses, basic.TotalTerminatedProcesses,
              *(getattr(row.IoInfo, name) for name, _ in Io._fields_))
    if (any(not 0 <= value <= 2**64 - 1 for value in values)
            or basic.ActiveProcesses > basic.TotalProcesses
            or basic.TotalTerminatedProcesses > basic.TotalProcesses):
        raise ValueError('Historical accounting counters are invalid')
    return dict(zip(CUMULATIVE, values)), basic.ActiveProcesses


@contextmanager
def measure(index, logger):
    if sys.platform != 'win32':
        yield
        return
    api = job = before = None
    try:
        api = kernel()
        tree = inherited_tree()
        if tree is None or tree.kind != 'job' or not isinstance(tree.group, str) or not tree.group:
            raise ValueError('Historical accounting requires an inherited named job')
        job = api.OpenJobObjectW(4, False, tree.group)
        if not job:
            raise OSError('Historical accounting job could not be opened')
        before = snapshot(api, job)
    except (OSError, ValueError, AttributeError):
        pass
    started = time.monotonic_ns()
    try:
        yield
    finally:
        failed = sys.exc_info()[0] is not None
        elapsed = time.monotonic_ns() - started
        try:
            if before is None:
                raise ValueError('Historical accounting has no starting observation')
            after = snapshot(api, job)
            delta = {key: after[0][key] - before[0][key] for key in CUMULATIVE}
            if not 0 <= elapsed <= 2**64 - 1 or any(value < 0 for value in delta.values()):
                raise ValueError('Historical accounting counters decreased')
            # Includes the observer and nested jobs, not exclusive compiler work,
            # disk-only I/O or a peak-memory attribution.
            logger.info('Historical resources index=%s coverage=owned-proof-scope-not-compiler-attribution '
                        'wall_ns=%s active_before=%s active_after=%s counters=%s',
                        index, elapsed, before[1], after[1], json.dumps(delta, sort_keys=True))
        except (OSError, ValueError, AttributeError):
            logger.warning('Historical resources index=%s status=unavailable', index)
        finally:
            if job and not api.CloseHandle(job):
                logger.error('Historical resources index=%s status=handle-close-failed', index)
                if not failed:
                    raise RuntimeError('Historical resource query handle did not close')
