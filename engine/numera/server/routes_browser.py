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
import threading
import time
from pathlib import Path
from typing import Optional
from .. import cookies as cookies_mod
from .. import proctree, selfexec
from .convert import OS_TO_DASH
from ..handshake import ChildTerminationFailed, StartupFailed
from .. import sessions
from ..paths import child_env, session_lease


class HelperTimeout(RuntimeError):
    """A finite browser-backed helper outran its deadline and was ended."""


#: How long one browser-backed helper may take before it is ended.
#:
#: These are the *finite* probes - cookies export, proxy test, trust, tls, dns,
#: align - not a session. Each starts a browser, so the budget has to cover a
#: cold start on a slow machine; five minutes is far above anything healthy and
#: only fires on a real stall. A long-lived session is not routed through here:
#: it goes to `spawn_owner` and keeps its own managed lifecycle, which is why
#: this is not a blanket timeout that could kill a browsing session.
#:
#: Overridable because "slow machine" is not a fixed quantity, and a user whose
#: probe legitimately takes longer should raise the budget rather than lose the
#: deadline entirely.
HELPER_TIMEOUT = float(os.environ.get("NUMERA_HELPER_TIMEOUT") or 300)

#: Every helper tree started by this process and not yet proved finished.
#:
#: Sessions have `running`, which the shutdown hook walks; helpers had nothing.
#: A helper that was mid-probe when the server stopped therefore left its
#: browser behind - a process nothing was accounting for, which is the one
#: condition `proctree` exists to prevent. They cannot share `running`: that
#: maps a session pid to a `stop_session` that stamps `_last_active` and closes
#: a scheduled occurrence, none of which is true of a probe.
_LIVE: set = set()
_LIVE_LOCK = threading.Lock()


def live_helpers() -> list:
    """The helper trees currently running. For shutdown and for tests."""
    with _LIVE_LOCK:
        return list(_LIVE)


def _release(entry) -> None:
    with _LIVE_LOCK:
        _LIVE.discard(entry)


def _close_pipes(proc) -> None:
    """Close what `communicate` would have closed had it returned.

    On the timeout path it does not return, so both pipes stayed open: one pair
    of file objects and OS handles per wedged helper, held until the garbage
    collector happened to run. Closing them directly is bounded; a second
    `communicate` to drain them is not - it would block again on the same
    child that just failed to finish.
    """
    for pipe in (proc.stdin, proc.stdout, proc.stderr):
        if pipe is None:
            continue
        try:
            if not pipe.closed:
                pipe.close()
        except OSError:                                  # pragma: no cover
            pass


def _end_quietly(proc, tree) -> bool:
    """End a helper tree, reporting whether it could be proved gone."""
    try:
        proctree.end_session(proc, tree)
    except ChildTerminationFailed:
        return False
    except Exception:                                    # noqa: BLE001
        return False                                     # pragma: no cover
    return True


def end_helpers() -> list:
    """End every helper still running. Returns those that could not be ended.

    Registered as an application shutdown hook, beside the one that stops
    sessions. Returning the stragglers rather than raising keeps a single stuck
    probe from stranding the ones after it, while still letting the caller say
    something truthful about what is left.
    """
    stuck = []
    for proc, tree in live_helpers():
        if not _end_quietly(proc, tree):
            stuck.append(getattr(proc, "pid", None))
        _close_pipes(proc)
        _release((proc, tree))
    return stuck


def run_helper(cli, timeout: float = None, env=None):
    """Run a finite helper command, bounded, and end its whole tree if it stalls.

    `subprocess.run(timeout=...)` would bound the *wait* and not the work: it
    kills the direct child, and the browser that child started is three
    processes further down and survives it. So the helper is spawned into a
    tree it owns (`proctree.spawn`) and, on timeout, that tree is ended - which
    is the same primitive a session stop uses, and the only one that can say
    the browser is actually gone.

    Returns a `CompletedProcess`, including for a non-zero exit: a failing
    proxy test is a result its caller reads, not a broken command.
    """
    budget = HELPER_TIMEOUT if timeout is None else timeout
    proc, tree = proctree.spawn(
        cli, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        stdin=subprocess.DEVNULL, text=True, env=env,
    )
    entry = (proc, tree)
    with _LIVE_LOCK:
        _LIVE.add(entry)
    try:
        try:
            out, err = proc.communicate(timeout=budget)
        except subprocess.TimeoutExpired:
            # The wait is over; the work may not be. End the tree before
            # reporting, so the caller is told about a helper that is really
            # finished.
            if not _end_quietly(proc, tree):
                raise HelperTimeout(
                    f"a browser helper did not finish within {budget:.0f}s and "
                    f"could not be fully stopped; its browser may still be "
                    f"running") from None
            raise HelperTimeout(
                f"a browser helper did not finish within {budget:.0f}s and was "
                f"stopped. Raise NUMERA_HELPER_TIMEOUT if this machine needs "
                f"longer, or check whether the profile's proxy is answering."
            ) from None
        return subprocess.CompletedProcess(cli, proc.returncode, out or "", err or "")
    except BaseException:
        # Everything that is neither "finished" nor "timed out": a cancelled
        # request, a KeyboardInterrupt, a failure inside the branch above. None
        # of them ran any cleanup before, so the browser the helper started was
        # simply left running.
        _end_quietly(proc, tree)
        raise
    finally:
        # `communicate` closes the pipes when it returns and not when it
        # raises, so this is the only path that covers both.
        _close_pipes(proc)
        _release(entry)


def register(app, ctx):
    """Attach these routes to `app`.

    The names are unpacked from `ctx` so each body below sees exactly the
    environment it had inside `create_app` - the bodies are unchanged.
    """
    from fastapi import HTTPException

    engine_store, dash, running, attached, is_running, decorate, stop_session = ctx.engine_store, ctx.dash, ctx.running, ctx.attached, ctx.is_running, ctx.decorate, ctx.stop_session
    spawn_owner, materialize_current = ctx.spawn_owner, ctx.materialize_current

    # Beside the hook that stops sessions. A probe holds a browser exactly as a
    # session does, and until this existed the server could exit while one was
    # mid-flight and leave it running with nothing tracking it.
    app.state.add_shutdown(end_helpers)

    # A session that never started, reported as the status that says why. 423
    # for a locked vault is the literal answer - the profile is there and the
    # request is fine, the server just cannot read its proxy password - and it
    # keeps 409 meaning exactly one thing: someone else owns this profile now.
    _STARTUP_STATUS = {"busy": 409, "locked": 423, "gone": 404}

    def _startup_error(failure: StartupFailed):
        return HTTPException(_STARTUP_STATUS.get(failure.kind, 500), str(failure))

    BUSY = "stop the profile first - its session is in use"
    NOT_OURS = ("another process has this profile's browser open - this server "
                "did not start it and cannot stop it. Close it where it was "
                "started.")

    @app.post("/api/profiles/{pid}/launch")
    def launch_profile(pid: str, body: Optional[dict] = None):
        """Open the profile in its own browser process.

        Pass ``{"headless": true}`` (or set ``headless_launch`` in the config)
        on a machine with no display, where a windowed launch cannot start at
        all. The browser stays up until POST /stop either way."""
        d = dash.get(pid)
        if not d:
            raise HTTPException(404, "profile not found")
        ours = running.get(pid)
        if ours is not None and ours.proc.poll() is None:
            return decorate(d)          # ours, already open - launch is idempotent
        # A lease held by some *other* process is not the same thing, and falls
        # through to `spawn_owner`, which refuses it: this server cannot stop
        # what it did not start, so answering "running" would offer a /stop
        # that does nothing.
        #
        # Materialise a real engine profile from the record as it is *now*,
        # then open it in its own process and wait for that process to say it
        # owns the session. Returning before then reported "running" for
        # launches that were about to exit with a locked vault or a session
        # someone else held.
        materialize_current(pid)
        headless = (body or {}).get("headless")
        if headless is None:
            headless = engine_store.get_headless_launch()
        cli = selfexec.self_command("--data-dir", str(engine_store.root),
                                    "launch", pid)
        if headless:
            cli.append("--headless")
        try:
            spawn_owner(pid, cli, kind="launch")
        except StartupFailed as failure:
            raise _startup_error(failure) from None
        # `spawn_owner` returning None means another request won the race for
        # this profile; it is running either way, which is what was asked for.
        return decorate(dash.get(pid) or d)

    @app.post("/api/profiles/{pid}/stop")
    def stop_profile(pid: str):
        """End the browser this server started for the profile.

        200 used to be unconditional, which made it a lie for the one case that
        matters: `stop_session` can only terminate a local child, so when the
        lease is held by another server, a desktop or a CLI, nothing was
        stopped and the browser is still open - and the dashboard showed the
        profile as stopped anyway. There is no cross-process stop, so the
        answer is the conflict rather than a success nobody produced.
        """
        d = dash.get(pid)
        if not d:
            raise HTTPException(404, "profile not found")
        try:
            stopped = stop_session(pid)
        except ChildTerminationFailed as e:
            # The child would not die. It still holds the profile, and this
            # server still owns the record of it - so the answer is the
            # failure, not a success the browser has not honoured.
            raise HTTPException(500, str(e)) from None
        if not stopped:
            # Nothing of ours was stopped. A lease still on the profile means
            # the browser is open in a process this server has no way to reach.
            if session_lease(engine_store.root, pid).held_by_other_process()                     or sessions.busy(engine_store.root, pid):
                raise HTTPException(409, NOT_OURS)
        return decorate(d)

    def _run_cli(*cli_args: str, allow_fail: bool = False) -> str:
        """Run a numera subcommand in its own process (same reasoning as launch:
        keeps sync Playwright out of the web server) and return its output.

        ``allow_fail`` is for commands whose non-zero exit means "the thing you
        asked about is unhealthy", not "the command broke" — a failing proxy test
        still has a result worth showing.
        """
        try:
            proc = run_helper(
                selfexec.self_command("--data-dir", str(engine_store.root), *cli_args),
                # The unlock reaches the child by environment, never argv: a
                # password on a command line is readable by every local process.
                # Without this a server unlocked with --password spawned locked
                # children, so probes and cookie exports silently lost the proxy.
                env=child_env(engine_store._password),
            )
        except HelperTimeout as stalled:
            # 504: the helper, not this server, is what ran out of time - and
            # it has been stopped, so the answer is truthful about the state.
            raise HTTPException(504, str(stalled)) from None
        if proc.returncode != 0 and not (allow_fail and proc.stdout.strip()):
            raise HTTPException(500, (proc.stdout + proc.stderr).strip() or "command failed")
        return proc.stdout

    def _prepare(pid: str) -> dict:
        """Fetch a dashboard profile, refuse if its session is busy, and bring the
        engine-side copy into step so a subcommand can open it."""
        d = dash.get(pid)
        if not d:
            raise HTTPException(404, "profile not found")
        if is_running(pid):
            raise HTTPException(409, BUSY)
        materialize_current(pid)
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
        _prepare(pid)
        rep = json.loads(_run_cli("trust", pid, "--json"))
        # Remember the last grade so the Health page can show it without
        # re-launching every profile (a trust run opens a real browser).
        #
        # Patched onto the *current* record, not onto the snapshot `_prepare`
        # read. A trust run opens a browser and takes tens of seconds; any edit
        # made in that window used to be reverted by saving the old object back.
        stamp = {"grade": rep.get("grade"), "score": rep.get("score"), "at": time.time()}
        dash.patch(pid, lambda current: ({**current, "_trust": stamp}, None))
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
            raise HTTPException(409, BUSY)
        engine = d.get("engine") or engine_store.get_default_engine()
        if engine not in CHROMIUM_ENGINES:
            raise HTTPException(400, f"the '{engine}' engine has no DevTools to attach "
                                     f"to — use one of: {', '.join(CHROMIUM_ENGINES)}")
        materialize_current(pid)
        port = _free_port()
        cli = selfexec.self_command("--data-dir", str(engine_store.root),
                                    "attach", pid, "--port", str(port))
        if (body or {}).get("headless"):
            cli.append("--headless")
        # Same protocol as launch, with the CDP wait layered on top: the child
        # is registered as the owner once its browser is actually open, and
        # only then is it worth waiting for DevTools to answer. Registering
        # first meant a child that exited on a locked vault still looked
        # attached until the CDP wait timed out.
        try:
            owner = spawn_owner(pid, cli, kind="attach")
        except StartupFailed as failure:
            raise _startup_error(failure) from None
        if owner is None:
            raise HTTPException(409, BUSY)
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
        from ..warmup import WarmupInvalid, validate_warmup

        body = body or {}
        # Validated before anything is prepared or spawned. A bare `float()`
        # accepted `inf`, which became a browser with no end time, and `nan`,
        # whose comparisons are false in both directions.
        try:
            minutes = validate_warmup(body.get("minutes", 5), body.get("preset"))
        except WarmupInvalid as e:
            raise HTTPException(400, str(e)) from None
        d = _prepare(pid)
        cli = selfexec.self_command("--data-dir", str(engine_store.root),
                                    "warmup", pid, "--minutes", str(minutes),
                                    "--headless")
        if body.get("preset"):
            cli += ["--preset", str(body["preset"])]
        # Warm-up runs for minutes, so it goes in the background exactly like a
        # launch — the profile shows as running and "stop" ends it. And like a
        # launch, it is not acknowledged until the child owns the session: a
        # warm-up that exits immediately on a locked vault used to leave the
        # profile showing as warming up until someone looked.
        try:
            owner = spawn_owner(pid, cli, kind="manual_warmup")
        except StartupFailed as failure:
            raise _startup_error(failure) from None
        if owner is None:
            raise HTTPException(409, BUSY)
        return decorate(dash.get(pid) or d)

    @app.post("/api/profiles/{pid}/align")
    def align(pid: str):
        """Auto-adjust: rewrite the profile to match what its browser presents,
        then write the aligned values back so the dashboard profile stays put."""
        _prepare(pid)
        res = json.loads(_run_cli("align", pid, "--json"))
        fp = res.get("fingerprint") or {}
        after = res.get("after") or {}

        # Observe, then commit. The route used to hold the record it read
        # *before* the child process launched a browser, and write the aligned
        # values onto that object minutes later - so a rename or a note made
        # during the align was silently undone. Only the fields alignment owns
        # are patched onto whatever the record is now.
        owned = {}
        if fp.get("os"):
            owned["os"] = OS_TO_DASH.get(fp["os"])
        for src, dst in (("userAgent", "userAgent"), ("browserVersion", "browserVersion"),
                        ("cores", "cores"), ("memory", "memory"), ("screen", "screen"),
                        ("webglVendor", "webglVendor"), ("webglRenderer", "webglRenderer"),
                        ("cameras", "mediaVideo"), ("microphones", "mediaAudio"),
                        ("locale", "locale"), ("timezone", "timezone")):
            if fp.get(src) is not None:
                owned[dst] = fp[src]
        if after.get("grade"):
            owned["_trust"] = {"grade": after["grade"], "score": after.get("score"),
                               "at": time.time()}

        def commit(current: dict):
            merged = {**current, **{k: v for k, v in owned.items() if v is not None}}
            if owned.get("os") is None and fp.get("os"):
                merged["os"] = current.get("os")
            return merged, None

        saved = dash.patch(pid, commit)
        res["profile"] = decorate(saved) if saved else None
        return res
