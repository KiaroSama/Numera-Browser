"""Rewrite the profile to match what the browser really presents.

The only probe that *changes* anything. The trust checker fails whenever the
declared identity and the presented one disagree, and with an engine that
renders the host's real hardware that gap cannot be closed from the profile
side - so this closes it from the other end.

`trust` is imported as a module, not by name: `_observe` is the seam the tests
replace, and a by-value import would leave them patching a copy nobody calls.
"""

from __future__ import annotations

from typing import Optional

from ..models import Profile
from ..store import ProfileStore
from . import trust as _trust
from .trust import _grade

# --------------------------------------------------------------------------
# Auto-adjust: align the profile to what the browser actually presents
# --------------------------------------------------------------------------
import re as _re

from .. import devices as _devices
from ..fingerprint import build_user_agent as _build_ua


def _os_from_platform(platform: str, fallback: str) -> str:
    p = (platform or "").lower()
    if "win" in p:
        return "windows"
    if "mac" in p:
        return "macos"
    if "arm" in p or "android" in p or "aarch" in p:
        return "android"
    if "linux" in p or "x11" in p:
        return "linux"
    return fallback


def _chrome_from_ua(ua: str) -> Optional[str]:
    m = _re.search(r"Chrome/(\d+\.\d+\.\d+\.\d+)", ua or "")
    return m.group(1) if m else None


def _vendor_from_renderer(renderer: str) -> str:
    r = (renderer or "").lower()
    for token, name in (("nvidia", "NVIDIA"), ("amd", "AMD"), ("radeon", "AMD"),
                        ("intel", "Intel"), ("apple", "Apple"),
                        ("adreno", "Qualcomm"), ("qualcomm", "Qualcomm"),
                        ("mali", "ARM"), ("arm", "ARM")):
        if token in r:
            return f"Google Inc. ({name})"
    return "Google Inc."


def align_to_reality(profile: Profile, store: ProfileStore,
                     engine: Optional[str] = None) -> dict:
    """Make the profile's declared fingerprint match what the browser really
    shows, then re-grade it.

    The trust checker fails whenever the *declared* identity and the *presented*
    one disagree. That gap is unavoidable with an engine that renders the host's
    real hardware (e.g. CloakBrowser shows this machine's GPU/CPU because it
    patches at the C++ level, not from Numera's JS). Rather than paper over it,
    this reads back platform, CPU, memory, GPU, screen, languages, fonts and
    media, and rewrites the fingerprint to those values — regenerating the OS
    identity (UA, platform) when the machine turns out to be a different OS than
    the profile claimed. Afterwards the browser and the profile tell one story,
    so a detector sees no contradiction.

    Returns ``{before, after, changed, checks, os, fingerprint}``.
    """
    all_fonts = sorted({f for pool in _devices.FONTS.values() for f in pool})
    seen = _trust._observe(profile, store, engine, font_candidates=all_fonts)
    fp = profile.fingerprint
    engine_name = engine or profile.engine or "playwright"
    before = _grade(fp, seen, engine_name)

    changed: list[str] = []

    def apply(attr: str, value, label: Optional[str] = None):
        if value is None:
            return
        if getattr(fp, attr) != value:
            setattr(fp, attr, value)
            changed.append(label or attr)

    # 1. Operating system — the root that everything else hangs off.
    #
    # `os_changed` is captured BEFORE the assignment. It used to be re-tested
    # as `os_key != fp.os` after `fp.os = os_key`, which can never be true, so
    # a profile crossing OS families kept the old version and handset: a
    # "windows" fingerprint still carrying macOS 15.5 as its os_version, and
    # with it the wrong UA-CH `platformVersion`, which `stealth` reports as a
    # high-entropy hint. The trust grade never caught it because it does not
    # read that hint.
    os_key = _os_from_platform(seen.get("platform"), fp.os)
    os_changed = os_key != fp.os
    chrome = _chrome_from_ua(seen.get("userAgent")) or fp.chrome_version
    if os_changed:
        fp.os = os_key
        fp.is_mobile = (os_key == "android")
        changed.append("os")
    fp.chrome_version = chrome
    apply("platform", _devices.NAV_PLATFORM.get(os_key, seen.get("platform")), "platform")
    fp.ch_platform = _devices.CH_PLATFORM.get(os_key, fp.ch_platform)
    if os_changed or not fp.os_version:
        # A changed OS invalidates the version and the handset the old one
        # carried. The version is the head of the target OS's own table, which
        # is also what a freshly generated fingerprint would pick.
        fp.os_version = _devices.OS_VERSIONS[os_key][0][0]
        if os_key != "android":
            fp.device_model = ""
        elif not fp.device_model:
            fp.device_model = _devices.ANDROID_MODELS[0]
    apply("user_agent",
          _build_ua(os_key, chrome, fp.os_version, fp.device_model), "userAgent")

    # 2. Hardware, straight from the machine.
    apply("hardware_concurrency", seen.get("cores"), "cores")
    apply("device_memory", seen.get("memory"), "memory")
    # Touch points and DPR are OS-shaped too - 0 on a desktop, 5 on a phone;
    # 2 on a Retina Mac against Windows' 1/1.25/1.5 - so a value carried across
    # an OS transition is a contradiction. Observed values win; an OS change
    # with nothing observed falls back to the target OS's own default rather
    # than keeping the old one.
    apply("max_touch_points", seen.get("maxTouchPoints"), "maxTouchPoints")
    apply("device_pixel_ratio", seen.get("devicePixelRatio"), "devicePixelRatio")
    if os_changed:
        if seen.get("maxTouchPoints") is None:
            apply("max_touch_points", _devices.TOUCH_POINTS[os_key][0], "maxTouchPoints")
        if seen.get("devicePixelRatio") is None:
            apply("device_pixel_ratio", _devices.PIXEL_RATIOS[os_key][0], "devicePixelRatio")
    if seen.get("webglRenderer"):
        apply("webgl_renderer", seen["webglRenderer"], "webglRenderer")
        fp.webgl_vendor = seen.get("webglVendor") or _vendor_from_renderer(seen["webglRenderer"])
    if seen.get("screenW") and seen.get("screenH"):
        if (fp.screen_width, fp.screen_height) != (seen["screenW"], seen["screenH"]):
            fp.screen_width, fp.screen_height = seen["screenW"], seen["screenH"]
            fp.viewport_width = min(fp.viewport_width or fp.screen_width, fp.screen_width)
            fp.viewport_height = min(fp.viewport_height or fp.screen_height, fp.screen_height)
            changed.append("screen")

    # 3. Media + languages, as reported.
    apply("cameras", seen.get("cameras"), "cameras")
    apply("microphones", seen.get("microphones"), "microphones")
    if seen.get("languages") and list(fp.languages) != list(seen["languages"]):
        fp.languages = list(seen["languages"])
        changed.append("languages")

    # 3b. Timezone + locale: match the machine's real clock. An engine that shows
    #     the host timezone (e.g. CloakBrowser) otherwise fails the "timezone
    #     matches" check forever, capping auto-adjust at B. We only move it when a
    #     locale whose canonical zone is that timezone exists, so the profile stays
    #     coherent (locale still implies its timezone).
    tz = seen.get("timezone")
    if tz:
        loc = next((l for l, (z, _lg) in _devices.LOCALES.items() if z == tz), None)
        if loc:
            if fp.timezone != tz:
                fp.timezone = tz
                changed.append("timezone")
            if fp.locale != loc:
                fp.locale = loc
                changed.append("locale")

    # 4. Fonts: keep this OS's fonts that actually render here (a real subset),
    #    falling back to the full OS set if the probe came back empty.
    rendered = set(seen.get("fontsRendered") or [])
    os_fonts = sorted(f for f in _devices.FONTS.get(os_key, []) if f in rendered)
    if not os_fonts:
        os_fonts = sorted(_devices.FONTS.get(os_key, []))
    if sorted(fp.fonts) != os_fonts:
        fp.fonts = os_fonts
        changed.append("fonts")

    store.save(profile)
    # The user-agent is fed to the engine at launch, so it will report the new
    # value next time — grade against that, not the stale observed UA.
    after = _grade(fp, {**seen, "userAgent": fp.user_agent}, engine_name)

    keep = ("score", "grade", "passed", "total")
    return {
        "before": {k: before[k] for k in keep},
        "after": {k: after[k] for k in keep},
        "changed": sorted(set(changed)),
        "checks": after["checks"],
        "os": os_key,
        "fingerprint": {
            "os": fp.os,
            "userAgent": fp.user_agent,
            "browserVersion": fp.chrome_version,
            "platform": fp.platform,
            "cores": fp.hardware_concurrency,
            "memory": fp.device_memory,
            "screen": f"{fp.screen_width}x{fp.screen_height}",
            "webglVendor": fp.webgl_vendor,
            "webglRenderer": fp.webgl_renderer,
            "cameras": fp.cameras,
            "microphones": fp.microphones,
            "languages": list(fp.languages),
            "locale": fp.locale,
            "timezone": fp.timezone,
        },
    }
