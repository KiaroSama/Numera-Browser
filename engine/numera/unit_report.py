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


def read(path) -> UnitReport:
    """The units a process recorded, and whether the record is whole.

    Read only after the writer has stopped, so a partial final line means a
    report that was cut off rather than one still being written.
    """
    from . import proctree           # local: `proctree` writes through `record`

    if path is None:
        return UnitReport(complete=False,
                          problem="no ownership report was created for this helper")
    path = Path(path)
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return UnitReport(complete=False,
                          problem=f"the ownership report {path.name} is missing, so "
                                  f"what this helper started cannot be established")
    except OSError as e:
        return UnitReport(complete=False,
                          problem=f"the ownership report {path.name} could not be "
                                  f"read ({e}), so what this helper started "
                                  f"cannot be established")
    found, damaged = [], []
    for number, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            parsed = json.loads(line)
        except ValueError:
            damaged.append(number)
            continue
        tree = proctree.Tree.from_record(parsed)
        if not tree.pid:
            damaged.append(number)
            continue
        found.append(tree)
    if damaged:
        return UnitReport(
            trees=found, complete=False,
            problem=f"the ownership report {path.name} has {len(damaged)} "
                    f"unreadable record(s) at line(s) "
                    f"{', '.join(str(n) for n in damaged)}, so it does not "
                    f"account for everything this helper started")
    # An empty report is a real answer: the helper started no separately
    # sessioned driver. It is only trustworthy because the file existed.
    return UnitReport(trees=found, complete=True)
