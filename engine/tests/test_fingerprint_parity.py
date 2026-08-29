"""Unit cover for the phase-5 parity work: the five OS targets, the realistic
distributions, and the dashboard <-> engine round-trip.

What a page actually *observes* is proved in ``test_parity_live.py`` against a
running browser — a dict round-trip here cannot show that, and this project has
been caught by exactly that gap five times. These tests pin the things a browser
cannot: that the weight tables have not drifted out of alignment with the pools
they weight, that the generated spread is not uniform, and that no field is
dropped in conversion.
"""

import collections

import pytest

from numera import devices, generate, validate
from numera.fingerprint import sec_ch_ua, ua_brands
from numera.server import to_dashboard_profile, to_engine_profile


# --------------------------------------------------------------------------
# Five OS targets
# --------------------------------------------------------------------------
@pytest.mark.parametrize("os_key", devices.ALL_OS)
def test_every_os_target_generates_coherently(os_key):
    for seed in range(60):
        fp = generate(seed=seed, os=os_key)
        assert fp.os == os_key
        assert validate(fp) == [], f"{os_key} seed {seed}: {validate(fp)}"


@pytest.mark.parametrize("os_key", devices.ALL_OS)
def test_every_os_target_has_a_full_set_of_device_tables(os_key):
    """A missing entry would surface as a KeyError at launch, not at generation."""
    for table in (devices.WEBGL, devices.SCREENS, devices.HARDWARE,
                  devices.OS_UA_TOKEN, devices.NAV_PLATFORM, devices.CH_PLATFORM,
                  devices.FONTS, devices.OS_VERSIONS, devices.TOUCH_POINTS,
                  devices.PIXEL_RATIOS):
        assert os_key in table


def test_mobile_targets_are_flagged_mobile_and_touch_capable():
    for os_key in devices.MOBILE_OS:
        fp = generate(seed=5, os=os_key)
        assert fp.is_mobile is True
        assert fp.max_touch_points >= 1
        assert fp.device_pixel_ratio >= 2


def test_ios_claims_the_webkit_shaped_surface():
    fp = generate(seed=5, os="ios")
    assert "CriOS/" in fp.user_agent and "iPhone OS" in fp.user_agent
    assert "Chrome/" not in fp.user_agent      # iOS browsers never say Chrome/
    # WebKit implements neither, so 0 is the marker for "no such property".
    assert fp.device_memory == 0
    assert validate(fp.__class__(**{**fp.to_dict(), "device_memory": 8})), (
        "an iOS profile reporting deviceMemory should fail coherence")


def test_desktop_targets_are_not_touch_phones():
    for os_key in devices.DESKTOP_OS:
        fp = generate(seed=5, os=os_key)
        assert fp.is_mobile is False
        assert fp.device_memory > 0


# --------------------------------------------------------------------------
# Realistic distributions
# --------------------------------------------------------------------------
def test_weight_tables_stay_aligned_with_the_pools_they_weight():
    """`weighted()` falls back to a uniform draw on a length mismatch, so drift
    is silent at runtime. This is the check that makes it loud."""
    w = devices.WEIGHTS
    for os_key in devices.ALL_OS:
        pairs = [
            ("screens", devices.SCREENS[os_key]),
            ("webgl", devices.WEBGL[os_key]),
            ("cores", devices.HARDWARE[os_key]["cores"]),
            ("memory", devices.HARDWARE[os_key]["memory"]),
            ("os_versions", devices.OS_VERSIONS[os_key]),
            ("touch", devices.TOUCH_POINTS[os_key]),
            ("pixel_ratios", devices.PIXEL_RATIOS[os_key]),
        ]
        for table, pool in pairs:
            weights = w[table].get(os_key)
            assert weights is not None, f"{table}[{os_key}] has no weights"
            assert len(weights) == len(pool), (
                f"{table}[{os_key}]: {len(weights)} weights for {len(pool)} entries")
    assert len(w["chrome"]) == len(devices.CHROME_VERSIONS)
    assert len(w["android_models"]) == len(devices.ANDROID_MODELS)
    assert set(w["os"]) == set(devices.DESKTOP_OS)


def test_os_choice_follows_desktop_market_share_not_a_coin_flip():
    spread = collections.Counter(generate(seed=s).os for s in range(1500))
    assert spread["windows"] > spread["macos"] > spread["linux"]
    # Uniform over three would be ~33%; the real desktop split is Windows-heavy.
    assert spread["windows"] / 1500 > 0.6


def test_gpus_are_drawn_from_a_realistic_mix_not_uniformly():
    """A fleet where every third profile carries an exotic GPU is itself the
    signal, which is the whole reason these are weighted."""
    spread = collections.Counter(
        generate(seed=s, os="windows").webgl_renderer for s in range(1500))
    common = max(spread, key=spread.get)
    assert "Intel" in common, f"most common Windows GPU should be integrated: {common}"
    rarest = min(spread.values()) / 1500
    assert rarest < 0.15, "every GPU is equally likely - the draw is still uniform"


def test_phones_do_not_have_a_randomly_trimmed_font_list():
    """Desktops gain and lose fonts; iOS and Android ship a sealed set, so a
    handset missing six of them is the anomaly."""
    for os_key in devices.MOBILE_OS:
        for seed in (1, 2, 3):
            assert generate(seed=seed, os=os_key).fonts == sorted(devices.FONTS[os_key])


def test_desktop_font_lists_do_vary_between_profiles():
    lists = {tuple(generate(seed=s, os="windows").fonts) for s in range(20)}
    assert len(lists) > 1


# --------------------------------------------------------------------------
# Client Hints: one source of truth for the header and the in-page object
# --------------------------------------------------------------------------
def test_the_header_and_the_brand_list_cannot_drift_apart():
    fp = generate(seed=4, os="windows")
    header = sec_ch_ua(fp)
    for brand in ua_brands(fp):
        assert f'"{brand["brand"]}";v="{brand["version"]}"' in header


# --------------------------------------------------------------------------
# Dashboard <-> engine conversion
# --------------------------------------------------------------------------
def _round_trip(fp):
    from numera.models import Profile
    dash = to_dashboard_profile(Profile(name="rt", fingerprint=fp))
    dash["id"] = "rt0000000000"
    return to_engine_profile(dash).fingerprint


@pytest.mark.parametrize("os_key", devices.ALL_OS)
def test_no_parity_field_is_dropped_in_conversion(os_key):
    """The bug class this phase exists to close: a value the dashboard renders
    that never survives `to_engine_profile`."""
    fp = generate(seed=13, os=os_key)
    back = _round_trip(fp)
    for field in ("client_rects", "webgl_image", "media_mode", "speakers",
                  "os_version", "device_model", "max_touch_points",
                  "device_pixel_ratio", "seed", "canvas", "audio", "font_mode",
                  "geolocation", "do_not_track", "webrtc", "fonts",
                  "cameras", "microphones", "device_memory"):
        assert getattr(back, field) == getattr(fp, field), (
            f"{os_key}: '{field}' did not survive the dashboard round-trip")


def test_a_trimmed_font_list_survives_conversion():
    fp = generate(seed=13, os="windows")
    fp.fonts = ["Arial", "Georgia"]
    assert _round_trip(fp).fonts == ["Arial", "Georgia"]


def test_a_seedless_dashboard_profile_gets_the_same_seed_every_time():
    """`to_engine_profile` used to mint a fresh random seed whenever the profile
    carried none — which is every profile the dashboard has ever created — so
    the canvas/audio noise changed on every launch."""
    dash = {"id": "abc123def456", "name": "p", "os": "Windows"}
    seeds = {to_engine_profile(dash).fingerprint.seed for _ in range(5)}
    assert len(seeds) == 1 and 0 not in seeds


def test_two_seedless_profiles_do_not_share_a_seed():
    a = to_engine_profile({"id": "aaaaaaaaaaaa", "name": "a", "os": "Windows"})
    b = to_engine_profile({"id": "bbbbbbbbbbbb", "name": "b", "os": "Windows"})
    assert a.fingerprint.seed != b.fingerprint.seed


def test_an_explicit_seed_still_wins():
    dash = {"id": "abc123def456", "name": "p", "os": "Windows", "seed": 4242}
    assert to_engine_profile(dash).fingerprint.seed == 4242


def test_ios_dashboard_profiles_convert_and_stay_coherent():
    dash = {"id": "iosiosiosios", "name": "phone", "os": "iOS",
            "screen": "393x852", "memory": 8, "cores": 4,
            "webglVendor": "Apple Inc.", "webglRenderer": "Apple GPU",
            "locale": "en-US", "browserVersion": "128.0.0.0"}
    fp = to_engine_profile(dash).fingerprint
    assert fp.os == "ios" and fp.is_mobile is True
    # The dashboard sent 8 GB; iOS has no deviceMemory to report it in.
    assert fp.device_memory == 0
    assert validate(fp) == [], validate(fp)


def test_unknown_dashboard_os_still_falls_back_to_windows():
    fp = to_engine_profile({"id": "x", "name": "p", "os": "Symbian"}).fingerprint
    assert fp.os == "windows"
