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
import subprocess
import sys
import time
import uuid
from pathlib import Path

from . import proctree
# Re-exported: the identity primitives moved to `proctree` with the rest of
# process ownership, and both names are part of this module's surface.
from .proctree import alive, started_at   # noqa: F401
from .paths import child_env, write_json
from typing import Optional

# How long a record may sit with no pid and no outcome before it is called lost.
# It only covers the gap between Popen returning and the parent's write landing.
_STARTUP_GRACE = 30.0


def _dir(root: Path) -> Path:
    d = Path(root) / "jobs"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _outcome_path(record: Path) -> Path:
    """Where a job's own exit record lives.

    Not ``.json``: `listing()` globs for those and would read this as a second
    job. Deliberately a different *file* rather than a lock, because the two
    writers are different processes - the parent recording the pid it spawned,
    and the child recording how it ended. A read-modify-write cannot be made
    safe between them with an in-process lock, and it was not safe: the parent
    read the record before `Popen` returned, a fast child wrote its outcome, and
    then the parent's write put back a record with no outcome in it. The job
    showed as running until the startup grace expired and then as "lost", having
    actually finished. Separate files, merged on read, and neither writer can
    lose the other's fields.
    """
    return record.with_suffix(".exit")


def _read_record(path: Path) -> dict:
    """The job's identity merged with its outcome, or ``{}``."""
    data = _read_json(path)
    if not data:
        return {}
    return {**data, **_read_json(_outcome_path(path))}


def _read_json(path: Path) -> dict:
    if not path.exists():
        return {}
    try:
        loaded = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return loaded if isinstance(loaded, dict) else {}


def _write(path: Path, patch: dict) -> dict:
    """Fold ``patch`` into the file at ``path`` and write it atomically.

    Safe as a read-modify-write only because each path here has a single
    writing process; see `_outcome_path`.
    """
    data = {**_read_json(path), **patch}
    write_json(path, data)
    return data


def child_argv(argv: list[str], root: Path) -> list[str]:
    """The argument list the detached copy should run.

    Drops ``--detach`` (or the job would fork forever) and pins the data
    directory, so the child resolves the same root as its parent even though it
    starts in a different working directory.

    **Also drops the master password.** `numera --password SECRET launch p
    --detach` used to hand `--password SECRET` straight to the child, where it
    sat in the process table for every local user to read - and `spawn()`
    records the argv it ran, so the same secret was written into
    `jobs/<id>.json` in the clear and stayed there. The unlock reaches the child
    by environment instead; see `spawn`.
    """
    rest, skip = [], False
    for arg in argv:
        if skip:                       # the value that followed a bare --password
            skip = False
            continue
        if arg == "--detach":
            continue
        if arg == "--password":
            skip = True
            continue
        if arg.startswith("--password="):
            continue
        rest.append(arg)
    if not any(a == "--data-dir" or a.startswith("--data-dir=") for a in rest):
        rest = ["--data-dir", str(root), *rest]
    return rest


def spawn(root: Path, argv: list[str], command: str = "",
          profile: Optional[str] = None, password: Optional[str] = None) -> dict:
    """Start ``argv`` as a background job and return its record.

    ``command`` is the subcommand's name. It is passed in rather than sniffed
    out of ``argv``, because the first bare word there is whatever value a
    global option happened to take.
    """
    jid = uuid.uuid4().hex[:12]
    d = _dir(root)
    rec_path = d / f"{jid}.json"
    log = d / f"{jid}.log"
    # The unlock travels by environment, never argv - and `argv` below is
    # persisted to the job record, so anything left in it is written to disk.
    env = {**child_env(password), "NUMERA_JOB_FILE": str(rec_path)}
    # The child's output goes to the log, never to our stdout: a --json caller
    # is parsing that stream and a background process writing into the middle of
    # it is the whole reason structured output was worth having.
    with log.open("wb") as fh:
        # Not a bare `Popen`: a detached `numera launch` owns a browser that
        # this record has to be able to end from a completely different
        # process, and the leader's pid is not a handle on it. See proctree.
        proc, tree = proctree.spawn(
            [sys.executable, "-m", "numera", *argv],
            stdout=fh, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
            env=env,
        )
    if not tree.verified:
        code = proc.poll()
        if code is not None:
            # It was gone before it could be named. How it ended is knowable
            # right now and never will be again, so it is recorded here - and
            # because the outcome is recorded, nothing later has any reason to
            # signal a pid that already belongs to somebody else.
            _write(_outcome_path(rec_path),
                   {"state": "done" if code == 0 else "failed",
                    "exit": int(code), "finished": time.time()})
    return _read_record_after(rec_path, {
        "id": jid,
        "argv": list(argv),
        "command": command,
        "profile": profile,
        "pid": proc.pid,
        # Recorded with the pid, not derived later: by the time anything asks,
        # the process may be gone and the number handed to someone else.
        "pidToken": tree.token,
        # What actually owns the browser: a POSIX process group, or a Windows
        # job object. Stored so a later `numera jobs stop` - a different
        # process, minutes later - ends the tree rather than the leader.
        "tree": tree.as_record(),
        # Whether this record may authorise a signal at all. A pid whose start
        # time could not be read is not an identity, and `stop` refuses it
        # rather than risk terminating whatever now holds that number.
        "verified": tree.verified,
        "log": str(log),
        "started": time.time(),
    })


def _read_record_after(rec_path: Path, identity: dict) -> dict:
    """Write the spawning half of the record and return the merged view."""
    _write(rec_path, identity)
    return _read_record(rec_path)


def record_exit(job_file: str, code: int) -> None:
    """Called by the job itself as it exits, via ``NUMERA_JOB_FILE``.

    Writes the sibling outcome file, never the record the parent owns - a job
    that finishes before `spawn()` has written the pid used to have its outcome
    overwritten by that write.
    """
    _write(_outcome_path(Path(job_file)), {
        "state": "done" if code == 0 else "failed",
        "exit": int(code),
        "finished": time.time(),
    })


def _decorate(rec: dict) -> dict:
    """Add the live state a stored record cannot know on its own.

    The leader is one process in a tree. A `numera launch` owns a browser that
    can outlive it, and that browser holds the profile's user-data directory
    whether or not the Python process that opened it is still there - so a
    record whose leader is gone is not necessarily a finished job, and calling
    it one is what let the next launch open a second browser on the same
    directory. The tree is asked as well, and only where the group handle is
    still unambiguously ours.
    """
    rec = dict(rec)
    tree = proctree.Tree.from_record(rec)
    leader = alive(rec.get("pid") or 0, rec.get("pidToken"))
    survivors = (not leader and tree.verified and proctree.owned(tree)
                 and proctree.alive_tree(tree))
    if not rec.get("state"):
        if leader or survivors:
            rec["state"] = "running"
        elif rec.get("pid") is None and time.time() - rec.get("started", 0) < _STARTUP_GRACE:
            rec["state"] = "starting"
        else:
            # It went away without recording an outcome — killed, or crashed
            # hard enough to skip its own exit hook.
            rec["state"] = "lost"
    if survivors:
        # The job finished and left something running. Reported rather than
        # hidden behind its stored outcome, and `running` stays true so
        # `prune` will not drop the record that says how to stop it.
        rec["orphaned"] = True
    rec["running"] = rec["state"] in ("running", "starting") or survivors
    return rec


def get(root: Path, jid: str) -> Optional[dict]:
    rec = _read_record(_dir(root) / f"{jid}.json")
    return _decorate(rec) if rec else None


def listing(root: Path) -> list[dict]:
    out = []
    for f in sorted(_dir(root).glob("*.json")):
        rec = _read_record(f)
        if rec:
            out.append(_decorate(rec))
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
        tree = proctree.Tree.from_record(rec)
        if not tree.verified:
            # No start-time token and no group handle, so this record names a
            # number and nothing else. The OS reuses pids; signalling it now
            # could kill whatever was given that number since. A missing
            # identity is not permission - it is the reason to refuse.
            raise RuntimeError(
                f"job {jid} cannot be stopped safely: Numera never captured a "
                f"verified identity for pid {rec['pid']}, so that number may "
                f"belong to an unrelated process by now. End it by hand, then "
                f"run `numera jobs prune`.")
        if not proctree.end_tree(tree):
            # Not stopped. Writing the outcome anyway is what made `jobs list`
            # say "stopped" for a browser that was still open and still owned
            # the profile - so the record stays as it is, and the caller is
            # told rather than reassured.
            raise RuntimeError(
                f"job {jid} (pid {rec['pid']}) did not stop: something it "
                f"started is still running and may still hold its profile's "
                f"browser directory. Stop it by hand and re-run this once it "
                f"is gone.")
        # The signal is a hard terminate on both platforms, so the job never
        # reaches its own exit hook — this side has to record the outcome.
        # The outcome file, like the job's own exit hook: this is an outcome,
        # and the record file belongs to whoever spawned it.
        _write(_outcome_path(_dir(root) / f"{jid}.json"),
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
        record = d / f"{rec['id']}.json"
        record.unlink(missing_ok=True)
        _outcome_path(record).unlink(missing_ok=True)
        log = rec.get("log")
        if log:
            Path(log).unlink(missing_ok=True)
        n += 1
    return n
