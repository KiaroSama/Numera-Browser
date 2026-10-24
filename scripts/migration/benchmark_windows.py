"""Windows benchmark birth-time job assignment, pipes and retained kernel handles."""
from __future__ import annotations

import ctypes
from ctypes import wintypes as wt
import json
import os
import subprocess
import uuid


class Startup(ctypes.Structure):
    _fields_ = [('cb', wt.DWORD), ('reserved', wt.LPWSTR), ('desktop', wt.LPWSTR), ('title', wt.LPWSTR),
                ('x', wt.DWORD), ('y', wt.DWORD), ('xsize', wt.DWORD), ('ysize', wt.DWORD),
                ('xchars', wt.DWORD), ('ychars', wt.DWORD), ('fill', wt.DWORD), ('flags', wt.DWORD),
                ('show', wt.WORD), ('reserved_count', wt.WORD), ('reserved_bytes', ctypes.c_void_p),
                ('stdin', wt.HANDLE), ('stdout', wt.HANDLE), ('stderr', wt.HANDLE)]


class StartupEx(ctypes.Structure):
    _fields_ = [('startup', Startup), ('attributes', ctypes.c_void_p)]


class ProcessInfo(ctypes.Structure):
    _fields_ = [('process', wt.HANDLE), ('thread', wt.HANDLE), ('pid', wt.DWORD), ('tid', wt.DWORD)]


def api():
    from numera.winjob import _kernel32
    kernel = _kernel32()
    for name, result, arguments in (
        ('InitializeProcThreadAttributeList', wt.BOOL, [ctypes.c_void_p, wt.DWORD, wt.DWORD, ctypes.POINTER(ctypes.c_size_t)]),
        ('UpdateProcThreadAttribute', wt.BOOL, [ctypes.c_void_p, wt.DWORD, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p]),
        ('DeleteProcThreadAttributeList', None, [ctypes.c_void_p]),
        ('CreateProcessW', wt.BOOL, [wt.LPCWSTR, wt.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, wt.BOOL, wt.DWORD, ctypes.c_void_p, wt.LPCWSTR, ctypes.POINTER(Startup), ctypes.POINTER(ProcessInfo)]),
        ('GetExitCodeProcess', wt.BOOL, [wt.HANDLE, ctypes.POINTER(wt.DWORD)]),
        ('TerminateProcess', wt.BOOL, [wt.HANDLE, wt.DWORD]),
    ):
        function = getattr(kernel, name)
        function.restype, function.argtypes = result, arguments
    return kernel


class Process:
    def __init__(self, kernel, info, stdout, stderr):
        self.kernel, self.handle, self.pid = kernel, info.process, info.pid
        self.stdout, self.stderr, self.returncode = stdout, stderr, None

    def poll(self):
        if self.returncode is None:
            result = self.kernel.WaitForSingleObject(self.handle, 0)
            if result == 0:
                code = wt.DWORD()
                if not self.kernel.GetExitCodeProcess(self.handle, ctypes.byref(code)):
                    raise ctypes.WinError(ctypes.get_last_error())
                self.returncode = code.value
            elif result != 258:
                raise ctypes.WinError(ctypes.get_last_error())
        return self.returncode

    def wait(self, timeout):
        result = self.kernel.WaitForSingleObject(self.handle, int(timeout * 1000))
        if result == 258:
            raise subprocess.TimeoutExpired('benchmark child', timeout)
        if result != 0:
            raise ctypes.WinError(ctypes.get_last_error())
        return self.poll()

    def close(self):
        if self.handle:
            self.kernel.CloseHandle(self.handle)
            self.handle = None


def spawn(argv, environment):
    import msvcrt
    from numera import proctree
    from numera.winjob import _job_open

    kernel = api()
    name = 'numera-benchmark-' + uuid.uuid4().hex
    job = _job_open(name)
    if not job:
        raise OSError('Cannot establish benchmark job')
    descriptors, streams = [], []
    attributes = None
    initialized = False
    info = ProcessInfo()
    transferred = False
    try:
        writers = []
        for _ in range(2):
            reader, writer = os.pipe()
            descriptors.extend((reader, writer))
            os.set_inheritable(writer, True)
            streams.append(os.fdopen(reader, 'rb', buffering=0))
            descriptors.remove(reader)
            writers.append(msvcrt.get_osfhandle(writer))
        null = os.open(os.devnull, os.O_RDONLY)
        descriptors.append(null)
        os.set_inheritable(null, True)
        inherited = (wt.HANDLE * 3)(msvcrt.get_osfhandle(null), *writers)
        jobs = (wt.HANDLE * 1)(job)
        size = ctypes.c_size_t()
        kernel.InitializeProcThreadAttributeList(None, 2, 0, ctypes.byref(size))
        if not 0 < size.value <= 65536:
            raise OSError('Benchmark process attribute size is invalid')
        attributes = ctypes.create_string_buffer(size.value)
        if not kernel.InitializeProcThreadAttributeList(attributes, 2, 0, ctypes.byref(size)):
            raise ctypes.WinError(ctypes.get_last_error())
        initialized = True
        # JOB_LIST assigns before any target instruction; HANDLE_LIST limits inheritance.
        for attribute, values in ((0x2000D, jobs), (0x20002, inherited)):
            if not kernel.UpdateProcThreadAttribute(attributes, 0, attribute, values,
                                                    ctypes.sizeof(values), None, None):
                raise ctypes.WinError(ctypes.get_last_error())
        startup = StartupEx()
        startup.startup.cb = ctypes.sizeof(startup)
        startup.startup.flags = 0x100
        startup.startup.stdin, startup.startup.stdout, startup.startup.stderr = inherited
        startup.attributes = ctypes.cast(attributes, ctypes.c_void_p)
        env = {**environment, proctree.TREE_ENV: json.dumps({'kind': 'job', 'group': name})}
        block = ctypes.create_unicode_buffer('\0'.join(f'{key}={value}' for key, value in
                                                       sorted(env.items(), key=lambda item: item[0].upper())) + '\0\0')
        command = ctypes.create_unicode_buffer(subprocess.list2cmdline(argv))
        if not kernel.CreateProcessW(None, command, None, None, True, 0x80000 | 0x400 | 0x08000000,
                                     block, None, ctypes.byref(startup.startup), ctypes.byref(info)):
            raise ctypes.WinError(ctypes.get_last_error())
        process = Process(kernel, info, *streams)
        tree = proctree.Tree(info.pid, None, name, 'job')
        transferred = True
        return process, tree, job
    finally:
        if initialized:
            kernel.DeleteProcThreadAttributeList(attributes)
        for descriptor in descriptors:
            os.close(descriptor)
        if info.thread:
            kernel.CloseHandle(info.thread)
        if not transferred:
            try:
                if info.process:
                    kernel.TerminateJobObject(job, 1)
                    if kernel.WaitForSingleObject(info.process, 3000) != 0:
                        raise RuntimeError('Untransferred benchmark child did not settle')
            finally:
                for stream in streams:
                    stream.close()
                if info.process:
                    kernel.CloseHandle(info.process)
                kernel.CloseHandle(job)
