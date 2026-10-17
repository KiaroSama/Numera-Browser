"""Single-owner bounded pipe frames; no input thread survives SDK shutdown."""
from __future__ import annotations

import json
import os


class FrameInput:
    def __init__(self, stream, limit):
        self.fd = stream.fileno()
        self.limit = limit
        self.pending = bytearray()
        if os.name == 'nt':
            import ctypes
            import msvcrt
            from ctypes import wintypes
            self.handle = msvcrt.get_osfhandle(self.fd)
            self.ctypes = ctypes
            self.available = wintypes.DWORD()
            self.peek = ctypes.WinDLL('kernel32', use_last_error=True).PeekNamedPipe
            self.peek.argtypes = [wintypes.HANDLE, wintypes.LPVOID, wintypes.DWORD,
                                  ctypes.POINTER(wintypes.DWORD), ctypes.POINTER(wintypes.DWORD),
                                  ctypes.POINTER(wintypes.DWORD)]
            self.peek.restype = wintypes.BOOL
            msvcrt.setmode(self.fd, os.O_BINARY)
        else:
            os.set_blocking(self.fd, False)

    def poll(self):
        if b'\n' in self.pending:
            return self._frame()
        if os.name == 'nt':
            if not self.peek(self.handle, None, 0, None, self.ctypes.byref(self.available), None):
                error = self.ctypes.get_last_error()
                if error in (109, 232):
                    return self._eof()
                raise self.ctypes.WinError(error)
            count = self.available.value
            if count == 0:
                return False, None
        else:
            import select
            if not select.select([self.fd], [], [], 0)[0]:
                return False, None
            count = 4096
        try:
            data = os.read(self.fd, min(count, self.limit + 1 - len(self.pending)))
        except BlockingIOError:
            return False, None
        if not data:
            return self._eof()
        self.pending.extend(data)
        newline = self.pending.find(b'\n')
        if newline < 0:
            if len(self.pending) >= self.limit:
                raise ValueError('Invalid frame size')
            return False, None
        return self._frame()

    def _frame(self):
        newline = self.pending.find(b'\n')
        if newline + 1 > self.limit:
            raise ValueError('Invalid frame size')
        line = bytes(self.pending[:newline + 1])
        del self.pending[:newline + 1]
        value = json.loads(line.decode('utf-8', errors='strict'))
        if not isinstance(value, dict):
            raise ValueError('Frame must be an object')
        return True, value

    def _eof(self):
        if self.pending:
            raise ValueError('Invalid frame size')
        return True, None
