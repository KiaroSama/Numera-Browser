"""Read-only snapshots of members of the benchmark's existing Windows job."""
from __future__ import annotations

import ctypes

MAX_PROCESSES = 512


class Memory(ctypes.Structure):
    _fields_ = [('cb', ctypes.c_uint32), ('PageFaultCount', ctypes.c_uint32),
                *[(name, ctypes.c_size_t) for name in (
                    'PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage',
                    'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage', 'QuotaNonPagedPoolUsage',
                    'PagefileUsage', 'PeakPagefileUsage', 'PrivateUsage')]]


class FileTime(ctypes.Structure):
    _fields_ = [('dwLowDateTime', ctypes.c_uint32), ('dwHighDateTime', ctypes.c_uint32)]


def api():
    from numera.winjob import _kernel32

    kernel = _kernel32()
    for name, result, arguments in (
        ('IsProcessInJob', ctypes.c_int,
         (ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_int))),
        ('K32GetProcessMemoryInfo', ctypes.c_int,
         (ctypes.c_void_p, ctypes.POINTER(Memory), ctypes.c_uint32)),
    ):
        function = getattr(kernel, name)
        function.restype, function.argtypes = result, arguments
    return kernel


def members(kernel, job):
    size = 8 + MAX_PROCESSES * ctypes.sizeof(ctypes.c_size_t)
    buffer = ctypes.create_string_buffer(size)
    returned = ctypes.c_uint32()
    if not kernel.QueryInformationJobObject(job, 3, buffer, size, ctypes.byref(returned)):
        raise OSError('Owned memory diagnostic job query failed')
    if not 8 <= returned.value <= size:
        raise ValueError('Owned memory diagnostic job list is incomplete')
    assigned = int.from_bytes(buffer.raw[:4], 'little')
    count = int.from_bytes(buffer.raw[4:8], 'little')
    width = ctypes.sizeof(ctypes.c_size_t)
    if assigned != count or count > MAX_PROCESSES or returned.value < 8 + count * width:
        raise ValueError('Owned memory diagnostic job list is truncated')
    pids = [int.from_bytes(buffer.raw[8 + i * width:8 + (i + 1) * width], 'little')
            for i in range(count)]
    if len(set(pids)) != count or any(not 0 < pid <= 0xffffffff for pid in pids):
        raise ValueError('Owned memory diagnostic job PID is invalid')
    return pids


def snapshot(job):
    kernel = api()
    rows, missing = [], 0
    for pid in members(kernel, job):
        handle = kernel.OpenProcess(0x1000, False, pid)
        if not handle:
            missing += 1
            continue
        try:
            contained = ctypes.c_int()
            created = FileTime()
            others = [FileTime() for _ in range(3)]
            counters = Memory()
            counters.cb = ctypes.sizeof(counters)
            if (not kernel.IsProcessInJob(handle, job, ctypes.byref(contained)) or not contained.value
                    or not kernel.GetProcessTimes(handle, ctypes.byref(created),
                                                  *(ctypes.byref(value) for value in others))
                    or not kernel.K32GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb)):
                missing += 1
                continue
            birth = (created.dwHighDateTime << 32) | created.dwLowDateTime
            if birth == 0:
                missing += 1
                continue
            rows.append((pid, birth, {'rss_bytes': counters.WorkingSetSize,
                                     'peak_rss_bytes': counters.PeakWorkingSetSize,
                                     'private_bytes': counters.PrivateUsage,
                                     'peak_private_bytes': counters.PeakPagefileUsage}))
        finally:
            if not kernel.CloseHandle(handle):
                raise RuntimeError('Owned memory diagnostic process handle did not close')
    return rows, missing
