"""A spawned session tells its parent when it actually owns the profile.

The API and the scheduler run a browser by spawning ``python -m numera launch``
(or ``warmup``, or ``attach``) and registering the child as the owner the
instant ``Popen`` returned. ``Popen`` proves a process started, nothing more.
The child takes the session lease and re-reads the vault seconds later, so the
parent acknowledged "running" for launches that were about to exit with
``SessionBusy`` or ``VaultLocked``; two simultaneous requests spawned two
children and the loser overwrote ``running[pid]``, which is the process
``/stop`` then terminated instead of the one holding the window; and the
scheduler wrote ``lastRun`` for occurrences whose browser never opened.

So the child says when it is ready, and the parent waits for it.

The channel is a file named by ``NUMERA_READY_FILE`` in the child's
environment. A pipe would have to be drained for the whole life of a browser
session or the child blocks on a full buffer, and an inherited handle is not
portable to Windows without extra care; a file is neither. The child writes one
small JSON object exactly once, atomically, and the parent polls for it against
a deadline while watching for the process to die - so neither side can wait
forever, and neither needs the other to be alive.

**The ready point** is the moment a browser is actually open on the profile.
It used to stop one step short of that: `drivers.open_session` announced
success as soon as it held the lease and had re-read the profile, and *then*
yielded to the caller that starts the browser. So the parent returned 200,
registered the child and let the scheduler stamp `lastRun` for sessions that
went on to die on a missing browser executable, a backend that would not
import, an invalid launch option, a headful launch with no display or a
persistent context that would not open. Owning the profile is a precondition
for opening a browser, not evidence that one opened.

Ownership is still taken first and held for the whole session; only the claim
of readiness moved to the far side of the browser starting. Every backend in
`drivers.py` calls `drivers._opened()` the instant it holds a live persistent
context and page, and `open_session` announces failure only for what goes wrong
before that.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Optional

#: Names the file a child writes its startup result to. Absent for a child
#: nobody is waiting on - a plain CLI run - which makes the signal a no-op.
READY_ENV = "NUMERA_READY_FILE"

#: How long a parent waits for that result. It spans a real browser start now -
#: a cold Chromium with extensions, on a machine already running several - and
#: not just a Python start-up and a few store reads. A ceiling with room in it
#: on purpose: timing out kills a child that was about to succeed.
READY_TIMEOUT = 120.0

_POLL_SECONDS = 0.05


class ChildTerminationFailed(RuntimeError):
    """A child could not be proved dead, so nothing may act as though it is.

    `end_child` used to answer `None` for this - a permission error on the
    signal, or a process still unreaped after the kill - and every caller read
    that as an ordinary exit. So `/stop` returned success for a browser that
    was still running, a schedule recorded "warm-up exited None", and the
    registry entry was dropped while the child still held the profile's session
    lease, which is the one state that makes the next launch fail for a reason
    nobody can act on.

    Not being able to kill something is rare. Reporting it as a clean stop is
    worse than reporting it at all.
    """

    def __init__(self, message: str, pid=None):
        self.pid = pid
        super().__init__(message)


class StartupFailed(RuntimeError):
    """A spawned session never reached the point of owning its profile.

    `kind` says which, so an HTTP caller can map it to the right status rather
    than turning every startup problem into a 500:

    ``busy``    another process owns the session;
    ``locked``  the vault cannot read the profile's proxy password;
    ``gone``    the profile was deleted before the child could open it;
    ``timeout`` no answer within the deadline;
    ``failed``  anything else, including a child that died silently.
    """

    def __init__(self, message: str, kind: str = "failed"):
        super().__init__(message)
        self.kind = kind


# ---- child side ----------------------------------------------------------
def announce(ok: bool, error: str = "", kind: str = "failed") -> None:
    """Report startup to whoever spawned us. Without a parent, a no-op.

    Written to a sibling temp name and renamed, so a parent polling the path
    reads either nothing or the whole object - never a half-written one.

    A failure to write is swallowed. The handshake exists to stop the parent
    believing a lie; making a healthy browser session die because its status
    file could not be written would be a worse trade than the parent falling
    back on its own deadline.
    """
    target = os.environ.get(READY_ENV)
    if not target:
        return
    path = Path(target)
    payload = {"ok": bool(ok), "error": str(error)[:500], "kind": kind}
    try:
        tmp = path.with_name(f"{path.name}.{os.getpid()}.part")
        tmp.write_text(json.dumps(payload), encoding="utf-8")
        os.replace(tmp, path)
    except OSError:                                          # pragma: no cover
        pass


def reported() -> bool:
    """Has a startup result already been written for this session?

    The failure path needs it. Readiness is announced by the backend once the
    browser is open, so an error raised *after* that - a startup URL that will
    not load, a window closing badly - must not overwrite a success the parent
    has already acted on. `drivers.open_session` reports a failure only when
    this says nothing has been reported yet.

    A file rather than a flag: the result is per session and per process, the
    write is atomic, and the path therefore exists only once a whole object is
    there.
    """
    target = os.environ.get(READY_ENV)
    return bool(target) and Path(target).exists()


# ---- parent side ---------------------------------------------------------
@contextmanager
def ready_channel():
    """A one-shot path for one child's startup result, removed afterwards.

    Its own directory, so the child's temporary write cannot collide with
    anything and cleanup is a single tree removal.
    """
    directory = tempfile.mkdtemp(prefix="numera-ready-")
    try:
        yield Path(directory) / "ready.json"
    finally:
        shutil.rmtree(directory, ignore_errors=True)


def _result(path: Path) -> Optional[dict]:
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):                            # pragma: no cover
        return None
    return data if isinstance(data, dict) else None


def await_ready(proc: subprocess.Popen, path: Path,
                timeout: float = READY_TIMEOUT) -> None:
    """Block until the child has a browser open, or raise `StartupFailed`.

    Three ways out, and every one of them is bounded: the child answers, the
    child dies, or the deadline passes. The process is re-checked for a written
    result after it is found dead, because a child that reports a failure and
    exits does both within microseconds of each other and the specific reason
    is worth more than "it exited".
    """
    deadline = time.monotonic() + timeout
    while True:
        answer = _result(path)
        if answer is not None:
            if answer.get("ok"):
                return
            raise StartupFailed(answer.get("error") or "the session failed to start",
                                answer.get("kind") or "failed")
        code = proc.poll()
        if code is not None:
            answer = _result(path)
            if answer is not None and not answer.get("ok"):
                raise StartupFailed(answer.get("error") or "the session failed to start",
                                    answer.get("kind") or "failed")
            if answer is not None:
                return                                       # ready, then exited
            raise StartupFailed(
                f"the session process exited with code {code} before it opened "
                f"the profile", "failed")
        if time.monotonic() >= deadline:
            raise StartupFailed(
                f"the session did not report itself started within "
                f"{timeout:.0f}s", "timeout")
        time.sleep(_POLL_SECONDS)


def end_child(proc: subprocess.Popen, *, grace: float = 5.0,
              kill_grace: float = 5.0) -> Optional[int]:
    """End a child that must not be left running, and return how it ended.

    Politely, then not - and then **wait again**, which is the part that was
    missing. The old version escalated to `kill()` on timeout and returned
    immediately, so every caller carried on while the child was still dying:

    * `stop_session` passed `proc.poll()` straight into the finalisation path,
      where it is `None` for a child that has not been reaped yet - so a
      schedule recorded "warm-up exited None" and the registry entry was
      dropped while the child still held the profile's session lease;
    * `/stop` therefore returned success on a profile that was still busy, and
      the next launch refused with `SessionBusy` for a browser the caller had
      just been told was stopped;
    * on POSIX the child stayed unreaped, because nothing ever waited on it.

    A signal is a request. Only `wait()` is the answer, so this returns the
    real exit code, and returns it only once the process is gone.

    It never returns `None`. Every path that cannot prove death raises
    `ChildTerminationFailed` instead - a signal that was refused, or a process
    still unreaped after the kill. Returning `None` for those made "I could not
    stop it" and "it exited" the same value, and every caller chose the second
    reading: the registry entry was dropped, `/stop` answered success, and the
    schedule recorded an exit code of `None` for a browser that was still up.
    """
    code = proc.poll()
    if code is not None:
        return code                     # already gone; nothing to escalate

    try:
        proc.terminate()
    except OSError as err:                                   # pragma: no cover
        code = proc.poll()
        if code is not None:
            return code                 # it died between the poll and the signal
        raise ChildTerminationFailed(
            f"the session process could not be signalled and is still "
            f"running: {err}", getattr(proc, "pid", None)) from err
    try:
        return proc.wait(timeout=grace)
    except subprocess.TimeoutExpired:
        pass                            # it ignored the polite ask; escalate

    try:
        proc.kill()
    except OSError as err:                                   # pragma: no cover
        code = proc.poll()
        if code is not None:
            return code
        raise ChildTerminationFailed(
            f"the session process refused the kill and is still running: {err}",
            getattr(proc, "pid", None)) from err
    try:
        return proc.wait(timeout=kill_grace)
    except subprocess.TimeoutExpired as err:
        raise ChildTerminationFailed(
            f"the session process was killed but had not exited after "
            f"{kill_grace:.0f}s, so it may still hold the profile",
            getattr(proc, "pid", None)) from err


