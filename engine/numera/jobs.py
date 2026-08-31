"""
Job handles for the operations that take minutes.

Launching a browser, warming a profile and attaching DevTools all block for as
long as the session is open. That is right at a prompt and useless to a script:
an agent needs to start the work, get a handle back, and poll it. ``--detach``
on any command re-runs it in a background process and records it here, so
``numera jobs`` can report on it and ``numera stop`` can end it.

The HTTP API has its own handle for the same thing — ``server.py`` keeps a
``running`` dict of subprocesses keyed by profile id — but that lives in one
server process's memory, and every CLI invocation is a fresh process with no
memory at all. Hence a registry on disk: one JSON file per job under
``<data-dir>/jobs``, next to the job's captured output.

Two processes write each record and they never write the same keys: the parent
writes the job's identity once at spawn, the child writes how it ended once at
exit. Both merge rather than overwrite, so the order they land in does not
matter.
"""

from __future__ import annotations

import json
import hashlib
import os
import signal
import subprocess
import sys
import time
import uuid
from pathlib import Path
from typing import Optional

# How long a record may sit with no pid and no outcome before it is called lost.
# It only covers the gap between Popen returning and the parent's write landing.
_STARTUP_GRACE = 30.0


def _dir(root: Path) -> Path:
    d = Path(root) / "jobs"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _merge(path: Path, patch: dict) -> dict:
    """Fold ``patch`` into the record at ``path``, keeping what is already there.

    ponytail: read-modify-write with no lock. The only two writers are the
    parent at spawn and the child at exit, and their key sets are disjoint, so
    the worst a collision can cost is one lost field. Add a lock if a third
    writer ever appears.
    """
    data: dict = {}
    if path.exists():
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            data = {}
    data.update(patch)
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")
    return data


def started_at(pid: int) -> "Optional[int]":
    """When this pid's process started, or None if it cannot be read.

    A pid on its own is not an identity: the OS reuses them, and this module
    hands stored pids to `taskkill /T /F`, which would take down whatever tree
    now happens to own that number. The start time is what makes the pair
    unique - two processes can share a pid, never at the same instant.

    Standard library only: the engine declares no required dependencies, and
    pulling in psutil for one field would end that.
    """
    if not pid:
        return None
    try:
        if os.name == "nt":
            import ctypes
            import ctypes.wintypes as wt

            PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
            k32 = ctypes.windll.kernel32
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
    if os.name == "nt":
        # os.kill(pid, 0) is NOT a liveness probe on Windows: CPython maps every
        # signal but CTRL_C/CTRL_BREAK to TerminateProcess, so the "harmless"
        # POSIX idiom would kill the very job being polled.
        import ctypes

        SYNCHRONIZE = 0x00100000
        WAIT_TIMEOUT = 0x00000102
        k32 = ctypes.windll.kernel32
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


def _terminate_tree(pid: int, token=None) -> bool:
    """End the job and everything it started.

    A detached ``numera launch`` owns a Chromium that outlives it, so signalling
    the job alone leaks a browser. Windows has ``taskkill /T`` for exactly this;
    elsewhere the job was started in its own process group and the group gets
    the signal.
    """
    # Refuse to touch a pid that is no longer the process we recorded.
    if token is not None and started_at(pid) != token:
        return False
    if os.name == "nt":
        return subprocess.run(["taskkill", "/T", "/F", "/PID", str(pid)],
                              capture_output=True).returncode == 0
    try:
        os.killpg(os.getpgid(int(pid)), signal.SIGTERM)
        return True
    except OSError:
        try:
            os.kill(int(pid), signal.SIGTERM)
            return True
        except OSError:
            return False


def child_argv(argv: list[str], root: Path) -> list[str]:
    """The argument list the detached copy should run.

    Drops ``--detach`` (or the job would fork forever) and pins the data
    directory, so the child resolves the same root as its parent even though it
    starts in a different working directory.
    """
    rest = [a for a in argv if a != "--detach"]
    if not any(a == "--data-dir" or a.startswith("--data-dir=") for a in rest):
        rest = ["--data-dir", str(root), *rest]
    return rest


def spawn(root: Path, argv: list[str], command: str = "",
          profile: Optional[str] = None) -> dict:
    """Start ``argv`` as a background job and return its record.

    ``command`` is the subcommand's name. It is passed in rather than sniffed
    out of ``argv``, because the first bare word there is whatever value a
    global option happened to take.
    """
    jid = uuid.uuid4().hex[:12]
    d = _dir(root)
    rec_path = d / f"{jid}.json"
    log = d / f"{jid}.log"
    env = {**os.environ, "NUMERA_JOB_FILE": str(rec_path)}
    # The child's output goes to the log, never to our stdout: a --json caller
    # is parsing that stream and a background process writing into the middle of
    # it is the whole reason structured output was worth having.
    with log.open("wb") as fh:
        proc = subprocess.Popen(
            [sys.executable, "-m", "numera", *argv],
            stdout=fh, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
            env=env, start_new_session=True,   # POSIX only; ignored on Windows
        )
    return _merge(rec_path, {
        "id": jid,
        "argv": list(argv),
        "command": command,
        "profile": profile,
        "pid": proc.pid,
        # Recorded with the pid, not derived later: by the time anything asks,
        # the process may be gone and the number handed to someone else.
        "pidToken": started_at(proc.pid),
        "log": str(log),
        "started": time.time(),
    })


def record_exit(job_file: str, code: int) -> None:
    """Called by the job itself as it exits, via ``NUMERA_JOB_FILE``."""
    _merge(Path(job_file), {
        "state": "done" if code == 0 else "failed",
        "exit": int(code),
        "finished": time.time(),
    })


def _decorate(rec: dict) -> dict:
    """Add the live state a stored record cannot know on its own."""
    rec = dict(rec)
    if not rec.get("state"):
        if alive(rec.get("pid") or 0, rec.get("pidToken")):
            rec["state"] = "running"
        elif rec.get("pid") is None and time.time() - rec.get("started", 0) < _STARTUP_GRACE:
            rec["state"] = "starting"
        else:
            # It went away without recording an outcome — killed, or crashed
            # hard enough to skip its own exit hook.
            rec["state"] = "lost"
    rec["running"] = rec["state"] in ("running", "starting")
    return rec


def get(root: Path, jid: str) -> Optional[dict]:
    path = _dir(root) / f"{jid}.json"
    if not path.exists():
        return None
    try:
        return _decorate(json.loads(path.read_text(encoding="utf-8")))
    except json.JSONDecodeError:
        return None


def listing(root: Path) -> list[dict]:
    out = []
    for f in sorted(_dir(root).glob("*.json")):
        try:
            out.append(_decorate(json.loads(f.read_text(encoding="utf-8"))))
        except json.JSONDecodeError:
            continue
    return sorted(out, key=lambda r: r.get("started", 0))


def tail(rec: dict, lines: int = 20) -> list[str]:
    """The last few lines the job printed, for a caller polling it."""
    log = rec.get("log")
    if not log or not Path(log).exists():
        return []
    text = Path(log).read_text(encoding="utf-8", errors="replace")
    return text.splitlines()[-lines:]


def stop(root: Path, jid: str) -> Optional[dict]:
    """Terminate a job. Returns the updated record, or None if there isn't one."""
    rec = get(root, jid)
    if rec is None:
        return None
    if rec["running"] and rec.get("pid"):
        _terminate_tree(rec["pid"], rec.get("pidToken"))
        # The signal is a hard terminate on both platforms, so the job never
        # reaches its own exit hook — this side has to record the outcome.
        _merge(_dir(root) / f"{jid}.json",
               {"state": "stopped", "finished": time.time()})
    return get(root, jid)


def stop_profile(root: Path, profile_id: str) -> list[dict]:
    """Stop every running job for one profile (the CLI's answer to POST /stop)."""
    return [r for r in
            (stop(root, j["id"]) for j in listing(root)
             if j.get("profile") == profile_id and j["running"])
            if r]


def prune(root: Path) -> int:
    """Delete the records and logs of jobs that have finished. Returns the count."""
    n = 0
    d = _dir(root)
    for rec in listing(root):
        if rec["running"]:
            continue
        (d / f"{rec['id']}.json").unlink(missing_ok=True)
        log = rec.get("log")
        if log:
            Path(log).unlink(missing_ok=True)
        n += 1
    return n
