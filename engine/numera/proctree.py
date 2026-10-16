"""One spawned session and every process it starts, owned as a unit.

Numera's answer to "is it stopped?" was about the process it spawned, and that
process is a Python interpreter. ``numera launch`` starts Playwright, which
starts a Node driver, which starts Chromium and its renderers - and none of
those are the process Numera holds a handle to. So ``handshake.end_child``
proved the immediate child had exited, and the detached-job terminator proved
the *group leader* was gone. Both then let every caller act on it:

* the server's registry entry was dropped;
* the session lease - held by that dead Python process - was released by the OS;
* ``POST /stop``, ``numera stop`` and ``jobs stop`` recorded "stopped";
* and a Chromium still holding ``user-data/<id>`` was invisible to all of it,
  so the next launch took the now-free lease and opened a second browser on the
  same profile directory.

Two browsers in one Chromium profile directory is exactly the corruption the
session lease exists to prevent, so proving the leader dead is not enough. What
has to be proved empty is the whole tree.

The tree is made an OS object at spawn, because after the fact there is nothing
left to ask. On POSIX the child gets its own session, so every descendant
inherits its process group and the group id outlives the leader; on Windows the
child is assigned to a named Job Object, which holds its descendants whether or
not the leader is alive and can be reopened by name from a different process
entirely. Emptiness then has an answer the OS gives - ``killpg(pgid, 0)``
raising ESRCH, or a job whose ``ActiveProcesses`` is zero - rather than one
Numera infers from the one process it happened to know about.

`Tree` is the durable half: a pid, the start-time token that makes that pid an
identity, and the group handle. It is small and JSON-shaped on purpose, because
`jobs.py` writes it to disk and a *different* process reads it back to stop a
detached session.
"""

from __future__ import annotations

import hashlib
import os
import signal
import subprocess
import threading
import time
import uuid
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

from .handshake import ChildTerminationFailed, end_child
from .driver_capture import Adopted, spawning_own_tree  # public capture API
from .posix_session import (_adopt_group, _group_is_running,  # noqa: F401
                            _rejoin, _restore_terminal, _terminal)  # noqa: F401
from .winjob import (_job_active, _job_assign, _job_assign_self,  # noqa: F401
                     _job_open, _job_structs, _job_terminate, _kernel32)  # noqa: F401

_WINDOWS = os.name == "nt"
_POLL_SECONDS = 0.05

#: How long to keep asking the OS for a just-spawned child's start time before
#: giving up on identifying it. Reading it is a single syscall; the retry is
#: for the moment right after `CreateProcess` when the handle is not yet
#: queryable, not for a slow machine.
_IDENTITY_DEADLINE = 2.0


class TreeUnavailable(RuntimeError):
    """A session could not be given an OS tree to own what it starts.

    Not a warning. Anything that can open a browser needs the tree primitive to
    exist *before* it opens one, because that is the only handle by which the
    browser can later be proved gone - and "proved gone" is what lets the next
    launch reuse the profile directory. Without it the session is refused.
    """


class Unverified(RuntimeError):
    """Numera never established that this tree is the one it started.

    A pid on its own is not an identity - the OS reuses them - so a stored pid
    with no start-time token and no group handle could name anything by the
    time somebody asks for it to be stopped. Raised instead of signalling, and
    the caller is expected to tell the user rather than take the chance.
    """


@dataclass(frozen=True)
class Tree:
    """A spawned leader and the process group or job object that holds it.

    ``kind`` is ``"pgid"`` (POSIX), ``"job"`` (Windows), or ``""`` when the
    tree primitive could not be created and only the leader is known.
    """

    pid: int
    token: object = None
    group: object = None
    kind: str = ""

    @property
    def verified(self) -> bool:
        """May this tree be signalled at all?

        A named job object we created is an identity in its own right: no other
        program has that name, so terminating it cannot reach an unrelated
        process however long ago the pid was recycled. A process group is only
        as good as the token that proves the leader was ours.
        """
        if self.kind == "job" and self.group:
            return True
        return self.token is not None

    def as_record(self) -> dict:
        """The JSON form `jobs.py` stores beside the pid."""
        return {"kind": self.kind, "group": self.group}

    @classmethod
    def from_record(cls, rec: dict) -> "Tree":
        """Rebuild a tree from a job record, tolerating one written by 0.24.

        An older record has a pid and maybe a token and no tree at all. It
        comes back as a leader-only tree, which is honest: there is no group
        handle, so `signal_tree` falls back to what that version did.
        """
        tree = (rec or {}).get("tree") or {}
        return cls(pid=int((rec or {}).get("pid") or 0),
                   token=(rec or {}).get("pidToken"),
                   group=tree.get("group"),
                   kind=str(tree.get("kind") or ""))


# ---- process identity ----------------------------------------------------
def started_at(pid: int) -> "Optional[int]":
    """When this pid's process started, or None if it cannot be read.

    A pid on its own is not an identity: the OS reuses them, and this module
    hands stored pids to terminators that would take down whatever tree now
    happens to own that number. The start time is what makes the pair unique -
    two processes can share a pid, never at the same instant.

    Standard library only: the engine declares no required dependencies, and
    pulling in psutil for one field would end that.
    """
    if not pid:
        return None
    try:
        if _WINDOWS:
            import ctypes
            import ctypes.wintypes as wt

            PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
            k32 = _kernel32()
            handle = k32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, int(pid))
            if not handle:
                return None
            try:
                created = wt.FILETIME()
                other = (wt.FILETIME(), wt.FILETIME(), wt.FILETIME())
                if not k32.GetProcessTimes(handle, ctypes.byref(created),
                                           ctypes.byref(other[0]),
                                           ctypes.byref(other[1]),
                                           ctypes.byref(other[2])):
                    return None
                return (created.dwHighDateTime << 32) | created.dwLowDateTime
            finally:
                k32.CloseHandle(handle)

        stat = Path(f"/proc/{int(pid)}/stat")
        if stat.exists():
            # Field 22 is starttime in clock ticks. The comm field can contain
            # spaces and parentheses, so split after the last ")".
            text = stat.read_text(encoding="utf-8", errors="replace")
            return int(text[text.rindex(")") + 2:].split()[19])

        out = subprocess.run(["ps", "-o", "lstart=", "-p", str(int(pid))],
                             capture_output=True, text=True, timeout=5)
        line = " ".join(out.stdout.split())      # normalise ps's padding
        if not line:
            return None
        return int(hashlib.sha256(line.encode("utf-8")).hexdigest()[:12], 16)
    except Exception:  # noqa: BLE001 - an unreadable start time is not an identity
        return None


def alive(pid: int, token=None) -> bool:
    """Is this process still running - and still the one we started?

    `token` is a `started_at` value captured when the pid was recorded. Without
    it this answers the weaker question and says so; with it, a pid the OS has
    handed to somebody else reads as dead, which is what stops this module
    terminating an unrelated process tree.
    """
    if not pid:
        return False
    if token is not None:
        now = started_at(pid)
        # None means the process is gone, or is one we may not query - either
        # way it is not ours to kill.
        if now is None or now != token:
            return False
    if _WINDOWS:
        # os.kill(pid, 0) is NOT a liveness probe on Windows: CPython maps every
        # signal but CTRL_C/CTRL_BREAK to TerminateProcess, so the "harmless"
        # POSIX idiom would kill the very job being polled.
        import ctypes

        SYNCHRONIZE = 0x00100000
        WAIT_TIMEOUT = 0x00000102
        k32 = _kernel32()
        handle = k32.OpenProcess(SYNCHRONIZE, False, int(pid))
        if not handle:
            return False
        try:
            return k32.WaitForSingleObject(handle, 0) == WAIT_TIMEOUT
        finally:
            k32.CloseHandle(handle)
    try:
        os.kill(int(pid), 0)
    except OSError:
        return False
    return True


def identify(proc: subprocess.Popen,
             deadline: float = _IDENTITY_DEADLINE) -> "Optional[int]":
    """The start-time token for a child we have just spawned, or None.

    Retried, briefly, and only while the child is *known to still be the
    process we created*: `proc.poll()` is None, so the pid cannot yet have been
    recycled and whatever the OS reports belongs to our child. Once it has
    exited there is nothing to identify and nothing that may be signalled, so
    the loop stops rather than reading a number that could already name
    somebody else.
    """
    if not getattr(proc, "pid", 0):
        return None                     # nothing to identify
    end = time.monotonic() + deadline
    while True:
        token = started_at(proc.pid)
        if token is not None:
            return token
        if proc.poll() is not None:
            return None                 # gone before we could name it
        if time.monotonic() >= end:
            return None
        time.sleep(_POLL_SECONDS)


# ---- spawning ------------------------------------------------------------
def spawn(argv, **kwargs) -> tuple:
    """Start `argv` as the leader of a tree this process owns.

    Returns `(proc, tree)`. Every session Numera starts goes through here -
    the API's launches, the scheduler's warm-ups and every detached CLI job -
    so there is one answer to "what did we start" and one way to end it.

    **It fails closed.** The first version treated the tree primitive as
    best-effort: if the job object could not be made, the child was spawned
    anyway with an empty `kind`, and everything downstream quietly fell back to
    the leader - which is the original defect, reached by a different route.
    Ownership of a browser tree is a correctness primitive, not an
    optimisation, so a child that cannot be owned is terminated before it can
    open anything and `TreeUnavailable` is raised. The environment carries the
    tree down to the child, which needs to name it to claim its own session.
    """
    if _WINDOWS:
        name = f"numera-tree-{uuid.uuid4().hex}"
        job = _job_open(name)
        if job is None:
            raise TreeUnavailable(
                "this session could not be given a job object to own the "
                "browser it starts, so it was not started: without one, "
                "nothing could prove the browser had stopped.")
        kwargs = _with_tree_env(kwargs, Tree(pid=0, group=name, kind="job"))
        # Handle inheritance is off by default for everything Python opens, so
        # this hands the child exactly one extra handle: its own job.
        proc = subprocess.Popen(argv, close_fds=False, **kwargs)
        if not _job_assign(job, proc):
            _abandon(proc)
            raise TreeUnavailable(
                f"the session process could not be put in its job object, so "
                f"it was stopped rather than left owning a browser nothing "
                f"could account for (pid {proc.pid}).")
        return proc, Tree(pid=proc.pid, token=identify(proc),
                          group=name, kind="job")
    # Its own session, so descendants inherit this group and the group id
    # outlives the leader. The server used to spawn without it and shared the
    # server's own process group, where a group signal would have taken the
    # engine down with the browser.
    kwargs["start_new_session"] = True
    proc = subprocess.Popen(argv, **kwargs)
    return proc, Tree(pid=proc.pid, token=identify(proc),
                      group=proc.pid, kind="pgid")


# The ownership report - the format, the writing and the reading - lives in
# `unit_report`. It is one responsibility and it was split across two
# modules, which is how the two halves came to disagree about what an absent
# record means. Re-exported because `proctree.UNITS_ENV` and
# `proctree.UnitReportFailed` are the names the server and the suites use.
from .unit_report import UNITS_ENV, UnitReportFailed  # noqa: E402,F401
from .unit_report import begin_spawn, cancel_spawn
from .unit_report import record as report_unit        # noqa: E402
from .disposal import abandon as _abandon, reported as _reported  # noqa: E402


def _spawn_leader(original, args, rest, kwargs) -> tuple:
    """Publish an intent before process creation and resolve it after ownership."""
    attempt = begin_spawn()
    try:
        proc, tree = _spawn_unit(original, args, rest, kwargs)
    except BaseException:
        # No generic cancellation here: a failed OS ownership operation may
        # have created a process. Leaving the intent pending is conservative.
        raise
    return proc, _reported(proc, tree, attempt=attempt)


def _spawn_unit(original, args, rest, kwargs) -> tuple:
    """`original(...)`, with the child made the leader of a new tree.

    Windows assigns after the fact, the same way `spawn()` does and for the
    same reason: `Popen` gives no way to hand a job in at creation. The window
    is the microseconds between the call returning and the assignment, and a
    browser driver spends the first of its life reading its own arguments - it
    starts a browser only when asked, which is a message away.

    The new tree is reported to whoever started this process, because it is not
    reachable from theirs: it is its own session by construction.
    """
    if _WINDOWS:
        name = f"numera-session-{uuid.uuid4().hex}"
        job = _job_open(name)
        if job is None:
            raise TreeUnavailable(
                "this session could not be given a job object to own the "
                "browser it starts, so it was not started: without one, "
                "nothing could prove the browser had stopped.")
        # `close_fds=False` for the same reason `spawn()` uses it: the job
        # handle is inheritable, and the child holding its own copy is what
        # keeps the job's *name* resolvable after `_job_assign` closes ours.
        # Without it the last handle went with that call, kill-on-close fired,
        # and the driver died the moment it was adopted.
        proc = original(args, *rest, **{**kwargs, "close_fds": False})
        if not _job_assign(job, proc):
            # It is running and it cannot be owned. Ending it here is the only
            # answer that leaves nothing behind; falling back to the host tree
            # made the guarantee optional, which is the defect the tree exists
            # to close.
            _abandon(proc)
            raise TreeUnavailable(
                f"the browser driver could not be put in its job object, so "
                f"it was stopped rather than left owning a browser nothing "
                f"could account for (pid {proc.pid}).")
        tree = Tree(pid=proc.pid, token=identify(proc), group=name, kind="job")
        return proc, tree
    kwargs = {**kwargs, "start_new_session": True}
    proc = original(args, *rest, **kwargs)
    tree = Tree(pid=proc.pid, token=identify(proc),
                group=proc.pid, kind="pgid")
    return proc, tree


# ---- asking whether it is gone -------------------------------------------
def alive_tree(tree: Tree) -> bool:
    """Is anything at all still running in this tree?

    The leader is only one member. This is the question every status surface
    should be asking, because the browser outliving the Python process that
    started it is the whole failure mode.
    """
    if tree is None:
        return False
    if tree.kind == "job" and tree.group:
        known, active = _job_active(str(tree.group))
        # Unknown is not empty. A query that could not be answered leaves the
        # tree owned, because the alternative is declaring a live browser gone.
        return (not known) or active > 0
    if tree.kind == "pgid" and tree.group:
        if _WINDOWS:
            # A process group recorded on another platform - a data root shared
            # with a POSIX machine. Nothing here can evaluate it, and unknown
            # is owned.
            return True
        pgid = int(tree.group)
        if pgid == os.getpgrp():        # pragma: no cover - never ours to poll
            return True
        try:
            os.killpg(pgid, 0)
        except ProcessLookupError:
            return False
        except OSError:
            # Only ESRCH proves absence. EPERM means present but not ours, and
            # any other failure is unknown - which, as for a job, is owned.
            return True
        return _group_is_running(pgid)
    return alive(tree.pid, tree.token)


def prove_empty(tree: Tree, *, deadline: float = 5.0) -> bool:
    """Poll until nothing is left in the tree, or the deadline passes.

    Bounded polling rather than a blind sleep: there is no `wait()` to call on
    processes this one did not spawn, so the only honest answer comes from
    asking repeatedly whether any of them are still there.
    """
    end = time.monotonic() + deadline
    while True:
        if not alive_tree(tree):
            return True
        if time.monotonic() >= end:
            return False
        time.sleep(min(_POLL_SECONDS, max(0.0, end - time.monotonic())))


def owned(tree: Tree) -> bool:
    """Is it still safe to signal this tree?

    While the leader lives, its start time answers it outright. Once the leader
    is gone the group handle is all that is left, and the two platforms differ:
    a job object was created by us under a name nothing else uses, so it stays
    unambiguous forever, while a process-group id is a pid and can in principle
    be handed to a new session leader. That needs the pid to be recycled *and*
    the new owner to call setsid; what this can rule out is the recycling,
    by refusing the moment the leader slot holds a process that is not ours.
    """
    if tree.kind == "job" and tree.group:
        return True
    if tree.token is None:
        return False                    # never identified: nothing may be sent
    now = started_at(tree.pid)
    return now is None or now == tree.token


def signal_tree(tree: Tree, *, force: bool = False) -> bool:
    """Ask everything in the tree to end. True if the request was delivered.

    Re-checks ownership immediately before every signal, not once at the start:
    between a first ask and an escalation the leader can exit and the OS can
    hand its number to something else.
    """
    if tree is None or not tree.verified:
        raise Unverified(
            f"pid {getattr(tree, 'pid', None)} has no verified identity, so "
            f"Numera will not signal it: the number may belong to another "
            f"process by now.")
    if not owned(tree):
        return False
    if tree.kind == "job" and tree.group:
        # There is no graceful form. A job object is terminated or it is not,
        # and the call is idempotent, so both passes are the same one.
        return _job_terminate(str(tree.group))
    if tree.kind == "pgid" and tree.group and not _WINDOWS:
        pgid = int(tree.group)
        if pgid == os.getpgrp():        # pragma: no cover - never ours to kill
            return False
        try:
            os.killpg(pgid, signal.SIGKILL if force else signal.SIGTERM)
            return True
        except OSError:
            return False
    # No tree primitive: the leader is all there is. Windows can still walk the
    # live parent links, which finds descendants only while the leader is
    # around - the weakness this module exists to fix, kept as the fallback for
    # a child that could not be put in a job.
    if _WINDOWS:                                             # pragma: no cover
        return subprocess.run(["taskkill", "/T", "/F", "/PID", str(tree.pid)],
                              capture_output=True).returncode == 0
    try:                                                     # pragma: no cover
        os.kill(int(tree.pid), signal.SIGKILL if force else signal.SIGTERM)
        return True
    except OSError:                                          # pragma: no cover
        return False


def _report_unproved(tree, graceful, forced):
    if tree is not None and tree.kind == 'pgid' and not _WINDOWS:
        from .posix_session import report_group_failure
        report_group_failure(int(tree.group), graceful=graceful, forced=forced, verified=tree.verified)
    return False


def end_tree(tree: Tree, *, grace: float = 5.0, kill_grace: float = 5.0,
             ends_at: Optional[float] = None) -> bool:
    """End every process in the tree and prove it. False means one survived.

    For a tree this process did not spawn - a detached job being stopped by a
    later CLI run - which is why it takes a `Tree` and no `Popen`.
    """
    if ends_at is not None:
        if not alive_tree(tree):
            return True
        graceful = signal_tree(tree)
        left = max(0.0, ends_at - time.monotonic())
        if prove_empty(tree, deadline=left / 2.0):
            return True
        forced = signal_tree(tree, force=True)
        return (prove_empty(tree, deadline=max(0.0, ends_at - time.monotonic()))
                or _report_unproved(tree, graceful, forced))
    if not alive_tree(tree):
        return True
    graceful = signal_tree(tree)
    if not graceful and not alive_tree(tree):
        return True                     # refused because it had already gone
    if prove_empty(tree, deadline=grace):
        return True
    forced = signal_tree(tree, force=True)
    return prove_empty(tree, deadline=kill_grace) or _report_unproved(tree, graceful, forced)


def end_session(proc: subprocess.Popen, tree: Tree, *, grace: float = 5.0,
                kill_grace: float = 5.0, ends_at: Optional[float] = None) -> int:
    """End a session we spawned, and return the leader's exit code.

    The leader first, through `handshake.end_child`, which is what turns the
    signal into a real exit code and raises when even that cannot be proved.
    Then the part that was missing: the browser it started is not the process
    we hold a handle to, and it does not die just because its parent did. What
    survives is asked to leave, then killed, and only an empty tree makes this
    return - so a caller that drops its registry entry, releases the session
    or reports "stopped" is acting on the whole thing being gone.
    """
    if ends_at is not None:
        # Stop the owned group first. Waiting for its leader alone consumes
        # time without stopping descendants that still hold output writers.
        gone = end_tree(tree, ends_at=ends_at) if tree is not None else False
        left = max(0.0, ends_at - time.monotonic())
        try:
            code = end_child(proc, grace=left / 2.0, kill_grace=left / 2.0)
        except ChildTerminationFailed:
            raise
        if gone or tree is None:
            return code
        raise ChildTerminationFailed("owned processes remain after the cleanup deadline", proc.pid)
    code = end_child(proc, grace=grace, kill_grace=kill_grace)
    if prove_empty(tree, deadline=grace):
        return code
    if tree.verified:
        signal_tree(tree)
        if prove_empty(tree, deadline=grace):
            return code
        signal_tree(tree, force=True)
        if prove_empty(tree, deadline=kill_grace):
            return code
    raise ChildTerminationFailed(
        f"the session process (pid {proc.pid}) exited, but processes it "
        f"started are still running and may still hold the profile's browser "
        f"directory. Close them before launching this profile again.",
        proc.pid)


# ---- the tree this process itself owns -----------------------------------
#: Names the tree a spawned child belongs to. POSIX children can derive their
#: own process group; a Windows child cannot discover the name of the job it
#: was put in, and it needs that name to record who owns the profile.
TREE_ENV = "NUMERA_TREE"

#: The job handle a self-adopting process opened for itself. Held for the life
#: of the process on purpose: the job is set to kill on close, so this handle
#: closing - including when the process is killed - is what takes the browser
#: with it.
_SELF_JOB = []
_SELF_TREE = []

#: Serialises adoption. Adopting is not a pure read: on Windows it creates a
#: job object and on POSIX it moves the whole process into a new group, and two
#: threads opening their first browser at the same moment did both.
_ADOPTING = threading.Lock()


def _with_tree_env(kwargs: dict, tree: "Tree") -> dict:
    """Put the tree in the child's environment without disturbing the rest."""
    import json

    env = dict(kwargs.get("env") or os.environ)
    env[TREE_ENV] = json.dumps(tree.as_record())
    return {**kwargs, "env": env}


def own_tree() -> "Tree":
    """The tree *this* process owns, adopting one if it has none yet.

    A spawned session inherits its tree from whoever started it: on POSIX it is
    a session leader and its own process group id is the answer, and on Windows
    the name arrives in the environment, because a process cannot ask which job
    it is in by name.

    A foreground `numera launch` has no parent that did any of that - it is the
    user's own CLI process - and it is the case the audit called out: hard-kill
    it and the Chromium it started has no owner, no record and no lease, so
    nothing can even tell that the profile is still in use. So it adopts a tree
    for itself:

    * Windows gets a job object set to kill on close, held open here for the
      life of the process. That closes the case completely - killing the CLI
      takes the browser with it, because the OS does it.
    * POSIX uses the process group when this process already leads one, which
      is what an interactive shell gives every command it runs. When it does
      not - a script with job control off, cron, systemd, an application
      importing the Python API - it makes one, and takes the terminal with it
      when there is one to take, so Ctrl+C still arrives. See `_adopt_group`.

    It never falls back to "the leader alone". That was the 0.26 compromise and
    it reopened the exact window the claim exists to close: kill a
    non-group-leader launcher, and its Chromium survives with the leader dead,
    so the claim reads free and the next opener takes the profile. A browser
    session that cannot be given a real tree is refused instead.

    Cached: the answer must not change while the process runs, and on Windows
    calling it twice would make a second job.
    """
    if _SELF_TREE:
        return _SELF_TREE[0]
    with _ADOPTING:
        if _SELF_TREE:                  # another thread got here first
            return _SELF_TREE[0]
        tree = _inherited_tree() or _adopt_tree()
        _SELF_TREE.append(tree)
        return tree


def _inherited_tree() -> "Optional[Tree]":
    """The tree our parent put us in, if it said so and it is still real."""
    import json

    me = os.getpid()
    raw = os.environ.get(TREE_ENV)
    if raw:
        try:
            rec = json.loads(raw)
        except ValueError:                                   # pragma: no cover
            rec = {}
        kind, group = str(rec.get("kind") or ""), rec.get("group")
        if kind == "job" and group and _job_active(str(group))[1]:
            return Tree(pid=me, token=started_at(me), group=group, kind="job")
    if not _WINDOWS and os.getpgrp() == me:
        # `start_new_session=True` made us a session and group leader, so the
        # group is ours and every descendant inherits it.
        return Tree(pid=me, token=started_at(me), group=me, kind="pgid")
    return None


def _adopt_tree() -> "Tree":
    """Make this process the owner of a tree it did not inherit."""
    me = os.getpid()
    if _WINDOWS:
        name = f"numera-tree-{uuid.uuid4().hex}"
        job = _job_open(name)
        if job is not None and _job_assign_self(job):
            _SELF_JOB.append(job)       # never closed; see `_SELF_JOB`
            return Tree(pid=me, token=started_at(me), group=name, kind="job")
        if job is not None:                                  # pragma: no cover
            _kernel32().CloseHandle(job)
        raise TreeUnavailable(
            "this process could not create the job object that would own the "
            "browser it is about to start, so the browser was not started: "
            "nothing could have proved it had stopped.")
    if not _adopt_group():
        raise TreeUnavailable(
            "this process could not be given a process group of its own, so "
            "the browser it was about to start would have had no owner that "
            "outlives it. Run it from a shell with job control, or through "
            "`--detach` or the API, both of which start their own session.")
    return Tree(pid=me, token=started_at(me), group=os.getpgrp(), kind="pgid")


def others_alive(tree: "Tree", besides: int) -> bool:
    """Is anything in the tree still running apart from `besides`?

    The question a session owner asks on its way out. `alive_tree` cannot
    answer it for a tree the asker is itself part of - it would always say yes -
    and "the browser closed" is not the same as "nothing it started is left".
    """
    if tree is None:
        return False
    if tree.kind == "job" and tree.group:
        known, active = _job_active(str(tree.group))
        return (not known) or active > 1
    if tree.kind == "pgid" and tree.group and not _WINDOWS:
        return _group_is_running(int(tree.group), ignore=besides)
    return False
