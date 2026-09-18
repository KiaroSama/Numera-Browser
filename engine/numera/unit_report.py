"""How a process tells its owner what it started, and how the owner reads it.

A browser driver gets its own job object or session (`proctree._spawn_leader`),
which on POSIX takes it out of the starting process's group entirely. So the
owner cannot find it by walking descendants - it is not a descendant any more -
and a bare pid is not safe to signal once the number has been recycled. The only
thing that reaches it is a written record carrying the job name or the
start-time token that keeps it identifiable.

Both halves of that record live here because they are one format. They were
split across `proctree` (writing) and `server.helpers` (reading), which is how
they came to disagree about what an absent record means.

The rule the format exists to enforce: **unknown ownership is not zero
ownership.** A missing file, an unreadable one, a truncated final line and a
process that genuinely started nothing all used to produce the same empty list,
so "the evidence is gone" cleared a registry exactly like "there was nothing to
clean up". `UnitReport.complete` is what separates them, and a report that
cannot be trusted strands its helper instead of settling it.
"""
from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

#: Names the file a process appends its separately sessioned leaders to. Set by
#: whoever started it; absent means nobody is collecting, which is the ordinary
#: case for a CLI run nothing is supervising.
UNITS_ENV = "NUMERA_HELPER_UNITS"


class UnitReportFailed(RuntimeError):
    """A separately sessioned leader could not be recorded for its owner.

    Fatal to that launch on purpose. The record is the only way the owner can
    reach a process that has left its group, so a browser started without one
    is a browser nothing can end - and "best effort" here means the failure
    shows up later as a cleanup that quietly succeeded over a live process.
    """


@dataclass
class UnitReport:
    """What a process said it owned, and whether that can be believed."""
    trees: list = field(default_factory=list)
    complete: bool = True
    problem: Optional[str] = None


def record(tree) -> None:
    """Append this tree's durable identity to the owner's report.

    Flushed before the handle closes, and one record is one line, so a reader
    running after this process has stopped sees whole records - or a partial
    final line, which it must treat as lost evidence rather than skip.

    This used to swallow `OSError`. A report that was never written and a
    process that genuinely started nothing then looked identical to the owner,
    which is the whole defect: the cleanup afterwards reported success without
    ever knowing the driver existed.
    """
    path = os.environ.get(UNITS_ENV)
    if not path:
        return
    try:
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(json.dumps({"pid": tree.pid, "pidToken": tree.token,
                                 "tree": tree.as_record()}) + "\n")
            fh.flush()
    except OSError as e:
        raise UnitReportFailed(
            f"the browser driver (pid {tree.pid}) could not be recorded for "
            f"its owner ({e}), so nothing would be able to prove it had "
            f"stopped. It was not left running.") from e


#: The tree primitives an owner can actually end. A record naming anything else
#: was written by a build this one does not understand, and guessing at it is
#: how a live browser gets called gone: `end_tree` has no branch for an unknown
#: kind, so it would return "nothing left to do" for a tree it cannot reach.
KINDS = ("job", "pgid")


def _unit(rec) -> "tuple[object, Optional[str]]":
    """One parsed record as a `Tree`, or the reason it cannot be trusted.

    Every field is checked before it is used, because this is the trust
    boundary: the file is written by another process, possibly a different
    build, possibly interrupted mid-line, and `Tree.from_record` coerces rather
    than validates - `int(rec["pid"])` on `"abc"` raises, `(17).get` raises, and
    a record with no identity at all comes back as a `Tree` nothing can signal.
    An exception here is not a smaller problem than a bad record: raised out of
    `read`, it escapes settlement entirely and leaves the non-daemon reader
    threads alive outside any cleanup.
    """
    from . import proctree           # local: `proctree` writes through `record`

    if not isinstance(rec, dict):
        return None, f"a {type(rec).__name__} where an object was expected"

    pid = rec.get("pid")
    # `bool` first: `isinstance(True, int)` is True, and pid 1 is init.
    if isinstance(pid, bool) or not isinstance(pid, int) or pid <= 0:
        return None, f"no usable pid ({pid!r})"

    tree = rec.get("tree")
    if tree is None:
        tree = {}
    if not isinstance(tree, dict):
        return None, f"a {type(tree).__name__} where the tree was expected"

    kind = tree.get("kind") or ""
    if not isinstance(kind, str):
        return None, f"a non-string tree kind ({kind!r})"
    if kind and kind not in KINDS:
        return None, f"an unsupported tree kind ({kind!r})"

    built = proctree.Tree(pid=pid, token=rec.get("pidToken"),
                          group=tree.get("group"), kind=kind)
    # `verified` is what `signal_tree` requires before it will touch anything:
    # a named job, or a start-time token proving the leader is still the process
    # that was recorded. Without one the record names a pid that may have been
    # recycled, and acting on it is worse than refusing it.
    if not built.verified:
        return None, ("no durable identity - neither a job name nor a "
                      "start-time token")
    return built, None


def read(path) -> UnitReport:
    """The units a process recorded, and whether the record is whole.

    Read only after the writer has stopped, so a partial final line means a
    report that was cut off rather than one still being written.

    This never raises. Every failure - absent, unreadable, undecodable,
    well-formed JSON of the wrong shape, a record with no identity - comes back
    as `complete=False` with the reason, and whatever *was* readable comes back
    in `trees` so a partial report still ends the units it does account for.
    """
    if path is None:
        return UnitReport(complete=False,
                          problem="no ownership report was created for this helper")
    path = Path(path)
    try:
        raw = path.read_bytes()
    except FileNotFoundError:
        return UnitReport(complete=False,
                          problem=f"the ownership report {path.name} is missing, so "
                                  f"what this helper started cannot be established")
    except OSError as e:
        return UnitReport(complete=False,
                          problem=f"the ownership report {path.name} could not be "
                                  f"read ({e}), so what this helper started "
                                  f"cannot be established")
    # Bytes, then an explicit decode. `read_text` raised `UnicodeDecodeError` on
    # a report with a truncated multi-byte sequence - a `ValueError`, not an
    # `OSError`, so it went straight past the handler above and out of `read`.
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as e:
        return UnitReport(complete=False,
                          problem=f"the ownership report {path.name} is not valid "
                                  f"UTF-8 ({e.reason} at byte {e.start}), so what "
                                  f"this helper started cannot be established")

    found, damaged = [], []
    for number, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            parsed = json.loads(line)
        except ValueError as e:
            damaged.append(f"{number} ({e.msg if hasattr(e, 'msg') else e})")
            continue
        unit, why = _unit(parsed)
        if why is not None:
            damaged.append(f"{number} ({why})")
            continue
        found.append(unit)
    if damaged:
        return UnitReport(
            trees=found, complete=False,
            problem=f"the ownership report {path.name} has {len(damaged)} "
                    f"unreadable record(s) at line(s) {'; '.join(damaged)}, so "
                    f"it does not account for everything this helper started")
    # An empty report is a real answer: the helper started no separately
    # sessioned driver. It is only trustworthy because the file existed.
    return UnitReport(trees=found, complete=True)
