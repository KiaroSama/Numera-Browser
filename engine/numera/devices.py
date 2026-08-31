"""
Realistic device profile database.

A fingerprint is only convincing when its parts agree with each other. A macOS
user-agent paired with an NVIDIA GPU, or a 4-core phone reporting a 2560x1440
screen, is an instant tell. This module holds curated, internally-consistent
building blocks so the generator can only ever assemble plausible devices.

Everything here reflects real-world hardware/software combinations. Update the
Chrome/Safari versions periodically to stay current.
"""

from __future__ import annotations

import random

# Recent stable Chrome major versions. Keep the newest 3-4 to look up-to-date.
CHROME_VERSIONS = ["125.0.0.0", "126.0.0.0", "127.0.0.0", "128.0.0.0"]

# WebGL unmasked vendor/renderer strings, grouped by OS so we never leak an
# Apple GPU on Windows or an NVIDIA card on a Mac.
WEBGL = {
    "windows": [
        ("Google Inc. (NVIDIA)",
         "ANGLE (NVIDIA, NVIDIA GeForce RTX 3060 Direct3D11 vs_5_0 ps_5_0, D3D11)"),
        ("Google Inc. (NVIDIA)",
         "ANGLE (NVIDIA, NVIDIA GeForce GTX 1660 Ti Direct3D11 vs_5_0 ps_5_0, D3D11)"),
        ("Google Inc. (Intel)",
         "ANGLE (Intel, Intel(R) UHD Graphics 630 Direct3D11 vs_5_0 ps_5_0, D3D11)"),
        ("Google Inc. (AMD)",
         "ANGLE (AMD, AMD Radeon RX 6600 Direct3D11 vs_5_0 ps_5_0, D3D11)"),
    ],
    "macos": [
        ("Google Inc. (Apple)", "ANGLE (Apple, Apple M1, OpenGL 4.1)"),
        ("Google Inc. (Apple)", "ANGLE (Apple, Apple M2, OpenGL 4.1)"),
        ("Google Inc. (Intel)",
         "ANGLE (Intel, Intel(R) Iris(TM) Plus Graphics OpenGL Engine, OpenGL 4.1)"),
    ],
    "linux": [
        ("Google Inc. (NVIDIA)",
         "ANGLE (NVIDIA, NVIDIA GeForce GTX 1050 Ti/PCIe/SSE2, OpenGL 4.5)"),
        ("Google Inc. (Intel)",
         "ANGLE (Intel, Mesa Intel(R) UHD Graphics (CML GT2), OpenGL 4.6)"),
    ],
    "android": [
        ("Google Inc. (Qualcomm)", "ANGLE (Qualcomm, Adreno (TM) 660, OpenGL ES 3.2)"),
        ("Google Inc. (ARM)", "ANGLE (ARM, Mali-G78, OpenGL ES 3.2)"),
    ],
    # iOS reports the plain unprefixed strings — no ANGLE wrapper, because
    # WebKit renders WebGL straight onto Metal. Every iPhone answers the same
    # two values, which is itself the correct signal: an iPhone that claimed a
    # distinctive GPU would be the anomaly.
    "ios": [
        ("Apple Inc.", "Apple GPU"),
    ],
}

# Screen resolutions (width, height) per OS. Mobile entries are CSS pixels,
# which is what `screen.width` reports in a mobile browser — the physical panel
# is that number times the device pixel ratio below.
SCREENS = {
    "windows": [(1920, 1080), (1366, 768), (2560, 1440), (1536, 864), (1600, 900)],
    "macos": [(2560, 1600), (1440, 900), (1728, 1117), (1512, 982)],
    "linux": [(1920, 1080), (1366, 768), (2560, 1440)],
    "android": [(360, 800), (393, 873), (412, 915), (384, 854)],
    "ios": [(390, 844), (393, 852), (430, 932), (375, 812), (428, 926)],
}

# Plausible hardware for each platform: CPU threads and RAM (GB).
HARDWARE = {
    "windows": {"cores": [4, 6, 8, 12, 16], "memory": [4, 8, 16, 32]},
    "macos": {"cores": [8, 10, 12], "memory": [8, 16, 32]},
    "linux": {"cores": [4, 8, 16], "memory": [8, 16]},
    "android": {"cores": [6, 8], "memory": [4, 6, 8]},
    # WebKit implements neither navigator.deviceMemory nor a truthful
    # hardwareConcurrency spread; iOS Safari reports a flat 4 threads and no
    # deviceMemory at all. memory 0 is the marker for "this OS does not expose
    # it" and stealth.py deletes the property rather than answering with a
    # number no real iPhone would give.
    "ios": {"cores": [4], "memory": [0]},
}

# OS token that appears in the User-Agent string. ``{v}`` is filled from the
# profile's OS version; the desktop tokens are frozen by Chrome (it reports
# "Windows NT 10.0" and "10_15_7" on every modern release) so only the mobile
# ones actually vary.
OS_UA_TOKEN = {
    "windows": "Windows NT 10.0; Win64; x64",
    "macos": "Macintosh; Intel Mac OS X 10_15_7",
    "linux": "X11; Linux x86_64",
    "android": "Linux; Android {v}; {model}",
    "ios": "iPhone; CPU iPhone OS {v_} like Mac OS X",
}

# navigator.platform value per OS.
NAV_PLATFORM = {
    "windows": "Win32",
    "macos": "MacIntel",
    "linux": "Linux x86_64",
    "android": "Linux armv8l",
    "ios": "iPhone",
}

# sec-ch-ua-platform header value per OS. iOS has no entry that a real device
# would send: WebKit ships no Client Hints at all, so an iOS profile suppresses
# the whole UA-CH surface instead (see stealth.py).
CH_PLATFORM = {
    "windows": '"Windows"',
    "macos": '"macOS"',
    "linux": '"Linux"',
    "android": '"Android"',
    "ios": '""',
}

# OS releases in circulation, and the value each one puts in the UA-CH
# ``platformVersion`` high-entropy hint. Windows is the odd one: the UA string
# says "Windows NT 10.0" for both 10 and 11, and only platformVersion tells
# them apart (>= 13.0.0 means Windows 11).
OS_VERSIONS = {
    "windows": [("11", "15.0.0"), ("10", "10.0.0")],
    "macos": [("15.5", "15.5.0"), ("14.7", "14.7.0"), ("13.7", "13.7.0")],
    "linux": [("", "")],
    "android": [("15", "15.0.0"), ("14", "14.0.0"), ("13", "13.0.0")],
    "ios": [("18.5", "18.5.0"), ("17.6", "17.6.0"), ("16.7", "16.7.0")],
}

# Handset models that appear in the Android UA string.
ANDROID_MODELS = ["SM-S918B", "SM-A546B", "Pixel 8", "Pixel 7", "2201116SG"]

# Touch points a device reports. Real desktops are overwhelmingly 0, but
# convertibles and touch monitors exist and report Windows' cap of 10; phones
# report 5. A fleet where every desktop is 0 is as uniform as one where every
# desktop is 10.
TOUCH_POINTS = {
    "windows": [0, 10],
    "macos": [0],
    "linux": [0],
    "android": [5],
    "ios": [5],
}

# window.devicePixelRatio. Windows carries the OS display-scaling spread
# (100% / 125% / 150%), Retina Macs are a flat 2, and phones are 2.6-3.
PIXEL_RATIOS = {
    "windows": [1.0, 1.25, 1.5],
    "macos": [2.0],
    "linux": [1.0],
    "android": [2.625, 3.0],
    "ios": [3.0, 2.0],
}

# Locale -> (timezone, accept-language list). Used to keep language, timezone
# and (optionally) proxy geography in agreement.
LOCALES = {
    "en-US": ("America/New_York", ["en-US", "en"]),
    "en-GB": ("Europe/London", ["en-GB", "en"]),
    "en-PK": ("Asia/Karachi", ["en-PK", "en", "ur"]),
    "en-IN": ("Asia/Kolkata", ["en-IN", "en", "hi"]),
    "de-DE": ("Europe/Berlin", ["de-DE", "de", "en"]),
    "fr-FR": ("Europe/Paris", ["fr-FR", "fr", "en"]),
    "es-ES": ("Europe/Madrid", ["es-ES", "es", "en"]),
    "tr-TR": ("Europe/Istanbul", ["tr-TR", "tr", "en"]),
    "it-IT": ("Europe/Rome", ["it-IT", "it", "en"]),
    # Belgium is officially trilingual; Dutch is the largest share and
    # French is the usual second, which is what a Belgian browser sends.
    "nl-BE": ("Europe/Brussels", ["nl-BE", "nl", "fr", "en"]),
}

# Rough country -> locale hint, so a proxy's country can steer the locale.
COUNTRY_TO_LOCALE = {
    "US": "en-US", "GB": "en-GB", "UK": "en-GB", "PK": "en-PK",
    "IN": "en-IN", "DE": "de-DE", "FR": "fr-FR", "ES": "es-ES", "TR": "tr-TR",
    "IT": "it-IT", "BE": "nl-BE",
}

# Approximate (city-level) coordinates per country, so a profile's
# geolocation can agree with its proxy/timezone instead of contradicting it.
# City chosen to match the locale's timezone (e.g. US -> New York for
# America/New_York). Accuracy is a plausible metres value, not a pinpoint.
COUNTRY_GEO = {
    "US": (40.7128, -74.0060), "GB": (51.5074, -0.1278),
    "UK": (51.5074, -0.1278),  "PK": (24.8607, 67.0011),
    "IN": (28.6139, 77.2090),  "DE": (52.5200, 13.4050),
    "FR": (48.8566, 2.3522),   "ES": (40.4168, -3.7038),
    "TR": (41.0082, 28.9784),  "IT": (41.9028, 12.4964),
    "BE": (50.8503, 4.3517),
}

# locale -> the country whose coordinates fit it, for when there's no proxy.
LOCALE_TO_COUNTRY = {
    "en-US": "US", "en-GB": "GB", "en-PK": "PK", "en-IN": "IN",
    "de-DE": "DE", "fr-FR": "FR", "es-ES": "ES", "tr-TR": "TR",
    "it-IT": "IT", "nl-BE": "BE",
}

# Fonts that actually ship with each OS. Font-probing is a classic fingerprint
# vector — a "Windows" browser that can render Helvetica Neue but not Segoe UI
# is a giveaway — so each archetype carries its own realistic set.
FONTS = {
    "windows": [
        "Arial", "Arial Black", "Bahnschrift", "Calibri", "Cambria", "Candara",
        "Comic Sans MS", "Consolas", "Constantia", "Corbel", "Courier New",
        "Ebrima", "Franklin Gothic Medium", "Gabriola", "Gadugi", "Georgia",
        "Impact", "Ink Free", "Lucida Console", "Lucida Sans Unicode",
        "Malgun Gothic", "Microsoft Sans Serif", "MS Gothic", "MV Boli",
        "Nirmala UI", "Palatino Linotype", "Segoe Print", "Segoe Script",
        "Segoe UI", "SimSun", "Sylfaen", "Symbol", "Tahoma", "Times New Roman",
        "Trebuchet MS", "Verdana", "Webdings", "Wingdings",
    ],
    "macos": [
        "American Typewriter", "Andale Mono", "Arial", "Arial Black",
        "Avenir", "Avenir Next", "Baskerville", "Big Caslon", "Bodoni 72",
        "Bradley Hand", "Chalkboard", "Chalkduster", "Cochin", "Comic Sans MS",
        "Copperplate", "Courier", "Courier New", "Didot", "Futura", "Geneva",
        "Georgia", "Gill Sans", "Helvetica", "Helvetica Neue", "Herculanum",
        "Hoefler Text", "Impact", "Lucida Grande", "Luminari", "Marker Felt",
        "Menlo", "Monaco", "Optima", "Palatino", "Papyrus", "Phosphate",
        "Rockwell", "SF Pro", "Skia", "Snell Roundhand", "Times", "Times New Roman",
        "Trattatello", "Trebuchet MS", "Verdana", "Zapfino",
    ],
    "linux": [
        "Bitstream Charter", "Cantarell", "Century Schoolbook L", "Courier 10 Pitch",
        "DejaVu Sans", "DejaVu Sans Mono", "DejaVu Serif", "FreeMono", "FreeSans",
        "FreeSerif", "Liberation Mono", "Liberation Sans", "Liberation Serif",
        "Nimbus Mono PS", "Nimbus Roman", "Nimbus Sans", "Noto Sans", "Noto Serif",
        "Ubuntu", "Ubuntu Condensed", "Ubuntu Mono", "URW Bookman", "URW Gothic",
    ],
    "android": [
        "Droid Sans Mono", "Noto Color Emoji", "Noto Sans", "Noto Serif",
        "Roboto", "Roboto Condensed", "Roboto Mono",
    ],
    # iOS ships a fixed set and no user-installable fonts, so the list does not
    # vary between handsets — see the note on `sample_fonts`.
    "ios": [
        "Academy Engraved LET", "American Typewriter", "Arial", "Arial Rounded MT Bold",
        "Avenir", "Avenir Next", "Baskerville", "Bodoni 72", "Bradley Hand",
        "Chalkboard SE", "Cochin", "Copperplate", "Courier New", "Damascus",
        "Didot", "Futura", "Georgia", "Gill Sans", "Helvetica", "Helvetica Neue",
        "Hoefler Text", "Kailasa", "Marker Felt", "Menlo", "Noteworthy", "Optima",
        "Palatino", "Papyrus", "Party LET", "Savoye LET", "Snell Roundhand",
        "Times New Roman", "Trebuchet MS", "Verdana", "Zapfino",
    ],
}

DESKTOP_OS = ["windows", "macos", "linux"]
MOBILE_OS = ["android", "ios"]
ALL_OS = DESKTOP_OS + MOBILE_OS


# --------------------------------------------------------------------------
# Realistic distributions
#
# Drawing every value uniformly makes a *fleet* implausible even when each
# profile is individually fine: a hundred profiles split evenly between an
# RTX 3060 and a UHD 630 is not what a hundred real visitors look like, and a
# rare GPU on every third profile is a signal in itself. These weights are
# index-aligned with the pools above and are approximate real-world shares
# (StatCounter for OS/screen, Steam hardware survey re-weighted towards
# integrated graphics for a general population, Chrome UX telemetry for the
# rest). Treat them as the right *shape*, not exact figures — the point is that
# the common case stays common.
# --------------------------------------------------------------------------
WEIGHTS: dict[str, dict] = {
    "os": {"windows": 72, "macos": 21, "linux": 7},
    "chrome": [8, 17, 30, 45],          # newest majors carry most of the share
    "screens": {
        "windows": [34, 20, 8, 20, 9],
        "macos": [30, 34, 20, 16],
        "linux": [55, 25, 20],
        "android": [30, 26, 28, 16],
        "ios": [26, 30, 20, 14, 10],
    },
    "webgl": {
        # Integrated Intel is the single most common desktop GPU by a wide
        # margin once you leave the gaming population.
        "windows": [12, 8, 58, 22],
        "macos": [40, 38, 22],
        "linux": [25, 75],
        "android": [55, 45],
        "ios": [100],
    },
    "cores": {
        "windows": [24, 17, 31, 15, 13],
        "macos": [52, 28, 20],
        "linux": [30, 50, 20],
        "android": [45, 55],
        "ios": [100],
    },
    "memory": {
        "windows": [10, 40, 38, 12],
        "macos": [30, 45, 25],
        "linux": [55, 45],
        "android": [35, 35, 30],
        "ios": [100],
    },
    "os_versions": {
        "windows": [62, 38],
        "macos": [45, 35, 20],
        "linux": [100],
        "android": [25, 45, 30],
        "ios": [50, 33, 17],
    },
    "android_models": [30, 18, 20, 17, 15],
    "touch": {
        "windows": [88, 12],
        "macos": [100],
        "linux": [100],
        "android": [100],
        "ios": [100],
    },
    "pixel_ratios": {
        "windows": [58, 27, 15],
        "macos": [100],
        "linux": [100],
        "android": [55, 45],
        "ios": [65, 35],
    },
}


def weighted(rng: random.Random, pool: list, weights=None):
    """Draw one item from ``pool`` following ``weights``.

    Falls back to a uniform draw when the weights are missing or have drifted
    out of alignment with the pool, so adding a device to a list can never
    raise — it just loses its weighting until the table is updated. The paired
    test asserts every pool and weight list are the same length, which is what
    actually catches the drift.
    """
    if weights and len(weights) == len(pool):
        return rng.choices(pool, weights=weights)[0]
    return rng.choice(pool)


def sample_fonts(rng: random.Random, os_key: str) -> list[str]:
    """A believable installed-font list for ``os_key``.

    Desktops accumulate and lose fonts (an Office install, a design tool, a
    trimmed OEM image), so we drop a few at random. Phones do not: iOS and
    Android ship a sealed set with no user-installable fonts, so a handset whose
    font list is missing six entries is the anomaly. Same reasoning as the GPU
    pools — the realistic distribution for a phone is *no* variance.
    """
    pool = FONTS.get(os_key, [])
    if not pool:
        return []
    if os_key in MOBILE_OS:
        return sorted(pool)
    return sorted(rng.sample(pool, k=max(6, len(pool) - rng.randint(0, 6))))


# --------------------------------------------------------------------------
# Timezone identity
# --------------------------------------------------------------------------
# One zone, two names. The tz database keeps its old ids as links when a place
# is renamed, and CLDR - which is what Chromium answers
# `Intl.DateTimeFormat().resolvedOptions().timeZone` from - did not follow every
# rename. So a profile set to `Asia/Kolkata` is reported by the browser as
# `Asia/Calcutta`, and comparing the strings called that a contradiction. It is
# the same zone: same offsets, same transitions, same abbreviations. A real
# Chrome on a machine set to Kolkata says `Asia/Calcutta` too, so the profile
# was coherent and the report was wrong.
#
# Pairs, not a canonical direction: either name may be the one stored and either
# may be the one reported. Add a line when a new rename lands in tzdata.
_TZ_ALIAS_PAIRS = [
    ("Asia/Kolkata", "Asia/Calcutta"),
    ("Asia/Ho_Chi_Minh", "Asia/Saigon"),
    ("Asia/Kathmandu", "Asia/Katmandu"),
    ("Asia/Yangon", "Asia/Rangoon"),
    ("Asia/Ulaanbaatar", "Asia/Ulan_Bator"),
    ("Asia/Chongqing", "Asia/Chungking"),
    ("Asia/Macau", "Asia/Macao"),
    ("Europe/Kyiv", "Europe/Kiev"),
    ("America/Nuuk", "America/Godthab"),
    ("America/Argentina/Buenos_Aires", "America/Buenos_Aires"),
    ("America/Argentina/Cordoba", "America/Cordoba"),
    ("America/Argentina/Mendoza", "America/Mendoza"),
    ("America/Indiana/Indianapolis", "America/Indianapolis"),
    ("America/Kentucky/Louisville", "America/Louisville"),
    ("Atlantic/Faroe", "Atlantic/Faeroe"),
    ("Pacific/Pohnpei", "Pacific/Ponape"),
    ("Pacific/Chuuk", "Pacific/Truk"),
    ("Pacific/Kanton", "Pacific/Enderbury"),
    ("Europe/Istanbul", "Asia/Istanbul"),
]

# Both directions, lower-cased: the comparison should not care which name is
# stored, nor about the case a browser happens to use.
TZ_ALIASES = {}
for _a, _b in _TZ_ALIAS_PAIRS:
    TZ_ALIASES[_a.lower()] = _b
    TZ_ALIASES[_b.lower()] = _a


def same_timezone(a, b) -> bool:
    """Do these two IANA names refer to the same zone?

    String equality, plus the tz-database links a browser may substitute. Not
    an offset comparison: two genuinely different zones can share an offset for
    half the year, and calling those equal would hide a real contradiction.
    """
    left = str(a or "").strip()
    right = str(b or "").strip()
    if not left or not right:
        return False
    if left.lower() == right.lower():
        return True
    return TZ_ALIASES.get(left.lower(), "").lower() == right.lower()

