"""Cancellable output readers for finite helpers.

EOF proves complete output. Cancellation only proves that our reader stopped;
these facts are deliberately separate. Never close a buffered pipe underneath
an active blocking read, and never leave a non-daemon reader waiting for a
writer whose ownership could not be established.
"""
from __future__ import annotations

import os
import threading
import time

CHUNK = 65536
ENCODING, ERRORS = "utf-8", "replace"
_POLL = 0.01


def _nonblocking_read(pipe):
    """Return a bounded read: bytes, b'' for EOF, or None for no data yet.

    Python 3.10/3.11 do not expose Windows pipe set_blocking. Use the same
    Win32 pipe mode directly there, with typed handles and error codes. This
    is bounded polling, not overlapped I/O. Unsupported handles fail at setup,
    before a reader thread starts; they never fall back to a blocking read.
    """
    fd = pipe.fileno()
    if os.name != "nt":
        os.set_blocking(fd, False)

        def read():
            try:
                return os.read(fd, CHUNK)
            except BlockingIOError:
                return None
        return read

    import ctypes
    from ctypes import wintypes as wt
    import msvcrt

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.SetNamedPipeHandleState.argtypes = [wt.HANDLE,
        ctypes.POINTER(wt.DWORD), ctypes.POINTER(wt.DWORD), ctypes.POINTER(wt.DWORD)]
    kernel.SetNamedPipeHandleState.restype = wt.BOOL
    kernel.ReadFile.argtypes = [wt.HANDLE, ctypes.c_void_p, wt.DWORD,
                               ctypes.POINTER(wt.DWORD), ctypes.c_void_p]
    kernel.ReadFile.restype = wt.BOOL
    handle = wt.HANDLE(msvcrt.get_osfhandle(fd))
    mode = wt.DWORD(1)  # PIPE_NOWAIT | PIPE_READMODE_BYTE
    if not kernel.SetNamedPipeHandleState(handle, ctypes.byref(mode), None, None):
        raise ctypes.WinError(ctypes.get_last_error())
    buffer = ctypes.create_string_buffer(CHUNK)

    def read():
        size = wt.DWORD()
        if kernel.ReadFile(handle, buffer, CHUNK, ctypes.byref(size), None):
            # A zero-byte write is not EOF while a writer remains connected.
            return buffer.raw[:size.value] if size.value else None
        error = ctypes.get_last_error()
        if error == 232:  # ERROR_NO_DATA: connected, currently empty
            return None
        if error in (109, 233):  # BROKEN_PIPE / PIPE_NOT_CONNECTED
            return b""
        raise ctypes.WinError(error)
    return read


class _Reader:
    """One non-daemon pump; cancellation and EOF are different outcomes."""

    def __init__(self, name: str, pipe) -> None:
        self.name, self.pipe = name, pipe
        self.chunks: list = []
        self.ended = threading.Event()
        self.cancel = threading.Event()
        self.error: BaseException | None = None
        self._read = _nonblocking_read(pipe)
        self.thread = threading.Thread(target=self._pump,
                                       name=f"helper-{name}", daemon=False)
        self.started = False

    def _pump(self) -> None:
        try:
            while not self.cancel.is_set():
                chunk = self._read()
                if chunk is None:
                    self.cancel.wait(_POLL)
                elif chunk == b"":
                    self.ended.set()
                    return
                else:
                    self.chunks.append(chunk)
        except BaseException as error:  # reported by the owning helper
            self.error = error

    def text(self) -> str:
        raw = b"".join(tuple(self.chunks)).decode(ENCODING, ERRORS)
        return raw.replace("\r\n", "\n").replace("\r", "\n")


class Streams:
    """Prepare first, attach to an owner, then start; direct callers may autostart."""

    def __init__(self, proc, *, start: bool = True) -> None:
        self.readers: list[_Reader] = []
        self._closed = False
        try:
            for name, pipe in (("stdout", proc.stdout), ("stderr", proc.stderr)):
                if pipe is not None:
                    self.readers.append(_Reader(name, pipe))
            if start:
                self.start()
        except BaseException:
            # No buffered reads run during preparation. On partial start,
            # cancellation wakes every started pump independently of writers.
            self.close(1.0)
            raise

    def start(self) -> None:
        if self._closed:
            raise RuntimeError("cannot start closed helper streams")
        try:
            for reader in self.readers:
                if not reader.started:
                    reader.thread.start()
                    reader.started = True
        except BaseException:
            # The managed caller owns this Streams instance already. It will
            # join under its cleanup deadline; direct construction unwinds in
            # __init__ instead of losing the first started thread.
            for reader in self.readers:
                reader.cancel.set()
            raise

    def collect(self, budget: float) -> tuple:
        end = time.monotonic() + max(0.0, budget)
        for reader in self.readers:
            if reader.started or reader.thread.ident is not None:
                reader.thread.join(max(0.0, end - time.monotonic()))
        return self._of("stdout"), self._of("stderr"), self.complete

    @property
    def complete(self) -> bool:
        return all(r.ended.is_set() and r.error is None for r in self.readers)

    def _of(self, name: str) -> str:
        return next((r.text() for r in self.readers if r.name == name), "")

    def unfinished(self) -> list:
        return [r.name for r in self.readers if not r.ended.is_set() or r.error]

    def close(self, budget: float = 2.0) -> list:
        """Cancel, join within one budget, then close only quiescent readers."""
        self._closed = True
        end = time.monotonic() + max(0.0, budget)
        for reader in self.readers:
            reader.cancel.set()
        for reader in self.readers:
            if reader.started or reader.thread.ident is not None:
                reader.thread.join(max(0.0, end - time.monotonic()))
        for reader in self.readers:
            if reader.thread.is_alive():
                continue
            try:
                if not reader.pipe.closed:
                    reader.pipe.close()
            except OSError as error:
                reader.error = reader.error or error
        return [r.name for r in self.readers if r.thread.is_alive() or not r.pipe.closed]

    def retained(self) -> list:
        out = []
        for reader in self.readers:
            if not reader.thread.is_alive() and reader.pipe.closed:
                continue
            try:
                fd = reader.pipe.fileno()
            except (OSError, ValueError):
                fd = "?"
            out.append(f"{reader.name}(fd {fd})")
        return out
