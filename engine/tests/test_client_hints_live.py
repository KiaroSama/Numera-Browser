"""Client Hints must describe the profile on the wire, not the host.

Chrome sends three hints unprompted and adds the high-entropy ones only after a
site returns `Accept-CH`. Those extra ones are filled in by the *browser*, from
the machine it is really running on - so this needs two requests to catch: the
first carries the low-entropy trio, the second carries whatever the browser
decided to reveal about the host.

A local server serves both, so the test needs no network and cannot be fooled by
a cache.
"""
import http.server
import threading

import pytest

from numera import generate
from numera.fingerprint import platform_version
from numera.models import Profile
from numera.store import ProfileStore

pytest.importorskip("playwright")

from numera import drivers  # noqa: E402

pytestmark = pytest.mark.slow

PAGE = b"""<!doctype html><meta charset=utf-8><title>ch</title>
<script>
  // The second request. The first response asked for the high-entropy hints,
  // so this one carries whatever the browser is willing to say.
  fetch('/second').then(() => { document.title = 'done'; });
</script>"""


class Site:
    """Serves one page with `Accept-CH`, then records the next request."""

    def __init__(self):
        self.seen = []
        outer = self

        class H(http.server.BaseHTTPRequestHandler):
            protocol_version = "HTTP/1.1"

            def do_GET(self):
                outer.seen.append((self.path, {k.lower(): v for k, v in self.headers.items()}))
                body = PAGE if self.path == "/" else b"ok"
                self.send_response(200)
                self.send_header("Content-Type", "text/html; charset=utf-8")
                self.send_header("Content-Length", str(len(body)))
                # Ask for everything a real site can ask for.
                self.send_header("Accept-CH", ", ".join([
                    "Sec-CH-UA-Platform-Version", "Sec-CH-UA-Arch",
                    "Sec-CH-UA-Bitness", "Sec-CH-UA-Model",
                    "Sec-CH-UA-Full-Version", "Sec-CH-UA-Full-Version-List",
                    "Sec-CH-UA-WoW64",
                ]))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *a):
                pass

        self.server = http.server.HTTPServer(("127.0.0.1", 0), H)
        self.url = f"http://127.0.0.1:{self.server.server_port}/"

    def __enter__(self):
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        return self

    def __exit__(self, *exc):
        self.server.shutdown()
        self.server.server_close()

    def hints(self, path):
        for p, h in self.seen:
            if p == path:
                return {k: v for k, v in h.items() if k.startswith("sec-ch-ua")}
        return None


def visit(tmp_path, fp, url):
    store = ProfileStore(tmp_path)
    p = Profile(name="ch", fingerprint=fp)
    store.save(p)
    with drivers.session(p, store, headless=True) as ctx:
        page = ctx.pages[0]
        page.goto(url, wait_until="load")
        page.wait_for_timeout(700)
        data = page.evaluate(
            "() => navigator.userAgentData"
            " ? navigator.userAgentData.getHighEntropyValues("
            "     ['platformVersion','architecture','bitness','model',"
            "      'uaFullVersion','fullVersionList','wow64'])"
            "     .then(v => ({present: true, ...v}))"
            " : {present: false}")
    return data


def test_the_second_request_reports_the_profile_not_the_host(tmp_path):
    """The leak this exists to catch: after Accept-CH the browser answers
    `sec-ch-ua-platform-version` from the OS it is really running on."""
    fp = generate(os="windows")
    with Site() as site:
        js = visit(tmp_path, fp, site.url)
        second = site.hints("/second")

    assert second, "the second request never arrived"
    assert second.get("sec-ch-ua-platform") == fp.ch_platform, second
    # Chrome encodes the OS release rather than naming it - Windows 11 is
    # "15.0.0" - so the wire, the page and the profile all agree on the same
    # encoding, not on the marketing name.
    want = platform_version(fp.os, fp.os_version)
    assert second["sec-ch-ua-platform-version"].strip('"') == want,         f"the host's OS version leaked: {second['sec-ch-ua-platform-version']} (want {want})"
    assert second["sec-ch-ua-arch"] == '"x86"', second["sec-ch-ua-arch"]
    assert second["sec-ch-ua-model"] == '""', second["sec-ch-ua-model"]
    assert js["present"] is True
    assert js["platformVersion"] == want, "the page and the wire disagree"
    assert js["architecture"] == "x86"
    assert js["wow64"] is False


def test_an_android_profile_reports_arm_and_its_handset(tmp_path):
    fp = generate(os="android")
    with Site() as site:
        js = visit(tmp_path, fp, site.url)
        second = site.hints("/second")

    assert second.get("sec-ch-ua-mobile") == "?1"
    assert second["sec-ch-ua-arch"] == '"arm"'
    assert second["sec-ch-ua-model"].strip('"') == fp.device_model
    assert js["architecture"] == "arm"
    assert js["model"] == fp.device_model


def test_an_ios_profile_has_no_userAgentData_and_coherent_hints(tmp_path):
    """Real Safari sends no Client Hints, and Chromium under Playwright cannot
    be made to stop.

    `route.continue_(headers=...)` and every `--disable-features` combination
    were measured against this server and changed nothing - Chromium appends the
    trio after both. So the achievable goal is the in-page object being absent
    (which is enforceable) and the headers agreeing with the rest of the
    identity rather than contradicting it. The residual tell - an "iPhone" that
    sends Client Hints at all - is a ceiling of the engine, recorded in
    docs/architecture.md.
    """
    fp = generate(os="ios")
    with Site() as site:
        js = visit(tmp_path, fp, site.url)
        first, second = site.hints("/"), site.hints("/second")

    assert js["present"] is False, "navigator.userAgentData exists on an iOS profile"
    assert first.get("sec-ch-ua-platform") == '"iOS"', first
    assert first.get("sec-ch-ua-mobile") == "?1", first
    # The OS version is a high-entropy hint, so it belongs on the opted-in
    # request, not the first one. This used to assert it on `/` because every
    # hint was pinned as a header and rode on every request.
    assert second.get("sec-ch-ua-platform-version", '""').strip('"') == \
        platform_version(fp.os, fp.os_version)


def test_the_low_entropy_trio_is_always_present_for_chrome(tmp_path):
    """Absence would be its own tell: every Chrome sends these unprompted."""
    fp = generate(os="macos")
    with Site() as site:
        visit(tmp_path, fp, site.url)
        first = site.hints("/")
    assert set(first) >= {"sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"}, first
    assert first["sec-ch-ua-platform"] == fp.ch_platform


HIGH = ("sec-ch-ua-platform-version", "sec-ch-ua-arch", "sec-ch-ua-bitness",
        "sec-ch-ua-model", "sec-ch-ua-full-version", "sec-ch-ua-full-version-list",
        "sec-ch-ua-wow64")


def test_the_first_request_carries_no_high_entropy_hints(tmp_path):
    """Timing, not values. Chrome sends the high-entropy hints only after a site
    has asked with `Accept-CH`; the first request to a new origin carries the
    trio and nothing else.

    These used to be pinned as `extra_http_headers`, which is the one lever that
    overrides what Chromium derives from the host - and a header goes on *every*
    request, so a profile announced its architecture and OS build to a site that
    had never asked. Unusual on its own, and trivially detectable. Handing the
    values to Chromium as UA-CH metadata instead leaves the timing to Chromium.
    """
    fp = generate(os="windows")
    with Site() as site:
        visit(tmp_path, fp, site.url)
        first = site.hints("/")

    assert first, "the first request never arrived"
    leaked = sorted(k for k in HIGH if k in first)
    assert not leaked, f"sent before the site asked for them: {leaked}"
    assert set(first) == {"sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"}, first


def test_the_opt_in_still_produces_the_profile_values(tmp_path):
    """The other half: giving Chromium the timing must not give it the host's
    values back. Both requests in one visit, so the pair cannot drift apart."""
    fp = generate(os="windows")
    with Site() as site:
        visit(tmp_path, fp, site.url)
        first, second = site.hints("/"), site.hints("/second")

    assert not [k for k in HIGH if k in first]
    assert [k for k in HIGH if k in second], "the opt-in produced nothing"
    assert second["sec-ch-ua-platform-version"].strip('"') == platform_version(
        fp.os, fp.os_version)
    # The trio is identical across both, as it is for a real browser.
    for key in ("sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"):
        assert first[key] == second[key], key


def test_an_apple_silicon_mac_reports_arm_in_the_page_and_on_the_wire(tmp_path):
    """The page used to derive architecture from `isMobile` alone, so an M-series
    Mac answered "x86" in JS while the header said "arm" - one call apart."""
    fp = generate(os="macos")
    fp.webgl_renderer = "ANGLE (Apple, Apple M2, OpenGL 4.1)"
    with Site() as site:
        js = visit(tmp_path, fp, site.url)
        second = site.hints("/second")

    assert second["sec-ch-ua-arch"] == '"arm"', second["sec-ch-ua-arch"]
    assert js["architecture"] == "arm", "the page and the wire disagree on the CPU"
