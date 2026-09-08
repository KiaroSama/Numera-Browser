"""Real-browser proof for the phase-5 parity fields.

Every field added in this phase claims to change what a page can observe. This
project has shipped that claim five times without it being true (B5, B14, B15),
so nothing here reads a value back out of a dict, an argument list or the
fingerprint object. Each test launches a browser and asks the *page*.

Two environment notes that shape the whole file:

* ``navigator.deviceMemory``, ``navigator.userAgentData`` and
  ``navigator.mediaDevices`` are secure-context only, and a page on
  ``about:blank`` has an opaque origin. Every page here is therefore served from
  a routed ``https://`` URL, fulfilled locally so nothing touches the network.
  Testing these on ``about:blank`` reads ``null`` in every mode and passes
  vacuously.
* Anything the host cannot demonstrate skips loudly rather than passing.
"""

from dataclasses import replace

import pytest

from numera import generate
from numera.models import Profile
from numera.store import ProfileStore

# Live: opens a real browser. `-m slow` must select every one of these, or a
# local run silently skips what only CI's glob was catching.
pytestmark = pytest.mark.slow

pytest.importorskip("playwright")

from _live import requires_browser        # noqa: E402

requires_browser()

PAGE = "https://numera.test/"


def _page(tmp_path, name, **fp_over):
    """Launch a profile on a secure local origin; hand back (page, teardown)."""
    from numera.drivers import get
    store = ProfileStore(tmp_path / name)
    prof = Profile(name=name, fingerprint=replace(generate(seed=11), **fp_over))
    store.save(prof)
    try:
        pw, ctx, page = get("playwright")(prof, store, headless=True, keep_open=True)
    except Exception as e:
        pytest.skip(f"chromium unavailable: {str(e).splitlines()[0][:80]}")

    # A secure context with no network: the request never leaves the process.
    page.route("**/*", lambda r: r.fulfill(
        status=200, content_type="text/html", body="<html><body></body></html>"))
    page.goto(PAGE)

    def close():
        ctx.close()
        pw.stop()

    return page, close


def _read(tmp_path, name, script, **fp_over):
    page, close = _page(tmp_path, name, **fp_over)
    try:
        return page.evaluate(script)
    finally:
        close()


# --------------------------------------------------------------------------
# Touch surface + device pixel ratio — what makes an emulated phone hold up
# --------------------------------------------------------------------------
TOUCH = """() => ({
  maxTouchPoints: navigator.maxTouchPoints,
  ontouchstart: 'ontouchstart' in window,
})"""


def test_max_touch_points_reaches_the_page(tmp_path):
    desktop = _read(tmp_path, "t-desk", TOUCH, max_touch_points=0)
    phone = _read(tmp_path, "t-phone", TOUCH, max_touch_points=5)
    assert desktop["maxTouchPoints"] == 0
    assert phone["maxTouchPoints"] == 5, (
        "maxTouchPoints did not reach the page - the field is inert")
    # Feature detection is the usual mobile test; the count alone is not enough.
    assert phone["ontouchstart"] is True
    assert desktop["ontouchstart"] is False


def test_device_pixel_ratio_reaches_the_page(tmp_path):
    """Playwright's context-level scale factor is a flat 1-or-2 guess, so real
    display scaling only shows up if the profile's own value gets through."""
    scaled = _read(tmp_path, "dpr-125", "() => window.devicePixelRatio",
                   device_pixel_ratio=1.25)
    retina = _read(tmp_path, "dpr-3", "() => window.devicePixelRatio",
                   device_pixel_ratio=3.0)
    assert scaled == 1.25, f"devicePixelRatio 1.25 read back as {scaled!r}"
    assert retina == 3.0, f"devicePixelRatio 3.0 read back as {retina!r}"


# --------------------------------------------------------------------------
# The edge of what an init script can reach
#
# The two tests below fail today and are meant to. maxTouchPoints and
# devicePixelRatio are patched in JavaScript, but the CSS media queries that
# describe the same two facts are answered below the page by CDP Emulation,
# which Playwright drives from the *context* options in ``drivers.py``. So an
# Android profile currently reports five touch points and a 2.625 pixel ratio
# in JS while `(pointer: coarse)` says mouse and `(resolution: 2dppx)` says the
# context's flat guess — measured, not assumed. One `matchMedia` call sees both
# contradictions.
#
# Those two lines have since landed in ``_launch_playwright_like``'s
# ``context_opts``, so these now pass outright and the xfail markers are gone.
# The mechanism is worth keeping in mind: writing them as xfail rather than as a
# note in a report meant the suite announced XPASS the moment the fix arrived.
# A note gets lost; a test does not.
# --------------------------------------------------------------------------
MEDIA = """() => ({
  pointerCoarse: matchMedia('(pointer: coarse)').matches,
  hoverNone: matchMedia('(hover: none)').matches,
  dpr: window.devicePixelRatio,
  cssMatchesDpr: matchMedia('(resolution: ' + window.devicePixelRatio + 'dppx)').matches,
})"""


def test_touch_media_queries_agree_with_max_touch_points(tmp_path):
    seen = _read(tmp_path, "mq-touch", MEDIA, **_os_fields("android"))
    assert seen["pointerCoarse"] is True
    assert seen["hoverNone"] is True


def test_css_resolution_agrees_with_device_pixel_ratio(tmp_path):
    seen = _read(tmp_path, "mq-dpr", MEDIA, device_pixel_ratio=1.25)
    assert seen["cssMatchesDpr"] is True, (
        f"window.devicePixelRatio is {seen['dpr']} but CSS disagrees")


# --------------------------------------------------------------------------
# User-Agent Client Hints — the header said one thing, JS said another
# --------------------------------------------------------------------------
UACH = """async () => {
  const d = navigator.userAgentData;
  if (!d) return null;
  const high = await d.getHighEntropyValues(['platform', 'platformVersion', 'architecture']);
  return {
    platform: d.platform,
    mobile: d.mobile,
    brands: d.brands.map(b => b.brand + '/' + b.version),
    platformVersion: high.platformVersion,
    highPlatform: high.platform,
  };
}"""


def test_user_agent_data_reports_the_profile_not_the_host(tmp_path):
    """A macOS profile on a Windows host used to answer platform 'Windows' here
    while navigator.platform said 'MacIntel' - a contradiction inside one page."""
    mac = _read(tmp_path, "uad-mac", UACH, **_os_fields("macos"))
    if mac is None:
        pytest.skip("this build exposes no navigator.userAgentData")
    assert mac["platform"] == "macOS", (
        f"userAgentData.platform leaked the host: {mac['platform']!r}")
    assert mac["highPlatform"] == "macOS"
    assert mac["mobile"] is False


def test_user_agent_data_brands_match_the_declared_chrome_version(tmp_path):
    """The sec-ch-ua header claims Google Chrome/<major>; an unpatched Chromium
    answers its own build in JS. Both are read by the same anti-bot script."""
    from numera.fingerprint import sec_ch_ua
    fp = replace(generate(seed=11, os="windows"), chrome_version="128.0.0.0")
    seen = _read(tmp_path, "uad-brands", UACH, os="windows",
                 chrome_version="128.0.0.0",
                 platform="Win32", ch_platform='"Windows"',
                 user_agent=fp.user_agent)
    if seen is None:
        pytest.skip("this build exposes no navigator.userAgentData")
    assert "Google Chrome/128" in seen["brands"], (
        f"brands do not match the header {sec_ch_ua(fp)!r}: {seen['brands']}")


def test_windows_11_and_10_differ_only_in_the_platform_version_hint(tmp_path):
    """The UA string reads 'Windows NT 10.0' on both, so this hint is the only
    thing that separates them - and it was never sent at all before."""
    win11 = _read(tmp_path, "osv-11", UACH, os="windows", os_version="11")
    win10 = _read(tmp_path, "osv-10", UACH, os="windows", os_version="10")
    if win11 is None:
        pytest.skip("this build exposes no navigator.userAgentData")
    assert win11["platformVersion"] == "15.0.0"
    assert win10["platformVersion"] == "10.0.0"


def test_android_os_version_reaches_the_user_agent(tmp_path):
    """On mobile the release is in the UA string itself, not just the hint."""
    ua = _read(tmp_path, "osv-a", "() => navigator.userAgent",
               **_os_fields("android", os_version="13", device_model="Pixel 7"))
    assert "Android 13; Pixel 7" in ua, ua


# --------------------------------------------------------------------------
# deviceMemory: 0 means "this platform has no such property"
# --------------------------------------------------------------------------
def test_device_memory_is_absent_when_the_platform_has_none(tmp_path):
    present = _read(tmp_path, "mem-8", "() => navigator.deviceMemory", device_memory=8)
    if present is None:
        pytest.skip("navigator.deviceMemory unavailable in this build")
    assert present == 8
    absent = _read(tmp_path, "mem-0", "() => navigator.deviceMemory", device_memory=0)
    assert absent is None, (
        f"deviceMemory should not exist at all, got {absent!r} - answering with a "
        "number is a harder tell than the value itself")


# --------------------------------------------------------------------------
# ClientRects mode
# --------------------------------------------------------------------------
RECT = """() => {
  const d = document.createElement('div');
  d.style.cssText = 'position:absolute;left:0;top:0;width:123px;height:45px';
  document.body.appendChild(d);
  const r = d.getBoundingClientRect();
  return {w: r.width, h: r.height};
}"""


def test_client_rects_mode_changes_what_layout_reports(tmp_path):
    real = _read(tmp_path, "cr-real", RECT, client_rects="real")
    noise = _read(tmp_path, "cr-noise", RECT, client_rects="noise")
    assert real == {"w": 123, "h": 45}, (
        f"'real' must hand back the untouched measurement, got {real}")
    assert noise != real, "ClientRects noise is not reaching the page"


def test_client_rects_noise_is_stable_across_reads(tmp_path):
    """Same reasoning as canvas: a measurement that moves per call is a tell."""
    page, close = _page(tmp_path, "cr-stable", client_rects="noise")
    try:
        first = page.evaluate(RECT)
        second = page.evaluate(RECT)
    finally:
        close()
    assert first == second


# --------------------------------------------------------------------------
# WebGL rendered image (not the vendor/renderer strings)
# --------------------------------------------------------------------------
WEBGL_HASH = """() => {
  const c = document.createElement('canvas');
  c.width = 64; c.height = 64;
  const gl = c.getContext('webgl') || c.getContext('experimental-webgl');
  if (!gl) return null;
  gl.clearColor(0.2, 0.45, 0.8, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  const px = new Uint8Array(64 * 64 * 4);
  gl.readPixels(0, 0, 64, 64, gl.RGBA, gl.UNSIGNED_BYTE, px);
  let h = 5381;
  for (let i = 0; i < px.length; i++) h = ((h * 33) ^ px[i]) >>> 0;
  return h;
}"""


def test_webgl_image_noise_changes_the_rendered_pixels(tmp_path):
    """Spoofing the vendor/renderer strings leaves the *image* hash tied to the
    host GPU, so every profile on one machine shares it."""
    real = _read(tmp_path, "gl-real", WEBGL_HASH, webgl_image="real")
    noise = _read(tmp_path, "gl-noise", WEBGL_HASH, webgl_image="noise")
    if real is None or noise is None:
        pytest.skip("no WebGL context in this browser build")
    assert real != noise, "WebGL image noise is not reaching the page"


GL_DATAURL = """(preserve) => {
  const c = document.createElement('canvas');
  c.width = 64; c.height = 64;
  const gl = c.getContext('webgl', {preserveDrawingBuffer: preserve});
  if (!gl) return null;
  gl.clearColor(0.2, 0.45, 0.8, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  const u = c.toDataURL();
  let h = 5381;
  for (let i = 0; i < u.length; i++) h = ((h * 33) ^ u.charCodeAt(i)) >>> 0;
  return {hash: h, url: u};
}"""

# Decode a data URL and read a pixel back. getImageData is not patched, so this
# is an independent look at what the noised PNG actually contains.
DECODE_PIXEL = """(url) => new Promise((res, rej) => {
  const img = new Image();
  img.onload = () => {
    const c = document.createElement('canvas');
    c.width = img.width; c.height = img.height;
    const x = c.getContext('2d');
    x.drawImage(img, 0, 0);
    const d = x.getImageData(32, 32, 1, 1).data;
    res([d[0], d[1], d[2], d[3]]);
  };
  img.onerror = () => rej(new Error('decode failed'));
  img.src = url;
})"""


@pytest.mark.parametrize("preserve", [True, False])
def test_webgl_to_data_url_is_covered_too(tmp_path, preserve):
    """readPixels is one of two ways to read a rendered WebGL image; toDataURL on
    the same canvas is the other, and fingerprintjs uses it. Covering only the
    first left this path answering with the host GPU for every profile."""
    a = _read(tmp_path, f"gl-url-a{int(preserve)}", GL_DATAURL, seed=11,
              webgl_image="noise")
    if a is None:
        pytest.skip("no WebGL context in this browser build")
    page, close = _page(tmp_path, f"gl-url-a2{int(preserve)}", seed=11,
                        webgl_image="noise")
    try:
        a = page.evaluate(GL_DATAURL, preserve)
        again = page.evaluate(GL_DATAURL, preserve)
        # The page's own drawing must survive the noising. getImageData is not
        # patched, so decoding the PNG back is an independent look at it.
        pixel = page.evaluate(DECODE_PIXEL, a["url"])
    finally:
        close()
    b = _read(tmp_path, f"gl-url-b{int(preserve)}", GL_DATAURL, seed=999,
              webgl_image="noise")
    assert a["hash"] == again["hash"], "per-call randomness is a detection signal"
    assert a["hash"] != b["hash"], (
        "two profiles share a WebGL toDataURL hash - the host GPU is still "
        "visible through this path")
    # clearColor(0.2, 0.45, 0.8, 1) is roughly rgb(51, 115, 204). Noise must be
    # sparse enough that the picture is still the picture.
    r, g, bl, alpha = pixel
    assert alpha == 255 and abs(r - 51) < 12 and abs(g - 115) < 12 and abs(bl - 204) < 12, (
        f"the noised image no longer carries what the page drew: rgba{tuple(pixel)}")


def test_webgl_image_noise_is_seeded_per_profile(tmp_path):
    """Two profiles must differ from each other and each stay stable in itself."""
    page, close = _page(tmp_path, "gl-stable", webgl_image="noise")
    try:
        first = page.evaluate(WEBGL_HASH)
        second = page.evaluate(WEBGL_HASH)
    finally:
        close()
    if first is None:
        pytest.skip("no WebGL context in this browser build")
    assert first == second, "per-call randomness is itself a detection signal"
    other = _read(tmp_path, "gl-other", WEBGL_HASH, webgl_image="noise", seed=999)
    assert other != first, "two profiles share a WebGL image hash"


# --------------------------------------------------------------------------
# Media devices: speaker count and the masking toggle
# --------------------------------------------------------------------------
DEVICES = """async () => {
  if (!navigator.mediaDevices) return null;
  const list = await navigator.mediaDevices.enumerateDevices();
  const out = {videoinput: 0, audioinput: 0, audiooutput: 0};
  for (const d of list) out[d.kind] = (out[d.kind] || 0) + 1;
  return out;
}"""


def test_speaker_count_reaches_enumerate_devices(tmp_path):
    seen = _read(tmp_path, "spk", DEVICES, cameras=1, microphones=2, speakers=3)
    if seen is None:
        pytest.skip("navigator.mediaDevices unavailable in this build")
    assert seen == {"videoinput": 1, "audioinput": 2, "audiooutput": 3}, seen


def test_media_real_mode_stops_the_masking(tmp_path):
    """'Real' must let the host's own device list through untouched."""
    masked = _read(tmp_path, "md-mask", DEVICES, cameras=2, microphones=2,
                   speakers=2, media_mode="masked")
    real = _read(tmp_path, "md-real", DEVICES, cameras=2, microphones=2,
                 speakers=2, media_mode="real")
    if masked is None or real is None:
        pytest.skip("navigator.mediaDevices unavailable in this build")
    assert masked == {"videoinput": 2, "audioinput": 2, "audiooutput": 2}
    assert real != masked, "media masking is still installed in 'real' mode"


# --------------------------------------------------------------------------
# The profile's own font list, not the whole OS set
# --------------------------------------------------------------------------
def test_an_edited_font_list_is_what_the_page_probes(tmp_path):
    """`to_engine_profile` used to overwrite the profile's fonts with the entire
    OS set, so trimming the list changed nothing a page could see."""
    probe = """(f) => document.fonts.check('12px "' + f + '"')"""
    page, close = _page(tmp_path, "fl", fonts=["Georgia"], font_mode="masked")
    try:
        kept = page.evaluate(probe, "Georgia")
        dropped = page.evaluate(probe, "Segoe UI")
    finally:
        close()
    assert kept is True, "a font in the profile's list should read as installed"
    assert dropped is False, (
        "a font removed from the profile's list is still reported as installed")


# --------------------------------------------------------------------------
# iOS: emulated, and honest about the parts that cannot be emulated
# --------------------------------------------------------------------------
def _os_fields(os_key: str, **over) -> dict:
    """Every field that has to move together when forcing an OS onto seed 11.

    Overriding ``os`` alone would leave a Windows UA on an iPhone profile, which
    is not what a real iOS profile looks like and would make the test prove
    nothing. ``over`` re-rolls the user agent so an OS version or model given
    here reaches the string too.
    """
    from numera.fingerprint import build_user_agent
    fp = generate(seed=11, os=os_key)
    fields = {
        f: getattr(fp, f) for f in (
            "os", "platform", "ch_platform", "user_agent", "is_mobile",
            "webgl_vendor", "webgl_renderer", "fonts", "device_memory",
            "max_touch_points", "device_pixel_ratio", "os_version",
            "device_model", "screen_width", "screen_height", "viewport_width",
            "viewport_height", "hardware_concurrency", "chrome_version")
    }
    fields.update(over)
    fields["user_agent"] = build_user_agent(
        os_key, fields["chrome_version"], fields["os_version"], fields["device_model"])
    return fields


IOS_SHAPE = """() => ({
  ua: navigator.userAgent,
  platform: navigator.platform,
  touch: navigator.maxTouchPoints,
  memory: navigator.deviceMemory === undefined ? 'undefined' : navigator.deviceMemory,
  uaData: navigator.userAgentData === undefined ? 'undefined' : String(navigator.userAgentData),
  gl: (() => {
    const gl = document.createElement('canvas').getContext('webgl');
    if (!gl) return null;
    return gl.getParameter(37446);
  })(),
})"""


def test_ios_profile_presents_an_ios_shaped_surface(tmp_path):
    seen = _read(tmp_path, "ios", IOS_SHAPE, **_os_fields("ios"))
    assert "CriOS/" in seen["ua"] and "iPhone OS" in seen["ua"], seen["ua"]
    assert seen["platform"] == "iPhone"
    assert seen["touch"] == 5
    # WebKit implements neither of these, so an iOS profile must not have them.
    assert seen["memory"] == "undefined", (
        f"iOS exposes no navigator.deviceMemory, got {seen['memory']!r}")
    assert seen["uaData"] == "undefined", (
        "iOS ships no Client Hints; navigator.userAgentData must be absent")
    if seen["gl"] is not None:
        assert seen["gl"] == "Apple GPU", seen["gl"]


def test_android_profile_keeps_its_client_hints(tmp_path):
    """The iOS suppression must be iOS-only - Android Chrome does send UA-CH."""
    seen = _read(tmp_path, "andr", UACH, **_os_fields("android"))
    if seen is None:
        pytest.skip("this build exposes no navigator.userAgentData")
    assert seen["platform"] == "Android"
    assert seen["mobile"] is True


# --------------------------------------------------------------------------
# The seed bug: a dashboard profile that looked different on every launch
# --------------------------------------------------------------------------
CANVAS_HASH = """() => {
  const c = document.createElement('canvas');
  c.width = 240; c.height = 60;
  const x = c.getContext('2d');
  x.textBaseline = 'top';
  x.font = '14px "Arial"';
  x.fillStyle = '#f60'; x.fillRect(10, 5, 80, 30);
  x.fillStyle = '#069'; x.fillText('numera-fp-probe', 12, 12);
  const url = c.toDataURL();
  let h = 5381;
  for (let i = 0; i < url.length; i++) h = ((h * 33) ^ url.charCodeAt(i)) >>> 0;
  return h;
}"""


def _hash_dashboard_profile(tmp_path, name, dash):
    """Convert a *dashboard* dict to an engine profile, launch it, hash a canvas."""
    from numera.drivers import get
    from numera.server import to_engine_profile
    store = ProfileStore(tmp_path / name)
    prof = to_engine_profile(dash)
    store.save(prof)
    try:
        pw, ctx, page = get("playwright")(prof, store, headless=True, keep_open=True)
    except Exception as e:
        pytest.skip(f"chromium unavailable: {str(e).splitlines()[0][:80]}")
    try:
        page.route("**/*", lambda r: r.fulfill(
            status=200, content_type="text/html", body="<html><body></body></html>"))
        page.goto(PAGE)
        return page.evaluate(CANVAS_HASH)
    finally:
        ctx.close()
        pw.stop()


def test_a_dashboard_profile_looks_identical_on_every_launch(tmp_path):
    """The dashboard has never sent a seed, and the old fallback minted a random
    one per conversion - so every launch of the same profile drew a different
    canvas. Only a browser can show that: the dict round-trips fine either way.
    """
    dash = {"id": "abc123def456", "name": "seedless", "os": "Windows",
            "screen": "1920x1080", "canvas": "Noise"}
    first = _hash_dashboard_profile(tmp_path, "seed-1", dash)
    second = _hash_dashboard_profile(tmp_path, "seed-2", dash)
    assert first == second, (
        "the same dashboard profile produced two different canvas hashes - "
        "its noise seed is not stable across launches")
