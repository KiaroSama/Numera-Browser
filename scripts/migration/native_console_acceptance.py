"""Compare cooked Windows console input through an owned, invisible ConPTY."""
from __future__ import annotations
import argparse
import ctypes
from ctypes import wintypes as wt
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
from baseline_source import source_root
ORIGINAL = source_root()
sys.path.insert(0, str(ROOT / 'engine'))
from numera import proctree
from numera.winjob import _job_open, _kernel32
from run_logging import run_log
from native_terminal_cases import followups


class Coord(ctypes.Structure):
    _fields_ = [('X', wt.SHORT), ('Y', wt.SHORT)]


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
    k = _kernel32()
    definitions = [
        ('CreatePipe', wt.BOOL, [ctypes.POINTER(wt.HANDLE), ctypes.POINTER(wt.HANDLE), ctypes.c_void_p, wt.DWORD]),
        ('CreatePseudoConsole', ctypes.c_long, [Coord, wt.HANDLE, wt.HANDLE, wt.DWORD, ctypes.POINTER(wt.HANDLE)]),
        ('ClosePseudoConsole', None, [wt.HANDLE]),
        ('InitializeProcThreadAttributeList', wt.BOOL, [ctypes.c_void_p, wt.DWORD, wt.DWORD, ctypes.POINTER(ctypes.c_size_t)]),
        ('UpdateProcThreadAttribute', wt.BOOL, [ctypes.c_void_p, wt.DWORD, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p]),
        ('DeleteProcThreadAttributeList', None, [ctypes.c_void_p]),
        ('CreateProcessW', wt.BOOL, [wt.LPCWSTR, wt.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, wt.BOOL, wt.DWORD, ctypes.c_void_p, wt.LPCWSTR, ctypes.POINTER(Startup), ctypes.POINTER(ProcessInfo)]),
        ('ResumeThread', wt.DWORD, [wt.HANDLE]),
        ('GetExitCodeProcess', wt.BOOL, [wt.HANDLE, ctypes.POINTER(wt.DWORD)]),
        ('PeekNamedPipe', wt.BOOL, [wt.HANDLE, ctypes.c_void_p, wt.DWORD, ctypes.c_void_p, ctypes.POINTER(wt.DWORD), ctypes.c_void_p]),
        ('ReadFile', wt.BOOL, [wt.HANDLE, ctypes.c_void_p, wt.DWORD, ctypes.POINTER(wt.DWORD), ctypes.c_void_p]),
        ('WriteFile', wt.BOOL, [wt.HANDLE, ctypes.c_void_p, wt.DWORD, ctypes.POINTER(wt.DWORD), ctypes.c_void_p]),
    ]
    for name, result, arguments in definitions:
        fn = getattr(k, name)
        fn.restype, fn.argtypes = result, arguments
    return k


def execute(argv, env, feed, prompt, *, wall=15, idle=5):
    k = api()
    handles = []
    console = None
    attributes = None
    initialized = False
    info = ProcessInfo()
    job = None
    tree = None
    try:
        ir, iw, ore, ow = wt.HANDLE(), wt.HANDLE(), wt.HANDLE(), wt.HANDLE()
        for reader, writer in ((ir, iw), (ore, ow)):
            if not k.CreatePipe(ctypes.byref(reader), ctypes.byref(writer), None, 0):
                raise ctypes.WinError(ctypes.get_last_error())
            handles.extend([reader.value, writer.value])
        console = wt.HANDLE()
        status = k.CreatePseudoConsole(Coord(120, 30), ir, ow, 0, ctypes.byref(console))
        if status < 0:
            raise RuntimeError(f'Hidden console creation failed HRESULT={status}')
        size = ctypes.c_size_t()
        k.InitializeProcThreadAttributeList(None, 1, 0, ctypes.byref(size))
        if not size.value or size.value > 65536:
            raise RuntimeError('Console attribute list size is invalid')
        attributes = ctypes.create_string_buffer(size.value)
        if not k.InitializeProcThreadAttributeList(attributes, 1, 0, ctypes.byref(size)):
            raise ctypes.WinError(ctypes.get_last_error())
        initialized = True
        if not k.UpdateProcThreadAttribute(attributes, 0, 0x20016, console, ctypes.sizeof(console), None, None):
            raise ctypes.WinError(ctypes.get_last_error())
        startup = StartupEx()
        startup.startup.cb = ctypes.sizeof(StartupEx)
        startup.startup.flags = 0x100
        startup.attributes = ctypes.cast(attributes, ctypes.c_void_p)
        command = ctypes.create_unicode_buffer(subprocess.list2cmdline(argv))
        environment = ctypes.create_unicode_buffer('\0'.join(f'{key}={value}' for key, value in sorted(env.items(), key=lambda v: v[0].upper())) + '\0\0')
        name = 'numera-console-fixture-' + uuid.uuid4().hex
        job = _job_open(name)
        if not job:
            raise RuntimeError('Hidden console job creation failed')
        # Suspended creation makes assignment precede all source/native effects.
        if not k.CreateProcessW(None, command, None, None, False, 0x80000 | 0x400 | 0x4,
                                environment, str(ROOT), ctypes.byref(startup.startup), ctypes.byref(info)):
            raise ctypes.WinError(ctypes.get_last_error())
        tree = proctree.Tree(pid=info.pid, token=proctree.started_at(info.pid), group=name, kind='job')
        if not k.AssignProcessToJobObject(job, info.process):
            raise ctypes.WinError(ctypes.get_last_error())
        if k.ResumeThread(info.thread) == 0xffffffff:
            raise ctypes.WinError(ctypes.get_last_error())
        for handle in (ir.value, ow.value):
            k.CloseHandle(handle)
            handles.remove(handle)
        output = bytearray()
        supplied = False
        stages = feed if isinstance(feed, list) else [(prompt, feed)]
        stage = 0
        supplied = not stages
        end = time.monotonic() + wall
        progress = time.monotonic()
        exit_code = None
        exited_at = None
        buffer = ctypes.create_string_buffer(8192)
        while True:
            available = wt.DWORD()
            if k.PeekNamedPipe(ore, None, 0, None, ctypes.byref(available), None) and available.value:
                read = wt.DWORD()
                if not k.ReadFile(ore, buffer, min(available.value, len(buffer)), ctypes.byref(read), None):
                    raise ctypes.WinError(ctypes.get_last_error())
                output.extend(buffer.raw[:read.value])
                progress = time.monotonic()
                if len(output) > 65536:
                    raise RuntimeError('Hidden console output exceeds bound')
            visible = re.sub(rb'\x1b\][^\x07]*(?:\x07|\x1b\\)|\x1b\[[0-?]*[ -/]*[@-~]', b'', output)
            if stage < len(stages) and stages[stage][0].rstrip().encode('utf-8') in visible:
                written = wt.DWORD()
                data = stages[stage][1].encode('utf-8')
                if not k.WriteFile(iw, data, len(data), ctypes.byref(written), None) or written.value != len(data):
                    raise RuntimeError('Hidden console input was not written completely')
                stage += 1
                supplied = stage == len(stages)
            if k.WaitForSingleObject(info.process, 0) == 0:
                code = wt.DWORD()
                if not k.GetExitCodeProcess(info.process, ctypes.byref(code)):
                    raise ctypes.WinError(ctypes.get_last_error())
                exit_code = code.value
                if exited_at is None:
                    exited_at = time.monotonic()
                # Conhost publishes the last screen delta after the client exit.
                if "Passwords didn't match.".encode() in visible or (not available.value and time.monotonic() - max(progress, exited_at) >= 0.1):
                    break
            if time.monotonic() >= end or time.monotonic() - progress > idle:
                markers = {name: name.encode('utf-8') in output for name in ('Master password: ', 'Confirm master password: ', 'Passwords didn\'t match.', 'could not', 'Error')}
                created, exited, kernel, user = (wt.FILETIME() for _ in range(4))
                if not k.GetProcessTimes(info.process, ctypes.byref(created), ctypes.byref(exited), ctypes.byref(kernel), ctypes.byref(user)):
                    raise ctypes.WinError(ctypes.get_last_error())
                cpu = sum((value.dwHighDateTime << 32) | value.dwLowDateTime for value in (kernel, user)) / 10_000_000
                raise TimeoutError(f'Hidden console wall/idle bound exceeded stage={stage} prompts={markers} cpu_seconds={cpu:.3f}')
            time.sleep(0.01)
        assert supplied, 'Console input was never exercised'
        return exit_code, output.decode('utf-8')
    finally:
        failures = []
        try:
            if job and not k.TerminateJobObject(job, 1):
                failures.append('Hidden console job termination failed')
            if info.process and k.WaitForSingleObject(info.process, 3000) != 0:
                failures.append('Hidden console process did not settle')
            if tree and not proctree.prove_empty(tree, deadline=3):
                failures.append('Hidden console job is not empty')
        finally:
            try:
                if console and console.value:
                    k.ClosePseudoConsole(console)
            finally:
                if initialized:
                    k.DeleteProcThreadAttributeList(attributes)
                for handle in [info.thread, info.process, job, *handles]:
                    if handle and not k.CloseHandle(handle):
                        failures.append('Hidden console handle closure failed')
        if failures:
            raise RuntimeError('; '.join(failures))


def main(candidate, log):
    if os.name != 'nt':
        raise RuntimeError('This fixture requires Windows ConPTY')
    with tempfile.TemporaryDirectory(prefix='native-console-', dir=ROOT / '.ci-work/migration-temp') as temporary:
        parent = Path(temporary)
        home = parent / 'home'
        home.mkdir()
        env = os.environ.copy()
        for key in ('NUMERA_PASSWORD', 'NUMERA_API_TOKEN', 'NUMERA_DATA_DIR', 'NUMERA_NATIVE_SUPERVISE',
                    'NUMERA_NATIVE_JOB_GATE', 'NUMERA_HELPER_UNITS', 'NUMERA_HELPER_SCOPE', 'NUMERA_PARENT_PID'):
            env.pop(key, None)
        env.update(PYTHONUTF8='1', PYTHONDONTWRITEBYTECODE='1', PYTHONPATH=str(ORIGINAL / 'engine'), HOME=str(home), USERPROFILE=str(home),
                   NUMERA_APP_DIR=str(parent), NUMERA_LOG_DIR=str(parent / 'logs'),
                   NUMERA_BROWSER_ADAPTER=str(parent / 'absent.py'), NUMERA_SDK_PYTHON=sys.executable)
        for label, executable in [('native', [str(candidate)]), ('source', [sys.executable, '-m', 'numera'])]:
            root = parent / label
            # Human pool import reads cooked console input until Ctrl-Z EOF.
            code, text = execute([*executable, '--data-dir', str(root), 'pool', 'add', '-'], env,
                                 'http://127.0.0.1:8888\r\x1a\r', '')
            assert code == 0, (label, code, text)
            records = json.loads((root / 'proxies.json').read_text(encoding='utf-8'))
            assert records, (label, text)
            log.info('Cooked hidden console pool import and EOF passed runtime=%s', label)
            vault_root = parent / f'{label}-vault'
            code, text = execute([*executable, '--data-dir', str(vault_root), 'vault', 'enable'], env,
                                 [('Master password: ', 'fixture-first\r'),
                                  ('Confirm master password: ', 'fixture-second\r')], '')
            assert code == 1 and "Passwords didn't match." in text, (label, code)
            assert 'fixture-first' not in text and 'fixture-second' not in text, 'Console echoed password input'
            assert not (vault_root / 'vault-migration.json').exists(), 'Mismatch started a vault migration'
            log.info('Hidden console vault confirmation mismatch refused before writes runtime=%s', label)
            enabled_root = parent / f'{label}-enabled'
            code, text = execute([*executable, '--data-dir', str(enabled_root), 'vault', 'enable'], env,
                                 [('Master password:', 'fixture-α-valid\r'),
                                  ('Confirm master password:', 'fixture-α-valid\r')], '', wall=45, idle=30)
            assert code == 0 and 'Vault enabled' in text, (label, code)
            assert 'fixture-α-valid' not in text, 'Console echoed accepted password'
            config = json.loads((enabled_root / 'config.json').read_text(encoding='utf-8'))
            assert config.get('vault'), 'Interactive vault enable did not persist its descriptor'
            log.info('Hidden console accepted password enabled vault without echo runtime=%s', label)
            followups(parent, executable, label, env,
                      lambda argv, env, stages, **limits: execute(argv, env, stages, '', **limits), '\r', log)
    return 0


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    args = parser.parse_args()
    with run_log('native-console-acceptance') as logger:
        raise SystemExit(main(args.candidate.resolve(strict=True), logger))
