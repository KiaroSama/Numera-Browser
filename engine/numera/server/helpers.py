"""The lifecycle of a finite browser-backed helper, from spawn to proved end.

These are the probes - cookies export, proxy test, trust, tls, dns, align. Each
starts a browser, so each owns a process tree, and the question this module
answers is not "did the command return" but **"is that tree provably empty"**.

`0.32.0` bounded the wait, closed the pipes and registered the helper. It still
took the wrong thing as an ending:

* the `finally` released the record whatever happened, so a tree that refused
  to die was forgotten by the only thing tracking it;
* a helper that exited normally was accepted without asking whether the browser
  it started had exited too - a plain `communicate()` return says nothing about
  a grandchild;
* `end_helpers` returned its stragglers and the shutdown hook ignored the list,
  so "cleanup failed" and "cleanup succeeded" reached the server identically.

The rule here: a record leaves `_LIVE` only when `proctree.prove_empty` says
the tree is gone. Anything else stays, with what is left recorded on it, and is
reported rather than dropped. A cleanup that could not be proved is not
completed work.
"""
from __future__ import annotations

import subprocess
import threading
import time
from contextlib import contextmanager
from dataclasses import dataclass, field
from typing import Optional

from .. import proctree
from ..handshake import ChildTerminationFailed


class HelperTimeout(RuntimeError):
    """A finite browser-backed helper outran its deadline."""


class HelperShuttingDown(RuntimeError):
    """The server is stopping, so no new helper may be started.

    Without this a request arriving during shutdown could spawn a browser
    *after* the snapshot `end_helpers` took, which is precisely the process
    nothing would then be tracking.
    """


@dataclass
class Helper:
    """One helper and everything known about how it ended."""
    proc: subprocess.Popen
    tree: proctree.Tree
    cli: tuple
    started: float = field(default_factory=time.monotonic)
    #: Set when termination was attempted and the tree could not be proved
    #: empty. Its presence is what keeps the record alive.
    straggler: Optional[str] = None

    @property
    def pid(self) -> Optional[int]:
        return getattr(self.proc, "pid", None)

    def describe(self) -> str:
        return f"pid {self.pid} ({self.tree.kind}:{self.tree.group})"


_LIVE: "dict[int, Helper]" = {}
_LOCK = threading.Lock()
_CLOSING = False


def live_helpers() -> list:
    """Every helper this process has not yet proved finished."""
    with _LOCK:
        return list(_LIVE.values())


def stragglers() -> list:
    """The helpers whose trees could not be proved empty. Never auto-cleared."""
    return [h for h in live_helpers() if h.straggler]


def _register(helper: Helper) -> None:
    with _LOCK:
        if _CLOSING:
            raise HelperShuttingDown(
                "the server is shutting down; this helper was not started")
        _LIVE[id(helper)] = helper


def _release(helper: Helper) -> None:
    with _LOCK:
        _LIVE.pop(id(helper), None)


def close_pipes(proc) -> None:
    """Close what `communicate` would have closed had it returned.

    Bounded and direct. A second `communicate` to drain them would block again
    on the same child that just failed to finish, and leaving them to the
    garbage collector is not a lifecycle.
    """
    for pipe in (proc.stdin, proc.stdout, proc.stderr):
        if pipe is None:
            continue
        try:
            if not pipe.closed:
                pipe.close()
        except OSError:                                  # pragma: no cover
            pass


def settle(helper: Helper, *, deadline: float = 5.0) -> bool:
    """End this helper's tree and say whether it is *provably* gone.

    Called on every exit, including a clean one: a helper returning 0 tells us
    the command finished, not that the browser three processes below it did.
    `prove_empty` is the only thing that answers the second question, and it is
    the same primitive a session stop uses.
    """
    reason = None
    try:
        proctree.end_session(helper.proc, helper.tree, grace=deadline)
    except ChildTerminationFailed as stuck:
        reason = str(stuck)
    except Exception as e:                               # noqa: BLE001
        reason = f"{type(e).__name__}: {e}"              # pragma: no cover
    close_pipes(helper.proc)
    try:
        empty = proctree.prove_empty(helper.tree, deadline=deadline)
    except Exception as e:                               # noqa: BLE001
        empty, reason = False, reason or f"{type(e).__name__}: {e}"
    if empty:
        helper.straggler = None
        _release(helper)
        return True
    helper.straggler = reason or "the process tree was still not empty"
    return False


def end_helpers(*, deadline: float = 5.0) -> list:
    """End every live helper. Returns the ones that could not be proved gone.

    Registered as an application shutdown hook beside the one that stops
    sessions. It closes the door first, so a request arriving mid-shutdown
    cannot start a browser behind the snapshot.

    Stragglers stay in `_LIVE`. Dropping them is what made a failed cleanup
    indistinguishable from a successful one.
    """
    left = []
    with closed_for_shutdown():
        for helper in live_helpers():
            if not settle(helper, deadline=deadline):
                left.append(helper)
    return left


@contextmanager
def closed_for_shutdown():
    """Refuse new helpers for the duration of a shutdown sweep.

    The window that needs protecting is exactly the sweep: between taking the
    snapshot of live helpers and finishing with it, a request must not be able
    to start a browser behind it. Outside that window there is no snapshot to
    escape.

    Scoped rather than latched, because the flag is process-global and a
    process builds more than one app - every test that constructs one does, and
    so does a server restarting in place. A permanent latch meant the first
    shutdown anywhere refused every probe afterwards, in a server that was still
    running. The `finally` also covers a sweep that raises.
    """
    global _CLOSING
    with _LOCK:
        _CLOSING = True
    try:
        yield
    finally:
        with _LOCK:
            _CLOSING = False


def reopen() -> None:
    """Allow helpers again. For a caller that closed the door explicitly."""
    global _CLOSING
    with _LOCK:
        _CLOSING = False


def run_helper(cli, timeout: float, env=None):
    """Run a finite helper command, bounded, and prove its tree ended.

    Returns a `CompletedProcess`, including for a non-zero exit: a failing
    proxy test is a result its caller reads, not a broken command.
    """
    proc, tree = proctree.spawn(
        cli, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        stdin=subprocess.DEVNULL, text=True, env=env,
    )
    helper = Helper(proc=proc, tree=tree, cli=tuple(cli))
    try:
        _register(helper)
    except HelperShuttingDown:
        settle(helper)
        raise

    timed_out = False
    try:
        try:
            out, err = proc.communicate(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            out = err = ""
        return subprocess.CompletedProcess(cli, proc.returncode, out or "", err or "")
    finally:
        # Every exit, not just the timeout: a clean return proves the command
        # ended and says nothing about the browser it started.
        proved = settle(helper)
        if timed_out:
            raise HelperTimeout(
                f"a browser helper did not finish within {timeout:.0f}s and was "
                f"stopped. Raise NUMERA_HELPER_TIMEOUT if this machine needs "
                f"longer, or check whether the profile's proxy is answering."
                if proved else
                f"a browser helper did not finish within {timeout:.0f}s and "
                f"could not be fully stopped: {helper.straggler}. Its browser "
                f"may still be running; it is listed as a straggler."
            ) from None
