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
from ..unit_report import UnitReport, initialize, read as read_units  # noqa: F401
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
    streams: Optional[Streams] = None
    output: tuple = ("", "", True)
    output_problem: Optional[str] = None
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

    def __init__(self, root=None) -> None:
        self.root = root
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
        initialize(helper.units_path)
        base = dict(env if env is not None else os.environ)
        base[proctree.UNITS_ENV] = str(helper.units_path)
        return base

    def check_open(self, helper: Helper) -> None:
        """Refuse a reservation that shutdown has already cancelled."""
        if helper.cancelled:
            raise HelperShuttingDown(
                "this server began shutting down before the helper was "
                "started; it was not started")

    def adopt(self, helper: Helper, proc, tree) -> None:
        """Take ownership of a spawned child **before** anything else can throw.

        Separate from `attach` because the two answer different questions.
        `adopt` says "this record owns that process"; `attach` says "the
        reservation is resolved, stop waiting for it". Between the spawn and
        the attach there used to be real work - constructing the readers - and
        an exception there left `proc` and `tree` in local variables only. The
        cleanup that followed looked at a record with no process, found nothing
        to end, released it, and then hung closing the pipes of the child it had
        just decided did not exist.

        Ownership is therefore recorded first and unconditionally, so that
        *every* post-spawn exit - `Streams` raising, `attach` raising, anything
        unexpected - ends or retains that exact unit.
        """
        with self._lock:
            helper.proc, helper.tree = proc, tree

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
        """Resolve a refused start without discarding a created process/report."""
        proc = proc if proc is not None else helper.proc
        if proc is not None:
            helper.proc = proc
            helper.tree = helper.tree or getattr(proc, "tree", None)
            helper.state = STRANDED
            helper.straggler = "startup failed after a process was created"
            helper.resolved.set()
            return
        if _forget_units(helper):
            helper.state = SETTLED
            self._release(helper)
        else:
            helper.state = STRANDED
            helper.straggler = "startup failed and its report could not be removed"
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

        Process termination, output collection and cancellable reader disposal
        share one deadline. The registry is released only after all of them
        finish. A failed reader is not silently replaced by partial output.
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
        # The clock starts here, not after the lock. `deadline` is what the
        # caller is prepared to wait in total, and acquisition is part of the
        # wait: a `settle(deadline=.05)` sat on `helper.finish` for over `.3s`
        # until something external released it, because the budget only began
        # once the lock was already held. A bound that excludes the blocking
        # part is not the bound the caller asked for.
        ends_at = time.monotonic() + max(0.0, deadline)
        if not helper.finish.acquire(timeout=max(0.0, ends_at - time.monotonic())):
            # Do not mutate a record another settlement currently owns - but a
            # refusal still has to say why. Written only when nothing is there:
            # a settlement that already recorded its own verdict keeps it, and
            # a caller that simply ran out of budget is no longer told "False"
            # with nothing on the record to explain it.
            if not helper.straggler:
                helper.straggler = ("the settlement lock was held by another "
                                    "caller for the whole deadline; nothing "
                                    "was touched and everything is retained")
            return False
        try:
            if helper.state == SETTLED:
                return True
            if not helper.resolved.is_set():
                helper.cancel.set()
                helper.resolved.wait(max(0.0, ends_at - time.monotonic()))
                if not helper.resolved.is_set():
                    helper.state = STRANDED
                    helper.straggler = "startup has not resolved; its reservation is retained"
                    return False
            helper.state = STOPPING
            try:
                return self._settle_locked(helper, ends_at)
            except BaseException:
                helper.state = STRANDED
                helper.straggler = "cleanup raised; ownership and remaining resources are retained"
                if helper.streams is not None:
                    helper.streams.close(max(0.0, ends_at - time.monotonic()))
                raise
        finally:
            helper.finish.release()

    def _settle_locked(self, helper: Helper, ends_at: float) -> bool:
        """Processes, output and record disposal are one terminal operation."""
        remaining = lambda: max(0.0, ends_at - time.monotonic())
        # Leave a portion of the SAME budget for EOF/cancellation and joins.
        drain_share = min(DRAIN_SHARE, remaining() / 2.0) if helper.streams else 0.0
        process_end = ends_at - drain_share
        reasons = []
        if helper.proc is not None:
            try:
                proctree.end_session(helper.proc, helper.tree,
                                     ends_at=process_end)
            except Exception as error:
                reasons.append(f"{type(error).__name__}: {error}")

        report = read_units(helper.units_path)
        # Retain earlier valid identities if a later read loses some evidence.
        for tree in report.trees:
            if tree not in helper.units:
                helper.units.append(tree)
        helper.units_problem = report.problem
        left = []
        for tree in helper.owned_trees():
            try:
                if not proctree.end_tree(tree, ends_at=process_end):
                    left.append(tree)
            except Exception as error:
                reasons.append(f"{type(error).__name__}: {error}")
                left.append(tree)
        # A group may be empty while its leader has not yet been reaped.
        leader_gone = helper.proc is None or helper.proc.poll() is not None
        resources_done = True
        if helper.streams is not None:
            # Preserve a cancellation/join slice rather than spending it all
            # waiting for EOF from an unaccounted writer.
            reserve = min(0.1, remaining() / 2.0)
            helper.output = helper.streams.collect(max(0.0, remaining() - reserve))
            retained = helper.streams.close(remaining())
            resources_done = not retained
            if not helper.output[2] or retained:
                # "still holding a pipe" is the condition's name in this
                # codebase and what callers and suites match on; the list that
                # follows is the detail. Both, not one.
                helper.output_problem = (
                    "output did not reach EOF, or a reader is still holding a "
                    "pipe and could not be disposed: "
                    + ", ".join(helper.streams.unfinished() + retained))
            else:
                # A previous join can time out after genuine EOF. Recompute
                # its verdict from current evidence, not a stale diagnostic.
                # Read errors and cancellation still make output[2] false.
                helper.output_problem = None
        elif helper.proc is not None:
            # Preparation failed before any pump was started.
            for pipe in (helper.proc.stdout, helper.proc.stderr):
                if pipe is not None and not pipe.closed:
                    try:
                        pipe.close()
                    except OSError as error:
                        resources_done = False
                        reasons.append(f"pipe close: {error}")

        if left or not leader_gone or not report.complete or not resources_done or helper.output_problem:
            helper.state = STRANDED
            helper.straggler = "; ".join(filter(None, [
                report.problem, helper.output_problem, *reasons,
                "owned processes remain" if left or not leader_gone else None]))
            return False
        # No release occurs until all process AND stream checks passed.
        if not _forget_units(helper):
            helper.state = STRANDED
            helper.straggler = "the ownership report could not be removed"
            return False
        with self._lock:
            helper.straggler = None
            helper.state = SETTLED
            self._live.pop(id(helper), None)
        return True

    def end_all(self, *, deadline: float = CLEANUP_BUDGET) -> list:
        """Cancel admission, then settle the snapshot under one total budget."""
        ends_at = time.monotonic() + max(0.0, deadline)
        with self._lock:
            self._closed = True
            snapshot = list(self._live.values())
            for helper in snapshot:
                if not helper.resolved.is_set():
                    helper.cancel.set()
        left = []
        for helper in snapshot:
            if not self.settle(helper, deadline=max(0.0, ends_at - time.monotonic())):
                left.append(helper)
        return left


def _forget_units(helper: Helper) -> bool:
    """Remove evidence only after completion; retain its path on failure."""
    path = helper.units_path
    if path is None:
        return True
    try:
        path.unlink(missing_ok=True)
    except OSError:
        return False
    helper.units_path = None
    return True


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
    """Return complete output only after the owned lifecycle has terminated."""
    from contextlib import nullcontext
    from ..update_admission import Admission
    manager = manager or default_manager()
    work = (Admission(manager.root).work('finite helper', settled=lambda: not manager.stragglers())
            if manager.root is not None else nullcontext())
    with work:
        return _run_helper(cli, timeout, env, manager, cleanup)


def _run_helper(cli, timeout, env, manager, cleanup):
    helper = manager.admit(cli)
    try:
        child_env = manager.child_env(helper, env)
        manager.check_open(helper)
        proc, tree = proctree.spawn(cli, stdout=subprocess.PIPE,
            stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, env=child_env)
    except BaseException:
        manager.fail_start(helper)
        raise
    # This assignment has no external callback between spawn and ownership.
    # A fallible adopt hook must not hide the already-created process.
    helper.proc, helper.tree = proc, tree
    timed_out = False
    try:
        manager.adopt(helper, proc, tree)
        helper.streams = Streams(proc, start=False)
        helper.streams.start()
        manager.attach(helper, proc, tree)
        if helper.cancelled:
            raise HelperShuttingDown("this server cancelled the helper during startup")
        try:
            proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
    except BaseException:
        helper.resolved.set()
        # Preserve the original error, and the record if cleanup also fails.
        try:
            manager.settle(helper, deadline=cleanup)
        except Exception as error:
            helper.state = STRANDED
            helper.straggler = f"startup and cleanup failed: {type(error).__name__}"
        raise
    proved = manager.settle(helper, deadline=cleanup)
    out, err, complete = helper.output
    if timed_out:
        raise HelperTimeout(
            f"helper exceeded {timeout:g}s; "
            + ("all owned work was stopped" if proved else
               f"a straggler could not be fully accounted for and is "
               f"retained: {helper.straggler}"))
    if not proved:
        error_type = HelperOutputIncomplete if helper.output_problem and not helper.units_problem else HelperUnsettled
        raise error_type(f"helper cleanup: a straggler could not be fully "
                         f"accounted for and is retained: {helper.straggler}")
    if not complete:
        raise HelperOutputIncomplete("helper output was not completely read")
    return subprocess.CompletedProcess(cli, proc.returncode, out, err)
