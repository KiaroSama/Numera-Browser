"""`ua_metadata` and `client_hint_headers` are two views of one identity.

Chromium derives the Sec-CH-UA headers from the metadata block, and the launcher
pins the low-entropy trio as headers. If the two ever disagree, the wire
contradicts itself: a site reads one architecture from the pinned header and
another from the hint Chromium built. Cheap to keep honest, so keep it honest.
"""
import pytest

from numera import generate
from numera.fingerprint import client_hint_headers, platform_version, ua_metadata

EVERY_OS = ["windows", "macos", "linux", "android", "ios"]


@pytest.mark.parametrize("os_name", EVERY_OS)
def test_the_header_view_is_the_metadata_view(os_name):
    fp = generate(os=os_name)
    m, h = ua_metadata(fp), client_hint_headers(fp)
    assert h["low"]["sec-ch-ua-platform"] == f'"{m["platform"]}"'
    assert h["low"]["sec-ch-ua-mobile"] == ("?1" if m["mobile"] else "?0")
    assert h["high"]["sec-ch-ua-arch"] == f'"{m["architecture"]}"'
    assert h["high"]["sec-ch-ua-model"] == f'"{m["model"]}"'
    assert h["high"]["sec-ch-ua-platform-version"] == f'"{m["platformVersion"]}"'
    assert h["high"]["sec-ch-ua-bitness"] == f'"{m["bitness"]}"'
    assert h["high"]["sec-ch-ua-wow64"] == ("?1" if m["wow64"] else "?0")


@pytest.mark.parametrize("os_name", EVERY_OS)
def test_the_metadata_is_shaped_the_way_cdp_wants_it(os_name):
    """`Emulation.setUserAgentOverride` rejects the whole call on a bad type, and
    a rejected call means the browser quietly keeps the host's values."""
    m = ua_metadata(generate(os=os_name))
    assert set(m) == {"brands", "fullVersionList", "platform", "platformVersion",
                      "architecture", "model", "mobile", "bitness", "wow64"}
    assert isinstance(m["mobile"], bool) and isinstance(m["wow64"], bool)
    for key in ("platform", "platformVersion", "architecture", "model", "bitness"):
        assert isinstance(m[key], str), key
    for entry in m["brands"] + m["fullVersionList"]:
        assert set(entry) == {"brand", "version"}
        assert isinstance(entry["brand"], str) and isinstance(entry["version"], str)
    # Unquoted: the quoting belongs to the header view, and a quoted value here
    # reaches the page as literal quotes inside navigator.userAgentData.
    assert '"' not in m["platform"] + m["platformVersion"] + m["architecture"]


def test_the_full_version_list_carries_four_part_versions():
    fp = generate(os="windows")
    m = ua_metadata(fp)
    for entry in m["fullVersionList"]:
        assert entry["version"].count(".") == 3, entry
    # brands is the short form; the two are not interchangeable.
    assert all("." not in e["version"] for e in m["brands"])


def test_apple_silicon_is_arm_and_an_intel_mac_is_not():
    fp = generate(os="macos")
    fp.webgl_renderer = "ANGLE (Apple, Apple M3 Pro, OpenGL 4.1)"
    assert ua_metadata(fp)["architecture"] == "arm"
    fp.webgl_renderer = "ANGLE (Intel, Intel(R) Iris(TM) Plus Graphics, OpenGL 4.1)"
    assert ua_metadata(fp)["architecture"] == "x86"


def test_the_platform_version_matches_the_shared_encoding():
    """Chrome reports the OS release, not its marketing name - Windows 11 is
    "15.0.0". One encoder, so the header, the page and the profile agree."""
    fp = generate(os="windows")
    fp.os_version = "11"
    assert ua_metadata(fp)["platformVersion"] == platform_version("windows", "11")


def test_a_phone_reports_its_handset_and_a_desktop_reports_none():
    assert ua_metadata(generate(os="android"))["model"]
    assert ua_metadata(generate(os="windows"))["model"] == ""
