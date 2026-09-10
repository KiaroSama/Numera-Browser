"""Thread-scoped driver interception with serialized spawn/claim recording.

Cloak and Camoufox prepare browsers inside their launch calls. Preparation can
download files or resolve proxies; it must not hold a process-wide spawn lock.
The shared dispatchers remain installed until the last capture scope exits,
but only a matching spawn on a registered thread enters the serialization gate.
"""

from contextlib import contextmanager
import os
import subprocess
import threading


class Adopted:
    """The driver and exact OS tree captured by one launch scope."""

    __slots__ = ("tree", "proc")

    def __init__(self):
        self.tree = self.proc = None


_SPAWNING = threading.Lock()
_SCOPES: dict[int, list] = {}
_INSTALLED: list = []


@contextmanager
def spawning_own_tree(executable, on_tree=None):
    """Capture one driver on this thread and record its tree before returning.

    An absent or unmatched executable leaves tree=None; the session gate then
    refuses the launch. A failed recording stops the driver before propagating.
    Nested scopes use their own holder and restore the enclosing registration.
    """
    holder = Adopted()
    if not executable:
        yield holder
        return
    targets = tuple(str(one) for one in
                    (executable if isinstance(executable, (list, tuple))
                     else [executable]) if one)
    thread = threading.get_ident()
    with _SPAWNING:
        if not _SCOPES:
            try:
                for module, attr in _spawn_points():
                    original = getattr(module, attr)
                    ours = _capture(original)
                    setattr(module, attr, ours)
                    _INSTALLED.append((module, attr, original, ours))
            except BaseException:
                _restore()
                raise
        _SCOPES.setdefault(thread, []).append((holder, targets, on_tree))
    try:
        yield holder
    finally:
        with _SPAWNING:
            _SCOPES[thread].pop()
            if not _SCOPES[thread]:
                del _SCOPES[thread]
            if not _SCOPES:
                _restore()


def _restore():
    for module, attr, original, ours in reversed(_INSTALLED):
        if getattr(module, attr) is ours:
            setattr(module, attr, original)
    _INSTALLED.clear()


def _spawn_points():
    from . import proctree

    points = [(subprocess, "Popen")]
    if proctree._WINDOWS:
        # asyncio's Windows Popen subclass was bound before our interception.
        import asyncio.windows_utils as windows_utils

        points.append((windows_utils, "Popen"))
    return points


def _capture(original):
    def popen(args, *rest, **kwargs):
        from . import proctree

        scopes = _SCOPES.get(threading.get_ident())
        if not scopes:
            return original(args, *rest, **kwargs)
        holder, targets, on_tree = scopes[-1]
        if holder.tree is not None or not _is_target(args, targets):
            return original(args, *rest, **kwargs)
        with _SPAWNING:
            proc, tree = proctree._spawn_leader(original, args, rest, kwargs)
            holder.proc = proc
            try:
                if on_tree is not None:
                    on_tree(tree)
            except BaseException:
                proctree._abandon(proc)
                raise
            holder.tree = tree
            return proc
    return popen


def _is_target(args, targets) -> bool:
    program = args[0] if isinstance(args, (list, tuple)) and args else args
    try:
        name = os.path.normcase(str(program))
    except Exception:                                      # pragma: no cover
        return False
    return any(name == os.path.normcase(one) for one in targets)
