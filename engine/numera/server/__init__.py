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
from ..handshake import (READY_ENV, READY_TIMEOUT, StartupFailed, await_ready,
                         ready_channel, terminate)
from ..paths import child_env, session_lease
from .stores import DashboardStore, DuplicateToken, ProxyPool, TokenStore
from .convert import (_parse_lines, _rel_time, _parse_startup_urls, _stable_seed,
                      dashboard_coherence, to_dashboard_profile,
                      to_engine_profile)
from .spa import _mount_dashboard, dashboard_dir
from . import (routes_browser, routes_config, routes_profiles,
               routes_proxies, routes_schedules)


#: What a child may have been started for. A registry that only held
#: processes could not tell them apart, so the scheduler's reaper matched on
#: `profileId` alone and wrote a schedule's outcome from an ordinary launch
#: being closed - or wrote both rows when a profile had two schedules.
KINDS = ("launch", "attach", "manual_warmup", "scheduled_warmup")


@dataclass
class Child:
    """One session process this server started, and what it was started for.

    `schedule_id` is set for `scheduled_warmup` and nothing else, and it names
    the exact occurrence that spawned this child rather than "some schedule on
    this profile". Nothing else may be attributed to a schedule: a user closing
    a launch is not a warm-up result, and a profile with two schedules has two
    rows that are not interchangeable.

    `finished_at` is stamped by the one finalisation path (`_finish`) as the
    child leaves, and is what `_last_active` is set from - so the moment the
    browser stopped and the moment recorded against the profile are the same
    moment, whichever caller noticed the exit first.
    """

    proc: subprocess.Popen
    kind: str
    started_at: float
    schedule_id: Optional[str] = None
    finished_at: Optional[float] = None


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
    stop_session: Callable[[str], bool]
    decorate: Callable[[dict], dict]
    spawn_owner: Callable[..., object]
    materialize_current: Callable[[str], object]
    reap_children: Callable[[], None]


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


def create_app(data_dir: Optional[str] = None, start_scheduler: bool = True,
               password: Optional[str] = None):
    """Build the app.

    `password` is the master password the caller already unlocked with. Without
    it the server built its own locked store and every browser it launched went
    out with no proxy password - `numera --password X serve` unlocked a store
    that was then thrown away.
    """
    _require_fastapi()
    from fastapi import FastAPI, HTTPException
    from fastapi.middleware.cors import CORSMiddleware

    from ..scheduler import ScheduleStore, due_schedules, next_run_at

    engine_store = ProfileStore(data_dir, password=password)
    dash = DashboardStore(engine_store.root, engine_store)
    pool = ProxyPool(engine_store.root, engine_store)
    tokens = TokenStore(engine_store.root)
    sched = ScheduleStore(engine_store.root)
    # Reachable for tests and introspection: whether this app's store is
    # unlocked is otherwise invisible from outside, and that is exactly the
    # thing that was silently wrong.
    app_state_stores = {"engine_store": engine_store, "dash": dash, "pool": pool}
    running: dict[str, Child] = {}   # pid -> the session process we started
    attached: dict[str, dict] = {}   # pid -> CDP connection info while attached

    # One place for "run this when the server stops". `on_event` is deprecated
    # in FastAPI and each use of it warned; more to the point, the cleanup was
    # registered from inside a route module, where it was easy to attach it to
    # the wrong branch - and it had been, so a server started without the
    # scheduler left its browsers running.
    shutdown_hooks: list = []
    shutdown_done: list = []

    def run_shutdown():
        """Run every shutdown hook once, whoever asks.

        The lifespan is not the only caller any more: when the desktop shell is
        force-killed the watcher leaves through `os._exit`, which runs no
        atexit handler and no lifespan teardown. That took the engine down and
        left every browser it had launched running - orphaned, holding the
        profile directories, and invisible to the next launch.
        """
        if shutdown_done:
            return
        shutdown_done.append(True)
        for fn in shutdown_hooks:
            try:
                fn()
            except Exception:   # noqa: BLE001 - one bad hook must not strand the rest
                pass

    @asynccontextmanager
    async def lifespan(_app):
        yield
        run_shutdown()

    app = FastAPI(title="Numera Browser API", version=__version__, lifespan=lifespan)
    app.state.add_shutdown = shutdown_hooks.append
    app.state.run_shutdown = run_shutdown

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

    def _touch_last_active(pid: str, when: float) -> None:
        """Stamp `_last_active` and nothing else.

        This used to read the whole record, set one field and save it all back.
        The read and the write straddled a browser session, so any edit made
        while the browser was open - a rename, a note - was silently reverted
        the moment it closed. A patch writes the one field it owns.
        """
        dash.patch(pid, lambda current: ({**current, "_last_active": when}, None))

    def _finish(pid: str, child: Child, code) -> None:
        """The one path every child this server started leaves by.

        Three things have to happen when a session process ends, and they used
        to happen in two places that each ran on their own clock: `is_running`
        popped the child and stamped `_last_active`, while the scheduler's
        reaper popped it and wrote a schedule outcome. Whichever got there
        first took the record away from the other - so a warm-up the reaper saw
        first never stamped `_last_active`, and an ordinary launch the reaper
        saw at all was written into the `lastOutcome` of every schedule on that
        profile. One path, in one order, once per child.

        The schedule outcome is written for the exact occurrence that spawned
        this child and for no other. Everything here is best-effort on purpose:
        a record deleted mid-session, or a store that is unwell, must not
        strand the rest of the cleanup - which is what actually frees the
        profile for the next launch.
        """
        if running.get(pid) is child:
            running.pop(pid, None)
        attached.pop(pid, None)
        child.finished_at = time.time()
        try:
            _touch_last_active(pid, child.finished_at)   # the browser closed
        except Exception:      # noqa: BLE001 - see above
            pass
        if child.kind == "scheduled_warmup" and child.schedule_id:
            try:
                sched.record(child.schedule_id,
                             lastOutcome="ok" if code == 0 else "failed",
                             lastError=None if code == 0 else f"warm-up exited {code}")
            except Exception:  # noqa: BLE001 - see above
                pass

    def reap_children() -> None:
        """Finalise every child that has exited since the last look.

        The scheduler runs this on its own clock, so a warm-up's outcome is
        recorded whether or not anything happens to ask whether that profile is
        running.
        """
        for pid, child in list(running.items()):
            code = child.proc.poll()
            if code is not None:
                _finish(pid, child, code)

    def is_running(pid: str) -> bool:
        child = running.get(pid)
        if child is not None:
            code = child.proc.poll()
            if code is None:
                return True
            _finish(pid, child, code)    # exited: the browser was closed
        # The local registry only knows what *this* server started. Another
        # server, a desktop or a CLI can own the same profile, and reporting
        # "stopped" for a profile someone else has open is how two processes
        # end up in one Chromium user-data directory.
        return session_lease(engine_store.root, pid).held_by_other_process()

    def stop_session(pid: str) -> bool:
        """End a profile's browser, and say whether there was one of ours.

        Lives here rather than in a route module because three of them need it:
        `POST /stop`, `DELETE /profiles/{id}` - which must not leave a browser
        running for a profile that no longer exists - and shutdown.

        The return value is the honest half. This can only terminate a child
        *this* server started; a lease held by another server, a desktop or a
        CLI is untouched by it, and `POST /stop` answered 200 for that anyway.
        False means nothing here was stopped, so the caller can find out why
        before claiming that it was.
        """
        child = running.get(pid)
        if child is None:
            attached.pop(pid, None)
            return False
        proc = child.proc
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
        _finish(pid, child, proc.poll())
        return True

    # One decision at a time per profile. Two HTTP launches for the same
    # profile arrive on two threadpool workers, and both used to spawn: the
    # loser's `running[pid] = proc` overwrote the winner's, so `/stop`
    # terminated a process that owned nothing while the real browser stayed up.
    #
    # ponytail: one lock per profile ever launched, never pruned. A few dozen
    # bytes each; prune if a data root ever holds enough profiles to matter.
    spawn_locks: dict[str, threading.Lock] = {}
    spawn_guard = threading.Lock()

    def _spawn_lock(pid: str) -> threading.Lock:
        with spawn_guard:
            return spawn_locks.setdefault(pid, threading.Lock())

    def spawn_owner(pid: str, cli: list, *, kind: str,
                    schedule_id: Optional[str] = None,
                    timeout: float = READY_TIMEOUT):
        """Start a session process, and register it only once its browser is up.

        `Popen` returning proves a process started, nothing else. The child
        takes the session lease and re-reads the vault seconds later, and later
        still actually opens a browser - so the old code acknowledged "running"
        for launches about to exit with `SessionBusy`, `VaultLocked` or a
        missing browser executable, and the scheduler recorded runs whose
        browser never opened. Now the parent waits for the child to say that a
        browser is open on the profile - see handshake.py - and a child that
        never gets there is terminated and never registered.

        `kind` says what the child is for, and it is not optional: the
        scheduler's reaper decides from it whether an exit belongs to a
        schedule at all, so a child registered as the wrong thing writes a
        result nobody produced. `schedule_id` names the exact occurrence a
        `scheduled_warmup` came from.

        Returns the registration, or None when *this* server already has a live
        child for the profile - the second of two simultaneous requests, which
        the caller may read as idempotent success. A lease held by some other
        process is different and raises instead: this server cannot stop what
        it did not start, so answering "running" would offer a `/stop` that
        does nothing.
        """
        if kind not in KINDS:
            raise ValueError(f"unknown child kind {kind!r}; "
                             f"known: {', '.join(KINDS)}")
        with _spawn_lock(pid):
            ours = running.get(pid)
            if ours is not None and ours.proc.poll() is None:
                return None
            if is_running(pid):
                raise StartupFailed(
                    "another process has this profile's browser session open",
                    "busy")
            with ready_channel() as ready:
                env = {**child_env(engine_store._password), READY_ENV: str(ready)}
                proc = subprocess.Popen(cli, env=env)
                try:
                    await_ready(proc, ready, timeout)
                except BaseException:
                    terminate(proc)      # never leave a child nobody owns
                    raise
            child = Child(proc=proc, kind=kind, started_at=time.time(),
                          schedule_id=schedule_id)
            running[pid] = child
            return child

    def materialize_current(pid: str):
        """Re-derive the engine copy from the dashboard record as it is now.

        A thin pass-through to the core coordinator, which holds the reasoning:
        the routes used to save an engine profile built from a snapshot read
        before a browser ran, reverting whatever had been edited meanwhile.
        """
        from ..repo import repo

        return repo(engine_store).materialize_current(pid)

    def decorate(d: dict) -> dict:
        # Attach live run state. Coherence is computed by the dashboard itself
        # (its meter mirrors the validator), so we don't duplicate it here.
        d = dict(d)
        running_now = is_running(d["id"])
        d["status"] = "running" if running_now else "stopped"
        d["lastActive"] = "now" if running_now else _rel_time(d.get("_last_active"))
        return d

    app.state.spawn_owner = spawn_owner
    app.state.materialize_current = materialize_current
    # The session registry and the sweep over it, for the same reason the
    # scheduler's tick is exposed: what the registry holds - and that it holds
    # nothing after a child dies - is not observable through any route, and a
    # stale entry is exactly the failure this bookkeeping exists to prevent.
    app.state.running = running
    app.state.reap_children = reap_children

    ctx = _Ctx(engine_store=engine_store, dash=dash, pool=pool, tokens=tokens, sched=sched,
               running=running, attached=attached, start_scheduler=start_scheduler,
               is_running=is_running, stop_session=stop_session, decorate=decorate,
               spawn_owner=spawn_owner, materialize_current=materialize_current,
               reap_children=reap_children)
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
    for _name, _store in app_state_stores.items():
        setattr(app.state, _name, _store)
    return app



# `""` is deliberately NOT here. An empty host binds every interface - the exact
# case this guard exists to refuse - and listing it as loopback waved
# `--host ""` through as if it were localhost.
LOOPBACK = {"127.0.0.1", "localhost", "::1", "0:0:0:0:0:0:0:1"}


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
          verbose: bool = False, password: Optional[str] = None):
    """Start the API server (blocking).

    `password` carries the caller's already-unlocked vault into the app; see
    `create_app`.
    """
    _require_fastapi()
    import uvicorn

    from .. import firstrun, logs
    from ..paths import resolve_data_dir, watch_parent

    root = resolve_data_dir(data_dir)
    # Normalise before the guard so an explicitly empty host is judged as what
    # it actually does - bind everything - rather than skipped as loopback.
    host = (host or "").strip() or "127.0.0.1"
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

    app = create_app(str(root), password=password)

    # The desktop shell sets NUMERA_PARENT_PID. Without this, a force-killed
    # shell on macOS left the engine serving on its port, and the next launch
    # started a second engine against the same portable data root.
    #
    # Armed after the app exists so it can take the browsers with it: the
    # default `on_gone` is a bare `os._exit`, which skips the lifespan and left
    # every launched browser orphaned. Nothing before this point has started
    # one, or bound the port, so the window it gives up is empty.
    def shell_is_gone():
        try:
            app.state.run_shutdown()
        finally:
            os._exit(0)

    watch_parent(on_gone=shell_is_gone)

    where = "the dashboard and API" if dashboard_dir() else "the API"
    print(f"Numera Browser: {where} on http://{host}:{port}  (data: {root})")
    if log_path:
        print(f"  log: {log_path}")
    log.info("listening on %s:%s", host, port)
    uvicorn.run(app, host=host, port=port, log_level="debug" if verbose else "info")
