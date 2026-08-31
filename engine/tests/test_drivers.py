"""Tests for the pluggable launch-driver registry."""

from numera import drivers
from numera.models import Profile
from numera.fingerprint import generate


def test_builtin_engines_registered():
    names = drivers.names()
    assert "playwright" in names
    assert "patchright" in names
    assert "camoufox" in names


def test_availability_map_covers_all():
    avail = drivers.available()
    assert set(avail) == set(drivers.names())
    assert all(isinstance(v, bool) for v in avail.values())


def test_unknown_engine_raises():
    try:
        drivers.get("does-not-exist")
        assert False, "expected RuntimeError"
    except RuntimeError as e:
        assert "Unknown engine" in str(e)


def test_uninstalled_engine_raises_with_hint():
    # camoufox isn't a test dependency, so this should raise a helpful error.
    if not drivers.is_installed("camoufox"):
        try:
            drivers.get("camoufox")
            assert False, "expected RuntimeError"
        except RuntimeError as e:
            assert "camoufox" in str(e)


def test_profile_defaults_to_playwright_engine():
    p = Profile(name="x", fingerprint=generate(os="windows"))
    assert p.engine == "playwright"
    # round-trips through serialization
    assert Profile.from_dict(p.to_dict()).engine == "playwright"


# ---- geolocation + WebRTC coherence --------------------------------------
from numera.models import Proxy
from numera.drivers import _geolocation_for, _webrtc_arg


def _p(country=None, locale="en-US"):
    proxy = Proxy(server="http://h:1", country=country) if country else None
    return Profile(name="g", fingerprint=generate(seed=1, locale=locale), proxy=proxy)


def test_geolocation_follows_proxy_country():
    geo = _geolocation_for(_p(country="DE"))
    assert (round(geo["latitude"]), round(geo["longitude"])) == (53, 13)  # Berlin


def test_geolocation_falls_back_to_locale_without_proxy():
    geo = _geolocation_for(_p(locale="fr-FR"))
    assert round(geo["latitude"]) == 49 and round(geo["longitude"]) == 2   # Paris


def test_geolocation_none_for_unknown_country():
    assert _geolocation_for(_p(country="ZZ")) is None


def test_webrtc_forces_proxy_when_one_is_set():
    assert "disable_non_proxied_udp" in _webrtc_arg(_p(country="US"))[0]


def test_webrtc_hides_local_ip_without_proxy():
    assert "default_public_interface_only" in _webrtc_arg(_p())[0]


# ---- headless has to use the full browser, not the headless shell -------
# Playwright's default headless binary is chromium_headless_shell, which cannot
# load extensions at all and announces itself as HeadlessChrome over CDP. The
# "chromium" channel is the same download in --headless=new mode.


class _FakeCdpSession:
    def __init__(self, sink):
        self.sink = sink

    def send(self, method, params=None):
        self.sink.append((method, params or {}))


class _FakeContext:
    def __init__(self):
        self.pages = []
        self.closed = False
        self.cdp = []          # every (method, params) the launcher sent
        self._handlers = {}

    def add_init_script(self, script):
        pass

    def on(self, event, handler):
        self._handlers.setdefault(event, []).append(handler)

    def new_cdp_session(self, page):
        return _FakeCdpSession(self.cdp)

    def new_page(self):
        page = _FakePage()
        self.pages.append(page)
        # Playwright emits this, and the launcher relies on it to reach pages
        # that did not exist when the context was built.
        for handler in self._handlers.get("page", []):
            handler(page)
        return page

    def close(self):
        self.closed = True


class _FakePage:
    url = "about:blank"

    def goto(self, *a, **k):
        pass


class _FakePlaywright:
    def __init__(self, seen):
        self.seen = seen

        class _Chromium:
            @staticmethod
            def launch_persistent_context(**kwargs):
                seen.update(kwargs)
                return _FakeContext()

        self.chromium = _Chromium()

    def start(self):
        return self

    def stop(self):
        pass


def _launch(headless=True, prof=None):
    """Run the real launch path against the fakes; return (kwargs, context)."""
    from numera.drivers import _launch_playwright_like
    from numera.store import ProfileStore
    import tempfile

    seen = {}
    with tempfile.TemporaryDirectory() as tmp:
        prof = prof or Profile(name="k", fingerprint=generate(seed=3))
        _, ctx, _page = _launch_playwright_like(
            lambda: _FakePlaywright(seen), prof, ProfileStore(tmp),
            headless, keep_open=True)
    return seen, ctx


def _launch_kwargs(headless):
    return _launch(headless)[0]


# --- Client Hints reach the browser, and only the trio rides as a header ----

def _ua_override(ctx):
    for method, params in ctx.cdp:
        if method == "Emulation.setUserAgentOverride":
            return params
    return None


def test_the_launcher_hands_chromium_the_ua_metadata():
    """Without this the browser derives the hints from the host: a Windows 11
    profile on a Windows 10 machine reported platform-version "10.0"."""
    from numera.fingerprint import ua_metadata

    fp = generate(seed=3)
    _, ctx = _launch(prof=Profile(name="k", fingerprint=fp))
    sent = _ua_override(ctx)
    assert sent, f"no setUserAgentOverride was sent: {ctx.cdp}"
    assert sent["userAgentMetadata"] == ua_metadata(fp)
    assert sent["userAgent"] == fp.user_agent


def test_the_high_entropy_hints_are_not_pinned_as_headers():
    """A header goes on every request; Chrome sends these only after a site asks
    with `Accept-CH`. Pinning them announced the OS build to sites that never
    asked - the whole reason the CDP override exists."""
    headers = {k.lower() for k in _launch_kwargs(True)["extra_http_headers"]}
    assert "sec-ch-ua" in headers, "the low-entropy trio must still be pinned"
    assert not (headers & {"sec-ch-ua-platform-version", "sec-ch-ua-arch",
                           "sec-ch-ua-model", "sec-ch-ua-bitness",
                           "sec-ch-ua-full-version-list", "sec-ch-ua-wow64"})


def test_headless_uses_the_full_chromium_channel():
    assert _launch_kwargs(headless=True)["channel"] == "chromium"


def test_headed_leaves_the_channel_alone():
    assert _launch_kwargs(headless=False)["channel"] is None


# --- camoufox: inspection must wear the same identity as a launch -----------

def test_camoufox_launch_and_session_build_the_same_options(tmp_path):
    """`session()` used to build a shorter set - no `os`, no `locale` - so the
    trust report, the leak test and the TLS check graded a browser that was not
    wearing the profile's identity, then reported it as the profile's."""
    from dataclasses import replace

    from numera import drivers, generate
    from numera.models import Profile, Proxy
    from numera.store import ProfileStore

    store = ProfileStore(tmp_path)
    fp = replace(generate(os="macos"), locale="de-DE")
    p = Profile(name="c", fingerprint=fp,
                proxy=Proxy(server="http://1.2.3.4:8080"))
    store.save(p)

    kwargs, relay = drivers._camoufox_kwargs(p, store, headless=True)
    if relay:
        relay.close()
    assert kwargs["os"] == "macos"
    assert kwargs["locale"] == "de-DE"
    assert kwargs["persistent_context"] is True
    assert "proxy" in kwargs


def test_camoufox_geoip_follows_the_profiles_own_choice(tmp_path):
    """`geoip=True` was unconditional, so a profile with every signal set to
    manual still had its timezone and locale rewritten from the exit IP - the
    opposite of what the editor said."""
    from numera import drivers, generate
    from numera.models import Profile, Proxy
    from numera.store import ProfileStore

    store = ProfileStore(tmp_path)
    fp = generate(os="windows")

    manual = Profile(name="m", fingerprint=fp, proxy=Proxy(server="http://1.2.3.4:8080"))
    store.save(manual)
    kwargs, relay = drivers._camoufox_kwargs(manual, store, headless=True)
    if relay:
        relay.close()
    assert "geoip" not in kwargs, "geoip was forced on a manual profile"

    by_ip = Profile(name="i", fingerprint=fp, proxy=Proxy(server="http://1.2.3.4:8080"),
                    ip_based=["timezone"])
    store.save(by_ip)
    kwargs, relay = drivers._camoufox_kwargs(by_ip, store, headless=True)
    if relay:
        relay.close()
    assert kwargs.get("geoip") is True


def test_the_all_or_nothing_limitation_is_stated(tmp_path):
    """Camoufox moves timezone, locale and geolocation together - it has no
    per-signal control - so a profile asking for only one gets all three. That
    is a capability gap worth naming rather than burying."""
    from numera import drivers

    assert drivers.CAMOUFOX_GEOIP_IS_ALL_OR_NOTHING is True


def test_a_mobile_profile_does_not_become_a_silently_wrong_desktop(tmp_path):
    """Camoufox is Firefox and models three desktop platforms. An Android
    profile has no equivalent, so it opens as Windows deliberately rather than
    claiming Android and rendering something else."""
    from numera import drivers, generate
    from numera.models import Profile
    from numera.store import ProfileStore

    store = ProfileStore(tmp_path)
    p = Profile(name="a", fingerprint=generate(os="android"))
    store.save(p)
    kwargs, relay = drivers._camoufox_kwargs(p, store, headless=True)
    if relay:
        relay.close()
    assert kwargs["os"] == "windows"
