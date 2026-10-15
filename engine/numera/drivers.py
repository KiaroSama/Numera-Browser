"""
Pluggable launch backends ("drivers").

Numera's real job is to *manage* browser identities — coherent fingerprints,
proxies, persistent sessions, folders, and the dashboard. Actually rendering an
identity in a browser is delegated to a launch backend, so a client can pick the
stealth engine that fits their needs and threat model:

  * ``playwright`` — stock Playwright Chromium + Numera's injected stealth script.
    Always available. Good for fingerprint test sites and lighter use. Its ceiling
    is the JS-injection ceiling: it can't change the TLS/JA3 fingerprint or hide
    the CDP transport.

  * ``patchright`` — a drop-in *patched* Playwright that fixes ``webdriver``/CDP
    and other runtime-level leaks that plain Playwright can't. Same API, stronger.
    Install:  ``pip install patchright`` then ``patchright install chromium``.

  * ``camoufox`` — a patched Firefox with C++-level fingerprint spoofing and its
    own coherent fingerprint generator; among the strongest open options.
    Install:  ``pip install "camoufox[geoip]"`` then ``camoufox fetch``.

  * ``cloak`` — CloakBrowser: a patched Chromium binary whose fingerprint, TLS and
    CDP are altered at the C++ source level. Reaches detection grades JS injection
    can't (Cloudflare, DataDome, reCAPTCHA v3). Install:  ``pip install cloakbrowser``.

Numera stays the manager on top of all of them: it owns the profile identity,
the proxy, the session directory, and the coherence checks; the driver just opens
a window wearing it.

Add your own backend by decorating a function with ``@register("name", requires=...)``.
Each driver has the signature ``(profile, store, headless, keep_open)`` and either
returns launch handles (``keep_open=True``) or blocks until the window closes
(``keep_open=False``).
"""

from __future__ import annotations

import importlib.util
import json
import sys
from contextlib import contextmanager
import threading
from dataclasses import replace
from typing import Callable, Optional

from . import fingerprint as fp_mod
from . import paths as paths_mod
from . import proctree, sessions
from . import bundled
from .models import Profile
from .stealth import build_init_script

_DRIVERS: dict[str, dict] = {}


from .launch_options import (          # noqa: F401 - re-exported on purpose
    CLEARABLE, DOH_TEMPLATES, SWITCHABLE, _browser_proxy, _clear_unkept,
    _dns_args, _extension_args, _geolocation_for, _open_startup_urls,
    _resolve_ip_based, _seed_bookmarks, _switched_off_args, _webrtc_arg,
)


def _wait_until_closed(page) -> None:
    """Block until the session ends, headed or headless.

    Headed, that is the user closing the window. Headless there is no window,
    but the session still has to stay up — a server profile is driven over CDP
    or stopped by the API, and returning early would tear the browser down the
    moment it finished opening. Blocking on the same event covers both: the page
    closes when the browser is stopped, and Ctrl-C still interrupts.
    """
    try:
        page.wait_for_event("close", timeout=0)
    except KeyboardInterrupt:
        pass
    except Exception:
        # The browser died on its own (crash, external kill) — nothing to wait
        # for any more; fall through to the caller's cleanup.
        pass


def register(name: str, requires: Optional[str] = None):
    """Register a launch backend. ``requires`` is the pip module it needs (if any)."""
    def deco(fn: Callable):
        _DRIVERS[name] = {"fn": fn, "requires": requires}
        return fn
    return deco


def names() -> list[str]:
    return sorted(_DRIVERS)


def is_installed(name: str) -> bool:
    req = _DRIVERS.get(name, {}).get("requires")
    return req is None or importlib.util.find_spec(req) is not None


def available() -> dict[str, bool]:
    """Map every known engine to whether its dependency is importable."""
    return {n: is_installed(n) for n in names()}


def get(name: str) -> Callable:
    if name not in _DRIVERS:
        raise RuntimeError(
            f"Unknown engine '{name}'. Known engines: {', '.join(names())}."
        )
    if not is_installed(name):
        req = _DRIVERS[name]["requires"]
        raise RuntimeError(
            f"The '{name}' engine needs the '{req}' package, which isn't installed.\n"
            f"Install it, then try again  (see numera/drivers.py for hints)."
        )
    return _DRIVERS[name]["fn"]


# Camoufox resolves timezone, locale and geolocation from the proxy's exit IP as
# one switch - it has no per-signal control. A profile that asked for only
# `timezone` would silently get all three moved, which is not what the editor
# said. Named here so the limitation is visible rather than buried in a kwarg.
CAMOUFOX_GEOIP_IS_ALL_OR_NOTHING = True


def _camoufox_kwargs(profile: Profile, store, headless: bool) -> tuple:
    """The options Camoufox is opened with, for *both* launching and inspecting.

    `session()` used to build its own shorter set - no `os`, no `locale` - so the
    trust report, the leak test and the TLS check all graded a browser that was
    not wearing the profile's identity, and reported the result as the profile's.
    Two call sites, one builder, no room to drift.
    """
    fp = profile.fingerprint
    kwargs: dict = {
        "headless": headless,
        # Camoufox is Firefox: it models the three desktop platforms. A mobile
        # profile has no Camoufox equivalent, so it is opened as Windows rather
        # than as a silently wrong Android.
        "os": {"windows": "windows", "macos": "macos", "linux": "linux"}.get(fp.os, "windows"),
        "locale": fp.locale,
        "persistent_context": True,
        "user_data_dir": str(store.user_data_path(profile.id)),
        **bundled.launch_options("camoufox", store.root),
    }
    proxy_cfg, relay = _browser_proxy(profile)
    if proxy_cfg:
        kwargs["proxy"] = proxy_cfg
        # Only when the profile actually asked for it. This was unconditional,
        # so a profile with every signal set to "manual" still had its timezone
        # and locale rewritten from the exit IP - the opposite of what it said.
        if getattr(profile, "ip_based", None):
            kwargs["geoip"] = True
    return kwargs, relay


class ProfileGone(RuntimeError):
    """The profile was deleted between the request and the session opening."""


def _startup_kind(exc: BaseException) -> str:
    """Which handshake `kind` a startup failure is - see handshake.py."""
    from . import crypto
    from .paths import SessionBusy

    if isinstance(exc, SessionBusy):
        return "busy"
    if isinstance(exc, crypto.VaultLocked):
        return "locked"
    if isinstance(exc, ProfileGone):
        return "gone"
    from .repo import ProfileDiverged

    if isinstance(exc, ProfileDiverged):
        return "diverged"
    return "failed"


class StartupRecoveryFailed(RuntimeError):
    """A startup failed *and* something it had acquired could not be released.

    `_abandon` used to swallow every cleanup error, so a caller was told only
    the original failure while a browser or a Playwright driver might still be
    running. "No browser survives a failed startup" is a promise; discarding
    the evidence that a close failed turns it into a guess.

    Carries the failure that started it and each step that could not be undone,
    because those are two different repairs.
    """

    def __init__(self, cause: BaseException, failures: list):
        self.cause = cause
        self.failures = list(failures)          # [(what, error), ...]
        detail = "; ".join(f"{what}: {err}" for what, err in self.failures)
        super().__init__(
            f"{type(cause).__name__}: {cause} - and the session could not be "
            f"cleaned up afterwards ({detail}). Something it started may still "
            f"be running and still holding the profile.")


def _abandon(context=None, driver=None, relay=None, cause=None):
    """Release everything a failed startup acquired, before the failure is told.

    A browser open is not one call. Between "the persistent context exists" and
    `_opened()` there is the init script, the client-hint overrides and the
    first page, and any of them can raise - as can a launch that fails after
    the driver has started. The parent is about to be told this session never
    opened, so nothing may survive it: a live Chromium keeps the profile's
    user-data directory busy, a running Playwright driver leaks a process, and
    a relay holds a port for a proxy nobody is using.

    Every step is attempted even if an earlier one raises. This runs while an
    exception is already on its way up, and *that* is the error worth having -
    not whatever the cleanup hit on the way out.

    Order is the reverse of acquisition: the browser first, then the driver
    that owns it, then the relay it was talking through.

    A step that fails does not stop the others, and it is not forgotten either:
    given `cause`, anything left behind is raised as `StartupRecoveryFailed`
    carrying both. Without it the failures are returned, for a caller that
    genuinely only wants a best-effort release.
    """
    failures = []
    for what, close in (("the browser", getattr(context, "close", None)),
                        ("the browser driver", getattr(driver, "stop", None)),
                        ("the proxy relay", getattr(relay, "close", None))):
        if close is None:
            continue
        try:
            close()
        except Exception as err:      # noqa: BLE001 - see above
            failures.append((what, err))
    if failures and cause is not None:
        raise StartupRecoveryFailed(cause, failures) from cause
    return failures


#: What each session running on this thread has learned about its browser:
#: whether one was ever opened, and whether it was shut again. `open_session`
#: reads it to answer a question it was previously guessing at - whether there
#: is anything left to keep a claim for.
#:
#: A stack, because one thread can open a second profile inside the first
#: session's body, and the inner session's answers are not the outer one's.
_LOCAL = threading.local()


def _live():
    """The innermost session on this thread, or None outside `open_session`."""
    stack = getattr(_LOCAL, "sessions", None)
    return stack[-1] if stack else None


def _cloak_programs(cloakbrowser) -> list:
    """Every program CloakBrowser might start its browser from.

    It exposes a Playwright `BrowserContext`, so it drives a Playwright-family
    node driver - but which one is its own business, and it may ship a bundled
    binary instead. So every candidate this machine can name is offered and the
    first spawn that matches one wins; an empty list means the launch is
    refused, because guessing would be the silent downgrade this replaced.
    """
    programs = []
    for attribute in ("executable_path", "EXECUTABLE_PATH", "browser_path"):
        value = getattr(cloakbrowser, attribute, None)
        try:
            value = value() if callable(value) else value
        except Exception:            # noqa: BLE001    # pragma: no cover
            value = None
        if value:
            programs.append(str(value))
    for package in ("patchright", "playwright"):
        found = _package_driver(package)
        if found:
            programs.append(found)
    return programs


def _package_driver(package: str) -> Optional[str]:
    """The node driver a Playwright-family package would spawn, or None."""
    try:
        import importlib

        driver = importlib.import_module(f"{package}._impl._driver")
        return str(driver.compute_driver_executable()[0])
    except Exception:            # noqa: BLE001 - not installed, or moved
        return None


def _playwright_program() -> Optional[str]:
    """Playwright's own driver, for the backends that do not hand us a module."""
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:                                      # pragma: no cover
        return None
    return _driver_program(sync_playwright)


def _driver_program(sync_playwright) -> Optional[str]:
    """The executable Playwright will start its driver from, or None.

    Asked of the package the caller handed us, because `patchright` ships its
    own driver and Camoufox rides on Playwright's. None means the launch is
    refused: a driver this process cannot name is one whose browser it could
    never prove had stopped.
    """
    package = (getattr(sync_playwright, "__module__", "") or "").split(".")[0]
    return _package_driver(package) if package else None


@contextmanager
def _own_driver_tree(program):
    """Start this session's driver in a tree that holds only its browser.

    The claim was taken before any of this - it has to be, or two openers race
    - so it names the host tree until now. Once the driver exists there is a
    tree that contains this session and nothing else, and the claim is narrowed
    to it: that is what lets `release` prove emptiness with the OS instead of
    trusting what this process reports about its own cleanup.

    **The narrowing happens inside the spawn**, not after this body returns.
    Playwright starts its browser well after `start()`, but Camoufox opens one
    inside `__enter__` - so a launcher hard-killed in that window used to leave
    a running browser whose only durable record still named the host tree, and
    a later opener had nothing pointing at the group it was actually in.

    **And it fails closed.** A driver that cannot be identified, a job object
    that cannot be created or assigned, a claim that cannot be narrowed: each
    of those used to fall back to the host tree, which quietly made the whole
    guarantee optional. The browser is refused instead, and anything already
    started is ended.
    """
    state = _live()
    if state is None:
        # No session, so no claim and nothing to own: a driver called directly
        # (a test, an internal probe) is not a browser open this has to answer
        # for. Ownership is the claim's, and the claim only exists in here.
        with proctree.spawning_own_tree(program):
            yield
        return
    if not program:
        raise proctree.TreeUnavailable(
            "this browser's driver could not be identified, so no tree could "
            "be made to own what it starts - and a browser nothing owns is "
            "one nothing can prove has stopped. The launch was refused.")

    def record(tree):
        state["tree"] = tree
        if not sessions.narrow(state["root"], state["profile_id"],
                               state["session"], tree):
            raise proctree.TreeUnavailable(
                "the session's claim could not be pointed at the tree its "
                "browser was starting in, so the browser was stopped: an "
                "unrecorded tree is one nothing could find again.")

    with proctree.spawning_own_tree(program, on_tree=record) as adopted:
        yield
    if adopted.tree is None:
        _abandon_tree(adopted.proc)
        raise proctree.TreeUnavailable(
            "this browser's driver did not start in a tree of its own, so "
            "nothing could prove its browser had stopped. The launch was "
            "refused and anything it had started was ended.")


def _abandon_tree(proc) -> None:
    """End a driver we have decided not to keep. Quiet: we are already failing."""
    if proc is None:
        return
    try:
        proc.kill()
        proc.wait(timeout=10)
    except Exception:            # noqa: BLE001                # pragma: no cover
        pass


def _opened() -> None:
    """Tell a parent that spawned this session that its browser is now open.

    Every backend calls this the instant it holds a live persistent context and
    a page, and nothing calls it earlier. That is the whole point of it being
    here rather than in `open_session`: readiness used to be announced before
    the browser was started at all - `open_session` yields, and only then does
    the caller invoke a backend - so an HTTP launch returned 200, registered
    the child and advanced a schedule for a session about to die on a missing
    browser executable or a backend that would not import. See handshake.py.

    It also records, for `open_session`, that a browser exists at all. That was
    inferred from "the yield happened", which is true of every caller body -
    including one that raises before it opens anything, and that left a claim
    standing for a browser there was never any of.

    Called before the startup URLs rather than after: the browser is open
    either way, and a site that will not load is not a failed launch.
    """
    from .handshake import announce

    state = _live()
    if state is not None:               # first: the browser is open either way
        state["opened"] = True
    announce(True)


def _closed() -> None:
    """Record that this session's browser is shut and its driver stopped.

    The other half of `_opened`, and the reason `open_session` no longer keeps
    a claim for every exception that reaches it. A body can raise *after* its
    browser has been closed - `session()` and `numera attach` both close in a
    `finally`, so the user's own exception passes through here with nothing
    left running - and holding the profile for that locked it out until a
    person went and deleted the claim by hand.

    Every close this module owns calls it, and calls it *after* the close
    returned: a close that raised never reaches this, which is the one case
    where something may still be holding `user-data/<id>` and the claim has to
    stand.
    """
    state = _live()
    if state is not None:
        state["closed"] = True


def _camoufox(kwargs):
    """Camoufox's own context manager. A seam: the exit protocol below is the
    part worth testing, and it needs a stand-in that can fail on close."""
    from camoufox.sync_api import Camoufox

    return Camoufox(**kwargs)


@contextmanager
def _camoufox_session(kwargs):
    """`with Camoufox(...)`, plus the one fact a plain `with` throws away.

    Camoufox closes the browser from `__exit__`, and when the body raised, the
    exception carries straight past every line after the `with` - including the
    one that would have said the browser is down. So the session kept its claim
    for a browser that had closed cleanly, and in a long-lived host the profile
    stayed busy until somebody deleted the claim file.

    The protocol is driven by hand for exactly that reason: `__exit__` is given
    the exception the way `with` would give it and still decides whether to
    suppress it, but *returning* from it is now observable. An `__exit__` that
    raises never reaches `_closed()`, which is right - that is the case where
    something may still be holding the profile.
    """
    cam = _camoufox(kwargs)
    # Camoufox drives Playwright's own Firefox driver, so the same seam works:
    # `__enter__` is where its driver is spawned.
    entered = yielded = False
    try:
        with _own_driver_tree(_playwright_program()):
            browser = cam.__enter__()
            entered = True
        yielded = True
        yield browser
    except BaseException as failure:
        if not entered:
            raise
        try:
            suppressed = cam.__exit__(*sys.exc_info())
        except BaseException as cleanup:
            raise StartupRecoveryFailed(failure, [("the browser", cleanup)]) from failure
        _closed()                       # it returned, so the browser is down
        # Ownership refusal happens after enter but before yield. Cleanup may
        # suppress a caller's error; it may never make an unowned launch valid.
        if not yielded or not suppressed:
            raise
    else:
        cam.__exit__(None, None, None)
        _closed()


def _may_release(state) -> bool:
    """May this session's claim go, given how its body ended?

    Only consulted when the session has no tree of its own. With one, `release`
    asks the OS - `tree` proved empty - and neither of these flags is needed.

    Without one the only tree available is the host's, which holds sibling
    sessions and every unrelated child of an embedding application, so the
    answer comes from what the backends reported: a claim is kept for a browser
    that was opened and not closed again. "The yield happened" was the older
    test, and it is true both of a caller that raises before opening anything
    and of one that raises after its own `finally` closed the browser.
    """
    return not state["opened"] or state["closed"]


@contextmanager
def open_session(profile: Profile, store):
    """The gate every browser open goes through. Yields the profile to launch.

    **Order matters, and it was wrong.** The check used to come first and the
    lease second, which is a TOCTOU with a delete: the profile was inspected,
    a delete committed and released its lease, and the opener then took the
    free lease and launched the snapshot it had read - recreating
    `user-data/<id>` for a profile that no longer existed. Now:

    1. take the session lease (the ownership decision);
    2. take a SHORT root transaction and re-read the profile and its raw
       stored credential inside it;
    3. verify the profile still exists and the credential is readable;
    4. release the transaction - it is never held for a browser's lifetime;
    5. keep only the lease, and yield the profile that was just re-read.

    Callers use the yielded profile, not the one they passed in, so a stale
    pre-lease object cannot reach the browser.

    **The lease** is what makes `user-data/<id>` owned. It is a Chromium
    profile directory and Chromium assumes it owns one exclusively - two
    processes on one directory corrupt its databases. The storage locks never
    covered this: they guard JSON writes and are held for milliseconds, while a
    session lasts as long as the window does.

    **The credential check** belongs here rather than in each caller because a
    locked vault hands out `proxy.password = None`, which is indistinguishable
    from "this proxy has no password". A launch would then start with no proxy
    credential at all - the profile browsing from the host's own IP, which is
    the one outcome an anti-detect profile exists to prevent. A genuinely
    passwordless proxy is untouched: `is_withheld` is false when nothing is
    stored.

    Reads and ordinary edits stay available while locked; only opening a
    browser is refused.

    **Readiness is not this function's to declare.** It used to be: the child
    announced success here, before the yield, so a parent that spawned it
    returned 200, registered `running[pid]` and let the scheduler write
    `lastRun` while the browser had not been started yet - the caller starts it
    *after* the yield. Every way a launch can still fail at that point happened
    behind the parent's back. The backends announce it now, from `_opened()`;
    what is announced here is only failure, and only a failure that got in
    before any browser opened.
    """
    import json

    from . import crypto, proctree, sessions, vault
    from .handshake import announce, reported
    from .paths import session_lease, transaction_lock
    from .repo import repo

    session, state = None, {"opened": False, "closed": False, "tree": None,
                            "root": store.root, "profile_id": profile.id,
                            "session": None}
    try:
        # The tree this process owns, established *before* anything is claimed
        # or opened. A foreground CLI adopts one for itself here; a spawned
        # session inherits the one its parent made. Either way, a browser
        # started after this point has an owner that can be named on disk and
        # proved gone later - which is what the claim below is for.
        tree = proctree.own_tree()
        with session_lease(store.root, profile.id):
            # Inside the lease, which is still the ownership decision and still
            # first: a delete that commits in the window before it must be the
            # one that wins, not one this claim refuses.
            #
            # The claim is given up on a clean end, and on a failure that left
            # no browser behind - one raised before anything opened, or after
            # the caller's own `finally` closed it again. It is kept only for
            # the state it exists for: a browser that may still be running.
            session = sessions.take(store.root, profile.id, tree,
                                    "a browser session")["session"]
            state["session"] = session
            # Short, and released before the browser starts.
            with transaction_lock(store.root):
                # The coherence gate, for every opener rather than only the
                # HTTP one. `materialize_current` has refused a launch on two
                # irreconcilable proxy passwords since 0.25, but only the API
                # called it: a direct `numera launch` re-read the engine copy
                # and went ahead with that credential. One gate, one answer.
                current = repo(store).coherent(profile.id)
                if current is None:
                    raise ProfileGone(
                        f"{profile.name} was deleted while its session was being "
                        f"opened, so there is nothing to launch.")
                stored = None
                try:
                    path = store._path(profile.id)
                    if path.exists():
                        raw = json.loads(path.read_text(encoding="utf-8"))
                        stored = ((raw.get("proxy") or {}) or {}).get("password")
                except (OSError, ValueError):                    # pragma: no cover
                    stored = None
                if vault.is_withheld(store, stored):
                    raise crypto.VaultLocked(
                        f"{current.name} has a proxy password this process cannot read, "
                        f"so the browser would start without it and browse from your own "
                        f"IP. Unlock the vault first: numera --password ... <command>")
            # Owned and revalidated. The caller opens the browser next, and
            # the backend is what says it opened - see `_opened`/`_closed`.
            stack = getattr(_LOCAL, "sessions", None)
            if stack is None:
                stack = _LOCAL.sessions = []
            stack.append(state)
            try:
                yield current
            finally:
                stack.pop()
        sessions.release(store.root, profile.id, session, tree=state["tree"])
    except BaseException as failure:
        # A backend can acquire resources and fail cleanup before readiness,
        # so opened=False alone does not prove that startup left nothing live.
        if (session is not None and not isinstance(failure, StartupRecoveryFailed)
                and _may_release(state)):
            sessions.release(store.root, profile.id, session, tree=state["tree"])
        # Only if nothing has been reported yet. Once a backend has said the
        # browser is open the parent has acted on it, and overwriting that with
        # a failure raised on the way out would be a second lie.
        if not reported():
            announce(False, str(failure), _startup_kind(failure))
        raise


@contextmanager
def session(profile: Profile, store, engine: Optional[str] = None,
            headless: bool = True):
    """Open a profile's persistent session and yield its browser context.

    This is the entry point for the tools that *inspect* a profile rather than
    hand it to a user — cookie transfer, the proxy/leak test, the trust report.
    Startup URLs are stripped: these callers want the identity, not a browsing
    session. The context is always closed on the way out, which is also what
    flushes cookies to disk.
    """
    # Every browser open goes through the same gate - see `open_session`. It
    # yields the profile as it is NOW, re-read under the lease.
    with open_session(profile, store) as profile:
        name = engine or profile.engine or "playwright"
        quiet = replace(profile, startup_urls=[])

        if name == "camoufox":
            # Camoufox's driver is launch-and-wait only, so open it directly.
            # Exactly the options a real launch uses. The proxy matters most - without
            # it the leak test and the trust report measured the host's own
            # connection and called it clean, which is worse than no check - but so
            # do the OS and locale, or the report grades an identity the profile
            # does not have.
            kwargs, relay = _camoufox_kwargs(quiet, store, headless)
            try:
                with _camoufox_session(kwargs) as browser:
                    _opened()
                    yield browser
            finally:
                if relay:
                    relay.close()
            return

        handles = get(name)(quiet, store, headless=headless, keep_open=True)
        pw, context, _page = handles
        try:
            yield context
        finally:
            context.close()
            if pw:
                pw.stop()
            _closed()


from .driver_backends import (  # noqa: E402,F401 - retained public test seams
    _launch_playwright_like, _strip_client_hints, _apply_client_hints,
    _driver_playwright, _driver_patchright, _driver_camoufox, _driver_cloak,
)
