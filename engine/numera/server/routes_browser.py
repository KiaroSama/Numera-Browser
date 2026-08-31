"""Everything that runs a real browser: launch, stop, cookies, the probes
(proxy, trust, TLS, DNS), attach and warm-up.

These are the slow routes. Several shell out to the CLI rather than
calling Playwright in-process, because driving its sync API from inside
an async web server deadlocks."""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Optional
from .. import cookies as cookies_mod
from .convert import OS_TO_DASH, to_engine_profile
from ..paths import child_env


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    engine_store, dash, running, attached, is_running, decorate, stop_session = ctx.engine_store, ctx.dash, ctx.running, ctx.attached, ctx.is_running, ctx.decorate, ctx.stop_session

    @app.post("/api/profiles/{pid}/launch")
    def launch_profile(pid: str, body: Optional[dict] = None):
        """Open the profile in its own browser process.

        Pass ``{"headless": true}`` (or set ``headless_launch`` in the config)
        on a machine with no display, where a windowed launch cannot start at
        all. The browser stays up until POST /stop either way."""
        d = dash.get(pid)
        if not d:
            raise HTTPException(404, "profile not found")
        if is_running(pid):
            return decorate(d)
        # Materialise a real engine profile, then open it in its own process.
        engine_store.save(to_engine_profile(d))
        headless = (body or {}).get("headless")
        if headless is None:
            headless = engine_store.get_headless_launch()
        cli = [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root),
               "launch", pid]
        if headless:
            cli.append("--headless")
        proc = subprocess.Popen(cli, env=child_env(engine_store._password))
        running[pid] = proc
        return decorate(d)

    @app.post("/api/profiles/{pid}/stop")
    def stop_profile(pid: str):
        d = dash.get(pid)
        if not d:
            raise HTTPException(404, "profile not found")
        stop_session(pid)
        return decorate(d)

    def _run_cli(*cli_args: str, allow_fail: bool = False) -> str:
        """Run a numera subcommand in its own process (same reasoning as launch:
        keeps sync Playwright out of the web server) and return its output.

        ``allow_fail`` is for commands whose non-zero exit means "the thing you
        asked about is unhealthy", not "the command broke" — a failing proxy test
        still has a result worth showing.
        """
        proc = subprocess.run(
            [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root), *cli_args],
            capture_output=True, text=True,
            # The unlock reaches the child by environment, never argv: a
            # password on a command line is readable by every local process.
            # Without this a server unlocked with --password spawned locked
            # children, so probes and cookie exports silently lost the proxy.
            env=child_env(engine_store._password),
        )
        if proc.returncode != 0 and not (allow_fail and proc.stdout.strip()):
            raise HTTPException(500, (proc.stdout + proc.stderr).strip() or "command failed")
        return proc.stdout

    def _prepare(pid: str) -> dict:
        """Fetch a dashboard profile, refuse if its session is busy, and make sure
        the engine-side profile exists so a subcommand can open it."""
        d = dash.get(pid)
        if not d:
            raise HTTPException(404, "profile not found")
        if is_running(pid):
            raise HTTPException(409, "stop the profile first — its session is in use")
        engine_store.save(to_engine_profile(d))
        return d

    def _free_port() -> int:
        """A currently-unused local TCP port for a DevTools endpoint."""
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
            s.bind(("127.0.0.1", 0))
            return s.getsockname()[1]

    @app.get("/api/profiles/{pid}/cookies")
    def export_cookies(pid: str):
        _prepare(pid)
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "cookies.json"
            _run_cli("cookies", "export", pid, str(out))
            data = json.loads(out.read_text(encoding="utf-8"))
        return {"cookies": data.get("cookies", [])}

    @app.post("/api/profiles/{pid}/cookies")
    def import_cookies(pid: str, body: dict):
        _prepare(pid)
        # Accept either parsed cookies or the raw text of a file the user picked,
        # so the browser doesn't have to know which format it is holding.
        raw = body.get("text")
        if raw is None:
            raw = json.dumps({"cookies": body.get("cookies") or []})
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "cookies.json"
            src.write_text(raw, encoding="utf-8")
            args = ["cookies", "import", pid, str(src)]
            if body.get("clear"):
                args.append("--clear")
            _run_cli(*args)
        return {"ok": True, "imported": len(cookies_mod.parse(raw))}

    @app.post("/api/profiles/{pid}/proxy-test")
    def proxy_test(pid: str):
        _prepare(pid)
        # --json makes the CLI the single implementation for both front doors.
        return json.loads(_run_cli("proxy", "test", pid, "--json", allow_fail=True))

    @app.post("/api/profiles/{pid}/trust")
    def trust(pid: str):
        d = _prepare(pid)
        rep = json.loads(_run_cli("trust", pid, "--json"))
        # Remember the last grade so the Health page can show it without
        # re-launching every profile (a trust run opens a real browser).
        d["_trust"] = {"grade": rep.get("grade"), "score": rep.get("score"),
                       "at": time.time()}
        dash.save(d)
        return rep

    @app.post("/api/profiles/{pid}/tls")
    def tls(pid: str):
        _prepare(pid)
        return json.loads(_run_cli("tls", pid, "--json", allow_fail=True))

    @app.post("/api/profiles/{pid}/dns")
    def dns(pid: str):
        _prepare(pid)
        return json.loads(_run_cli("dns", pid, "--json", allow_fail=True))

    @app.post("/api/profiles/{pid}/attach")
    def attach_profile(pid: str, body: Optional[dict] = None):
        """Open the profile with a Chrome DevTools endpoint your own automation
        can drive, and return the connection details.

        The browser stays open — attached and 'running' — until it's stopped
        (POST /stop), exactly like a launch. The attach itself runs as a
        subprocess that owns the window; we read the DevTools endpoint's own
        /json/version over HTTP to hand back the WebSocket URL. Pass
        ``{"headless": true}`` to attach without a visible window."""
        from ..automation import CHROMIUM_ENGINES, cdp_info, snippets
        d = dash.get(pid)
        if not d:
            raise HTTPException(404, "profile not found")
        if is_running(pid):
            info = attached.get(pid)
            if info:
                return info                  # already attached — idempotent
            raise HTTPException(409, "stop the profile first — its session is in use")
        engine = d.get("engine") or engine_store.get_default_engine()
        if engine not in CHROMIUM_ENGINES:
            raise HTTPException(400, f"the '{engine}' engine has no DevTools to attach "
                                     f"to — use one of: {', '.join(CHROMIUM_ENGINES)}")
        engine_store.save(to_engine_profile(d))
        port = _free_port()
        cli = [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root),
               "attach", pid, "--port", str(port)]
        if (body or {}).get("headless"):
            cli.append("--headless")
        running[pid] = subprocess.Popen(cli, env=child_env(engine_store._password))
        try:
            info = cdp_info(port)            # waits for DevTools to answer
        except RuntimeError as e:
            stop_session(pid)
            raise HTTPException(500, str(e))
        ws = info.get("webSocketDebuggerUrl", "")
        result = {"pid": pid, "port": port, "wsUrl": ws,
                  "headless": bool((body or {}).get("headless")),
                  "browser": info.get("Browser", ""), "snippets": snippets(port, ws)}
        attached[pid] = result
        return result

    @app.post("/api/profiles/{pid}/warmup")
    def warmup(pid: str, body: Optional[dict] = None):
        d = _prepare(pid)
        body = body or {}
        minutes = float(body.get("minutes", 5))
        cli = [sys.executable, "-m", "numera", "--data-dir", str(engine_store.root),
               "warmup", pid, "--minutes", str(minutes), "--headless"]
        if body.get("preset"):
            cli += ["--preset", str(body["preset"])]
        # Warm-up runs for minutes, so it goes in the background exactly like a
        # launch — the profile shows as running and "stop" ends it.
        running[pid] = subprocess.Popen(cli, env=child_env(engine_store._password))
        return decorate(d)

    @app.post("/api/profiles/{pid}/align")
    def align(pid: str):
        """Auto-adjust: rewrite the profile to match what its browser presents,
        then write the aligned values back so the dashboard profile stays put."""
        d = _prepare(pid)
        res = json.loads(_run_cli("align", pid, "--json"))
        fp = res.get("fingerprint") or {}
        if fp.get("os"):
            d["os"] = OS_TO_DASH.get(fp["os"], d.get("os"))
        for src, dst in (("userAgent", "userAgent"), ("browserVersion", "browserVersion"),
                        ("cores", "cores"), ("memory", "memory"), ("screen", "screen"),
                        ("webglVendor", "webglVendor"), ("webglRenderer", "webglRenderer"),
                        ("cameras", "mediaVideo"), ("microphones", "mediaAudio"),
                        ("locale", "locale"), ("timezone", "timezone")):
            if fp.get(src) is not None:
                d[dst] = fp[src]
        after = res.get("after") or {}
        if after.get("grade"):
            d["_trust"] = {"grade": after["grade"], "score": after.get("score"),
                           "at": time.time()}
        res["profile"] = decorate(dash.save(d))
        return res
