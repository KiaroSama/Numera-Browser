"""
Scheduled warm-up — keep a profile's session from going stale, unattended.

A profile you log into once and then leave for three weeks looks, on your next
visit, like a browser that was frozen and thawed: no fresh cookies, no recent
history, a session token about to expire. Sites that score risk treat that
"cold" return with suspicion. Scheduled warm-up fixes it by running the ordinary
:mod:`numera.warmup` browse on a recurring interval, so each profile keeps a
believable, up-to-date trail without anyone sitting there clicking.

This module is only the *bookkeeping*: a small JSON-backed store of schedules
and the pure rule for which ones are due. Actually running the warm-up is left
to whoever owns a browser — the API server drives it from a background thread
(see :mod:`numera.server`), and ``numera schedule run-due`` drives it from
cron / Task Scheduler for people who don't run the server.
"""

from __future__ import annotations

import json
import uuid
from pathlib import Path

from .paths import _lock_for, write_json
from typing import Optional

# A schedule can't fire more often than this, whatever interval is asked for —
# a runaway "every 0 hours" would just launch browsers back to back.
MIN_INTERVAL_HOURS = 0.25

# Written by the scheduler as it runs, never by a client: patching `lastRun`
# moves the next run, and patching an outcome forges a result nobody produced.
SCHEDULER_OWNED = ("lastRun", "lastOutcome", "lastError", "lastFailedAt",
                   "claimedBy", "claimedAt")

# How long a claim stands before another worker may take the occurrence. Long
# enough to cover a warm-up plus its browser startup, short enough that a
# machine that lost power does not park a schedule forever. The claim is
# advisory bookkeeping, not a lock: the session lease is what actually stops
# two browsers, and this stops two *records* of one run.
CLAIM_TTL_SECONDS = 12 * 60 * 60


class ScheduleStore:
    """A JSON-backed list of warm-up schedules, one file for the whole set."""

    def __init__(self, root: Path):
        self.path = Path(root) / "schedules.json"

    def list(self) -> list[dict]:
        if not self.path.exists():
            return []
        try:
            return json.loads(self.path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return []

    def _save(self, items: list[dict]) -> None:
        write_json(self.path, items)

    @property
    def _lock(self):
        return _lock_for(self.path)

    def get(self, sid: str) -> Optional[dict]:
        return next((s for s in self.list() if s.get("id") == sid), None)

    def add(self, *, profile_id: str, every_hours: float, minutes: float = 5,
            preset: str = "general", enabled: bool = True,
            now: Optional[float] = None) -> dict:
        # Through the same validator `update()` and the HTTP create route use.
        # `add()` converted with a bare `float()`, and argparse happily accepts
        # `nan` and `inf` - so `numera schedule add --minutes nan` wrote a
        # non-finite number into `schedules.json`, which is not even valid JSON
        # to a strict reader. Validity is a property of the store, not of which
        # surface happened to call it.
        clean = validate_schedule({
            "everyHours": every_hours,
            "minutes": minutes,
            "preset": preset,
            "enabled": enabled,
        })
        entry = {
            "id": uuid.uuid4().hex[:12],
            "profileId": profile_id,
            "everyHours": clean["everyHours"],
            "minutes": clean["minutes"],
            "preset": clean["preset"],
            "enabled": clean["enabled"],
            "lastRun": None,
            "createdAt": now,
        }
        with self._lock:
            items = self.list()
            items.append(entry)
            self._save(items)
        return entry

    def update(self, sid: str, patch: dict) -> Optional[dict]:
        """Apply a *user* patch, normalised.

        Validation used to live only in the HTTP route, so `everyHours: "soon"`
        went straight to disk from the CLI or any other internal caller and
        surfaced later as a `next_run_at()` that could not compute. The store is
        the boundary every writer crosses, so the rule belongs here.
        """
        return self._apply(sid, validate_schedule(patch, partial=True))

    def claim_due(self, sid: str, owner: str, now: Optional[float] = None) -> bool:
        """Take the current due occurrence of `sid`, or return False.

        Read-decide-write inside the store's own lock, which is what makes it a
        claim rather than a check. `due_schedules()` is pure and honest, but two
        servers - or a server and `schedule run-due` - could both read the same
        due row, both start a warm-up and both write `lastRun`, leaving one
        occurrence recorded twice and a second browser started for a profile
        that already had one.
        """
        import time as _time

        stamp = _time.time() if now is None else now
        with self._lock:
            items = self.list()
            for s in items:
                if s.get("id") != sid:
                    continue
                if not s.get("enabled", True) or next_run_at(s) > stamp:
                    return False
                held = s.get("claimedAt")
                if held and stamp - float(held) < CLAIM_TTL_SECONDS:
                    return False              # someone else has this occurrence
                s["claimedBy"] = owner
                s["claimedAt"] = stamp
                self._save(items)
                return True
        return False

    def release(self, sid: str) -> None:
        """Drop a claim without recording a run - a spawn that never started."""
        self._apply(sid, {"claimedBy": None, "claimedAt": None})

    def record(self, sid: str, **outcome) -> Optional[dict]:
        """Write the fields the scheduler owns: what ran, when, and how it went.

        Separate from `update()` because `validate_schedule` refuses these by
        design - a client must not be able to move the next run or forge a
        result - while the scheduler itself has to write them every tick.
        """
        unknown = sorted(set(outcome) - set(SCHEDULER_OWNED))
        if unknown:
            raise ValueError(f"not a scheduler-owned field: {', '.join(unknown)}")
        if "lastRun" in outcome:
            # The occurrence is over; the next one must not inherit its claim.
            outcome = {**outcome, "claimedBy": None, "claimedAt": None}
        return self._apply(sid, outcome)

    def _apply(self, sid: str, patch: dict) -> Optional[dict]:
        # Re-read inside the lock: a background tick and an HTTP edit hold
        # different snapshots, and writing a stale one back drops the other.
        with self._lock:
            items = self.list()
            out = None
            for s in items:
                if s.get("id") == sid:
                    s.update(patch)
                    out = s
            if out:
                self._save(items)
            return out

    def delete(self, sid: str) -> bool:
        with self._lock:
            items = self.list()
            kept = [s for s in items if s.get("id") != sid]
            if len(kept) == len(items):
                return False
            self._save(kept)
            return True

    def mark_run(self, sid: str, when: float) -> Optional[dict]:
        return self.record(sid, lastRun=when)


MAX_INTERVAL_HOURS = 24 * 30


def validate_schedule(patch: dict, *, partial: bool = False) -> dict:
    """A normalised schedule patch, or ValueError saying what is wrong.

    `update()` applied raw patches, so `everyHours: "soon"` or a negative value
    was stored happily and only surfaced later as a broken `next_run_at()` or a
    warm-up asked to browse for minus four minutes. One schema for create and
    update, so the two cannot disagree.

    A too-small interval is *clamped* to `MIN_INTERVAL_HOURS`, which is what
    `add()` has always done - only values that are not finite numbers, or are
    not positive at all, are refused.
    """
    out: dict = {}
    if "everyHours" in patch or not partial:
        raw = patch.get("everyHours", MIN_INTERVAL_HOURS)
        try:
            hours = float(raw)
        except (TypeError, ValueError):
            raise ValueError(f"'everyHours' must be a number, got {raw!r}") from None
        if hours != hours or hours in (float("inf"), float("-inf")):
            raise ValueError("'everyHours' must be a finite number")
        if hours <= 0:
            raise ValueError(f"'everyHours' must be greater than 0, got {hours}")
        if hours > MAX_INTERVAL_HOURS:
            raise ValueError(f"'everyHours' must be at most {MAX_INTERVAL_HOURS}")
        out["everyHours"] = max(MIN_INTERVAL_HOURS, hours)

    if "minutes" in patch or not partial:
        # The same validator the CLI, the HTTP route and warm_up() itself use.
        # This had its own copy with its own bound (1440 against warmup.py 480),
        # so a schedule could store a duration a direct run would refuse.
        from .warmup import WarmupInvalid, validate_warmup
        try:
            out["minutes"] = validate_warmup(patch.get("minutes", 5))
        except WarmupInvalid as e:
            raise ValueError(f"'minutes': {e}") from None

    if "preset" in patch:
        preset = patch["preset"]
        if preset is not None and not isinstance(preset, str):
            raise ValueError(f"'preset' must be a string, got {type(preset).__name__}")
        if preset is not None:
            from .warmup import WarmupInvalid as _WI, validate_warmup as _vw
            try:
                _vw(1, preset)
            except _WI as e:
                raise ValueError(f"'preset': {e}") from None
        out["preset"] = preset

    if "enabled" in patch:
        if not isinstance(patch["enabled"], bool):
            raise ValueError("'enabled' must be true or false")
        out["enabled"] = patch["enabled"]

    # Fields the scheduler owns. A client writing lastRun would move the next
    # run; writing lastOutcome would forge a result nobody produced.
    for key in (*SCHEDULER_OWNED, "id", "profileId"):
        if key in patch and partial:
            raise ValueError(f"'{key}' is set by the scheduler and cannot be patched")
    return out


def next_run_at(schedule: dict) -> float:
    """When this schedule is next due, as a unix timestamp.

    Never run before? Due at creation time (so a new schedule fires on the next
    tick rather than waiting a full interval). Otherwise last run + interval.
    """
    interval = max(MIN_INTERVAL_HOURS, float(schedule.get("everyHours", 1))) * 3600
    last = schedule.get("lastRun")
    if last is None:
        return float(schedule.get("createdAt") or 0)
    return float(last) + interval


def due_schedules(schedules: list[dict], now: float) -> list[dict]:
    """The enabled schedules whose next-run time has arrived. Pure — no clock,
    no browser — so it can be tested directly."""
    return [s for s in schedules
            if s.get("enabled", True) and next_run_at(s) <= now]
