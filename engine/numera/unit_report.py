"""Versioned ownership receipts with a pre-spawn intent.

A versioned header proves a valid zero-unit report; file existence alone does
not. Each separately owned spawn is recorded BEFORE it can create a process.
An interrupted transfer therefore remains pending rather than looking empty.
Nonempty legacy identity records remain readable for older callers.
"""
from __future__ import annotations

import json
import os
import uuid
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

UNITS_ENV = "NUMERA_HELPER_UNITS"
KINDS = ("job", "pgid")
VERSION = 2


class UnitReportFailed(RuntimeError):
    """Ownership could not be communicated; the caller must end or retain it."""


@dataclass
class UnitReport:
    trees: list = field(default_factory=list)
    complete: bool = True
    problem: Optional[str] = None


def _append(path, data) -> None:
    payload = (json.dumps(data, separators=(",", ":")) + "\n").encode("utf-8")
    try:
        # No O_CREAT: a lost report must not silently turn into a new one.
        fd = os.open(path, os.O_WRONLY | os.O_APPEND | getattr(os, "O_BINARY", 0))
        try:
            if os.write(fd, payload) != len(payload):
                raise OSError("partial ownership-record write")
        finally:
            os.close(fd)
    except OSError as error:
        raise UnitReportFailed("ownership report could not be written") from error


def initialize(path) -> None:
    """Initialize the parent's newly created empty report, before child spawn."""
    _append(path, {"version": VERSION, "event": "header"})


def begin_spawn() -> Optional[str]:
    """Reserve an ownership transfer before spawning; failure starts nothing."""
    path = os.environ.get(UNITS_ENV)
    if not path:
        return None
    attempt = uuid.uuid4().hex
    _append(path, {"event": "pending", "attempt": attempt})
    return attempt


def cancel_spawn(attempt: Optional[str]) -> None:
    """Resolve an intent only after proving that no process was left behind."""
    path = os.environ.get(UNITS_ENV)
    if path and attempt:
        _append(path, {"event": "cancelled", "attempt": attempt})


def record(tree, *, attempt: Optional[str] = None) -> None:
    path = os.environ.get(UNITS_ENV)
    if not path:
        return
    data = {"pid": tree.pid, "pidToken": tree.token, "tree": tree.as_record()}
    if attempt:
        data.update(event="unit", attempt=attempt)
    _append(path, data)


def _positive_int(value) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value > 0


def _unit(rec) -> tuple:
    from . import proctree
    # Each reason keeps the generic "unreadable record" alongside the specific
    # cause. The specific half is what a person debugging needs; the generic
    # half is the category callers and suites match on, and dropping it made
    # every one of them stop recognising a refusal they still receive.
    if not isinstance(rec, dict) or not _positive_int(rec.get("pid")):
        return None, "unreadable record: no usable positive integer pid"
    pid, token = rec["pid"], rec.get("pidToken")
    tree = rec.get("tree")
    if not isinstance(tree, dict) or tree.get("kind") not in KINDS:
        return None, "unreadable record: missing or unsupported tree kind"
    kind, group = tree["kind"], tree.get("group")
    if kind == "pgid":
        if not _positive_int(token) or not _positive_int(group) or group != pid:
            return None, ("unreadable record: process-group identity needs "
                          "matching pid/group and a start token")
    elif not isinstance(group, str) or not group.strip() or "\0" in group:
        return None, "unreadable record: job identity needs a nonempty job name"
    elif token is not None and not _positive_int(token):
        return None, "unreadable record: invalid process start token"
    return proctree.Tree(pid=pid, token=token, group=group, kind=kind), None


def read(path) -> UnitReport:
    if path is None:
        return UnitReport(complete=False, problem="no ownership report was created")
    path = Path(path)
    try:
        raw = path.read_bytes()
    except FileNotFoundError:
        return UnitReport(complete=False, problem=f"ownership report {path.name} is missing")
    except OSError as error:
        return UnitReport(complete=False, problem=f"ownership report {path.name} cannot be read: {error}")
    found, damaged, pending, resolved = [], [], set(), set()
    header = False
    records = 0
    for number, line in enumerate(raw.splitlines(keepends=True), 1):
        if not line.strip():
            continue
        records += 1
        if not line.endswith(b"\n"):
            damaged.append(f"line {number}: interrupted final write")
        try:
            rec = json.loads(line.decode("utf-8"))
        except (ValueError, UnicodeError, RecursionError):
            damaged.append(f"line {number}: invalid JSON/UTF-8")
            continue
        event = rec.get("event") if isinstance(rec, dict) else None
        if event == "header":
            if header or records != 1 or type(rec.get("version")) is not int or rec["version"] != VERSION:
                damaged.append(f"line {number}: invalid header")
            else:
                header = True
            continue
        if event is not None:
            attempt = rec.get("attempt")
            if not header or not isinstance(attempt, str) or not attempt:
                damaged.append(f"line {number}: transfer without a valid header/identity")
                continue
            if event == "pending":
                if attempt in pending or attempt in resolved:
                    damaged.append(f"line {number}: repeated transfer identity")
                pending.add(attempt)
                continue
            if event not in ("unit", "cancelled") or attempt not in pending:
                damaged.append(f"line {number}: unrecognized transfer outcome")
                continue
            if event == "cancelled":
                pending.remove(attempt)
                resolved.add(attempt)
                continue
        tree, why = _unit(rec)
        if why:
            damaged.append(f"line {number}: {why}")
            continue
        if tree not in found:
            found.append(tree)
        if event == "unit":
            pending.remove(attempt)
            resolved.add(attempt)
    if pending:
        damaged.append(f"{len(pending)} ownership transfer(s) did not finish")
    if not records:
        damaged.append("empty report has no initialized zero-unit receipt")
    return UnitReport(found, not damaged,
                      f"ownership report {path.name}: " + "; ".join(damaged) if damaged else None)
