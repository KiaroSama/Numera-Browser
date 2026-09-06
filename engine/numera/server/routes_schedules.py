"""Recurring unattended warm-up schedules."""

from __future__ import annotations

import sys
import threading
import os
import time
from typing import Optional


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    from ..scheduler import due_schedules, next_run_at, validate_schedule

    engine_store, dash, sched, running, is_running, start_scheduler = ctx.engine_store, ctx.dash, ctx.sched, ctx.running, ctx.is_running, ctx.start_scheduler
    spawn_owner, materialize_current = ctx.spawn_owner, ctx.materialize_current
    reap_children, stop_session = ctx.reap_children, ctx.stop_session

    # ---- Scheduled warm-up ----------------------------------------------
    def _spawn_warmup(pid: str, minutes: float, preset: Optional[str], sid: str):
        """Start a headless warm-up in the background and wait for its browser
        to open. It shows as running until it finishes.

        Returns the registration, or None when something else already owns the
        profile - see `spawn_owner`, which also raises rather than returning a
        child that never took the session.

        `sid` is carried into the registry so this child's exit updates that
        occurrence and no other. The reaper used to match on `profileId`, so a
        profile with two schedules had both rows written by one warm-up.
        """
        cli = [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root),
               "warmup", pid, "--minutes", str(minutes), "--headless"]
        if preset:
            cli += ["--preset", str(preset)]
        return spawn_owner(pid, cli, kind="scheduled_warmup", schedule_id=sid)

    def _decorate_schedule(s: dict) -> dict:
        d = dash.get(s["profileId"])
        nxt = next_run_at(s)
        return {**s, "profileName": (d or {}).get("name", s["profileId"]),
                "nextRun": nxt, "due": nxt <= time.time()}

    @app.get("/api/schedules")
    def list_schedules():
        return [_decorate_schedule(s) for s in sched.list()]

    @app.post("/api/schedules")
    def add_schedule(body: dict):
        pid = body.get("profileId")
        if not pid or not dash.get(pid):
            raise HTTPException(404, "profile not found")
        try:
            clean = validate_schedule(body)
        except ValueError as e:
            raise HTTPException(400, str(e))
        entry = sched.add(profile_id=pid, every_hours=clean["everyHours"],
                          minutes=clean["minutes"],
                          preset=clean.get("preset") or body.get("preset", "general"),
                          enabled=bool(body.get("enabled", True)), now=time.time())
        return _decorate_schedule(entry)

    @app.put("/api/schedules/{sid}")
    def update_schedule(sid: str, body: dict):
        try:
            patch = validate_schedule(body, partial=True)
        except ValueError as e:
            raise HTTPException(400, str(e))
        if not patch:
            raise HTTPException(400, "nothing to change - send everyHours, "
                                     "minutes, preset or enabled")
        out = sched.update(sid, patch)
        if not out:
            raise HTTPException(404, "schedule not found")
        return _decorate_schedule(out)

    @app.delete("/api/schedules/{sid}")
    def delete_schedule(sid: str):
        if not sched.delete(sid):
            raise HTTPException(404, "schedule not found")
        return {"ok": True}

    def _scheduler_tick() -> None:
        """Fire any warm-up that's due and whose profile is free right now."""
        try:
            rows = due_schedules(sched.list(), time.time())
        except Exception:           # pragma: no cover - a corrupt store must not
            return                  # kill the loop; the next tick re-reads it

        for s in rows:
            # Per row, not per tick. One unconvertible profile used to abort the
            # whole loop, so every schedule after it silently never ran - and the
            # only symptom was warm-ups that stopped happening.
            claimed = False
            try:
                pid = s["profileId"]
                d = dash.get(pid)
                if not d or is_running(pid):
                    continue        # gone, or busy - try again next tick
                # Claim the occurrence before spawning. `due_schedules()` is a
                # pure read, so two servers - or this tick and a `schedule
                # run-due` - could both see the same row due and both start it.
                if not sched.claim_due(s["id"], f"server:{os.getpid()}"):
                    continue
                claimed = True
                materialize_current(pid)
                # Blocks until the child has a browser open on the profile,
                # and raises otherwise - so a warm-up that dies on a locked
                # vault or a missing browser executable never reaches
                # `mark_run`. The old `poll()` check could only catch a child
                # that had already exited by the time `Popen` returned.
                if _spawn_warmup(pid, s.get("minutes", 5), s.get("preset"),
                                 s["id"]) is None:
                    raise RuntimeError("another session took the profile first")
                sched.mark_run(s["id"], time.time())
                sched.record(s["id"], lastOutcome="started", lastError=None)
            except Exception as e:  # noqa: BLE001 - one row's problem is its own
                if claimed:
                    # This occurrence never started, so the claim must not sit
                    # on it for the whole TTL - the row would be silently
                    # skipped by every tick until it expired. `record()` clears
                    # a claim only alongside `lastRun`, which a failure has no
                    # business writing, so it is released explicitly and the
                    # row is due again next tick.
                    try:
                        sched.release(s["id"])
                    except Exception:
                        pass
                try:
                    sched.record(s.get("id"),
                        lastOutcome="failed",
                        lastError=str(e).splitlines()[0][:200],
                        lastFailedAt=time.time())
                except Exception:
                    pass            # the store itself is unwell; the next tick retries

    # Exposed so the behaviours below can be exercised directly: a tick that
    # skips rows, records a false success or leaks a child all fail silently,
    # and none of it is reachable through an HTTP route.
    app.state.scheduler_tick = _scheduler_tick

    if start_scheduler:
        stop = threading.Event()

        def _loop():
            while not stop.wait(60):     # check once a minute
                # Reaping used to live here, and it walked every exited process
                # in the registry to write the outcome of *every* schedule whose
                # `profileId` matched - but that registry also holds ordinary
                # launches, attach sessions and manual warm-ups. It belongs to
                # the shared finalisation path, which knows what each child was
                # started for; this is only the clock that drives it.
                reap_children()
                _scheduler_tick()

        threading.Thread(target=_loop, daemon=True, name="numera-scheduler").start()

        app.state.add_shutdown(stop.set)

    def _stop_children():
        """Terminate every warm-up and browser this process started.

        Each holds a lock on a profile's user-data directory. Left behind, the
        next launch of that profile fails with a directory-in-use error that
        names nothing the user can act on - and the browser keeps running.

        Through `stop_session`, so a shutdown leaves the same trail any other
        exit does: `_last_active` stamped, attached state dropped, and a
        scheduled warm-up's occurrence closed rather than left reading
        "started" forever.
        """
        for pid in list(running):
            try:
                stop_session(pid)
            except Exception:       # noqa: BLE001 - one bad child must not
                pass                # strand the ones after it

    app.state.add_shutdown(_stop_children)
