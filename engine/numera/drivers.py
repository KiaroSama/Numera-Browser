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
import time
import warnings
from contextlib import contextmanager
from dataclasses import replace
from pathlib import Path
from typing import Callable, Optional

from . import devices as fp_mod_devices
from . import fingerprint as fp_mod
from .models import Profile
from .relay import ProxyRelay
from .stealth import build_init_script

_DRIVERS: dict[str, dict] = {}


def _open_startup_urls(context, page, profile: Profile) -> None:
    """Navigate the first tab to the profile's first startup URL and open the
    rest in new tabs. A bad URL is skipped rather than aborting the launch."""
    urls = getattr(profile, "startup_urls", None) or []
    if not urls:
        return
    for i, url in enumerate(urls):
        try:
            target = page if i == 0 else context.new_page()
            target.goto(url, wait_until="domcontentloaded", timeout=30000)
        except Exception:
            continue  # unreachable site / typo — don't crash the session


def _geolocation_for(profile: Profile) -> Optional[dict]:
    """Coordinates that agree with the profile's proxy country (or its locale
    when there's no proxy), so navigator.geolocation doesn't contradict the IP."""
    country = None
    if profile.proxy and profile.proxy.country:
        country = profile.proxy.country.upper()
    if not country:
        country = fp_mod_devices.LOCALE_TO_COUNTRY.get(profile.fingerprint.locale)
    coords = fp_mod_devices.COUNTRY_GEO.get(country or "")
    if not coords:
        return None
    lat, lon = coords
    return {"latitude": lat, "longitude": lon, "accuracy": 60}


def _webrtc_arg(profile: Profile) -> list[str]:
    """Stop WebRTC from leaking the real IP. With a proxy, force everything
    through it (or don't send UDP at all); otherwise expose only the public
    interface so local addresses stay hidden."""
    if profile.proxy:
        return ["--force-webrtc-ip-handling-policy=disable_non_proxied_udp"]
    return ["--force-webrtc-ip-handling-policy=default_public_interface_only"]


def _browser_proxy(profile: Profile):
    """The proxy config to hand the browser, and the relay behind it (or ``None``).

    A proxy with credentials is forwarded through a local unauthenticated relay
    rather than given to the browser directly. Every engine can point at
    ``http://127.0.0.1:<port>`` with nothing to authenticate, which is the only
    thing they all agree on: fingerprint-chromium has no password support on
    ``--proxy-server``, Firefox cannot do authenticated SOCKS, and Chromium
    refuses SOCKS authentication outright (BUGS.md B4). See relay.py.
    """
    if not profile.proxy:
        return None, None
    if not (profile.proxy.username or profile.proxy.password):
        return profile.proxy.to_playwright(), None
    relay = ProxyRelay(profile.proxy).start()
    return {"server": relay.url}, relay


# Well-known resolver addresses mapped to their DNS-over-HTTPS endpoint.
# Chromium has no "use this nameserver" flag - the OS resolver owns that - but
# it does speak DoH, which is the stronger answer anyway: it takes DNS off the
# local network entirely, so the lookups can't leak beside the proxy.
DOH_TEMPLATES = {
    "1.1.1.1": "https://cloudflare-dns.com/dns-query",
    "1.0.0.1": "https://cloudflare-dns.com/dns-query",
    "8.8.8.8": "https://dns.google/dns-query",
    "8.8.4.4": "https://dns.google/dns-query",
    "9.9.9.9": "https://dns.quad9.net/dns-query",
    "149.112.112.112": "https://dns.quad9.net/dns-query",
    "94.140.14.14": "https://dns.adguard-dns.com/dns-query",
    "208.67.222.222": "https://doh.opendns.com/dns-query",
}


# Chromium subsystems a profile can switch off, and the flags that do it.
#
# Both are reachable by typing into the launch-arguments box; they are named
# switches because "stop Chrome phoning Google" is a thing people want, not a
# flag people remember. Deliberately no `--disable-features=` here: Chromium
# keeps only the *last* occurrence of that switch, so adding one would silently
# cancel the DNS-over-HTTPS flag.
SWITCHABLE = {
    # Sync, Safe Browsing updates, field trials, the extension updater and the
    # usage pings - all traffic to Google from the proxy's IP that has nothing to
    # do with what the profile is being used for.
    "googleServices": ["--disable-sync", "--disable-background-networking",
                       "--disable-client-side-phishing-detection", "--no-pings",
                       "--disable-domain-reliability"],
    # Chromium's own hidden extensions (the Web Store connector, Media Router,
    # Cloud Print). They run in the background whether or not anything uses them.
    "systemExtensions": ["--disable-component-extensions-with-background-pages",
                         "--disable-component-update"],
}


def _switched_off_args(profile: Profile) -> list[str]:
    return [flag for key in (getattr(profile, "switched_off", None) or [])
            for flag in SWITCHABLE.get(key, ())]


def _dns_args(profile: Profile) -> list[str]:
    """Force this session's DNS through the profile's resolver.

    Raises rather than launching without it: a profile configured to resolve
    through Cloudflare that quietly resolves through the ISP is a leak the user
    believes they closed, which is worse than a failed launch they can see.
    """
    dns = (getattr(profile, "dns", "") or "").strip()
    if not dns:
        return []
    template = dns if dns.startswith("https://") else DOH_TEMPLATES.get(dns)
    if not template:
        raise ValueError(
            f"{dns!r} is not a DNS-over-HTTPS resolver. Chromium cannot be pointed "
            f"at a plain nameserver; give a DoH URL (https://...) or one of: "
            f"{', '.join(sorted(DOH_TEMPLATES))}.")
    return ["--enable-features=DnsOverHttps",
            f"--dns-over-https-templates={template}",
            "--dns-over-https-mode=secure"]


# What each "don't carry this over" switch has to remove. Paths are relative to
# the profile directory Chromium actually uses; the journal/companion files come
# too, because leaving one behind gets the store rebuilt from it.
CLEARABLE = {
    "history": ["Default/History", "Default/History-journal",
                "Default/Top Sites", "Default/Top Sites-journal",
                "Default/Visited Links"],
    "passwords": ["Default/Login Data", "Default/Login Data-journal",
                  "Default/Login Data For Account",
                  "Default/Login Data For Account-journal"],
    "cookies": ["Default/Network/Cookies", "Default/Network/Cookies-journal"],
    "localStorage": ["Default/Local Storage"],
    "indexedDb": ["Default/IndexedDB"],
    "sessions": ["Default/Sessions", "Default/Current Session",
                 "Default/Current Tabs", "Default/Last Session",
                 "Default/Last Tabs"],
    "extensionData": ["Default/Local Extension Settings",
                      "Default/Extension State", "Default/Sync Extension Settings"],
    "bookmarks": ["Default/Bookmarks", "Default/Bookmarks.bak"],
}


def _clear_unkept(profile: Profile, user_data_dir) -> None:
    """Drop the stores this profile is set not to carry between launches.

    Cleared on the way *in* rather than on the way out: there is exactly one
    place to do it, and a session that ends by crashing still starts clean next
    time. Chromium recreates each of these on demand, so removing them is a
    reset, not damage.
    """
    import shutil

    root = Path(user_data_dir)
    for key in (getattr(profile, "clear_on_start", None) or []):
        for rel in CLEARABLE.get(key, ()):
            target = root / rel
            if target.is_dir():
                shutil.rmtree(target, ignore_errors=True)
            elif target.exists():
                try:
                    target.unlink()
                except OSError:
                    pass  # locked by a session still shutting down; next launch gets it


def _seed_bookmarks(profile: Profile, user_data_dir) -> None:
    """Merge the profile's bookmarks into Chromium's bookmark bar.

    Merge, not overwrite: anything the user bookmarked inside the browser stays,
    and editing the list in Numera still takes effect. Matched by URL, so
    re-launching doesn't stack duplicates.

    Written before launch because Chromium reads this file at startup and
    rewrites it on exit - touching it while the browser is open loses the edit.
    """
    wanted = [b for b in (getattr(profile, "bookmarks", None) or [])
              if isinstance(b, dict) and b.get("url")]
    if not wanted:
        return
    path = Path(user_data_dir) / "Default" / "Bookmarks"
    path.parent.mkdir(parents=True, exist_ok=True)

    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
        bar = doc["roots"]["bookmark_bar"]
        children = bar.setdefault("children", [])
    except Exception:  # noqa: BLE001 - absent or unreadable: start a fresh file
        children = []
        bar = {"children": children, "date_added": "0", "id": "1",
               "name": "Bookmarks bar", "type": "folder"}
        doc = {"roots": {"bookmark_bar": bar,
                         "other": {"children": [], "date_added": "0", "id": "2",
                                   "name": "Other bookmarks", "type": "folder"},
                         "synced": {"children": [], "date_added": "0", "id": "3",
                                    "name": "Mobile bookmarks", "type": "folder"}},
               "version": 1}

    have = {c.get("url") for c in children if isinstance(c, dict)}
    # Chromium timestamps are microseconds since 1601-01-01, not the Unix epoch.
    stamp = str(int((time.time() + 11644473600) * 1_000_000))
    next_id = max((int(c.get("id", 0)) for c in children if str(c.get("id", "")).isdigit()),
                  default=int(bar.get("id", 1)))
    for b in wanted:
        if b["url"] in have:
            continue
        next_id += 1
        children.append({"date_added": stamp, "id": str(next_id), "type": "url",
                         "name": b.get("name") or b["url"], "url": b["url"]})
    # No "checksum" key: Chromium recomputes a missing one, but rejects a wrong
    # one and resets the file - which would silently drop the user's own marks.
    doc.pop("checksum", None)
    path.write_text(json.dumps(doc, indent=2), encoding="utf-8")


def _resolve_ip_based(profile: Profile) -> Profile:
    """Point the signals the user marked "based on IP" at the real exit country.

    A stored timezone is a guess made when the profile was created; the proxy it
    ends up using may exit somewhere else entirely, and a browser whose clock
    disagrees with its IP is the cheapest possible tell. Costs one request
    through the proxy at launch, and only when something is actually marked.
    """
    wanted = set(getattr(profile, "ip_based", None) or [])
    if not wanted or not profile.proxy:
        return profile
    from . import proxypool

    try:
        country = proxypool.exit_country(profile.proxy)
    except Exception:  # noqa: BLE001 - a launch must not fail on a lookup
        country = None
    if not country:
        return profile

    fp = profile.fingerprint
    changes: dict = {}
    locale = fp_mod_devices.COUNTRY_TO_LOCALE.get(country)
    if locale and locale in fp_mod_devices.LOCALES:
        tz, langs = fp_mod_devices.LOCALES[locale]
        if "timezone" in wanted:
            changes["timezone"] = tz
        if "languages" in wanted:
            changes["locale"] = locale
            changes["languages"] = list(langs)
    if "geolocation" in wanted and country in fp_mod_devices.COUNTRY_GEO:
        # The coordinates themselves live on the proxy, which is what
        # `_geolocation_for` already reads first.
        profile = replace(profile, proxy=replace(profile.proxy, country=country))
    return replace(profile, fingerprint=replace(fp, **changes)) if changes else profile


def _extension_args(profile: Profile) -> list[str]:
    """Chromium flags that load the profile's unpacked extensions.

    Both flags are required: ``--load-extension`` alone still leaves every other
    extension enabled, which would leak identity between profiles.
    """
    paths = [str(Path(p).expanduser().resolve())
             for p in (getattr(profile, "extensions", None) or [])]
    paths = [p for p in paths if Path(p).is_dir()]
    if not paths:
        return []
    joined = ",".join(paths)
    return [f"--disable-extensions-except={joined}", f"--load-extension={joined}"]


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

    pw = sync_playwright().start()
    try:
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
    except BaseException:
        if relay:
            relay.close()   # the browser never opened; don't strand its port
        raise
    if relay:
        # Tie the relay to the session: whoever closes the context — the user
        # closing the window, `session()`, the API — frees the port with it.
        context.on("close", lambda *_: relay.close())
    context.add_init_script(build_init_script(fp))
    _apply_client_hints(context, fp)
    page = context.pages[0] if context.pages else context.new_page()
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
    proxy_cfg, relay = _browser_proxy(profile)
    try:
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
    except BaseException:
        if relay:
            relay.close()   # the browser never opened; don't strand its port
        raise
    if relay:
        context.on("close", lambda *_: relay.close())
    page = context.pages[0] if context.pages else context.new_page()
    _open_startup_urls(context, page, profile)

    if keep_open:
        return None, context, page
    try:
        _wait_until_closed(page)
    finally:
        context.close()
    return None
