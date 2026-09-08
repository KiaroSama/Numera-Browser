"""Who owns a profile's browser, written down so it outlives the process.

The session lease in `paths.py` is an OS file lock, and that is exactly its
weakness: the OS drops it when the holder dies. Numera 0.25 made the *local*
owner aware that a browser can outlive its launcher - the server's registry and
a detached job record both keep the tree - but neither is consulted by a
different process about to open the same profile. So:

1. a launcher dies (crash, `kill -9`, a closed laptop lid);
2. its Chromium survives in the process group;
3. the file lease is released by the kernel;
4. any other Numera process takes the free lease and opens the same
   `user-data/<id>` in a second browser.

That is the corruption the lease exists to prevent, reached by outliving it.

So ownership is also written down. A claim is a small file naming the profile's
owner *and the tree it owns* - a Windows job object, a POSIX process group -
created before the browser can be opened and removed only once nothing that
session started is left. A later opener reads it and asks the OS, not the
claim, whether that tree is still occupied:

* occupied -> refused, whoever holds it and whether or not their leader lives;
* provably empty -> the claim is stale and is recovered;
* cannot be established either way -> refused. A claim that cannot be checked
  is the one case where guessing costs a corrupted profile, so it fails closed.

The claim is not a lock. The lease still serialises two openers racing in the
same instant; this answers the different question of whether the *previous*
owner is really finished, which a lock cannot express once its holder is gone.
"""

from __future__ import annotations

import json
import os
import platform
import time
from contextlib import contextmanager
from pathlib import Path
from typing import Optional

from . import proctree
from .paths import SessionBusy, id_path, transaction_lock, write_json


def _dir(root) -> Path:
    d = Path(root) / "sessions"
    d.mkdir(parents=True, exist_ok=True)
    return d


def _path(root, profile_id: str) -> Path:
    return id_path(_dir(root), profile_id, ".json")


def read(root, profile_id: str) -> Optional[dict]:
    """The claim on this profile, or None. No judgement, just the record."""
    try:
        path = _path(root, profile_id)
    except ValueError:
        return None
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return None
    return data if isinstance(data, dict) else None


def _tree_of(record: dict) -> proctree.Tree:
    tree = (record or {}).get("tree") or {}
    return proctree.Tree(pid=int((record or {}).get("pid") or 0),
                         token=(record or {}).get("token"),
                         group=tree.get("group"),
                         kind=str(tree.get("kind") or ""))


def state(record: Optional[dict]) -> str:
    """`"free"`, `"busy"`, or `"unknown"` for a claim record.

    `"unknown"` is not a shade of free. It means the record names an owner this
    process cannot ask about - another machine's pid on a shared data root, or
    a record with no identity in it at all - and every caller treats it as busy.
    Silence about ownership is the one thing that must never read as available.
    """
    if not record:
        return "free"
    if record.get("host") and record["host"] != platform.node():
        return "unknown"
    tree = _tree_of(record)
    if not tree.pid and not tree.group:
        return "unknown"
    if tree.kind == "" and tree.token is None:
        return "unknown"                # a bare pid is not an identity
    return "busy" if proctree.alive_tree(tree) else "free"


def busy(root, profile_id: str) -> bool:
    """Does something outside this process still own this profile's browser?

    The question a status surface asks. It is deliberately not "is the lease
    held": a lease dies with its holder, and the browser does not.
    """
    record = read(root, profile_id)
    if not record:
        return False
    if _is_ours(record):
        return False                    # ours; the caller knows its own state
    return state(record) != "free"


def _is_ours(record: dict) -> bool:
    """Is this claim written by *this* process, on this machine?

    The host has to match before the pid means anything. Two machines sharing a
    data root hand out the same pid numbers, so a claim from another one can
    carry our own number - and reading that as "ours, so not busy" would let
    this process open a profile a different machine has a browser on.
    """
    if record.get("host") and record["host"] != platform.node():
        return False
    return (record.get("pid") == os.getpid()
            and record.get("token") == proctree.started_at(os.getpid()))


def describe(record: Optional[dict]) -> str:
    """One line about a claim, for an error a person has to act on."""
    if not record:
        return "no session"
    what = record.get("what") or "a session"
    pid = record.get("pid")
    if state(record) == "unknown":
        return (f"{what} claimed by pid {pid} on {record.get('host') or 'another machine'}, "
                f"which this process cannot verify")
    return f"{what} started by pid {pid}, whose browser is still running"


@contextmanager
def owned(root, profile_id: str, tree: proctree.Tree, what: str = "a session"):
    """Claim this profile for `tree`, and give the claim up when it is finished.

    Taken inside the caller's session lease, so two openers cannot race here;
    the claim is what survives the lease.
    """
    take(root, profile_id, tree, what)
    try:
        yield
    finally:
        release(root, profile_id, tree)


def take(root, profile_id: str, tree: proctree.Tree, what: str = "a session") -> dict:
    """Record this process as the owner, or raise `SessionBusy`.

    The check and the write are one transaction, so two processes that both
    find the previous claim stale cannot both recover it.
    """
    with transaction_lock(root):
        current = read(root, profile_id)
        if current is not None:
            standing = state(current)
            if standing != "free":
                raise SessionBusy(
                    f"this profile is still owned: {describe(current)}. "
                    f"Stop it first - `numera stop <profile>` - or wait for it "
                    f"to finish.")
        record = {
            "pid": tree.pid or os.getpid(),
            "token": tree.token,
            "tree": tree.as_record(),
            "host": platform.node(),
            "what": what,
            "started": time.time(),
        }
        write_json(_path(root, profile_id), record)
        return record


def release(root, profile_id: str, tree: proctree.Tree) -> bool:
    """Give up our claim, unless something we started is still running.

    True when the claim was removed. False means the session is over for *this*
    process but the tree it started is not empty - a browser that outlived the
    launcher - and the claim stays so the next opener still finds the profile
    owned. That is the whole point of writing it down.
    """
    me = os.getpid()
    with transaction_lock(root):
        current = read(root, profile_id)
        if not current or current.get("pid") not in (tree.pid, me):
            return False                # not ours to remove
        if proctree.others_alive(tree, besides=me):
            return False
        try:
            _path(root, profile_id).unlink(missing_ok=True)
        except OSError:                                      # pragma: no cover
            return False
        return True


def clear(root, profile_id: str) -> bool:
    """Drop a claim whose tree is provably gone. True if one was removed.

    For the recovery paths - a stop that proved the tree empty, a delete. It
    refuses a claim that is still occupied or cannot be checked, so it can
    never be the thing that lets a second browser in.
    """
    with transaction_lock(root):
        current = read(root, profile_id)
        if not current or state(current) != "free":
            return False
        try:
            _path(root, profile_id).unlink(missing_ok=True)
        except OSError:                                      # pragma: no cover
            return False
        return True


def refuse_if_busy(root, profile_id: str) -> None:
    """Raise `SessionBusy` when something still owns this profile's browser.

    For the operations that move or replace `user-data/<id>` - delete, backup,
    restore. Each already takes the session lease, and the lease is exactly
    what a killed launcher does not leave behind: its Chromium goes on holding
    the directory while the lock reads free, so a delete would rename a live
    profile out from under a running browser and a backup would zip a database
    being written.
    """
    if busy(root, profile_id):
        raise SessionBusy(
            f"this profile's browser is still open: "
            f"{describe(read(root, profile_id))}. Stop it first.")
