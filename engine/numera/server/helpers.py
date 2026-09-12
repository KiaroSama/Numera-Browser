"""The lifecycle of a finite browser-backed helper, from admission to proved end.

These are the probes - cookies export, proxy test, trust, tls, dns, align. Each
starts a browser, so each owns one or more process trees, and the question this
module answers is not "did the command return" but **"is everything it owned
provably gone"**.

`0.32.0` bounded the wait, closed the pipes and registered the helper, and
`settle` refused to release a tree it could not prove empty. Four things were
still wrong, and they are connected:

* only the *timeout* path raised. A helper that exited 0 while its cleanup
  failed handed back a `CompletedProcess`, so failed cleanup and successful
  work reached the caller identically;
* the spawn happened *before* the shutdown check, so a rejected admission could
  already have created a child - and a child that was never registered is
  invisible to every diagnostic;
* only the helper's own outer tree was tracked. A driver started inside the
  helper gets its **own** session (`proctree._spawn_leader`), which on POSIX
  leaves the helper's process group entirely, so an empty outer tree proved
  nothing about the browser;
* `_LIVE` and `_CLOSING` were module globals. One process builds more than one
  app - every test that constructs one does, and so does a server restarting in
  place - so one app's shutdown sweep reached another app's helpers, and the
  flag that closed admission had to be un-closed again afterwards.

So the state belongs to an owner, and the owner is the app. `HelperManager`
holds it, `register()` gives each app its own, and a manager that has swept is
closed for the rest of its life - a restart gets a new one rather than
reopening a shared flag. Callers with no app (the CLI, a test) use the
process-level manager `reopen()` replaces.

Each helper moves through one of two endings:

    starting -> running -> stopping -> settled     (every unit proved empty)
                                    -> stranded    (something is still there)

and a stranded record is never dropped. A cleanup that could not be proved is
not completed work.
"""
from __future__ import annotations

import json
import os
import subprocess
import tempfile
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

from .. import proctree
from ..handshake import ChildTerminationFailed

#: The five states a helper record can be in. `starting` exists because
#: admission is reserved before the spawn: a child that was created and then
#: refused has a record to be found on.
STARTING, RUNNING, STOPPING, SETTLED, STRANDED = (
    "starting", "running", "stopping", "settled", "stranded")


class HelperFailed(RuntimeError):
    """A helper did not complete as owned, bounded work."""


class HelperTimeout(HelperFailed):
    """A finite browser-backed helper outran its deadline."""


class HelperUnsettled(HelperFailed):
    """The command returned, and something it owned is still running.

    Its own class because the exit code is not the story: a probe can answer 0
    and leave a browser holding the profile directory, and reporting that as a
    successful result is how the next launch finds the profile locked by a
    process nobody is tracking.
    """


class HelperShuttingDown(HelperFailed):
    """This app is stopping, so no new helper may be started.

    Raised *before* anything is spawned. The previous order - spawn, then check
    - could create a browser and then refuse the request, leaving a process
    that was never registered anywhere.
    """


@dataclass
class Helper:
    """One helper, everything it owns, and how it ended."""
    cli: tuple
    proc: Optional[subprocess.Popen] = None
    #: The tree the helper itself leads.
    tree: Optional[proctree.Tree] = None
    #: Separately sessioned trees the helper reported starting - a browser
    #: driver is one. Read from its units file when it is being settled.
    units: list = field(default_factory=list)
    units_path: Optional[Path] = None
    state: str = STARTING
    started: float = field(default_factory=time.monotonic)
    #: Set when termination was attempted and something could not be proved
    #: gone. Its presence is what keeps the record alive.
    straggler: Optional[str] = None

    @property
    def pid(self) -> Optional[int]:
        return getattr(self.proc, "pid", None)

    def describe(self) -> str:
        where = (f"{self.tree.kind}:{self.tree.group}" if self.tree else "no tree")
        extra = f" +{len(self.units)} session(s)" if self.units else ""
        return f"pid {self.pid} ({where}){extra} [{self.state}]"

    def owned_trees(self) -> list:
        """Every tree that must be empty before this helper is finished."""
        return ([self.tree] if self.tree else []) + list(self.units)


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


#: How long the readers *together* may go on after the helper process has
#: exited. The wait that matters is the process's; this one only collects what
#: it already wrote, which is in the pipe buffer and arrives at once. It is a
#: shared deadline rather than one per reader: two pipes held open by the same
#: grandchild would otherwise cost this twice for nothing.
DRAIN_GRACE = 1.0


def _drain(pipe, into: list) -> None:
    """Collect a pipe line by line until it ends or is closed under us."""
    try:
        for line in pipe:
            into.append(line)
    except (OSError, ValueError):        # closed by `close_pipes` mid-read
        pass


def _read_bounded(proc, timeout: float) -> tuple:
    """`(stdout, stderr, timed_out)` - bounded by the *process*, not the pipes.

    `communicate()` waits for EOF, and EOF is not the helper exiting: on
    Windows `_spawn_leader` starts a browser driver with `close_fds=False` so
    that it inherits its own job handle, and that hands it the helper's stdout
    writer as well. The helper then prints, exits, and `communicate` goes on
    waiting on a pipe the *browser* is holding open - so a finite probe's
    deadline was really the browser's lifetime.

    Waiting on the process and draining on threads separates the two. Whatever
    the readers have collected when the process ends is the output; a writer
    somebody else still holds delays nothing, and `close_pipes` ends the reads.
    """
    buffers = {"out": [], "err": []}
    readers = [threading.Thread(target=_drain, args=(pipe, buffers[key]),
                                daemon=True)
               for key, pipe in (("out", proc.stdout), ("err", proc.stderr))
               if pipe is not None]
    for reader in readers:
        reader.start()
    timed_out = False
    try:
        proc.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
    deadline = time.monotonic() + DRAIN_GRACE
    for reader in readers:
        reader.join(max(0.0, deadline - time.monotonic()))
    return "".join(buffers["out"]), "".join(buffers["err"]), timed_out


def read_units(path) -> list:
    """The ownership units a helper recorded for itself, as trees.

    One JSON record per line, written by `proctree._spawn_leader` in the child
    whenever it makes a separately sessioned leader. A *record*, not an
    inference: walking descendants cannot find a process that left the group,
    and a bare pid is not safe to signal after it has been recycled - the
    record carries the job name or the start-time token that makes it
    identifiable.
    """
    if not path:
        return []
    try:
        text = Path(path).read_text(encoding="utf-8")
    except OSError:
        return []
    found = []
    for line in text.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            record = json.loads(line)
        except ValueError:
            continue                     # a half-written final line
        tree = proctree.Tree.from_record(record)
        if tree.pid:
            found.append(tree)
    return found


class HelperManager:
    """Every helper one app started, and what is known about each one.

    Owned by the app rather than the module. Two apps in one process - two
    tests, or a server restarting in place - get two of these, so neither
    sweeps the other's helpers and neither can be locked out by the other's
    shutdown.
    """

    def __init__(self) -> None:
        self._live: "dict[int, Helper]" = {}
        self._lock = threading.Lock()
        self._closed = False

    # ---- the registry ----------------------------------------------------
    def live(self) -> list:
        """Every helper this manager has not yet proved finished."""
        with self._lock:
            return list(self._live.values())

    def stragglers(self) -> list:
        """The helpers something survived. Never auto-cleared."""
        return [h for h in self.live() if h.straggler]

    @property
    def closed(self) -> bool:
        return self._closed

    def admit(self, cli, env=None) -> Helper:
        """Reserve a record for a helper **before** it is spawned.

        The order is the point. Checking shutdown after the spawn meant a
        refused request could still have started a browser, and the record that
        would have named it was never created.
        """
        helper = Helper(cli=tuple(cli))
        with self._lock:
            if self._closed:
                raise HelperShuttingDown(
                    "this server is shutting down; the helper was not started")
            self._live[id(helper)] = helper
        handle, path = tempfile.mkstemp(prefix="numera-units-", suffix=".jsonl")
        os.close(handle)
        helper.units_path = Path(path)
        return helper

    def child_env(self, helper: Helper, env=None) -> dict:
        """The environment that lets the child report what it owns."""
        base = dict(env if env is not None else os.environ)
        base[proctree.UNITS_ENV] = str(helper.units_path)
        return base

    def attach(self, helper: Helper, proc, tree) -> None:
        helper.proc, helper.tree, helper.state = proc, tree, RUNNING

    def discard(self, helper: Helper) -> None:
        """Drop a reservation that never became a process."""
        if helper.proc is not None:                      # pragma: no cover
            raise RuntimeError("a started helper is settled, never discarded")
        _forget_units(helper)
        self._release(helper)

    def _release(self, helper: Helper) -> None:
        with self._lock:
            self._live.pop(id(helper), None)

    # ---- the ending ------------------------------------------------------
    def settle(self, helper: Helper, *, deadline: float = 5.0) -> bool:
        """End everything this helper owns, and say whether it is provably gone.

        Called on every exit, including a clean one. A helper returning 0 says
        the command finished; it says nothing about the browser three processes
        below it, and on POSIX that browser is not even in the same process
        group - `_spawn_leader` gave it its own session. So the outer tree is
        the first unit, not the only one, and the record is released only when
        every unit is empty.
        """
        helper.state = STOPPING
        reason = None
        if helper.proc is not None:
            try:
                proctree.end_session(helper.proc, helper.tree, grace=deadline)
            except ChildTerminationFailed as stuck:
                reason = str(stuck)
            except Exception as e:                       # noqa: BLE001
                reason = f"{type(e).__name__}: {e}"      # pragma: no cover
            close_pipes(helper.proc)

        # After the helper has been ended, so nothing is still appending.
        helper.units = read_units(helper.units_path)
        left = []
        for tree in helper.owned_trees():
            try:
                if not proctree.end_tree(tree, grace=deadline,
                                         kill_grace=deadline):
                    left.append(tree)
            except Exception as e:                       # noqa: BLE001
                reason = reason or f"{type(e).__name__}: {e}"
                left.append(tree)

        if not left:
            helper.straggler = None
            helper.state = SETTLED
            _forget_units(helper)
            self._release(helper)
            return True
        helper.state = STRANDED
        helper.straggler = reason or (
            "still running: " + ", ".join(f"{t.kind}:{t.group}" for t in left))
        return False

    def end_all(self, *, deadline: float = 5.0) -> list:
        """End every live helper, and close this manager for good.

        Registered as the app's shutdown hook beside the one that stops
        sessions. Admission closes *first*, so a request arriving mid-shutdown
        cannot start a browser behind the snapshot - and it stays closed,
        because this app is ending. A restart builds a new manager rather than
        re-opening a shared flag, which is what made the first sweep anywhere
        in a process refuse every probe afterwards.

        Stragglers stay live. Dropping them is what made a failed cleanup
        indistinguishable from a successful one.
        """
        with self._lock:
            self._closed = True
        return [h for h in self.live() if not self.settle(h, deadline=deadline)]


def _forget_units(helper: Helper) -> None:
    """Drop the reporting file; its contents are on the record by now."""
    path, helper.units_path = helper.units_path, None
    if path is None:
        return
    try:
        path.unlink(missing_ok=True)
    except OSError:                                      # pragma: no cover
        pass


# --------------------------------------------------------------------------
# The process-level manager, for callers with no app
# --------------------------------------------------------------------------
# The CLI and the suites call `run_helper` directly. They get this one; an app
# gets its own from `register()`. `reopen()` replaces it rather than clearing a
# flag, because a swept manager is finished by design.
_DEFAULT = HelperManager()
_DEFAULT_LOCK = threading.Lock()


def default_manager() -> HelperManager:
    with _DEFAULT_LOCK:
        return _DEFAULT


def reopen() -> HelperManager:
    """Start a fresh process-level manager. Returns it."""
    global _DEFAULT
    with _DEFAULT_LOCK:
        _DEFAULT = HelperManager()
        return _DEFAULT


def live_helpers() -> list:
    return default_manager().live()


def stragglers() -> list:
    return default_manager().stragglers()


def settle(helper: Helper, *, deadline: float = 5.0) -> bool:
    return default_manager().settle(helper, deadline=deadline)


def end_helpers(*, deadline: float = 5.0) -> list:
    """Sweep the process-level manager, and let the process work again after.

    An *app's* manager closes for good when it is swept, because its app is
    over. This one has no app lifetime to end - it belongs to the process - so
    a sweep here is a sweep, not a shutdown, and it is replaced rather than
    latched shut. Latching it is what made the first sweep anywhere refuse
    every helper afterwards.

    Unless something was left behind: then the closed manager stays, so the
    stragglers remain visible and nothing new starts on top of them.
    """
    left = default_manager().end_all(deadline=deadline)
    if not left:
        reopen()
    return left


def run_helper(cli, timeout: float, env=None, manager: HelperManager = None):
    """Run a finite helper command, bounded, and prove everything it owned ended.

    Returns a `CompletedProcess`, including for a non-zero exit: a failing
    proxy test is a result its caller reads, not a broken command. It returns
    **only** when the tree is provably gone; an unproved cleanup raises, for a
    clean return exactly as for a timeout, because the alternative is handing
    back a result that looks finished while a browser still holds the profile.
    """
    manager = manager or default_manager()
    helper = manager.admit(cli)               # before the spawn, not after
    try:
        proc, tree = proctree.spawn(
            cli, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            stdin=subprocess.DEVNULL, text=True,
            env=manager.child_env(helper, env),
        )
    except BaseException:
        manager.discard(helper)
        raise
    manager.attach(helper, proc, tree)

    try:
        out, err, timed_out = _read_bounded(proc, timeout)
        done = subprocess.CompletedProcess(cli, proc.returncode, out, err)
    finally:
        # Every exit, not just the timeout, and outside the `return` so an
        # unrelated exception on the way out is not swallowed by this.
        proved = manager.settle(helper)

    if timed_out:
        raise HelperTimeout(
            f"a browser helper did not finish within {timeout:.0f}s and was "
            f"stopped. Raise NUMERA_HELPER_TIMEOUT if this machine needs "
            f"longer, or check whether the profile's proxy is answering."
            if proved else
            f"a browser helper did not finish within {timeout:.0f}s and could "
            f"not be fully stopped: {helper.straggler}. Its browser may still "
            f"be running; it is listed as a straggler."
        ) from None
    if not proved:
        raise HelperUnsettled(
            f"a browser helper exited with code {done.returncode} but could "
            f"not be fully stopped: {helper.straggler}. Its browser may still "
            f"be holding the profile directory; it is listed as a straggler.")
    return done
