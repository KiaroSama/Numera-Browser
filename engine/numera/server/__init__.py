"""
HTTP API for the dashboard.

The React dashboard is a thin front-end; this module is the bridge that lets it
drive the *real* engine. It wraps :class:`ProfileStore`, the fingerprint
generator/validator and the launcher behind a small JSON API, and it manages the
lifecycle of launched browsers.

Design notes
------------
* The dashboard has a richer profile shape than the engine (folders, per-signal
  spoof modes, media counts, …). To keep the UI lossless we store the dashboard
  profile JSON as-is in ``<data-dir>/dashboard`` and only convert to an engine
  :class:`Fingerprint`/:class:`Profile` at launch time.
* Launching runs ``numera launch <id>`` as a subprocess. That sidesteps the
  hazards of driving Playwright's sync API from inside an async web server, and
  makes "stop" a clean process terminate. A profile is "running" as long as its
  subprocess is alive.

Run it with:  ``numera serve``  (see ``cli.py``).
"""

from __future__ import annotations

import json
import os
import secrets
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Optional
from .. import __version__
from .. import cookies as cookies_mod
from .. import devices
from ..fingerprint import validate
from ..models import Fingerprint, Proxy
from .. import folders
from ..store import ProfileStore
from contextlib import asynccontextmanager
from dataclasses import dataclass
from typing import Callable
# Re-exported so `numera.server` keeps its whole surface: `cli/`, `mcp.py`
# and the test suite import these by name.
from .stores import DashboardStore, ProxyPool, TokenStore
from .convert import (_parse_lines, _rel_time, _parse_startup_urls, _stable_seed,
                      dashboard_coherence, to_dashboard_profile,
                      to_engine_profile)
from .spa import _mount_dashboard, dashboard_dir
from . import (routes_browser, routes_config, routes_profiles,
               routes_proxies, routes_schedules)


@dataclass
class _Ctx:
    """The state the route modules share.

    Every route used to be a closure over `create_app`'s locals. Passing them
    in one object keeps that exact binding environment available to each
    module's `register()`, so no route body had to change when they moved.
    """

    engine_store: object
    dash: object
    pool: object
    tokens: object
    sched: object
    running: dict
    attached: dict
    start_scheduler: bool
    is_running: Callable[[str], bool]
    stop_session: Callable[[str], None]
    decorate: Callable[[dict], dict]


def _require_fastapi():
    try:
        import fastapi  # noqa: F401
        import uvicorn  # noqa: F401
    except ImportError as e:  # pragma: no cover - environment dependent
        raise RuntimeError(
            "The API server needs FastAPI and Uvicorn.\n"
            'Install them with:  pip install -e ".[api]"'
        ) from e


# --------------------------------------------------------------------------
# OS name mapping between the dashboard ("Windows") and engine ("windows").
# --------------------------------------------------------------------------
OS_TO_ENGINE = {"Windows": "windows", "macOS": "macos", "Linux": "linux",
                "Android": "android", "iOS": "ios"}
OS_TO_DASH = {v: k for k, v in OS_TO_ENGINE.items()}


def create_app(data_dir: Optional[str] = None, start_scheduler: bool = True):
    _require_fastapi()
    from fastapi import FastAPI, HTTPException
    from fastapi.middleware.cors import CORSMiddleware

    from ..scheduler import ScheduleStore, due_schedules, next_run_at

    engine_store = ProfileStore(data_dir)
    dash = DashboardStore(engine_store.root, engine_store)
    pool = ProxyPool(engine_store.root, engine_store)
    tokens = TokenStore(engine_store.root)
    sched = ScheduleStore(engine_store.root)
    running: dict[str, subprocess.Popen] = {}
    attached: dict[str, dict] = {}   # pid -> CDP connection info while attached

    # One place for "run this when the server stops". `on_event` is deprecated
    # in FastAPI and each use of it warned; more to the point, the cleanup was
    # registered from inside a route module, where it was easy to attach it to
    # the wrong branch - and it had been, so a server started without the
    # scheduler left its browsers running.
    shutdown_hooks: list = []

    @asynccontextmanager
    async def lifespan(_app):
        yield
        for fn in shutdown_hooks:
            try:
                fn()
            except Exception:   # noqa: BLE001 - one bad hook must not strand the rest
                pass

    app = FastAPI(title="Numera Browser API", version=__version__, lifespan=lifespan)
    app.state.add_shutdown = shutdown_hooks.append

    # The API can create profiles, launch browsers and export cookies, so it is
    # only safe unauthenticated because it binds to localhost. Setting
    # NUMERA_API_TOKEN turns on bearer auth for anything that leaves the
    # machine (a server behind a tunnel, a reverse proxy, an agent on the LAN)
    # and narrows CORS at the same time, since "*" plus a token is a token any
    # web page could spend.
    api_token = (os.environ.get("NUMERA_API_TOKEN") or "").strip()

    def auth_required() -> bool:
        """Read the current state, do not remember it.

        This used to be decided once, here, at startup - so minting the first
        named token protected nothing until the process restarted, while the UI
        said the API now required one. The token store is a small JSON file; a
        read per request is cheaper than being wrong about this.
        """
        return bool(api_token) or bool(tokens.list())

    # Same-origin by default, and no wildcard unless someone asks for one.
    #
    # The previous version picked the list once, here: a server that started
    # open kept `allow_origins=["*"]` for its whole life, so minting the first
    # token locked the API while still telling every web page it could send
    # credentials to it. Deciding per request is possible but pointless - the
    # engine serves the dashboard itself, so the normal case needs no CORS at
    # all. A dev server against this engine sets NUMERA_ALLOWED_ORIGINS
    # explicitly (`http://localhost:5173`), and `*` remains available for
    # someone who genuinely wants it and has read what it means.
    allow_origins = _parse_lines(os.environ.get("NUMERA_ALLOWED_ORIGINS") or "")
    app.add_middleware(
        CORSMiddleware,
        allow_origins=allow_origins,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Only `/api/*` is protected. Gating the SPA too would leave no page on
    # which to enter a token, and the shell itself carries nothing secret.
    @app.middleware("http")
    async def _require_token(request, call_next):
        # Preflight carries no Authorization header by design.
        if request.method == "OPTIONS" or not request.url.path.startswith("/api/"):
            return await call_next(request)
        # One public endpoint, deliberately. The desktop shell polls it to know
        # the engine is up, and a 401 there meant an authenticated engine could
        # never be reached - not even to show the screen that asks for a token.
        if request.url.path.rstrip("/") == "/api/ready":
            return await call_next(request)
        if not auth_required():
            return await call_next(request)
        sent = request.headers.get("authorization", "")
        if sent.lower().startswith("bearer "):
            sent = sent[7:]
        else:
            sent = request.headers.get("x-numera-token", "")
        sent = sent.strip()
        # The env var stays valid alongside named tokens: it is the credential
        # the dashboard and any existing script already use, and pulling it out
        # from under them the moment a named token is created would be a trap.
        ok = bool(api_token) and secrets.compare_digest(sent, api_token)
        if not ok:
            ok = tokens.verify(sent) is not None
        if not ok:
            from fastapi.responses import JSONResponse
            return JSONResponse({"detail": "invalid or missing API token"}, status_code=401)
        return await call_next(request)

    def is_running(pid: str) -> bool:
        proc = running.get(pid)
        if proc is None:
            return False
        if proc.poll() is not None:      # exited (browser was closed)
            running.pop(pid, None)
            # Session just ended — record when it was last active.
            stored = dash.get(pid)
            if stored is not None:
                stored["_last_active"] = time.time()
                dash.save(stored)
            return False
        return True

    def stop_session(pid: str):
        """End a profile's browser and forget any CDP session it was serving.

        Lives here rather than in a route module because two of them need it:
        `POST /stop` and `DELETE /profiles/{id}`, which must not leave a browser
        running for a profile that no longer exists.
        """
        proc = running.pop(pid, None)
        attached.pop(pid, None)
        if proc and proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()

    def decorate(d: dict) -> dict:
        # Attach live run state. Coherence is computed by the dashboard itself
        # (its meter mirrors the validator), so we don't duplicate it here.
        d = dict(d)
        running_now = is_running(d["id"])
        d["status"] = "running" if running_now else "stopped"
        d["lastActive"] = "now" if running_now else _rel_time(d.get("_last_active"))
        return d

    ctx = _Ctx(engine_store=engine_store, dash=dash, pool=pool, tokens=tokens, sched=sched,
               running=running, attached=attached, start_scheduler=start_scheduler,
               is_running=is_running, stop_session=stop_session, decorate=decorate)
    @app.get("/api/ready")
    def ready():
        """Is the engine up, and does it want a token? Public by design.

        It names the product and the version and says whether auth is on. That
        is what a launcher needs and it is not worth protecting: anything that
        can reach this port can already see a socket answering on it. Nothing
        about the user's profiles, proxies or tokens is here.
        """
        return {"ok": True, "service": "numera-browser", "version": __version__,
                "authRequired": auth_required()}

    routes_config.register(app, ctx)
    routes_profiles.register(app, ctx)
    routes_browser.register(app, ctx)
    routes_proxies.register(app, ctx)
    routes_schedules.register(app, ctx)

    _mount_dashboard(app)   # must be last: its catch-all would shadow /api/*
    return app



LOOPBACK = {"127.0.0.1", "localhost", "::1", "0:0:0:0:0:0:0:1", ""}


def _refuse_insecure_bind(host: str, root) -> "Optional[str]":
    """Why this bind must not happen, or None if it may.

    The API creates profiles, launches browsers and exports cookies. It is only
    safe unauthenticated because nothing outside the machine can reach it, so
    the moment it stops being loopback it needs a credential. Returns the
    message rather than raising, so the CLI and the tests can use the same rule.
    """
    import os as _os

    from .stores import TokenStore as _TokenStore

    if str(host).strip().lower() in LOOPBACK:
        return None
    if (_os.environ.get("NUMERA_API_TOKEN") or "").strip():
        return None
    try:
        if _TokenStore(root).list():
            return None
    except Exception:  # noqa: BLE001 - an unreadable store is not a credential
        pass
    if (_os.environ.get("NUMERA_ALLOW_INSECURE_BIND") or "").strip():
        return None
    return (
        f"Refusing to serve on {host} without authentication.\n"
        f"  This API can create profiles, launch browsers and export cookies, and\n"
        f"  on a non-loopback address anything that can reach the port can use it.\n"
        f"  Fix it with either:\n"
        f"    numera token create \"<what it is for>\"   (recommended)\n"
        f"    set NUMERA_API_TOKEN=<secret>\n"
        f"  Then set NUMERA_ALLOWED_ORIGINS if a browser page will call it.\n"
        f"  To bind anyway - only when something else in front already\n"
        f"  authenticates - set NUMERA_ALLOW_INSECURE_BIND=1."
    )


def serve(host: str = "127.0.0.1", port: int = 8787, data_dir: Optional[str] = None,
          verbose: bool = False):
    """Start the API server (blocking)."""
    _require_fastapi()
    import uvicorn

    from .. import firstrun, logs
    from ..paths import resolve_data_dir

    root = resolve_data_dir(data_dir)
    refusal = _refuse_insecure_bind(host, root)
    if refusal:
        raise SystemExit(refusal)
    log_path = logs.setup(root, verbose=verbose)
    log = logs.get("serve")

    # Before anything opens the store: carry an older install's profiles across.
    imported = firstrun.run_if_needed(root)
    if imported:
        log.info("first run: imported from %s - copied %s, skipped %s, %d profile(s)",
                 imported["source"], imported["copied"] or "nothing",
                 imported["skipped"] or "nothing", imported["profiles"])
        for problem in imported["failed"]:
            log.warning("first-run import problem: %s", problem)
        if imported["copied"]:
            print(f"  Imported {imported['profiles']} profile(s) from {imported['source']}")

    app = create_app(str(root))
    where = "the dashboard and API" if dashboard_dir() else "the API"
    print(f"Numera: {where} on http://{host}:{port}  (data: {root})")
    if log_path:
        print(f"  log: {log_path}")
    log.info("listening on %s:%s", host, port)
    uvicorn.run(app, host=host, port=port, log_level="debug" if verbose else "info")
