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

**The ready point** is the moment the child owns the session and has
revalidated what it is about to launch: the lease is held, and the current
profile and its stored credential were re-read under the root transaction. That
is `drivers.open_session`, the single gate every browser open goes through,
which is why the signal is emitted there rather than in each command.
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

#: How long a parent waits for that result. The ready point is reached before
#: any browser starts, so this covers a Python start-up and a few store reads,
#: not a Chromium launch. Generous on purpose: timing out kills a child that
#: was about to succeed.
READY_TIMEOUT = 60.0

_POLL_SECONDS = 0.05


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
    """Block until the child owns its session, or raise `StartupFailed`.

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


def terminate(proc: subprocess.Popen) -> None:
    """End a child that must not be left running, politely then not."""
    if proc.poll() is not None:
        return
    try:
        proc.terminate()
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    except OSError:                                          # pragma: no cover
        pass
