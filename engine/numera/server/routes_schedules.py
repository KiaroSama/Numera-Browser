"""Recurring unattended warm-up schedules."""

from __future__ import annotations

import subprocess
import sys
import threading
import time
from typing import Optional
from .convert import to_engine_profile


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    from ..scheduler import due_schedules, next_run_at

    engine_store, dash, sched, running, is_running, start_scheduler = ctx.engine_store, ctx.dash, ctx.sched, ctx.running, ctx.is_running, ctx.start_scheduler

    # ---- Scheduled warm-up ----------------------------------------------
    def _spawn_warmup(pid: str, minutes: float, preset: Optional[str]) -> None:
        """Launch a headless warm-up in the background (shared by the endpoint
        and the scheduler). The profile shows as running until it finishes."""
        cli = [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root),
               "warmup", pid, "--minutes", str(minutes), "--headless"]
        if preset:
            cli += ["--preset", str(preset)]
        running[pid] = subprocess.Popen(cli)

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
        entry = sched.add(profile_id=pid, every_hours=float(body.get("everyHours", 12)),
                          minutes=float(body.get("minutes", 5)),
                          preset=body.get("preset", "general"),
                          enabled=bool(body.get("enabled", True)), now=time.time())
        return _decorate_schedule(entry)

    @app.put("/api/schedules/{sid}")
    def update_schedule(sid: str, body: dict):
        patch = {k: body[k] for k in ("everyHours", "minutes", "preset", "enabled")
                 if k in body}
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
            for s in due_schedules(sched.list(), time.time()):
                pid = s["profileId"]
                d = dash.get(pid)
                if not d or is_running(pid):
                    continue        # gone, or busy — try again next tick
                engine_store.save(to_engine_profile(d))
                _spawn_warmup(pid, s.get("minutes", 5), s.get("preset"))
                sched.mark_run(s["id"], time.time())
        except Exception:           # pragma: no cover - a bad tick must not kill the loop
            pass

    if start_scheduler:
        stop = threading.Event()

        def _loop():
            while not stop.wait(60):     # check once a minute
                _scheduler_tick()

        threading.Thread(target=_loop, daemon=True, name="numera-scheduler").start()

        @app.on_event("shutdown")
        def _stop_scheduler():
            stop.set()
