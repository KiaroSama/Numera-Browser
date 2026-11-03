"""Browser SDK launch implementations, sharing the driver's ownership seam."""
from __future__ import annotations

import warnings
from . import drivers as _d
from . import bundled, paths as paths_mod, fingerprint as fp_mod
from .models import Profile

# --------------------------------------------------------------------------
# Shared Playwright-style launch (used by both playwright and patchright).
# --------------------------------------------------------------------------
def _launch_playwright_like(sync_playwright, profile: Profile, store, headless: bool,
                            keep_open: bool, extra_args=None, backend="playwright"):
    profile = _d._resolve_ip_based(profile)
    _d._clear_unkept(profile, store.user_data_path(profile.id))
    _d._seed_bookmarks(profile, store.user_data_path(profile.id))
    fp = profile.fingerprint
    launch_args = [
        f"--window-size={fp.viewport_width},{fp.viewport_height}",
        "--disable-blink-features=AutomationControlled",
        "--no-first-run",
        "--no-default-browser-check",
        *_d._webrtc_arg(profile),
        *_d._extension_args(profile),
        *_d._dns_args(profile),
        *_d._switched_off_args(profile),
        *(profile.launch_args or []),
        *(extra_args or []),
    ]
    hints = fp_mod.client_hint_headers(fp)
    geolocation = _d._geolocation_for(profile)
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
        proxy_cfg, relay = _d._browser_proxy(profile)
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
        browser_options = bundled.launch_options(backend, store.root)
        if not browser_options and backend == "playwright":
            paths_mod.use_bundled_browsers()
        # Started inside the session's own tree, which is the only moment it
        # can be decided: everything this driver goes on to start - the
        # browser, its renderers - inherits the group or job it was born in.
        with _d._own_driver_tree(_d._driver_program(sync_playwright)):
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
            **browser_options,
            **context_opts,
        )
        if relay:
            # Tie the relay to the session: whoever closes the context — the user
            # closing the window, `session()`, the API — frees the port with it.
            context.on("close", lambda *_: relay.close())
        context.add_init_script(_d.build_init_script(fp))
        _apply_client_hints(context, fp)
        page = context.pages[0] if context.pages else context.new_page()
    except BaseException as failure:
        _d._abandon(context, pw, relay, failure)
        raise
    _d._opened()
    _d._open_startup_urls(context, page, profile)

    if keep_open:
        return pw, context, page
    try:
        _d._wait_until_closed(page)
    finally:
        context.close()
        pw.stop()
        _d._closed()
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


@_d.register("playwright", requires="playwright")
def _driver_playwright(profile, store, headless=False, keep_open=True, extra_args=None):
    from playwright.sync_api import sync_playwright
    return _launch_playwright_like(sync_playwright, profile, store, headless, keep_open, extra_args)


@_d.register("patchright", requires="patchright")
def _driver_patchright(profile, store, headless=False, keep_open=True, extra_args=None):
    # Patchright is an API-compatible, patched drop-in for Playwright.
    from patchright.sync_api import sync_playwright
    return _launch_playwright_like(sync_playwright, profile, store, headless, keep_open, extra_args,
                                   backend="patchright")


@_d.register("camoufox", requires="camoufox")
def _driver_camoufox(profile, store, headless=False, keep_open=True, extra_args=None):
    """Camoufox (patched Firefox) generates its own coherent fingerprint; we feed
    it the profile's OS/locale/proxy as hints and keep the persistent session."""
    if keep_open:
        raise RuntimeError(
            "The camoufox engine supports launch-and-wait only (keep_open=False), "
            "which is what `numera launch` and the dashboard use."
        )
    kwargs, relay = _d._camoufox_kwargs(profile, store, headless)

    try:
        with _d._camoufox_session(kwargs) as browser:
            page = browser.pages[0] if getattr(browser, "pages", None) else browser.new_page()
            _d._opened()
            _d._open_startup_urls(browser, page, profile)
            if not headless:
                page.wait_for_event("close", timeout=0)
    finally:
        if relay:
            relay.close()
    return None


@_d.register("cloak", requires="cloakbrowser")
def _driver_cloak(profile, store, headless=False, keep_open=True, extra_args=None):
    """CloakBrowser — a patched Chromium binary whose fingerprints are altered at
    the C++ source level. Numera feeds it the profile's identity + proxy and keeps
    the persistent session; the binary does the heavy stealth work (its own
    fingerprint, TLS, CDP), so we don't inject our JS stealth script here.

    Missing binaries require a separately approved finite acquisition.
    """
    from .cloak_acquisition import launch_options
    browser_options = bundled.launch_options('cloak', store.root)
    browser_options.update(launch_options())
    import cloakbrowser

    profile = _d._resolve_ip_based(profile)
    _d._clear_unkept(profile, store.user_data_path(profile.id))
    _d._seed_bookmarks(profile, store.user_data_path(profile.id))
    fp = profile.fingerprint
    # Same guard as the Playwright path, and for the same reason: the window
    # between the first acquired resource and READY is where a failure used to
    # leave a browser - or just a relay port - behind for a session nobody owns.
    relay = context = None
    try:
        proxy_cfg, relay = _d._browser_proxy(profile)
        # The same seam the other engines use. CloakBrowser hands back a
        # Playwright context, so it drives a Playwright-family node driver, and
        # that driver's spawn is where this session's tree is decided. If the
        # program cannot be named, or the tree cannot be made, the launch is
        # refused rather than quietly falling back to the host tree - a browser
        # nothing owns is one nothing can prove has stopped.
        with _d._own_driver_tree(_d._cloak_programs(cloakbrowser)):
            context = cloakbrowser.launch_persistent_context(
                **browser_options,
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
                      *_d._webrtc_arg(profile), *_d._extension_args(profile),
                      *_d._dns_args(profile), *_d._switched_off_args(profile),
                      *(profile.launch_args or []),
                      *(extra_args or [])],
            )
        if relay:
            context.on("close", lambda *_: relay.close())
        page = context.pages[0] if context.pages else context.new_page()
    except BaseException as failure:
        # cloakbrowser owns no separate driver, so there is nothing to stop
        # between the browser and the relay.
        _d._abandon(context, None, relay, failure)
        raise
    _d._opened()
    _d._open_startup_urls(context, page, profile)

    if keep_open:
        return None, context, page
    try:
        _d._wait_until_closed(page)
    finally:
        context.close()
        _d._closed()
    return None
