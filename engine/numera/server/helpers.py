"""The lifecycle of a finite browser-backed helper, from admission to proved end.

These are the probes - cookies export, proxy test, trust, tls, dns, align. Each
starts a browser, so each owns one or more process trees, and the question is
never "did the command return" but **"is everything it owned provably gone, and
is the answer it gave complete"**.

`0.33.0` gave each app its own manager, reserved admission before the spawn and
raised on an unproved cleanup. Three things were still wrong, and each was
measured rather than reasoned about:

* **Output was silently truncated.** A helper that wrote `TRAILING-NO-NEWLINE`
  and exited 0 handed its caller `returncode=0` with `stdout=''`. Line-wise
  draining never yields a trailing write with no newline, the reader threads
  were daemons nobody joined, and the pipes were closed while a separately
  sessioned driver still held the writers. Streams are now owned
  (`helper_streams`), the units are ended *before* anything is closed so EOF is
  reachable, and an incomplete drain is an error rather than a shorter string.
* **A shutdown could release a reservation that had not spawned yet.**
  `end_all` settled a `starting` record, found no process on it, and dropped it
  from the registry - after which the spawn went ahead and produced a child
  nothing tracked, behind a shutdown that had already reported success.
  Admission, spawn and attach are now one state machine: a reservation resolves
  or is reported stranded, and it is never released on the strength of having
  no process yet.
* **Unknown ownership read as zero ownership.** A missing, unreadable,
  truncated or malformed unit report all returned `[]`, exactly like a helper
  that legitimately started nothing - and a failure to *write* the report was
  swallowed in the child. Reading now answers with evidence
  (`UnitReport.complete`), and a report that cannot be trusted strands the
  helper instead of clearing it.

    starting -> running -> stopping -> settled     (every unit proved empty,
                                                    output complete)
                                    -> stranded    (something is unproved)

A stranded record is never dropped. A cleanup that could not be proved, and an
answer that could not be completed, are both failures.
"""
from __future__ import annotations

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
from ..unit_report import UnitReport, read as read_units  # noqa: F401
from .helper_streams import Streams

#: The states a helper record can be in. `starting` exists because admission is
#: reserved before the spawn: a child that was created and then refused has a
#: record to be found on.
STARTING, RUNNING, STOPPING, SETTLED, STRANDED = (
    "starting", "running", "stopping", "settled", "stranded")

#: The cleanup budget, shared across everything a helper owns - the outer tree,
#: every reported driver session, and the stream drain. Shared rather than per
#: unit on purpose: multiplying it by the number of processes turns a declared
#: bound into an undeclared one, and a caller cannot state a deadline it does
#: not control.
CLEANUP_BUDGET = 5.0

#: Of that, what the drain may take once the writers are gone. EOF should be
#: immediate by then; this only covers scheduling.
DRAIN_SHARE = 2.0


class HelperFailed(RuntimeError):
    """A helper did not complete as owned, bounded work."""


class HelperTimeout(HelperFailed):
    """A finite browser-backed helper outran its deadline."""


class HelperUnsettled(HelperFailed):
    """The command returned, and something it owned is still unaccounted for.

    Its own class because the exit code is not the story: a probe can answer 0
    and leave a browser holding the profile directory, and reporting that as a
    successful result is how the next launch finds the profile locked by a
    process nobody is tracking.
    """


class HelperOutputIncomplete(HelperFailed):
    """The command returned and its output could not be read to the end.

    Never a shorter string. A probe's answer is parsed by its caller, and half
    a JSON document is not a smaller answer - it is a different one, or a
    crash, depending on where the cut landed.
    """


class HelperShuttingDown(HelperFailed):
    """This app is stopping, so no new helper may be started."""


@dataclass
class Helper:
    """One helper, everything it owns, and how it ended."""
    cli: tuple
    proc: Optional[subprocess.Popen] = None
    tree: Optional[proctree.Tree] = None
    units: list = field(default_factory=list)
    units_path: Optional[Path] = None
    units_problem: Optional[str] = None
    state: str = STARTING
    started: float = field(default_factory=time.monotonic)
    straggler: Optional[str] = None
    #: Set by a shutdown that arrived while this was still `starting`. The
    #: spawner checks it before and after creating the child.
    cancel: threading.Event = field(default_factory=threading.Event)
    #: Set once the reservation has become a process or been proved not to be
    #: one. A shutdown waits on this rather than assuming either.
    resolved: threading.Event = field(default_factory=threading.Event)
    #: Held for the whole of `settle`, so two callers cannot end the same
    #: helper at once. `run_helper` settles its own helper and a shutdown
    #: settles every live one; that overlap is ordinary, not rare.
    finish: threading.Lock = field(default_factory=threading.Lock)

    @property
    def pid(self) -> Optional[int]:
        return getattr(self.proc, "pid", None)

    @property
    def cancelled(self) -> bool:
        return self.cancel.is_set()

    def describe(self) -> str:
        where = f"{self.tree.kind}:{self.tree.group}" if self.tree else "no tree"
        extra = f" +{len(self.units)} session(s)" if self.units else ""
        return f"pid {self.pid} ({where}){extra} [{self.state}]"

    def owned_trees(self) -> list:
        """Every tree that must be empty before this helper is finished."""
        return ([self.tree] if self.tree else []) + list(self.units)


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
        with self._lock:
            return list(self._live.values())

    def stragglers(self) -> list:
        """The helpers something survived. Never auto-cleared."""
        return [h for h in self.live() if h.straggler]

    @property
    def closed(self) -> bool:
        return self._closed

    def admit(self, cli) -> Helper:
        """Reserve a record for a helper **before** it is spawned.

        The order is the point, and so is what happens to the reservation
        afterwards: it is registered until it resolves, because "no process
        yet" is not "no process".
        """
        helper = Helper(cli=tuple(cli))
        with self._lock:
            if self._closed:
                raise HelperShuttingDown(
                    "this server is shutting down; the helper was not started")
            self._live[id(helper)] = helper
        return helper

    def child_env(self, helper: Helper, env=None) -> dict:
        """The environment that lets the child report what it owns.

        The report file is created here rather than in `admit` so that a
        failure to create it is a start failure with a registered reservation
        behind it, not an exception thrown at a caller holding nothing.
        """
        handle, path = tempfile.mkstemp(prefix="numera-units-", suffix=".jsonl")
        os.close(handle)
        helper.units_path = Path(path)
        base = dict(env if env is not None else os.environ)
        base[proctree.UNITS_ENV] = str(helper.units_path)
        return base

    def check_open(self, helper: Helper) -> None:
        """Refuse a reservation that shutdown has already cancelled."""
        if helper.cancelled:
            raise HelperShuttingDown(
                "this server began shutting down before the helper was "
                "started; it was not started")

    def attach(self, helper: Helper, proc, tree) -> None:
        """Bind the process to its reservation, and resolve it.

        Under the lock and before anything else can look at the record, so a
        shutdown waiting on `resolved` sees a helper with a process rather than
        an empty reservation it might drop.
        """
        with self._lock:
            helper.proc, helper.tree = proc, tree
            helper.state = RUNNING
        helper.resolved.set()

    def fail_start(self, helper: Helper, proc=None) -> None:
        """A start that raised. Discard only with no process to account for.

        `proctree.spawn` abandons the child on every failure path of its own,
        so a raise from it leaves nothing running - but that is its guarantee
        to make, not an assumption to bake in here. A caller that does have a
        process passes it, and the record keeps it.
        """
        if proc is not None:                                  # pragma: no cover
            self.attach(helper, proc, getattr(proc, "tree", None))
            return
        _forget_units(helper)
        self._release(helper)
        helper.state = SETTLED
        helper.resolved.set()

    def _release(self, helper: Helper) -> None:
        with self._lock:
            self._live.pop(id(helper), None)

    # ---- the ending ------------------------------------------------------
    def settle(self, helper: Helper, *, deadline: float = CLEANUP_BUDGET) -> bool:
        """End everything this helper owns, and say whether it is provably gone.

        Called on every exit, including a clean one. A helper returning 0 says
        the command finished; it says nothing about the browser three processes
        below it, and on POSIX that browser is not even in the same process
        group - `_spawn_leader` gave it its own session.

        The pipes are **not** closed here. Closing one while a live separately
        sessioned process still holds the writer is the blocking close this
        ordering exists to avoid; `run_helper` collects and closes once the
        units are gone. One shared deadline covers every unit, rather than each
        unit getting the whole of it.
        """
        # One settle at a time for this helper, and the second caller gets the
        # first one's verdict rather than a second opinion.
        #
        # Checking the state alone was not enough, and the gap is the whole
        # defect: the first caller sets STOPPING and releases the lock before
        # doing the slow part, so a second caller arriving *during* that window
        # saw "not SETTLED yet", ran the whole of settle again, and reached
        # `read_units` after the winner had already succeeded and cleared
        # `units_path`. An absent report reads as lost evidence, so a helper
        # that had demonstrably been ended - SIGTERM delivered, returncode -15 -
        # was reported STRANDED. Found on Linux in CI; the same race exists on
        # Windows and simply lost more often there.
        with helper.finish:
            with self._lock:
                if helper.state == SETTLED:
                    # The only state that may not be re-entered: the success
                    # path consumes the ownership report, so a second pass
                    # would read an absent one and call it lost evidence.
                    #
                    # STRANDED deliberately is *not* short-circuited here. A
                    # stranded record is never dropped, which is about keeping
                    # it - not about refusing to try again. Nothing was
                    # consumed on that path, and a later sweep runs under
                    # different conditions: the reason it could not be proved
                    # may simply have gone away.
                    return True
                helper.state = STOPPING
            return self._settle_locked(helper, deadline)

    def _settle_locked(self, helper: Helper, deadline: float) -> bool:
        """The body of `settle`, with this helper's `finish` lock already held."""
        reason = None
        left_at = time.monotonic() + deadline
        remaining = lambda: max(0.0, left_at - time.monotonic())   # noqa: E731

        if helper.proc is not None:
            try:
                proctree.end_session(helper.proc, helper.tree,
                                     grace=min(deadline, remaining()))
            except ChildTerminationFailed as stuck:
                reason = str(stuck)
            except Exception as e:                       # noqa: BLE001
                reason = f"{type(e).__name__}: {e}"      # pragma: no cover

        # After the helper has been ended, so nothing is still appending to it.
        report = read_units(helper.units_path)
        helper.units = report.trees
        helper.units_problem = report.problem

        left = []
        for tree in helper.owned_trees():
            grace = remaining()
            try:
                if not proctree.end_tree(tree, grace=grace, kill_grace=grace):
                    left.append(tree)
            except Exception as e:                       # noqa: BLE001
                reason = reason or f"{type(e).__name__}: {e}"
                left.append(tree)

        if left or not report.complete:
            helper.state = STRANDED
            helper.straggler = reason or report.problem or (
                "still running: "
                + ", ".join(f"{t.kind}:{t.group}" for t in left))
            return False
        with self._lock:
            helper.straggler = None
            helper.state = SETTLED           # under the lock, before the report
            self._live.pop(id(helper), None)  # goes, so a racing settle sees it
        _forget_units(helper)
        return True

    def end_all(self, *, deadline: float = CLEANUP_BUDGET) -> list:
        """End every live helper, and close this manager for good.

        Admission closes first, so a request arriving mid-shutdown cannot start
        a browser behind the snapshot - and it stays closed, because this app is
        ending. A restart builds a new manager rather than re-opening a shared
        flag.

        A reservation that has not spawned yet is **cancelled and waited for**,
        never released: dropping it on the strength of having no process is
        what let a spawn complete behind a finished shutdown. The wait is
        bounded and happens with the lock released, because resolving it needs
        that same lock.
        """
        with self._lock:
            self._closed = True
            pending = [h for h in self._live.values() if h.state == STARTING]
        for helper in pending:
            helper.cancel.set()
        end = time.monotonic() + deadline
        for helper in pending:
            helper.resolved.wait(max(0.0, end - time.monotonic()))

        left = []
        for helper in self.live():
            if helper.state == STARTING and not helper.resolved.is_set():
                # Still unresolved: it may yet create a process, so it keeps its
                # record and is reported rather than quietly forgotten.
                helper.state = STRANDED
                helper.straggler = (
                    "a helper was still starting when this server shut down, "
                    "and did not resolve within the cleanup budget; whether it "
                    "created a process is unknown")
                left.append(helper)
                continue
            if not self.settle(helper, deadline=deadline):
                left.append(helper)
        return left


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


def settle(helper: Helper, *, deadline: float = CLEANUP_BUDGET) -> bool:
    return default_manager().settle(helper, deadline=deadline)


def end_helpers(*, deadline: float = CLEANUP_BUDGET) -> list:
    """Sweep the process-level manager, and let the process work again after.

    An *app's* manager closes for good when it is swept, because its app is
    over. This one belongs to the process, so a sweep here is a sweep and not a
    shutdown - unless something was left behind, in which case the closed
    manager stays so the stragglers remain visible.
    """
    left = default_manager().end_all(deadline=deadline)
    if not left:
        reopen()
    return left


def run_helper(cli, timeout: float, env=None, manager: HelperManager = None,
               cleanup: float = CLEANUP_BUDGET):
    """Run a finite helper command, bounded, and prove what it owned ended.

    The declared bound is `timeout` for the operation plus `cleanup` for
    everything afterwards - ending the units and draining the streams share
    that one budget rather than each taking it in turn.

    Returns a `CompletedProcess`, including for a non-zero exit: a failing
    proxy test is a result its caller reads, not a broken command. It returns
    only when the tree is provably gone **and** the output was read to the end;
    either failure raises, because a result that looks finished while a browser
    still holds the profile - or one whose JSON stops halfway - is worse than
    an error.
    """
    manager = manager or default_manager()
    helper = manager.admit(cli)               # before the spawn, not after
    try:
        child_env = manager.child_env(helper, env)
        manager.check_open(helper)            # shutdown may have arrived
        proc, tree = proctree.spawn(
            cli, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            stdin=subprocess.DEVNULL, env=child_env,
        )
    except BaseException:
        # Nothing was created: `proctree.spawn` abandons its own child on every
        # failure path, and the steps before it start no process at all.
        manager.fail_start(helper)
        raise
    # From here the child exists, so every exit has to end it and close its
    # pipes - including an exception from `attach` itself, or any unexpected
    # one. Without this the failure propagated with two `BufferedReader`s open,
    # which surfaced as a `ResourceWarning` and is a leaked handle on a live
    # pipe whatever the warning settings say.
    streams = None
    timed_out = False
    try:
        streams = Streams(proc)               # reading from now, not after
        manager.attach(helper, proc, tree)
        if helper.cancelled:
            # Shutdown landed while this was spawning. The child exists and is
            # registered, so it is ended here rather than left to a sweep that
            # has already taken its snapshot.
            raise HelperShuttingDown(
                "this server began shutting down while the helper was "
                "starting; it was stopped rather than left running")
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
    except BaseException:
        manager.settle(helper, deadline=cleanup)
        if streams is not None:
            streams.close()
        raise

    # Order matters: every owner of a pipe writer has to be gone before the
    # drain can reach end-of-file, and before a close can be safe.
    proved = manager.settle(helper, deadline=max(0.0, cleanup - DRAIN_SHARE))
    out, err, complete = streams.collect(DRAIN_SHARE)
    unfinished = streams.unfinished()
    streams.close()
    done = subprocess.CompletedProcess(cli, proc.returncode, out, err)

    if timed_out:
        raise HelperTimeout(
            f"a browser helper did not finish within {timeout:.0f}s and was "
            f"stopped. Raise NUMERA_HELPER_TIMEOUT if this machine needs "
            f"longer, or check whether the profile's proxy is answering."
            if proved else
            f"a browser helper did not finish within {timeout:.0f}s and could "
            f"not be fully stopped: {helper.straggler}. Its browser may still "
            f"be running; it is listed as a straggler.")
    if not proved:
        raise HelperUnsettled(
            f"a browser helper exited with code {done.returncode} but could "
            f"not be fully accounted for: {helper.straggler}. It is listed as "
            f"a straggler.")
    if not complete:
        raise HelperOutputIncomplete(
            f"a browser helper exited with code {done.returncode} but its "
            f"{' and '.join(unfinished)} could not be read to the end within "
            f"{DRAIN_SHARE:.0f}s, so its answer is incomplete and has not been "
            f"used. {len(out)} character(s) of stdout were collected.")
    return done
