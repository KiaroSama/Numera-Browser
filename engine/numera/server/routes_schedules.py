"""Recurring unattended warm-up schedules."""

from __future__ import annotations

import subprocess
import sys
import threading
import os
import time
from typing import Optional
from .convert import to_engine_profile
from ..paths import child_env


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    from ..scheduler import due_schedules, next_run_at, validate_schedule

    engine_store, dash, sched, running, is_running, start_scheduler = ctx.engine_store, ctx.dash, ctx.sched, ctx.running, ctx.is_running, ctx.start_scheduler

    # ---- Scheduled warm-up ----------------------------------------------
    def _spawn_warmup(pid: str, minutes: float, preset: Optional[str]) -> None:
        """Launch a headless warm-up in the background (shared by the endpoint
        and the scheduler). The profile shows as running until it finishes."""
        cli = [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root),
               "warmup", pid, "--minutes", str(minutes), "--headless"]
        if preset:
            cli += ["--preset", str(preset)]
        proc = subprocess.Popen(cli, env=child_env(engine_store._password))
        running[pid] = proc
        return proc

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
                engine_store.save(to_engine_profile(d))
                proc = _spawn_warmup(pid, s.get("minutes", 5), s.get("preset"))
                # A process that is already dead never ran. Marking the schedule
                # as run here would hide the failure until someone noticed the
                # profile had gone cold.
                if proc.poll() not in (None, 0):
                    raise RuntimeError(f"warm-up exited immediately ({proc.returncode})")
                sched.mark_run(s["id"], time.time())
                sched.record(s["id"], lastOutcome="started", lastError=None)
            except Exception as e:  # noqa: BLE001 - one row's problem is its own
                try:
                    sched.record(s.get("id"),
                        lastOutcome="failed",
                        lastError=str(e).splitlines()[0][:200],
                        lastFailedAt=time.time())
                except Exception:
                    pass            # the store itself is unwell; the next tick retries

    def _reap_warmups() -> None:
        """Record how the warm-ups that have finished actually ended.

        `Popen` returning is not success - it means a process started. Without
        this, a warm-up that crashed on its first page looked identical to one
        that browsed for thirty minutes.
        """
        for pid, proc in list(running.items()):
            code = proc.poll()
            if code is None:
                continue
            running.pop(pid, None)
            for s in sched.list():
                if s.get("profileId") != pid:
                    continue
                try:
                    sched.record(s["id"],
                        lastOutcome="ok" if code == 0 else "failed",
                        lastError=None if code == 0 else f"warm-up exited {code}")
                except Exception:
                    pass

    # Exposed so the behaviours below can be exercised directly: a tick that
    # skips rows, records a false success or leaks a child all fail silently,
    # and none of it is reachable through an HTTP route.
    app.state.scheduler_tick = _scheduler_tick
    app.state.reap_warmups = _reap_warmups

    if start_scheduler:
        stop = threading.Event()

        def _loop():
            while not stop.wait(60):     # check once a minute
                _reap_warmups()
                _scheduler_tick()

        threading.Thread(target=_loop, daemon=True, name="numera-scheduler").start()

        app.state.add_shutdown(stop.set)

    def _stop_children():
        """Terminate every warm-up and browser this process started.

        Each holds a lock on a profile's user-data directory. Left behind, the
        next launch of that profile fails with a directory-in-use error that
        names nothing the user can act on - and the browser keeps running.
        """
        for pid, proc in list(running.items()):
            if proc.poll() is not None:
                continue
            try:
                proc.terminate()
                proc.wait(timeout=5)
            except Exception:       # noqa: BLE001 - it is going away either way
                try:
                    proc.kill()
                except Exception:
                    pass
            running.pop(pid, None)

    app.state.add_shutdown(_stop_children)
