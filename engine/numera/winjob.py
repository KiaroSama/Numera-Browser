"""The Windows Job Object layer: one named kernel object per owned tree.

Split out of `proctree` because it is the only part of tree ownership written
against `kernel32` rather than against the idea of a tree: a job is created
named and inheritable, a process is assigned to it, its `ActiveProcesses` is
read to answer "is it empty", and it is terminated as a unit. `proctree`
imports these names unchanged and every caller - including the tests that
replace `_job_open`, `_job_assign` or `_job_active` - still reaches them
through `proctree`, because the code that calls them stayed there.

Nothing here imports `proctree`, so the dependency runs one way only.
"""
from __future__ import annotations

import os
import subprocess

#: Windows job objects, as much of them as this needs.
#: Windows job objects, as much of them as this needs.
_JOB_QUERY = 0x0004
_JOB_TERMINATE = 0x0008
_PROCESS_SET_QUOTA = 0x0100
_PROCESS_TERMINATE = 0x0001
_JOB_BASIC_ACCOUNTING = 1
_JOB_EXTENDED_LIMIT = 9
_JOB_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000


#: The one bound `kernel32`. Cached because `WinDLL` returns a NEW object every
#: call and ctypes caches signatures *per object*, so a fresh instance per call
#: re-binds nothing that any other call site can see.
_K32 = []


def _kernel32():
    """`kernel32` with every signature this module uses declared.

    ctypes defaults an unbound argument and return value to `c_int` - 32 bits -
    and a Windows `HANDLE` is pointer-sized. An unbound `OpenProcess` therefore
    hands back a truncated, sign-extended handle, and the `CloseHandle` or
    `WaitForSingleObject` that follows acts on whatever that value names. It
    looked like it worked because handle values are small; that is luck, not a
    guarantee, and `alive()` and `started_at()` - the two liveness checks the
    whole durable claim rests on - were the call sites reaching for the
    unbound `ctypes.windll.kernel32`.
    """
    if _K32:
        return _K32[0]
    import ctypes
    import ctypes.wintypes as wt

    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    for fn, restype, argtypes in (
        ("CreateJobObjectW", wt.HANDLE, (ctypes.c_void_p, wt.LPCWSTR)),
        ("OpenJobObjectW", wt.HANDLE, (wt.DWORD, wt.BOOL, wt.LPCWSTR)),
        ("OpenProcess", wt.HANDLE, (wt.DWORD, wt.BOOL, wt.DWORD)),
        ("CloseHandle", wt.BOOL, (wt.HANDLE,)),
        ("WaitForSingleObject", wt.DWORD, (wt.HANDLE, wt.DWORD)),
        ("GetProcessTimes", wt.BOOL, (wt.HANDLE, ctypes.c_void_p, ctypes.c_void_p,
                                      ctypes.c_void_p, ctypes.c_void_p)),
        ("AssignProcessToJobObject", wt.BOOL, (wt.HANDLE, wt.HANDLE)),
        ("TerminateJobObject", wt.BOOL, (wt.HANDLE, ctypes.c_uint)),
        ("SetInformationJobObject", wt.BOOL, (wt.HANDLE, ctypes.c_int,
                                              ctypes.c_void_p, wt.DWORD)),
        ("QueryInformationJobObject", wt.BOOL, (wt.HANDLE, ctypes.c_int,
                                                ctypes.c_void_p, wt.DWORD,
                                                ctypes.c_void_p)),
    ):
        handle = getattr(k32, fn)
        handle.restype = restype
        handle.argtypes = argtypes
    _K32.append(k32)
    return k32


def _job_structs():
    """The two job-object structures, built where ctypes is known to exist."""
    import ctypes
    import ctypes.wintypes as wt

    class SECURITY_ATTRIBUTES(ctypes.Structure):
        _fields_ = [("nLength", wt.DWORD),
                    ("lpSecurityDescriptor", ctypes.c_void_p),
                    ("bInheritHandle", wt.BOOL)]

    class BASIC_LIMITS(ctypes.Structure):
        _fields_ = [("PerProcessUserTimeLimit", ctypes.c_int64),
                    ("PerJobUserTimeLimit", ctypes.c_int64),
                    ("LimitFlags", wt.DWORD),
                    ("MinimumWorkingSetSize", ctypes.c_size_t),
                    ("MaximumWorkingSetSize", ctypes.c_size_t),
                    ("ActiveProcessLimit", wt.DWORD),
                    ("Affinity", ctypes.c_size_t),
                    ("PriorityClass", wt.DWORD),
                    ("SchedulingClass", wt.DWORD)]

    class IO_COUNTERS(ctypes.Structure):
        _fields_ = [(name, ctypes.c_uint64) for name in
                    ("ReadOperationCount", "WriteOperationCount",
                     "OtherOperationCount", "ReadTransferCount",
                     "WriteTransferCount", "OtherTransferCount")]

    class EXTENDED_LIMITS(ctypes.Structure):
        _fields_ = [("BasicLimitInformation", BASIC_LIMITS),
                    ("IoInfo", IO_COUNTERS),
                    ("ProcessMemoryLimit", ctypes.c_size_t),
                    ("JobMemoryLimit", ctypes.c_size_t),
                    ("PeakProcessMemoryUsed", ctypes.c_size_t),
                    ("PeakJobMemoryUsed", ctypes.c_size_t)]

    return SECURITY_ATTRIBUTES, EXTENDED_LIMITS


def _job_open(name: str):
    """A new named job for a child about to be spawned, or None.

    Two properties, and the session's whole guarantee on Windows rests on them.

    **The handle is inheritable**, and the child is spawned with inheritance
    on, so the child holds one from the moment it exists. A job object's *name*
    is only resolvable while some handle to it is open - closing ours the
    instant the child was assigned made the name disappear immediately, which
    is how the first version of this read "empty" for a tree with a live
    browser in it. The child's own handle is what keeps the name answerable for
    as long as the tree exists, without this process having to hold anything.

    **Kill-on-close** then closes the case the whole module is about: when the
    leader exits, its handle goes with it, and if nothing else holds one
    Windows terminates whatever is left in the job. A Chromium that outlives
    the Python process that started it is not detected and cleaned up after the
    fact - it cannot happen. A detached job has no other holder, so this is
    what makes `numera launch --detach` safe to stop from a later process.
    """
    import ctypes

    sa_type, limits_type = _job_structs()
    k32 = _kernel32()
    sa = sa_type(ctypes.sizeof(sa_type), None, True)
    job = k32.CreateJobObjectW(ctypes.byref(sa), name)
    if not job:
        return None
    limits = limits_type()
    limits.BasicLimitInformation.LimitFlags = _JOB_LIMIT_KILL_ON_JOB_CLOSE
    if not k32.SetInformationJobObject(job, _JOB_EXTENDED_LIMIT,
                                       ctypes.byref(limits),
                                       ctypes.sizeof(limits)):   # pragma: no cover
        k32.CloseHandle(job)
        return None
    return job


def _job_assign(job, proc: subprocess.Popen) -> bool:
    """Put a just-spawned process in `job`, and let go of our handle.

    The window between `CreateProcess` and the assignment is real and cannot be
    closed without a suspended start, which `subprocess` does not expose - it
    hands back no thread handle to resume. It is microseconds wide and the
    child is a fresh interpreter that has not imported anything yet, let alone
    started a browser, so nothing it owns can escape through it.
    """
    k32 = _kernel32()
    try:
        handle = k32.OpenProcess(_PROCESS_SET_QUOTA | _PROCESS_TERMINATE,
                                 False, int(proc.pid))
        if not handle:                                       # pragma: no cover
            return False
        try:
            return bool(k32.AssignProcessToJobObject(job, handle))
        finally:
            k32.CloseHandle(handle)
    finally:
        # Ours goes; the child's inherited one keeps the job named and alive.
        k32.CloseHandle(job)


def _job_active(name: str) -> tuple:
    """`(known, active)` for a named job.

    Two different facts used to arrive as the same `None`: *there is no such
    job* and *the job is there but could not be queried*. `alive_tree` read
    both as empty, so a failing query - a handle refused, a transient error -
    reported a live browser tree as gone, which is the one answer that lets a
    second browser open on the same profile directory.

    `(True, 0)` means proved empty, `(True, n)` proved occupied, and
    `(False, 0)` means unknown, which every caller must treat as still owned.
    """
    import ctypes

    k32 = _kernel32()
    job = k32.OpenJobObjectW(_JOB_QUERY, False, name)
    if not job:
        # ERROR_FILE_NOT_FOUND is the job being gone, which is real evidence.
        # Anything else - access denied, a broken handle table - is not.
        absent = ctypes.get_last_error() in (2, 3)
        return (True, 0) if absent else (False, 0)
    try:
        buf = ctypes.create_string_buffer(48)
        got = ctypes.c_ulong(0)
        if not k32.QueryInformationJobObject(job, _JOB_BASIC_ACCOUNTING,
                                             buf, 48, ctypes.byref(got)):
            return (False, 0)           # it exists; we simply cannot see in
        # JOBOBJECT_BASIC_ACCOUNTING_INFORMATION: four LARGE_INTEGERs, then
        # TotalPageFaultCount, TotalProcesses, ActiveProcesses.
        return (True, int.from_bytes(buf.raw[40:44], "little"))
    finally:
        k32.CloseHandle(job)


def _job_terminate(name: str) -> bool:
    """Kill everything in this job. True when the job is gone or was ended."""
    k32 = _kernel32()
    job = k32.OpenJobObjectW(_JOB_TERMINATE | _JOB_QUERY, False, name)
    if not job:
        return True                     # already gone
    try:
        return bool(k32.TerminateJobObject(job, 1))
    finally:
        k32.CloseHandle(job)


def _job_assign_self(job) -> bool:
    """Put this process in `job`, keeping the job handle open.

    Through a real handle from `OpenProcess`, not the `GetCurrentProcess`
    pseudo-handle: that one is `(HANDLE)-1`, and ctypes would pass it through a
    32-bit int parameter it does not have a declared type for.
    """
    k32 = _kernel32()
    handle = k32.OpenProcess(_PROCESS_SET_QUOTA | _PROCESS_TERMINATE,
                             False, os.getpid())
    if not handle:                                           # pragma: no cover
        return False
    try:
        return bool(k32.AssignProcessToJobObject(job, handle))
    finally:
        k32.CloseHandle(handle)
