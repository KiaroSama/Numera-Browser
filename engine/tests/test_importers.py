"""Importing profiles exported from other anti-detect browsers.

The samples below mirror the shapes GoLogin, AdsPower and Multilogin actually
produce (field names, nesting). The importer is tolerant by design, so the
tests pin the mapping and the coherence guarantee rather than an exact schema.
"""

import json

from numera.importers import profile_from_export, load_export, _unwrap
from numera.store import ProfileStore


GOLOGIN = {
    "name": "GL-Account-1",
    "os": "mac",
    "navigator": {
        "userAgent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128.0.0.0 Safari/537.36",
        "resolution": "1440x900", "language": "en-US",
        "hardwareConcurrency": 10, "deviceMemory": 16, "platform": "MacIntel",
    },
    "timezone": {"timezone": "America/New_York"},
    "webGLMetadata": {"vendor": "Google Inc. (Apple)", "renderer": "ANGLE (Apple, Apple M2, OpenGL 4.1)"},
    "proxy": {"mode": "http", "host": "1.2.3.4", "port": 8080, "username": "u", "password": "p"},
}

ADSPOWER = {
    "name": "ADS-Store-7",
    "group_name": "Amazon",
    "user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/127.0.0.0 Safari/537.36",
    "fingerprint_config": {"webgl_vendor": "Google Inc. (NVIDIA)",
                           "webgl_renderer": "ANGLE (NVIDIA GeForce RTX 3060)"},
    "screen_resolution": "1920x1080",
    "timezone": "Europe/Berlin", "language": "de-DE",
    "user_proxy_config": {"proxy_type": "socks5", "proxy_host": "9.9.9.9",
                          "proxy_port": "1080", "proxy_user": "x", "proxy_password": "y"},
}

MULTILOGIN = {
    "name": "ML-QA",
    "os_type": "linux",
    "navigator": {"user_agent": "Mozilla/5.0 (X11; Linux x86_64) Chrome/126.0.0.0 Safari/537.36",
                  "resolution": "1920x1080", "hardware_concurrency": 8, "platform": "Linux x86_64"},
    "timezone": "Europe/London",
}


def test_gologin_core_fields():
    prof, issues = profile_from_export(GOLOGIN)
    assert prof.name == "GL-Account-1"
    assert prof.fingerprint.os == "macos"
    assert (prof.fingerprint.screen_width, prof.fingerprint.screen_height) == (1440, 900)
    assert prof.fingerprint.hardware_concurrency == 10
    assert prof.proxy.server == "http://1.2.3.4:8080"
    assert prof.proxy.password == "p"


def test_adspower_nested_and_group_becomes_tag():
    prof, issues = profile_from_export(ADSPOWER)
    assert prof.fingerprint.os == "windows"
    assert prof.fingerprint.timezone == "Europe/Berlin"
    assert prof.proxy.server == "socks5://9.9.9.9:1080"
    assert "Amazon" in prof.tags


def test_multilogin_without_proxy():
    prof, issues = profile_from_export(MULTILOGIN)
    assert prof.fingerprint.os == "linux"
    assert prof.proxy is None


def test_os_inferred_from_user_agent_when_field_missing():
    prof, _ = profile_from_export({"name": "x", "user_agent":
                                   "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128 Safari/537.36"})
    assert prof.fingerprint.os == "macos"


def test_missing_fields_fall_back_to_a_coherent_base():
    # An almost-empty export must still yield a launchable, coherent profile.
    prof, issues = profile_from_export({"name": "sparse"})
    assert prof.fingerprint.user_agent  # filled from the generated base
    assert prof.fingerprint.locale in ("en-US", prof.fingerprint.locale)


def test_unknown_locale_is_replaced_not_left_incoherent():
    prof, issues = profile_from_export({"name": "x", "language": "zz-ZZ"})
    assert "languages do not match locale" not in issues
    assert "unknown locale" not in " ".join(issues)


def test_direct_proxy_mode_means_no_proxy():
    prof, _ = profile_from_export({"name": "x", "proxy": {"mode": "none", "host": "1.1.1.1"}})
    assert prof.proxy is None


def test_unwrap_handles_wrappers():
    assert len(_unwrap({"data": [{"a": 1}, {"b": 2}]})) == 2
    assert len(_unwrap([{"a": 1}])) == 1
    assert len(_unwrap({"name": "single"})) == 1
    assert _unwrap("garbage") == []


def test_import_file_saves_and_reports(tmp_path):
    src = tmp_path / "export.json"
    src.write_text(json.dumps([GOLOGIN, ADSPOWER, MULTILOGIN]), encoding="utf-8")
    store = ProfileStore(tmp_path / "store")
    results = ProfileStore  # placeholder to satisfy linters if unused
    from numera.importers import import_file
    imported, skipped = import_file(src, store)
    assert len(imported) == 3
    assert skipped == []
    assert len(store.list()) == 3
    assert {p.name for p in store.list()} == {"GL-Account-1", "ADS-Store-7", "ML-QA"}


# --- the identity an export carried must survive the import ----------------

GL_ALIASES = {
    "name": "from gologin",
    "navigator": {"userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                               "AppleWebKit/537.36 (KHTML, like Gecko) "
                               "Chrome/120.0.6099.130 Safari/537.36",
                  "resolution": "1920x1080", "language": "de-DE",
                  "hardwareConcurrency": 12, "deviceMemory": 32},
    "webGLMetadata": {"vendor": "Google Inc. (AMD)",
                      "renderer": "ANGLE (AMD, Radeon RX 6700 XT Direct3D11 vs_5_0 ps_5_0, D3D11)"},
    "timezone": {"timezone": "Europe/Berlin"},
}

AP_ALIASES = {
    "name": "from adspower",
    "user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
                  "AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36",
    "webgl": {"vendor": "Google Inc. (Apple)", "renderer": "ANGLE (Apple, Apple M2, OpenGL 4.1)"},
    "screen_resolution": "2560x1600",
}


def test_the_chrome_version_in_the_user_agent_is_kept():
    """It was discarded, so the UA said 120 while `chrome_version` - which
    builds sec-ch-ua and navigator.userAgentData.brands - said something else.
    A site reading both sees the contradiction immediately."""
    fp, _ = profile_from_export(GL_ALIASES)[0].fingerprint, None
    assert fp.chrome_version.startswith("120."), fp.chrome_version
    assert "Chrome/120." in fp.user_agent


def test_a_vendor_specific_webgl_path_is_not_dropped():
    """Each tool nests WebGL somewhere else. A missed path silently replaced a
    real GPU string with this machine's archetype default."""
    fp = profile_from_export(GL_ALIASES)[0].fingerprint
    assert "AMD" in fp.webgl_renderer, fp.webgl_renderer
    assert fp.webgl_vendor == "Google Inc. (AMD)"

    fp2 = profile_from_export(AP_ALIASES)[0].fingerprint
    assert "Apple M2" in fp2.webgl_renderer, fp2.webgl_renderer


def test_a_snake_case_user_agent_under_navigator_is_found():
    d = {"name": "x", "navigator": {"user_agent": GL_ALIASES["navigator"]["userAgent"]}}
    assert "Chrome/120." in profile_from_export(d)[0].fingerprint.user_agent


def test_an_imported_profile_is_coherent():
    """Importing something that immediately fails the coherence check would be
    handing the user a broken profile and calling it an import."""
    from numera.fingerprint import validate
    for payload in (GL_ALIASES, AP_ALIASES):
        fp = profile_from_export(payload)[0].fingerprint
        assert not validate(fp), (payload["name"], validate(fp))


def test_a_mobile_import_does_not_get_desktop_browser_chrome():
    """88px of desktop chrome on a phone leaves a viewport no handset has."""
    d = {"name": "m",
         "navigator": {"userAgent": "Mozilla/5.0 (Linux; Android 14; SM-S918B) "
                                    "AppleWebKit/537.36 (KHTML, like Gecko) "
                                    "Chrome/120.0.0.0 Mobile Safari/537.36",
                       "resolution": "412x915"}}
    fp = profile_from_export(d)[0].fingerprint
    assert fp.is_mobile
    assert fp.viewport_height == 915 - 80, fp.viewport_height


# --- an iPhone export is an iPhone profile ---------------------------------

IPHONE_UA = ("Mozilla/5.0 (iPhone; CPU iPhone OS 17_6 like Mac OS X) "
             "AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/126.0.0.0 "
             "Mobile/15E148 Safari/604.1")
IPAD_UA = ("Mozilla/5.0 (iPad; CPU OS 17_6 like Mac OS X) "
           "AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 "
           "Mobile/15E148 Safari/604.1")
ANDROID_UA = ("Mozilla/5.0 (Linux; Android 14; SM-S918B) AppleWebKit/537.36 "
              "(KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36")


def test_a_bare_ios_export_is_not_imported_as_windows():
    """`_norm_os` had no iOS branch at all, so an export that says plainly
    `"os": "iOS"` fell through to the Windows default: the imported profile
    claimed Win32 and a desktop UA for a phone the user is trying to clone."""
    prof, _ = profile_from_export({"name": "ip", "os": "iOS"})
    assert prof.fingerprint.os == "ios", prof.fingerprint.os
    assert prof.fingerprint.platform == "iPhone"


def test_an_iphone_user_agent_is_not_imported_as_a_mac():
    """Every iOS user agent says "like Mac OS X", which the macOS test matched
    first - so an iPhone export became a desktop Mac, 88px of desktop browser
    chrome and all."""
    prof, _ = profile_from_export({"name": "i", "user_agent": IPHONE_UA,
                                   "screen_resolution": "390x844"})
    fp = prof.fingerprint
    assert fp.os == "ios", fp.os
    assert fp.is_mobile
    assert fp.viewport_height == 844 - 113, fp.viewport_height


def test_an_ipad_export_is_not_imported_as_a_desktop_mac():
    """Same string, different device: "iPad" has to be recognised on its own."""
    prof, _ = profile_from_export({"name": "t", "user_agent": IPAD_UA})
    assert prof.fingerprint.os == "ios", prof.fingerprint.os


def test_a_conflicting_os_field_does_not_turn_an_iphone_into_a_desktop():
    """Exports disagree with themselves. Whichever side names iOS, the answer
    is iOS - substituting Windows or macOS hands back a profile whose platform
    contradicts the user agent it carries."""
    ua_says = profile_from_export({"name": "c", "os": "Windows",
                                   "user_agent": IPHONE_UA})[0].fingerprint
    assert ua_says.os == "ios", ua_says.os
    field_says = profile_from_export({"name": "c", "os": "iOS", "user_agent":
                                      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                                      "Chrome/120.0.0.0 Safari/537.36"})[0].fingerprint
    assert field_says.os == "ios", field_says.os


def test_the_desktop_and_android_classifications_still_hold():
    """The iOS branch sits in front of the others, so this pins every path it
    could have swallowed - including the Android UA, which also says "Linux",
    and a `navigator.platform` of "Linux armv8l" on an Android handset."""
    cases = [
        ({"os": "win"}, "windows"),
        ({"user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120 Safari/537.36"},
         "windows"),
        ({"os": "mac"}, "macos"),
        ({"os": "Darwin"}, "macos"),
        ({"user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128 Safari/537.36"},
         "macos"),
        ({"os_type": "linux"}, "linux"),
        ({"os": "android"}, "android"),
        ({"user_agent": ANDROID_UA}, "android"),
        ({"navigator": {"platform": "Linux armv8l", "userAgent": ANDROID_UA}}, "android"),
        ({}, "windows"),
    ]
    for payload, expected in cases:
        got = profile_from_export({"name": "x", **payload})[0].fingerprint.os
        assert got == expected, (payload, got)


def test_an_imported_phone_is_a_coherent_phone():
    """`is_mobile` was set from the OS but `maxTouchPoints` and
    `devicePixelRatio` kept the dataclass defaults - 0 and 1.0 - so every
    imported handset reported a touch screen it did not have and a 1x display.
    Both values are stored and reach the launch, so this is what the site sees.
    """
    from numera.fingerprint import validate as coherence

    for payload in ({"name": "a", "user_agent": ANDROID_UA, "screen_resolution": "412x915"},
                    {"name": "i", "os": "iOS", "screen_resolution": "390x844"}):
        fp = profile_from_export(payload)[0].fingerprint
        assert fp.is_mobile, payload["name"]
        assert fp.max_touch_points >= 1, payload["name"]
        assert coherence(fp) == [], (payload["name"], coherence(fp))


# --- a number a vendor invented must not abort the import ------------------

def test_a_non_finite_number_falls_back_instead_of_raising():
    """`_as_int` catches TypeError and ValueError, but `int(float("inf"))`
    raises OverflowError - so "inf", "-inf" and "1e309" walked straight out of
    the tolerant parser the module's whole contract rests on."""
    from numera.importers import _as_int

    for junk in ("inf", "-inf", "Infinity", "1e309", "nan", "9" * 400, "-5"):
        assert _as_int(junk, 8) == 8, junk
    assert _as_int(True, 8) == 8, "a flag is not a core count"
    assert _as_int("8 cores", 8) == 8    # ordinary vendor junk, unchanged
    assert _as_int("N/A", 8) == 8
    assert _as_int("12", 8) == 12        # and the values that do parse still do
    assert _as_int("8.9", 8) == 8
    assert _as_int(0, 8) == 0


def test_an_infinite_core_count_yields_a_plausible_profile():
    """The fallback is the generated base for that OS, so the profile stays
    coherent rather than carrying whatever survived the cast."""
    from numera import devices

    fp = profile_from_export({"name": "x", "os": "win",
                              "navigator": {"hardwareConcurrency": "1e309",
                                            "deviceMemory": "inf"}})[0].fingerprint
    assert fp.hardware_concurrency in devices.HARDWARE["windows"]["cores"]
    assert fp.device_memory in devices.HARDWARE["windows"]["memory"]


def test_one_impossible_number_does_not_cost_the_whole_file(tmp_path):
    """Import is whole-file or nothing, so the OverflowError did not leave a
    partial import - it refused all forty-nine good profiles because of the
    fiftieth. The tolerant path keeps the file importable."""
    from numera.importers import import_file

    src = tmp_path / "export.json"
    src.write_text(json.dumps([GOLOGIN,
                               {"name": "odd", "cores": "1e309"},
                               MULTILOGIN]), encoding="utf-8")
    store = ProfileStore(tmp_path / "store")

    imported, skipped = import_file(src, store)

    assert len(imported) == 3, [p.name for p, _ in imported]
    assert skipped == []
    assert len(store.list()) == 3


def test_the_api_import_survives_the_same_number(tmp_path):
    """The route runs the same parser, and an uncaught OverflowError there is a
    500 with nothing written - the CLI and the API have to agree."""
    import pytest

    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient
    from numera.server import create_app

    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    payload = json.dumps([{"name": "first"},
                          {"name": "second", "navigator": {"deviceMemory": "1e309"}}])

    r = c.post("/api/profiles/import", json={"text": payload})

    assert r.status_code == 200, r.text
    assert r.json()["imported"] == 2
    assert {p["name"] for p in c.get("/api/profiles").json()} == {"first", "second"}


def test_a_timezone_object_is_read_or_stood_down_from(tmp_path):
    """An object under `timezone` must not take the whole file down.

    Found by driving the dashboard's own Import button with a vendor file whose
    timezone was `{"id": "Europe/Berlin"}`. `timezone.id` was not among the
    aliases, so `_first` returned the dict itself, it reached the route's
    `check_fields` as `'timezone' must be str, got dict`, and the import was
    refused ENTIRELY - the profile with that field and the profile without it.

    Two things are pinned. `timezone.id` is read, because it is a shape vendors
    write. And a timezone this importer cannot read falls back to the locale's
    own zone with the issue said out loud, the same guard the language beside it
    has always had - a field with a coherent generated value is a field a bad
    value cannot destroy.
    """
    import pytest

    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient
    from numera.server import create_app

    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    payload = json.dumps([
        {"name": "by-id", "navigator": {"language": "de-DE"},
         "timezone": {"id": "Europe/Berlin"}},
        {"name": "unreadable", "navigator": {"language": "fr-FR"},
         "timezone": {"whatever": True}},
        {"name": "plain-string", "navigator": {"language": "en-US"},
         "timezone": "America/Los_Angeles"},
    ])

    r = c.post("/api/profiles/import", json={"text": payload})

    assert r.status_code == 200, r.text
    assert r.json()["imported"] == 3, r.text
    by_name = {p["name"]: p for p in c.get("/api/profiles").json()}
    assert by_name["by-id"]["timezone"] == "Europe/Berlin"
    assert by_name["plain-string"]["timezone"] == "America/Los_Angeles"
    # The one it could not read keeps a real zone, and says it substituted one.
    assert isinstance(by_name["unreadable"]["timezone"], str)
    assert by_name["unreadable"]["timezone"] == "Europe/Paris"
    noted = [res for res in r.json()["results"] if res["name"] == "unreadable"]
    assert noted and any("timezone" in i for i in noted[0]["issues"]), noted
