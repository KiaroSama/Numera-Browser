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
from .paths import SessionBusy, id_path, safe_id, transaction_lock, write_json


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
        safe_id(profile_id)
    except ValueError:
        return None
    try:
        path = _path(root, profile_id)
    except (OSError, ValueError):
        return UNREADABLE
    try:
        # exists() also hides access errors; only a missing read proves absence.
        data = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return None
    except (OSError, ValueError):
        return UNREADABLE
    return data if isinstance(data, dict) else UNREADABLE


#: The tree kinds a browser-session claim may name, and the platform that can
#: answer for each. A claim naming the other platform's kind is not "probably
#: fine" - this machine has no way to ask whether that tree still holds a
#: browser, and `state` calls that `unknown`.
_EVALUABLE = {"job": True, "pgid": False}       # kind -> needs Windows


def _tree_of(record: dict) -> Optional[proctree.Tree]:
    """The tree a claim names, or None when it does not name a usable one.

    Strict on purpose, because this reads a file another process wrote and a
    person may have edited, and because the leader-only fall-through it used to
    allow is the exact shape that outlives its own guarantee:

    * a claim with `kind: ""` and a token is a **0.26-era leader-only** record.
      `alive_tree` falls back to "is the leader alive" for it, so an older
      launcher killed while its Chromium survived reads as free and the next
      opener takes the profile - the corruption the claim exists to prevent,
      reached by upgrading.
    * a `kind` this build does not know, or one belonging to the other platform
      (a POSIX `pgid` read on Windows), cannot be evaluated here at all.

    Both are `None`, which `state` turns into `unknown`, which every caller
    treats as owned. Recovering one is a deliberate act by a person who knows
    no browser is running - not something the next opener does by accident.
    """
    record = record or {}
    tree = record.get("tree")
    if not isinstance(tree, dict):
        return None
    kind = tree.get("kind")
    if kind not in _EVALUABLE or _EVALUABLE[kind] is not proctree._WINDOWS:
        return None
    group = tree.get("group")
    if kind == "pgid":
        # `bool` is an `int`; `True` is not a process group.
        if isinstance(group, bool) or not isinstance(group, int) or group <= 0:
            return None
    elif not isinstance(group, str) or not group:
        return None
    token = record.get("token")
    if token is not None and (isinstance(token, bool)
                              or not isinstance(token, (int, float, str))):
        return None
    pid = record.get("pid")
    if isinstance(pid, bool) or not isinstance(pid, int) or pid < 0:
        return None
    return proctree.Tree(pid=pid, token=token, group=group, kind=kind)


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
    session = record.get("session")
    if not isinstance(session, str) or not session:
        return "unknown"                # a claim nothing can be released by
    tree = _tree_of(record)
    if tree is None:
        return "unknown"                # no durable tree this machine can ask
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
    if state(record) != "unknown":
        return f"{what} started by pid {pid}, whose browser is still running"
    host = record.get("host")
    if host and host != platform.node():
        return (f"{what} claimed by pid {pid} on {host}, which this process "
                f"cannot verify")
    # Same machine, so the record itself is the problem: an older or
    # hand-edited claim that names no tree this build can ask about. Saying
    # "cannot verify" alone leaves a person with nothing to do about it.
    return (f"{what} claimed by pid {pid} in a form this version cannot check "
            f"- remove `sessions/<id>.json` by hand once you know no browser "
            f"is using this profile")


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
        from .update_admission import Admission
        Admission(root).assert_open()
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


def narrow(root, profile_id: str, session: str, tree) -> bool:
    """Point this session's claim at the tree its browser actually landed in.

    The claim has to exist before a browser can be opened - that ordering is
    what stops two openers racing - so it is taken naming the *host* tree, the
    only one that exists at that moment. It is over-broad, which is the safe
    direction: it refuses more than it must, never less.

    Once the driver is running there is a better answer: a group or job holding
    that session's browser and nothing else. Narrowing to it is what lets
    `release` ask the OS whether this browser is gone instead of believing a
    report, and what lets a *different* process see the same exact tree.
    """
    if tree is None:
        return False
    with transaction_lock(root):
        current = read(root, profile_id)
        if not current or current is UNREADABLE or current.get("session") != session:
            return False                # not this session's to change
        write_json(_path(root, profile_id),
                   {**current, "pid": tree.pid or current.get("pid"),
                    "token": tree.token, "tree": tree.as_record()})
        return True


def release(root, profile_id: str, session: str, tree=None) -> bool:
    """Give up one session's claim. True when it was removed.

    Only the claim with this session's id, and - when `tree` is given - only
    once the OS says that tree holds nothing. That is the whole invariant:

        this session's own tree, proved empty, then the claim goes

    `tree` is the one `narrow()` recorded, which contains this session's
    browser and nothing else. With it, a browser that survived a nominally
    clean exit keeps the profile owned, and a body that raised over an already
    closed browser does not - both answered by asking the OS rather than by
    what any part of this process reported.

    Without one - the host could not give this session a tree of its own - it
    falls back to the caller's report, because the only tree available holds
    sibling sessions and every unrelated child of an embedding application, and
    treating those as "my browser survived" kept a profile owned until the host
    exited.
    """
    with transaction_lock(root):
        current = read(root, profile_id)
        if not current or current is UNREADABLE                 or current.get("session") != session:
            return False                # not this session's to remove
        if tree is not None and proctree.alive_tree(tree):
            return False                # its browser is still in there
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
