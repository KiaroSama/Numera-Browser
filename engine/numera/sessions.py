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
import uuid
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


#: What a claim file that cannot be parsed comes back as. It is not `None`:
#: "there is no claim" and "there is a claim and I cannot read it" were the
#: same answer, and `take()` read both as an empty slot - so a corrupt file,
#: which may be the only surviving record of a browser whose launcher died, was
#: overwritten by the next opener.
UNREADABLE = object()


def read(root, profile_id: str):
    """The claim on this profile: a dict, `None` for none, or `UNREADABLE`.

    Three states, because they are three different facts. Anything that cannot
    be turned into a claim record - bad JSON, a truncated write, a directory
    where a file should be, a list instead of an object - is the third one, and
    every caller treats it as owned until a person repairs it.
    """
    try:
        path = _path(root, profile_id)
    except ValueError:
        return None
    try:
        if not path.exists():
            return None
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return UNREADABLE
    return data if isinstance(data, dict) else UNREADABLE


def _tree_of(record: dict) -> Optional[proctree.Tree]:
    """The tree a claim names, or None when the record does not describe one.

    Defensive about every field, because this reads a file that another process
    wrote and that a person may have edited: a pid that is not a number, a kind
    that is not a string, a group of the wrong shape. None means "cannot tell",
    which `state` turns into `unknown`, never into free.
    """
    tree = (record or {}).get("tree")
    if tree is not None and not isinstance(tree, dict):
        return None
    tree = tree or {}
    try:
        pid = int((record or {}).get("pid") or 0)
    except (TypeError, ValueError):
        return None
    kind = tree.get("kind") or ""
    if not isinstance(kind, str):
        return None
    group = tree.get("group")
    if kind == "pgid":
        try:
            group = int(group)
        except (TypeError, ValueError):
            return None
    elif kind == "job" and not isinstance(group, str):
        return None
    return proctree.Tree(pid=pid, token=(record or {}).get("token"),
                         group=group, kind=kind)


def state(record: Optional[dict]) -> str:
    """`"free"`, `"busy"`, or `"unknown"` for a claim record.

    `"unknown"` is not a shade of free. It means the record names an owner this
    process cannot ask about - another machine's pid on a shared data root, or
    a record with no identity in it at all - and every caller treats it as busy.
    Silence about ownership is the one thing that must never read as available.
    """
    if record is UNREADABLE:
        return "unknown"                # a claim we cannot read is not absent
    if not record:
        return "free"
    if record.get("host") and record["host"] != platform.node():
        return "unknown"
    if not isinstance(record.get("session", ""), str):
        return "unknown"
    tree = _tree_of(record)
    if tree is None:
        return "unknown"                # a record whose tree makes no sense
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
    if not record or record is UNREADABLE:
        return record is UNREADABLE     # unreadable is owned, not free
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


def describe(record) -> str:
    """One line about a claim, for an error a person has to act on."""
    if record is UNREADABLE:
        return ("a session claim that cannot be read - repair or remove "
                "`sessions/<id>.json` by hand once you know no browser is "
                "using this profile")
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
    """Claim this profile for one session, and give it up when that ends.

    Taken inside the caller's session lease, so two openers cannot race here;
    the claim is what survives the lease.

    The claim belongs to the *session*, not to the process. One Python process
    can hold two profiles open at once, and it used to write two claims that
    named the same process tree - so closing the first found the second's
    browser still in that tree and kept the first profile owned, sometimes
    until the host exited. Each session carries its own id and gives up its own
    claim.
    """
    record = take(root, profile_id, tree, what)
    ended_cleanly = False
    try:
        yield record["session"]
        ended_cleanly = True
    finally:
        if ended_cleanly:
            release(root, profile_id, record["session"])


def take(root, profile_id: str, tree: proctree.Tree, what: str = "a session") -> dict:
    """Record this process as the owner, or raise `SessionBusy`.

    The check and the write are one transaction, so two processes that both
    find the previous claim stale cannot both recover it.
    """
    with transaction_lock(root):
        current = read(root, profile_id)
        if current is not None:
            if state(current) != "free":
                raise SessionBusy(
                    f"this profile is still owned: {describe(current)}. "
                    f"Stop it first - `numera stop <profile>` - or wait for it "
                    f"to finish.")
        record = {
            "session": uuid.uuid4().hex,
            "pid": tree.pid or os.getpid(),
            "token": tree.token,
            "tree": tree.as_record(),
            "host": platform.node(),
            "what": what,
            "started": time.time(),
        }
        write_json(_path(root, profile_id), record)
        return record


def release(root, profile_id: str, session: str) -> bool:
    """Give up one session's claim. True when it was removed.

    Only the claim with this session's id, and only on a clean end - `owned`
    keeps it when the body raised, because a browser whose close failed is
    exactly the thing the claim is for.

    It deliberately does *not* ask whether anything else is alive in the
    process tree. That tree is per process, not per session: with two profiles
    open here the other one's browser is in it, and so is every unrelated child
    of an application embedding this API - and treating either as "my browser
    survived" kept a profile owned until the host exited.

    What that gives up is the case where this session's own browser leaked
    while the process lives on. Nothing can distinguish it from a sibling in a
    shared tree, so it is answered by the clean-exit rule above instead of by
    guessing.
    """
    with transaction_lock(root):
        current = read(root, profile_id)
        if not current or current is UNREADABLE                 or current.get("session") != session:
            return False                # not this session's to remove
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


def held(root, profile_id: str) -> bool:
    """Is there any standing claim on this profile - including one of ours?

    The safety question, and deliberately not the status one. `busy()` answers
    "is somebody *else* using it", which is what a listing wants and what a
    destructive operation must not ask: this process can own a session whose
    browser failed to close, and "it is ours" is not a reason to rename the
    directory that browser is writing to.
    """
    return state(read(root, profile_id)) != "free"


def refuse_if_busy(root, profile_id: str) -> None:
    """Raise `SessionBusy` when anything still owns this profile's browser.

    For the operations that move or replace `user-data/<id>` - delete, backup,
    restore, session delete. Each already takes the session lease, and the
    lease is exactly what a killed launcher does not leave behind: its Chromium
    goes on holding the directory while the lock reads free, so a delete would
    rename a live profile out from under a running browser and a backup would
    zip a database being written.

    A claim of our own counts. `busy()` used to be the check here, and it
    returns false for our own claim - so a process whose session ended with a
    browser that would not close, leaving the claim standing on purpose, could
    then delete that very profile.
    """
    if held(root, profile_id):
        raise SessionBusy(
            f"this profile's browser is still open: "
            f"{describe(read(root, profile_id))}. Stop it first.")


def settle(root, profile_id: str) -> bool:
    """The one transition from "this tree is proved gone" to "no claim".

    Every stop path ends here - the server's, a detached `jobs stop`, `numera
    stop`, the scheduler's cleanup, a successful delete - so ownership
    metadata does not outlive the thing it described and wait for some later
    opener to tidy it. It is `clear` under a name that says when to call it,
    and it keeps `clear`'s refusal: a claim that is still occupied, or that
    this machine cannot check, is left exactly where it is.
    """
    return clear(root, profile_id)
