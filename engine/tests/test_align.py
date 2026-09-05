"""Auto-adjust: aligning a profile to what its browser actually presents.

`align_to_reality` needs a real browser to *observe*, but the reconciliation
logic — turn an observation into a rewritten, coherent fingerprint that now
grades clean — is pure. We stub the observation and pin that logic, which is the
part that would silently do the wrong thing if it broke.
"""

from numera import generate, validate, devices
from numera.models import Profile
from numera.store import ProfileStore
from numera.probe import (
    align_to_reality, _os_from_platform, _chrome_from_ua, _vendor_from_renderer,
)
import numera.probe as probe
# Patched on probe.trust, which is where align_to_reality looks it
# up. Setting it on the package would replace a re-export nobody calls, and
# the test would quietly open a real browser instead of failing.


def test_os_inferred_from_platform():
    assert _os_from_platform("Win32", "macos") == "windows"
    assert _os_from_platform("MacIntel", "windows") == "macos"
    assert _os_from_platform("Linux x86_64", "windows") == "linux"
    assert _os_from_platform("Linux armv8l", "windows") == "android"
    assert _os_from_platform("", "linux") == "linux"   # falls back


def test_chrome_pulled_from_ua():
    assert _chrome_from_ua("... Chrome/127.0.0.0 Safari/537.36") == "127.0.0.0"
    assert _chrome_from_ua("no version here") is None


def test_vendor_guessed_from_renderer():
    assert "NVIDIA" in _vendor_from_renderer("ANGLE (NVIDIA, RTX 3050 Ti, D3D11)")
    assert "Apple" in _vendor_from_renderer("ANGLE (Apple, Apple M1, OpenGL 4.1)")


# A macOS profile running under an engine that shows the real *Windows* host —
# exactly the cloak-on-Windows situation that scores grade D.
def _windows_host_seen(profile):
    fp = profile.fingerprint
    return {
        "webdriver": False, "webdriverOwn": False, "hasChrome": True, "plugins": 3,
        "userAgent": fp.user_agent,               # the (mac) UA the engine was fed
        "platform": "Win32",
        "languages": ["en-US"],
        # 16 threads is a real Windows figure and appears in no macOS pool, so
        # the "cores changed" assertion below tests the reconciliation itself
        # rather than whichever core count this seed happened to draw.
        "cores": 16, "memory": 8,
        "screenW": 1920, "screenH": 1080,
        "timezone": fp.timezone,
        "toStringNative": True, "permissionsAgree": True,
        "canvasStable": True, "canvasEmpty": False,
        "webglVendor": "Google Inc. (NVIDIA)",
        "webglRenderer": "ANGLE (NVIDIA, NVIDIA GeForce RTX 3050 Ti Direct3D11 vs_5_0 ps_5_0, D3D11)",
        "fontKnown": False, "fontAlien": False,
        "cameras": 1, "microphones": 1,
        "fontsRendered": ["Segoe UI", "Calibri", "Consolas", "Arial", "Tahoma"],
    }


def test_align_reconciles_mac_profile_to_windows_host(tmp_path, monkeypatch):
    store = ProfileStore(tmp_path)
    prof = Profile(name="mac-on-win", fingerprint=generate(seed=1, os="macos", locale="en-US"),
                   engine="cloak")
    store.save(prof)

    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: _windows_host_seen(prof))
    res = align_to_reality(prof, store, engine="cloak")

    # The identity now reads as the machine it actually runs on.
    assert res["os"] == "windows"
    assert prof.fingerprint.os == "windows"
    assert prof.fingerprint.platform == "Win32"
    assert prof.fingerprint.hardware_concurrency == 16
    assert prof.fingerprint.device_memory == 8
    assert "RTX 3050 Ti" in prof.fingerprint.webgl_renderer
    assert prof.fingerprint.user_agent.startswith("Mozilla/5.0 (Windows NT")

    # And it grades clean — the whole point.
    assert res["before"]["score"] < res["after"]["score"]
    assert res["after"]["score"] == 100
    assert set(res["changed"]) >= {"os", "platform", "userAgent", "cores", "memory",
                                   "webglRenderer", "fonts"}


def test_aligned_profile_is_coherent(tmp_path, monkeypatch):
    store = ProfileStore(tmp_path)
    prof = Profile(name="p", fingerprint=generate(seed=2, os="macos", locale="en-US"))
    store.save(prof)
    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: _windows_host_seen(prof))
    align_to_reality(prof, store, engine="cloak")
    # A real host GPU/screen must not trip the coherence validator.
    assert validate(prof.fingerprint) == []


def test_align_is_a_noop_when_already_matching(tmp_path, monkeypatch):
    store = ProfileStore(tmp_path)
    prof = Profile(name="p", fingerprint=generate(seed=3, os="windows", locale="en-US"),
                   engine="playwright")
    store.save(prof)
    fp = prof.fingerprint

    # A profile whose browser presents exactly what it declares.
    seen = {
        "webdriver": False, "webdriverOwn": False, "hasChrome": True, "plugins": 3,
        "userAgent": fp.user_agent, "platform": fp.platform,
        "languages": list(fp.languages), "cores": fp.hardware_concurrency,
        "memory": fp.device_memory, "screenW": fp.screen_width, "screenH": fp.screen_height,
        "timezone": fp.timezone, "toStringNative": True, "permissionsAgree": True,
        "canvasStable": True, "canvasEmpty": False,
        "webglVendor": fp.webgl_vendor, "webglRenderer": fp.webgl_renderer,
        "fontKnown": True, "fontAlien": False,
        "cameras": fp.cameras, "microphones": fp.microphones,
        "fontsRendered": list(fp.fonts),
    }
    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: seen)
    res = align_to_reality(prof, store, engine="playwright")
    assert res["changed"] == []
    assert res["after"]["score"] == 100


def _fully_consistent_windows_seen(tz):
    """An observation where every check would pass — so the only thing keeping
    the grade below A is whether align picks up the timezone."""
    return {
        "webdriver": False, "webdriverOwn": False, "hasChrome": True, "plugins": 3,
        "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
                     "(KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36",
        "platform": devices.NAV_PLATFORM["windows"], "languages": ["en-US", "en"],
        "cores": 8, "memory": 8, "timezone": tz,
        "toStringNative": True, "permissionsAgree": True,
        "canvasStable": True, "canvasEmpty": False,
        "webglVendor": "Google Inc. (NVIDIA)",
        "webglRenderer": "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)",
        "screenW": 1920, "screenH": 1080,
        "fontsRendered": [devices.FONTS["windows"][0]],
        "cameras": 1, "microphones": 1,
    }


def test_align_moves_timezone_and_locale_to_reality(tmp_path, monkeypatch):
    # A US profile (America/New_York) on a machine whose real clock is Karachi —
    # the case where auto-adjust used to stop at B because the timezone check
    # stayed failed. Now align should follow the clock and reach A.
    store = ProfileStore(tmp_path)
    prof = Profile(name="flow", engine="cloak",
                   fingerprint=generate(seed=1, os="windows", locale="en-US"))
    store.save(prof)
    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: _fully_consistent_windows_seen("Asia/Karachi"))

    res = align_to_reality(prof, store, engine="cloak")

    assert prof.fingerprint.timezone == "Asia/Karachi"
    assert prof.fingerprint.locale == "en-PK"       # the coherent locale for that zone
    assert "timezone" in res["changed"]
    assert res["after"]["grade"] == "A" and res["after"]["score"] == 100
    assert validate(prof.fingerprint) == []          # still coherent


def test_align_keeps_timezone_when_no_locale_models_it(tmp_path, monkeypatch):
    # A zone Numera doesn't model: leave it alone rather than break coherence.
    store = ProfileStore(tmp_path)
    prof = Profile(name="flow", engine="cloak",
                   fingerprint=generate(seed=1, os="windows", locale="en-US"))
    store.save(prof)
    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: _fully_consistent_windows_seen("Antarctica/Troll"))

    res = align_to_reality(prof, store, engine="cloak")

    assert prof.fingerprint.timezone == devices.LOCALES["en-US"][0]   # unchanged
    assert "timezone" not in res["changed"]


# --- crossing OS families ---------------------------------------------------
#
# The reset of OS-derived values was gated on `os_key != fp.os` *after*
# `fp.os = os_key`, which can never be true. A macOS profile aligned onto a
# Windows host therefore kept macOS 15.5 as its `os_version` - and with it the
# UA-CH `platformVersion` hint, which is the only thing that distinguishes
# Windows 10 from 11 and is read by sites that check. The trust grade never saw
# it, so these assertions go at the stored fingerprint and at the hints
# `stealth` actually emits.

def test_crossing_os_families_resets_the_values_the_old_os_owned(tmp_path, monkeypatch):
    from numera.fingerprint import platform_version

    store = ProfileStore(tmp_path)
    prof = Profile(name="mac-to-win", fingerprint=generate(seed=3, os="macos", locale="en-US"),
                   engine="cloak")
    mac_version = prof.fingerprint.os_version
    assert mac_version and mac_version.startswith(("13", "14", "15"))
    store.save(prof)

    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: _windows_host_seen(prof))
    align_to_reality(prof, store, engine="cloak")

    fp = ProfileStore(tmp_path).resolve("mac-to-win").fingerprint
    assert fp.os == "windows"
    assert fp.os_version != mac_version, "the macOS release survived the move to Windows"
    assert fp.os_version in [v for v, _hint in devices.OS_VERSIONS["windows"]]
    assert fp.device_model == ""
    assert "Windows NT" in fp.user_agent and "Mac OS X" not in fp.user_agent


def test_the_client_hints_agree_with_the_new_os(tmp_path, monkeypatch):
    """`platformVersion` is generated from `os_version`, so a stale version is
    a hint that contradicts the UA string on the one axis the UA cannot carry."""
    from numera import stealth
    from numera.fingerprint import platform_version

    store = ProfileStore(tmp_path)
    prof = Profile(name="hints", fingerprint=generate(seed=4, os="macos", locale="en-US"),
                   engine="cloak")
    store.save(prof)
    stale_hint = platform_version("macos", prof.fingerprint.os_version)

    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: _windows_host_seen(prof))
    align_to_reality(prof, store, engine="cloak")

    fp = prof.fingerprint
    hint = platform_version(fp.os, fp.os_version)
    assert hint != stale_hint
    assert hint in [h for _v, h in devices.OS_VERSIONS["windows"]]

    script = stealth.build_init_script(fp)
    assert f'"platformVersion": "{hint}"' in script or f"'platformVersion': '{hint}'" in script, \
        "the injected client hints still carry the old OS version"
    # `uaPlatform` is the client-hint one; `platform` is navigator.platform.
    assert '"uaPlatform": "Windows"' in script


def test_touch_and_dpr_follow_the_observed_machine(tmp_path, monkeypatch):
    """Neither is graded, so nothing else would notice them going stale."""
    store = ProfileStore(tmp_path)
    prof = Profile(name="touch", fingerprint=generate(seed=5, os="android", locale="en-US"),
                   engine="cloak")
    store.save(prof)
    assert prof.fingerprint.max_touch_points == 5          # a phone

    seen = {**_windows_host_seen(prof), "maxTouchPoints": 0, "devicePixelRatio": 1.25}
    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: seen)
    align_to_reality(prof, store, engine="cloak")

    fp = prof.fingerprint
    assert fp.os == "windows"
    assert fp.max_touch_points == 0, "a phone's touch points survived onto a desktop"
    assert fp.device_pixel_ratio == 1.25
    assert fp.is_mobile is False


def test_an_unobserved_form_factor_falls_back_to_the_new_os(tmp_path, monkeypatch):
    """An older engine that reports neither must still not keep the phone's."""
    store = ProfileStore(tmp_path)
    prof = Profile(name="fallback", fingerprint=generate(seed=6, os="android", locale="en-US"),
                   engine="cloak")
    store.save(prof)

    monkeypatch.setattr(probe.trust, "_observe", lambda *a, **k: _windows_host_seen(prof))
    align_to_reality(prof, store, engine="cloak")

    fp = prof.fingerprint
    assert fp.max_touch_points in devices.TOUCH_POINTS["windows"]
    assert fp.device_pixel_ratio in devices.PIXEL_RATIOS["windows"]
