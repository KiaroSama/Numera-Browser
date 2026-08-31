"""
Consistent fingerprint generation.

The whole point of this module is *coherence*. Anti-bot systems don't just read
individual values; they cross-check them. A convincing fingerprint is one where
the OS, user-agent, GPU, screen, CPU, timezone and language all tell the same
story. So instead of randomising each field independently, we pick a device
archetype first and then draw every value from that archetype's realistic pool.

Public API:
    generate(seed=None, os=None, locale=None, proxy=None, mobile=False) -> Fingerprint
"""

from __future__ import annotations

import random
from typing import Optional

from . import devices
from .models import Fingerprint, Proxy


def _build_user_agent(os_key: str, chrome: str, os_version: str = "",
                      model: str = "") -> str:
    token = devices.OS_UA_TOKEN[os_key].format(
        v=os_version or "14",
        v_=(os_version or "18.5").replace(".", "_"),
        model=model or devices.ANDROID_MODELS[0],
    )
    if os_key == "ios":
        # Chrome on iOS is Blink-in-name-only: Apple forbids third-party engines
        # on the App Store, so every iOS browser is WebKit wearing a badge, and
        # its UA says so — WebKit's build number and a CriOS token, never
        # "Chrome/". Numera emulates this shape on a real Blink browser, so the
        # string is honest about what iOS Chrome reports and dishonest about
        # what is actually rendering it. See the limits note in stealth.py.
        return (
            f"Mozilla/5.0 ({token}) AppleWebKit/605.1.15 (KHTML, like Gecko) "
            f"CriOS/{chrome} Mobile/15E148 Safari/604.1"
        )
    if os_key == "android":
        return (
            f"Mozilla/5.0 ({token}) AppleWebKit/537.36 (KHTML, like Gecko) "
            f"Chrome/{chrome} Mobile Safari/537.36"
        )
    return (
        f"Mozilla/5.0 ({token}) AppleWebKit/537.36 (KHTML, like Gecko) "
        f"Chrome/{chrome} Safari/537.36"
    )


def platform_version(os_key: str, os_version: str) -> str:
    """The UA-CH ``platformVersion`` hint that goes with an OS release.

    Windows is the case that matters: the UA string reads "Windows NT 10.0" on
    both 10 and 11, so this hint is the only thing that distinguishes them, and
    a profile claiming Windows 11 whose hint says 10.0.0 is contradicting
    itself in a way sites actively check.
    """
    for name, hint in devices.OS_VERSIONS.get(os_key, []):
        if name == os_version:
            return hint
    return os_version


def _pick_locale(rng: random.Random, locale: Optional[str],
                 proxy: Optional[Proxy]) -> str:
    """Choose a locale. Explicit wins; else follow the proxy's country; else random."""
    if locale and locale in devices.LOCALES:
        return locale
    if proxy and proxy.country:
        hint = devices.COUNTRY_TO_LOCALE.get(proxy.country.upper())
        if hint:
            return hint
    return rng.choice(list(devices.LOCALES.keys()))


def generate(
    seed: Optional[int] = None,
    os: Optional[str] = None,
    locale: Optional[str] = None,
    proxy: Optional[Proxy] = None,
    mobile: bool = False,
) -> Fingerprint:
    """Produce one internally-consistent :class:`Fingerprint`.

    Args:
        seed: fixed seed for reproducibility (also drives canvas/audio noise).
              If ``None``, a random seed is generated and stored.
        os: force an OS ("windows"|"macos"|"linux"|"android"); random if None.
        locale: force a locale key; otherwise inferred from proxy or random.
        proxy: if given and it has a country, locale/timezone align to it.
        mobile: allow/force a mobile (Android) device.
    """
    if seed is None:
        seed = random.randint(1, 2**31 - 1)
    rng = random.Random(seed)
    w = devices.WEIGHTS

    # 1. Pick the operating system (the archetype root).
    if os:
        os_key = os.lower()
    elif mobile:
        os_key = devices.weighted(rng, devices.MOBILE_OS, [70, 30])
    else:
        os_key = devices.weighted(
            rng, devices.DESKTOP_OS, [w["os"][k] for k in devices.DESKTOP_OS])
    is_mobile = os_key in devices.MOBILE_OS

    # 2. Browser and OS version.
    chrome = devices.weighted(rng, devices.CHROME_VERSIONS, w["chrome"])
    os_version, _hint = devices.weighted(
        rng, devices.OS_VERSIONS[os_key], w["os_versions"].get(os_key))
    model = (devices.weighted(rng, devices.ANDROID_MODELS, w["android_models"])
             if os_key == "android" else "")

    # 3. Screen + viewport. Viewport is the screen minus realistic chrome/taskbar.
    sw, sh = devices.weighted(rng, devices.SCREENS[os_key], w["screens"].get(os_key))
    if is_mobile:
        # The bar a mobile browser keeps on screen: iOS Safari/Chrome reserve
        # more of the viewport than Android does.
        vw, vh = sw, sh - rng.choice([113, 140] if os_key == "ios" else [0, 80, 120])
    else:
        vw = sw - rng.choice([0, 16])
        vh = sh - rng.choice([74, 88, 104, 120])  # browser UI + OS bar

    # 4. Hardware consistent with the OS.
    cores = devices.weighted(rng, devices.HARDWARE[os_key]["cores"], w["cores"].get(os_key))
    memory = devices.weighted(rng, devices.HARDWARE[os_key]["memory"], w["memory"].get(os_key))
    touch = devices.weighted(rng, devices.TOUCH_POINTS[os_key], w["touch"].get(os_key))
    dpr = devices.weighted(rng, devices.PIXEL_RATIOS[os_key], w["pixel_ratios"].get(os_key))

    # 5. GPU that actually exists on this OS.
    webgl_vendor, webgl_renderer = devices.weighted(
        rng, devices.WEBGL[os_key], w["webgl"].get(os_key))

    # 6. Locale -> timezone + languages (optionally steered by the proxy).
    loc = _pick_locale(rng, locale, proxy)
    timezone, languages = devices.LOCALES[loc]

    # 7. Secondary signals. Media devices match the form factor — phones always
    #    have both, desktops sometimes have neither.
    fonts = devices.sample_fonts(rng, os_key)
    if is_mobile:
        cameras, microphones, speakers = 2, 1, 1
    else:
        cameras = rng.choice([0, 1, 1, 1, 2])
        microphones = rng.choice([0, 1, 1, 1, 2]) if cameras else rng.choice([0, 1])
        speakers = rng.choice([1, 1, 2]) if microphones else rng.choice([0, 1])

    return Fingerprint(
        os=os_key,
        user_agent=_build_user_agent(os_key, chrome, os_version, model),
        platform=devices.NAV_PLATFORM[os_key],
        ch_platform=devices.CH_PLATFORM[os_key],
        chrome_version=chrome,
        screen_width=sw,
        screen_height=sh,
        viewport_width=vw,
        viewport_height=vh,
        hardware_concurrency=cores,
        device_memory=memory,
        webgl_vendor=webgl_vendor,
        webgl_renderer=webgl_renderer,
        locale=loc,
        timezone=timezone,
        languages=languages,
        is_mobile=is_mobile,
        seed=seed,
        fonts=fonts,
        cameras=cameras,
        microphones=microphones,
        speakers=speakers,
        os_version=os_version,
        device_model=model,
        max_touch_points=touch,
        device_pixel_ratio=dpr,
    )


def ua_brands(fp: Fingerprint) -> list[dict]:
    """The UA-CH brand list for this fingerprint.

    Single source of truth for both the ``sec-ch-ua`` *header* and the in-page
    ``navigator.userAgentData.brands``. They are the same claim made twice, and
    a site that reads both will compare them — an unpatched Chromium answers
    "Chromium/151" in JS while the header says "Google Chrome/128", which is the
    kind of contradiction this project exists to avoid.
    """
    major = fp.chrome_version.split(".")[0]
    return [
        {"brand": "Chromium", "version": major},
        {"brand": "Google Chrome", "version": major},
        {"brand": "Not-A.Brand", "version": "99"},
    ]


def sec_ch_ua(fp: Fingerprint) -> str:
    """Build a Client Hints ``sec-ch-ua`` header matching the Chrome version."""
    return ", ".join(f'"{b["brand"]}";v="{b["version"]}"' for b in ua_brands(fp))


def build_user_agent(os_key: str, chrome: str, os_version: str = "",
                     model: str = "") -> str:
    """Public wrapper so callers (e.g. the aligner) can rebuild a UA for an OS.

    ``os_version``/``model`` only appear in the mobile strings; desktop Chrome
    freezes its OS token, so a caller that has no version to offer still gets a
    correct desktop UA.
    """
    return _build_user_agent(os_key, chrome, os_version, model)


def webgl_plausible(os_key: str, vendor: str, renderer: str) -> bool:
    """Is this WebGL vendor/renderer believable for ``os_key``?

    The curated pool in :mod:`devices` can't list every real GPU — there are
    thousands. So on top of "is it one we generate?", we accept any renderer
    whose API shape matches the OS: Windows drives WebGL through Direct3D
    (ANGLE/D3D11), while macOS and Linux go through desktop OpenGL. This lets a
    profile carry the machine's *actual* GPU (e.g. after `numera align`)
    without being flagged as incoherent, while still catching an Apple GPU
    pretending to be on Windows.
    """
    if (vendor, renderer) in devices.WEBGL.get(os_key, []):
        return True
    r = (renderer or "").lower()
    if not r:
        return False
    is_d3d = "direct3d" in r or "d3d11" in r or "d3d9" in r
    is_gl = "opengl" in r
    if os_key == "windows":
        return is_d3d
    if os_key == "macos":
        return is_gl and not is_d3d and ("apple" in r or "intel" in r or "amd" in r or "metal" in r or "opengl 4" in r)
    if os_key == "linux":
        return is_gl and not is_d3d
    if os_key == "android":
        return "opengl es" in r or "adreno" in r or "mali" in r
    if os_key == "ios":
        # WebKit hands back "Apple GPU" and nothing else; anything more specific
        # is a value no iPhone has ever reported.
        return "apple" in r
    return False


def validate(fp: Fingerprint) -> list[str]:
    """Return a list of consistency problems (empty list == coherent).

    Useful in tests and as a sanity gate before launching a profile.
    """
    issues: list[str] = []

    if fp.platform != devices.NAV_PLATFORM.get(fp.os):
        issues.append(f"platform '{fp.platform}' does not match os '{fp.os}'")

    if not webgl_plausible(fp.os, fp.webgl_vendor, fp.webgl_renderer):
        issues.append("webgl vendor/renderer not valid for this os")

    # Accept the curated resolutions, but also any real-world one: after aligning
    # to a machine, a profile carries that display's actual size, which won't be
    # in the short curated list yet is perfectly coherent.
    sane = 320 <= fp.screen_width <= 8000 and 480 <= fp.screen_height <= 5000
    if (fp.screen_width, fp.screen_height) not in devices.SCREENS.get(fp.os, []) and not sane:
        issues.append("screen resolution not typical for this os")

    if fp.viewport_width > fp.screen_width or fp.viewport_height > fp.screen_height:
        issues.append("viewport larger than screen")

    if fp.locale in devices.LOCALES:
        tz, langs = devices.LOCALES[fp.locale]
        if fp.timezone != tz:
            issues.append("timezone does not match locale")
        # navigator.languages varies with the user's settings (one entry or
        # several), so we require the *primary* language to agree with the
        # locale rather than an exact list — a machine-aligned profile that
        # reports just ["en-US"] is as coherent as ["en-US", "en"]. We compare
        # the base language (en), not the region (en-US vs en-PK): an English
        # browser in a Karachi timezone is a real, coherent user, and matching
        # on region would wrongly flag it.
        def _base(tag):
            return (tag or "").split("-")[0].lower()
        if not fp.languages or _base(fp.languages[0]) != _base(langs[0]):
            issues.append("languages do not match locale")
    else:
        issues.append(f"unknown locale '{fp.locale}'")

    if fp.is_mobile != (fp.os in devices.MOBILE_OS):
        issues.append("is_mobile flag inconsistent with os")

    # A phone that reports no touch, or a machine claiming more contact points
    # than any real digitiser, contradicts its own form factor.
    if fp.is_mobile and fp.max_touch_points < 1:
        issues.append("a phone with no touch points is implausible")
    if fp.max_touch_points < 0 or fp.max_touch_points > 20:
        issues.append("maxTouchPoints outside the range real devices report")

    if not 0.5 <= fp.device_pixel_ratio <= 4:
        issues.append("devicePixelRatio outside the range real displays report")

    # deviceMemory is the one value iOS has no answer for: WebKit does not
    # implement it, so 0 (meaning "not exposed") is the only coherent setting.
    if fp.os == "ios" and fp.device_memory:
        issues.append("iOS does not expose navigator.deviceMemory")

    # A font that doesn't ship with this OS is as loud a tell as the wrong GPU.
    if fp.fonts:
        pool = set(devices.FONTS.get(fp.os, []))
        alien = [f for f in fp.fonts if f not in pool]
        if alien:
            issues.append(f"fonts not found on {fp.os}: {', '.join(sorted(alien)[:3])}")

    if fp.cameras < 0 or fp.microphones < 0:
        issues.append("media device counts cannot be negative")
    elif fp.is_mobile and (fp.cameras == 0 or fp.microphones == 0):
        issues.append("a phone with no camera or microphone is implausible")

    return issues


def _full_version(fp: Fingerprint) -> str:
    """The four-part Chrome version the full-version hints report."""
    major = fp.chrome_version.split(".")[0]
    return fp.chrome_version if fp.chrome_version.count(".") == 3 else f"{major}.0.0.0"


def ua_metadata(fp: Fingerprint) -> dict:
    """The profile as a CDP ``Emulation.setUserAgentOverride`` metadata block.

    Chromium builds the Sec-CH-UA headers *and* answers
    ``navigator.userAgentData`` from this structure, so handing it over is what
    lets the browser keep its own ``Accept-CH`` logic - low-entropy hints on
    every request, high-entropy ones only once a site has asked - while
    reporting the profile's values instead of the host's.
    """
    full = _full_version(fp)
    brands = ua_brands(fp)
    # Apple Silicon and Android are arm; everything else this project models is
    # x86. Reporting x86 for an M-series Mac contradicts the GPU string.
    arm = fp.os in ("android", "ios") or (
        fp.os == "macos" and "Apple" in (fp.webgl_renderer or ""))
    return {
        "brands": [{"brand": b["brand"], "version": b["version"]} for b in brands],
        "fullVersionList": [
            {"brand": b["brand"],
             "version": "99.0.0.0" if b["brand"] == "Not-A.Brand" else full}
            for b in brands],
        "platform": fp.ch_platform.strip('"') or ("iOS" if fp.os == "ios" else "Unknown"),
        "platformVersion": platform_version(fp.os, fp.os_version),
        "architecture": "arm" if arm else "x86",
        "model": fp.device_model or "",
        "mobile": fp.is_mobile,
        "bitness": "64",
        "wow64": False,
    }


def client_hint_headers(fp: Fingerprint) -> dict:
    """The same values as :func:`ua_metadata`, formatted as headers.

    Split into what Chrome always sends and what it only sends after a site
    opts in with ``Accept-CH``. The launcher pins the low set as a header - the
    values are identical to the ones Chromium derives, so it costs nothing and
    keeps the trio right if the CDP override never lands - and leaves the high
    set to Chromium, which is the only way to get the *timing* right.

    Not empty for iOS, though real Safari implements no Client Hints at all.
    Chromium adds the low-entropy trio to every request itself and nothing
    removes it - `route.continue_`, `--disable-features=UserAgentClientHint`
    and `--blink-settings=clientHintsEnabled=false` were all measured against a
    local server and changed nothing. So an iPhone profile sends hints a real
    iPhone would not; the best available answer is values that at least agree
    with the rest of the identity, and the residual tell is written down in
    docs/architecture.md rather than papered over.
    """
    m = ua_metadata(fp)
    return {
        "low": {
            "sec-ch-ua": sec_ch_ua(fp),
            "sec-ch-ua-mobile": "?1" if m["mobile"] else "?0",
            "sec-ch-ua-platform": f'"{m["platform"]}"',
        },
        "high": {
            "sec-ch-ua-platform-version": f'"{m["platformVersion"]}"',
            "sec-ch-ua-arch": f'"{m["architecture"]}"',
            "sec-ch-ua-bitness": f'"{m["bitness"]}"',
            "sec-ch-ua-model": f'"{m["model"]}"',
            "sec-ch-ua-full-version": f'"{_full_version(fp)}"',
            "sec-ch-ua-full-version-list": ", ".join(
                f'"{b["brand"]}";v="{b["version"]}"' for b in m["fullVersionList"]),
            "sec-ch-ua-wow64": "?0",
        },
    }
