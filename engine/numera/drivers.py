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
import warnings
from contextlib import contextmanager
from dataclasses import replace
from typing import Callable, Optional

from . import fingerprint as fp_mod
from . import paths as paths_mod
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


def _opened() -> None:
    """Tell a parent that spawned this session that its browser is now open.

    Every backend calls this the instant it holds a live persistent context and
    a page, and nothing calls it earlier. That is the whole point of it being
    here rather than in `open_session`: readiness used to be announced before
    the browser was started at all - `open_session` yields, and only then does
    the caller invoke a backend - so an HTTP launch returned 200, registered
    the child and advanced a schedule for a session about to die on a missing
    browser executable or a backend that would not import. See handshake.py.

    Called before the startup URLs rather than after: the browser is open
    either way, and a site that will not load is not a failed launch.
    """
    from .handshake import announce

    announce(True)


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

    session, started = None, False
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
            # The claim is given up on a clean end, and kept when the failure
            # happened *after* the caller took over - the one case where a
            # browser may be left running. A refusal on the way in (a locked
            # credential, a divergent one) started nothing, so keeping it would
            # lock the profile out of this process for no reason.
            session = sessions.take(store.root, profile.id, tree,
                                    "a browser session")["session"]
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
            # the backend is what says it opened - see `_opened`.
            started = True
            yield current
        sessions.release(store.root, profile.id, session)
    except BaseException as failure:
        if session is not None and not started:
            sessions.release(store.root, profile.id, session)
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
            from camoufox.sync_api import Camoufox
            # Exactly the options a real launch uses. The proxy matters most - without
            # it the leak test and the trust report measured the host's own
            # connection and called it clean, which is worse than no check - but so
            # do the OS and locale, or the report grades an identity the profile
            # does not have.
            kwargs, relay = _camoufox_kwargs(quiet, store, headless)
            try:
                with Camoufox(**kwargs) as browser:
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


# --------------------------------------------------------------------------
# Shared Playwright-style launch (used by both playwright and patchright).
# --------------------------------------------------------------------------
def _launch_playwright_like(sync_playwright, profile: Profile, store, headless: bool,
                            keep_open: bool, extra_args=None):
    profile = _resolve_ip_based(profile)
    _clear_unkept(profile, store.user_data_path(profile.id))
    _seed_bookmarks(profile, store.user_data_path(profile.id))
    fp = profile.fingerprint
    launch_args = [
        f"--window-size={fp.viewport_width},{fp.viewport_height}",
        "--disable-blink-features=AutomationControlled",
        "--no-first-run",
        "--no-default-browser-check",
        *_webrtc_arg(profile),
        *_extension_args(profile),
        *_dns_args(profile),
        *_switched_off_args(profile),
        *(profile.launch_args or []),
        *(extra_args or []),
    ]
    hints = fp_mod.client_hint_headers(fp)
    geolocation = _geolocation_for(profile)
    context_opts: dict = {
        "user_agent": fp.user_agent,
        "locale": fp.locale,
        "timezone_id": fp.timezone,
        "viewport": {"width": fp.viewport_width, "height": fp.viewport_height},
        "screen": {"width": fp.screen_width, "height": fp.screen_height},
        "is_mobile": fp.is_mobile,
        # Both come from the profile now. `device_scale_factor` used to be
        # hardcoded to 2-or-1, which ignored the profile's own value and left
        # CSS resolution queries disagreeing with window.devicePixelRatio;
        # without `has_touch`, `(pointer: coarse)` said desktop while
        # maxTouchPoints said phone. Two contradictions a site can just read.
        "device_scale_factor": fp.device_pixel_ratio or (2 if fp.is_mobile else 1),
        "has_touch": fp.max_touch_points > 0,
        "extra_http_headers": {
            # The low-entropy trio only. Chrome sends these on every request, so
            # pinning them as headers matches real behaviour and keeps them
            # right even if the CDP override below never lands.
            #
            # The high-entropy hints are deliberately *not* here. They used to
            # be, because Chromium otherwise answers `sec-ch-ua-platform-version`
            # from the machine it is really running on - a Windows 11 profile on
            # this host reported "10.0". But a header applies to every request,
            # including the first, and real Chrome sends those hints only after
            # a site asks with `Accept-CH`. `_apply_ua_metadata` gets both:
            # Chromium's own timing, the profile's values.
            **hints["low"],
            "accept-language": ",".join(
                f"{l};q={1.0 - i*0.1:.1f}" if i else l
                for i, l in enumerate(fp.languages)
            ),
        },
    }
    # Everything acquired from here to READY is covered by one guard. The
    # relay used to be started *before* it: an authenticated proxy opens a
    # local listener, so if `sync_playwright().start()` then raised, that
    # port was leaked with nothing left holding a reference to close it.
    relay = pw = context = None
    try:
        proxy_cfg, relay = _browser_proxy(profile)
        if proxy_cfg:
            context_opts["proxy"] = proxy_cfg
        geo_mode = (fp.geolocation or "prompt").lower()
        if geo_mode == "block":
            # Deny outright: no coordinates, and the permission is explicitly refused
            # so a site gets a hard "denied" instead of a prompt that never resolves.
            context_opts["permissions"] = []
        elif geolocation:
            context_opts["geolocation"] = geolocation
            if geo_mode == "allow":
                # Grant it, so a site that asks gets the coordinates rather than a
                # prompt that would stall an unattended session.
                context_opts["permissions"] = ["geolocation"]
            # "prompt" keeps the coordinates ready but leaves the grant to the user,
            # which is what a real browser does on first visit.

        if fp.do_not_track:
            context_opts["extra_http_headers"]["dnt"] = "1"

        # Before Playwright starts, not after: it reads the browsers path when the
        # driver launches. A portable build ships its own Chromium and there is no
        # developer cache to fall back on.
        paths_mod.use_bundled_browsers()
        pw = sync_playwright().start()
        context = pw.chromium.launch_persistent_context(
            user_data_dir=str(store.user_data_path(profile.id)),
            headless=headless,
            # Headless defaults to the "headless shell" binary, which cannot load
            # extensions at all and announces itself as HeadlessChrome over CDP.
            # The chromium channel is the full browser in --headless=new mode: same
            # download, extensions work, and the browser stops advertising that it
            # is headless.
            channel="chromium" if headless else None,
            args=launch_args,
            # Drop Playwright's automation switch so Chrome doesn't show the
            # "controlled by automated test software" infobar.
            ignore_default_args=["--enable-automation"],
            **context_opts,
        )
        if relay:
            # Tie the relay to the session: whoever closes the context — the user
            # closing the window, `session()`, the API — frees the port with it.
            context.on("close", lambda *_: relay.close())
        context.add_init_script(build_init_script(fp))
        _apply_client_hints(context, fp)
        page = context.pages[0] if context.pages else context.new_page()
    except BaseException as failure:
        _abandon(context, pw, relay, failure)
        raise
    _opened()
    _open_startup_urls(context, page, profile)

    if keep_open:
        return pw, context, page
    try:
        _wait_until_closed(page)
    finally:
        context.close()
        pw.stop()
    return None


def _strip_client_hints(cdp) -> None:
    """Remove every `sec-ch-ua*` header from this page's requests.

    Safari implements no Client Hints, so an iPhone profile that sends them
    contradicts itself. Chromium adds them on its own and nothing declarative
    takes them off: `route.continue_(headers=...)`,
    `--disable-features=UserAgentClientHint,AcceptCHFrame,CriticalClientHint`
    and `--blink-settings=clientHintsEnabled=false` were each measured against a
    local server and changed nothing. Playwright's route API rebuilds the header
    set from `request.headers()`, which is why it cannot drop what Chromium
    appends further down the stack.

    Raw CDP can. `Fetch.continueRequest` *replaces* the header set outright, so
    a name simply left out never reaches the network. Measured: the server sees
    no `sec-ch-ua` at all, before or after it sends `Accept-CH`.

    The cost is a round trip per request, so this is only switched on for a
    profile that needs it. Every path has to reach `continueRequest` - with
    `Fetch` enabled, a request that is never continued hangs the page for good.
    """
    def on_paused(params):
        request_id = params.get("requestId")
        if not request_id:
            return
        try:
            headers = params.get("request", {}).get("headers") or {}
            cdp.send("Fetch.continueRequest", {
                "requestId": request_id,
                "headers": [{"name": k, "value": v} for k, v in headers.items()
                            if not k.lower().startswith("sec-ch-ua")],
            })
        except Exception:                                   # noqa: BLE001
            # Better a request with the hints than a page that never loads.
            try:
                cdp.send("Fetch.continueRequest", {"requestId": request_id})
            except Exception:                               # noqa: BLE001
                pass

    cdp.on("Fetch.requestPaused", on_paused)
    # Request stage only: auth challenges stay untouched, so proxy credentials
    # keep working, and response bodies are never buffered through here.
    cdp.send("Fetch.enable",
             {"patterns": [{"urlPattern": "*", "requestStage": "Request"}]})


def _apply_client_hints(context, fp) -> None:
    """Hand Chromium the profile's UA-CH values and let it decide when to send.

    `Emulation.setUserAgentOverride` takes a structured metadata block, and
    Chromium then derives the Sec-CH-UA headers and answers
    `navigator.userAgentData.getHighEntropyValues()` from it. Measured against a
    local server: the first request carries the low-entropy trio only, and the
    request after the site's `Accept-CH` carries the profile's platform version
    rather than the host's. Putting the same values in `extra_http_headers` gets
    the values right and the timing wrong - they ride on the first request too,
    which no real Chrome does.

    A failure on a page that already exists is fatal: continuing would launch a
    browser that reports the host's OS build while claiming to be the profile,
    which is the leak this exists to close. A page opened later may simply have
    closed before the handler ran, so that path only warns.
    """
    sessions: list = []
    opts = {
        "userAgent": fp.user_agent,
        "acceptLanguage": ",".join(fp.languages),
        "platform": fp.platform,
        "userAgentMetadata": fp_mod.ua_metadata(fp),
    }

    def apply(page):
        cdp = context.new_cdp_session(page)
        cdp.send("Emulation.setUserAgentOverride", opts)
        if not fp_mod.sends_client_hints(fp):
            _strip_client_hints(cdp)
            # Held so the session is not collected while its handler is live.
            sessions.append(cdp)

    def apply_later(page):
        try:
            apply(page)
        except Exception as exc:                            # noqa: BLE001
            warnings.warn(f"client hints not applied to a new page: {exc}",
                          RuntimeWarning, stacklevel=2)

    context.on("page", apply_later)
    for page in context.pages:
        apply(page)


@register("playwright", requires="playwright")
def _driver_playwright(profile, store, headless=False, keep_open=True, extra_args=None):
    from playwright.sync_api import sync_playwright
    return _launch_playwright_like(sync_playwright, profile, store, headless, keep_open, extra_args)


@register("patchright", requires="patchright")
def _driver_patchright(profile, store, headless=False, keep_open=True, extra_args=None):
    # Patchright is an API-compatible, patched drop-in for Playwright.
    from patchright.sync_api import sync_playwright
    return _launch_playwright_like(sync_playwright, profile, store, headless, keep_open, extra_args)


@register("camoufox", requires="camoufox")
def _driver_camoufox(profile, store, headless=False, keep_open=True, extra_args=None):
    """Camoufox (patched Firefox) generates its own coherent fingerprint; we feed
    it the profile's OS/locale/proxy as hints and keep the persistent session."""
    if keep_open:
        raise RuntimeError(
            "The camoufox engine supports launch-and-wait only (keep_open=False), "
            "which is what `numera launch` and the dashboard use."
        )
    from camoufox.sync_api import Camoufox

    kwargs, relay = _camoufox_kwargs(profile, store, headless)

    try:
        with Camoufox(**kwargs) as browser:
            page = browser.pages[0] if getattr(browser, "pages", None) else browser.new_page()
            _opened()
            _open_startup_urls(browser, page, profile)
            if not headless:
                page.wait_for_event("close", timeout=0)
    finally:
        if relay:
            relay.close()
    return None


@register("cloak", requires="cloakbrowser")
def _driver_cloak(profile, store, headless=False, keep_open=True, extra_args=None):
    """CloakBrowser — a patched Chromium binary whose fingerprints are altered at
    the C++ source level. Numera feeds it the profile's identity + proxy and keeps
    the persistent session; the binary does the heavy stealth work (its own
    fingerprint, TLS, CDP), so we don't inject our JS stealth script here.

    The binary auto-downloads on first launch. Docs: https://github.com/CloakHQ/CloakBrowser
    """
    import cloakbrowser

    profile = _resolve_ip_based(profile)
    _clear_unkept(profile, store.user_data_path(profile.id))
    _seed_bookmarks(profile, store.user_data_path(profile.id))
    fp = profile.fingerprint
    # Same guard as the Playwright path, and for the same reason: the window
    # between the first acquired resource and READY is where a failure used to
    # leave a browser - or just a relay port - behind for a session nobody owns.
    relay = context = None
    try:
        proxy_cfg, relay = _browser_proxy(profile)
        context = cloakbrowser.launch_persistent_context(
            user_data_dir=str(store.user_data_path(profile.id)),
            headless=headless,
            proxy=proxy_cfg,
            user_agent=fp.user_agent,
            viewport={"width": fp.viewport_width, "height": fp.viewport_height},
            locale=fp.locale,
            timezone=fp.timezone,
            geoip=bool(profile.proxy),   # align timezone/locale to the proxy exit IP
            # CloakBrowser launches with --no-sandbox (its stealth args), which makes
            # Chromium show a local "unsupported command-line flag" infobar. --test-type
            # hides that purely-cosmetic bar; it isn't visible to websites (JS can't
            # read command-line flags), so it doesn't affect the fingerprint.
            args=["--test-type", "--no-first-run", "--no-default-browser-check",
                  *_webrtc_arg(profile), *_extension_args(profile),
                  *_dns_args(profile), *_switched_off_args(profile),
                  *(profile.launch_args or []),
                  *(extra_args or [])],
        )
        if relay:
            context.on("close", lambda *_: relay.close())
        page = context.pages[0] if context.pages else context.new_page()
    except BaseException as failure:
        # cloakbrowser owns no separate driver, so there is nothing to stop
        # between the browser and the relay.
        _abandon(context, None, relay, failure)
        raise
    _opened()
    _open_startup_urls(context, page, profile)

    if keep_open:
        return None, context, page
    try:
        _wait_until_closed(page)
    finally:
        context.close()
    return None
