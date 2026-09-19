"""
Import profiles from other anti-detect browsers.

The biggest reason people don't switch tools is the profiles they've already
built. This reads exports from the common ones — GoLogin, AdsPower, Multilogin —
and turns them into Numera profiles.

It's deliberately *tolerant*: those tools version their formats and rename fields
between releases, so instead of demanding an exact schema we look for a value
under any of the names a field is known by, fall back to a coherent generated
value when it's missing, and then run the profile through ``validate()`` so you
can see and fix anything that didn't line up. An import is a starting point you
review, not a black box.
"""

from __future__ import annotations

import json
import math
import re
from pathlib import Path
from typing import Optional

from . import devices
from .fingerprint import generate, validate
from .models import Fingerprint, Profile, Proxy


def _first(d: dict, *names, default=None):
    """First present, non-empty value among ``names`` (supports 'a.b' paths)."""
    for name in names:
        node = d
        for part in name.split("."):
            if isinstance(node, dict) and part in node:
                node = node[part]
            else:
                node = None
                break
        if node not in (None, "", [], {}):
            return node
    return default


_CHROME_IN_UA = re.compile(r"Chrome/(\d+(?:\.\d+){0,3})")


def _chrome_from_ua(ua: str) -> str:
    """The Chrome version an imported user agent claims, or "" if it names none.

    Kept because the rest of the fingerprint is built from it: `sec-ch-ua` and
    `navigator.userAgentData.brands` both carry the major version, and a profile
    whose UA says 120 while its brands say 128 is contradicting itself.
    """
    m = _CHROME_IN_UA.search(ua or "")
    if not m:
        return ""
    parts = m.group(1).split(".")
    while len(parts) < 4:
        parts.append("0")
    return ".".join(parts[:4])


# Tested in this order, and the order is the whole point: every iOS user agent
# says "like Mac OS X" and every Android one says "Linux", so each device has to
# be recognised before the family whose wording it borrows. Windows is the
# answer for an export that names nothing recognisable.
_OS_TOKENS = (
    ("ios", ("ios", "iphone", "ipad", "ipod")),
    ("android", ("android",)),
    ("macos", ("mac", "os x", "darwin")),
    ("linux", ("linux",)),
)


def _norm_os(value: Optional[str], ua: str = "") -> str:
    """Which OS an export means, from its own field and its user agent.

    iOS had no branch: a bare ``"os": "iOS"`` fell through to the Windows
    default, and an iPhone or iPad user agent matched the macOS test first - so
    a phone was imported as a desktop, with a platform contradicting the user
    agent stored beside it.
    """
    text = f"{value or ''} {ua}".lower()
    for key, tokens in _OS_TOKENS:
        if any(token in text for token in tokens):
            return key
    return "windows"


def _parse_resolution(value) -> tuple[int, int]:
    try:
        w, h = (int(x) for x in str(value).lower().replace(" ", "").split("x")[:2])
        return w, h
    except (ValueError, AttributeError):
        return 1920, 1080


def _parse_proxy(d: dict) -> Optional[Proxy]:
    px = _first(d, "proxy", "proxyConfig", "user_proxy_config", default=None)
    if not isinstance(px, dict):
        return None
    host = _first(px, "host", "ip", "proxy_host", "addr")
    if not host:
        return None
    # "mode: none" / "type: no_proxy" means the profile is really direct.
    mode = str(_first(px, "mode", "type", "proxy_type", "proxy_soft", default="")).lower()
    if mode in ("none", "no_proxy", "direct", ""):
        if not _first(px, "type", "proxy_type"):
            return None
    # Preserve what the export actually said. Mapping every "socks*" to SOCKS5
    # and everything else to HTTP silently changed the protocol: a SOCKS4 entry
    # came back as SOCKS5 and an HTTPS one as HTTP, so the imported profile
    # claimed one protocol while the browser spoke another.
    _DECLARED = {"http": "http", "https": "https", "socks4": "socks4",
                 "socks4a": "socks4", "socks5": "socks5", "socks5h": "socks5",
                 "socks": "socks5"}
    declared = str(_first(px, "type", "proxy_type", "mode", default="")).lower().strip()
    scheme = _DECLARED.get(declared) or ("socks5" if "socks" in mode else "http")

    port = _first(px, "port", "proxy_port", default="")
    return Proxy(
        server=f"{scheme}://{host}:{port}".rstrip(":"),
        username=_first(px, "username", "user", "proxy_user", "login"),
        password=_first(px, "password", "pass", "proxy_password"),
        country=_first(px, "country", "geo"),
    )


# The only numbers this parser reads are CPU threads and RAM in gigabytes. No
# browser reports either anywhere near this, so past it the export is holding
# something that is not a hardware count.
_MAX_HARDWARE = 4096


def _as_int(value, fallback: int) -> int:
    """An int from whatever a vendor export happened to put there.

    A bare `int()` raised on `"8 cores"` or `"N/A"`, and the import route saves
    as it goes - so one odd value aborted the request with a 500 and left the
    profiles before it already written. The module's contract is to be tolerant
    (see the header) and `_parse_resolution` already is; this is the same rule
    for the numeric fields.

    The catch used to be `TypeError, ValueError`, which `int(float("inf"))`
    is not: it raises `OverflowError`, so `"inf"`, `"-inf"`, `"1e309"` and a
    400-digit integer went straight through the tolerant path and aborted the
    import. One check now covers every shape of that - non-finite, negative or
    past what hardware reports - and lands on the caller's coherent fallback.
    """
    try:
        number = float(str(value).strip())
    except (TypeError, ValueError):
        return fallback
    if not math.isfinite(number) or not 0 <= number <= _MAX_HARDWARE:
        return fallback
    return int(number)


def _usable(value, fallback, field: str, issues: list):
    """A vendor value this field can be read as, or the coherent fallback.

    The rule ``_as_int`` already applies to the numbers, for the three fields
    whose *type* the parsing below cannot survive at all: the user agent goes
    through a regex, the language is looked up as a dictionary key, and the tag
    list is iterated - so a number, an object and an integer respectively raised
    ``TypeError`` straight out of the import route as a 500.

    Falling back rather than refusing, because each of these three already has a
    coherent generated value computed on the line above for the case where the
    export simply omitted the field, and a value of the wrong type is a value
    this field does not have. Said out loud on the profile, because a silently
    replaced field is the part of a bad import nobody notices.

    Every *other* field keeps its value and is refused by name if the dashboard
    cannot render it - there is no honest guess at what a name that is not text
    was meant to say.
    """
    if isinstance(value, type(fallback)):
        return value
    issues.append(f"{field} in this export was {type(value).__name__}, not "
                  f"{type(fallback).__name__}; the generated value was used "
                  f"instead - check it")
    return fallback


def profile_from_export(d: dict) -> tuple[Profile, list[str]]:
    """Map one exported profile dict to a Numera :class:`Profile`.

    Returns the profile plus the list of coherence issues ``validate()`` found,
    so the caller can surface what needs a look - plus anything ``_usable``
    could not read, for the same reason.
    """
    issues: list[str] = []
    ua = _usable(_first(d, "navigator.userAgent", "navigator.user_agent",
                        "user_agent", "ua", "useragent", "fingerprint.ua",
                        "fingerprint.userAgent", "browser.userAgent",
                        "userAgent", default="") or "",
                 "", "the user agent", issues)
    os_key = _norm_os(_first(d, "os", "os_type", "osType", "navigator.platform"), ua)

    # Start from a coherent base for this OS, then overlay whatever the export
    # actually specified. Gaps stay coherent instead of becoming holes.
    base = generate(os=os_key)

    sw, sh = _parse_resolution(_first(d, "navigator.resolution", "screen_resolution",
                                      "resolution", "screen", default=""))
    locale = _usable(_first(d, "navigator.language", "language", "locale",
                            default=base.locale),
                     base.locale, "the language", issues)
    if locale not in devices.LOCALES:
        locale = base.locale
    # `timezone.id` is a real vendor shape, and the guard is the same one the
    # language above carries: a timezone has a coherent generated value sitting
    # right here - the locale's own zone - so an export that puts an object
    # under `timezone` is a field this profile does not have, not a file the
    # importer should refuse. Unguarded, the dict travelled all the way to the
    # route's `check_fields` as `'timezone' must be str, got dict` and took the
    # WHOLE file down with it, including the profiles that were readable.
    tz = _usable(_first(d, "timezone.timezone", "timezone.id", "timezone",
                        "time_zone", "fingerprint.timezone",
                        default=devices.LOCALES[locale][0]),
                 devices.LOCALES[locale][0], "the timezone", issues)

    fp = Fingerprint(
        os=os_key,
        user_agent=ua or base.user_agent,
        platform=devices.NAV_PLATFORM[os_key],
        ch_platform=devices.CH_PLATFORM[os_key],
        chrome_version=_chrome_from_ua(ua) or base.chrome_version,
        screen_width=sw, screen_height=sh,
        viewport_width=sw,
        viewport_height=max(1, sh - (113 if os_key == "ios" else
                                     80 if os_key == "android" else 88)),
        hardware_concurrency=_as_int(_first(d, "navigator.hardwareConcurrency",
                                            "hardware_concurrency", "cores",
                                            default=base.hardware_concurrency),
                                     base.hardware_concurrency),
        device_memory=_as_int(_first(d, "navigator.deviceMemory", "device_memory",
                                     "memory", default=base.device_memory),
                              base.device_memory),
        webgl_vendor=_first(d, "webGLMetadata.vendor", "webgl.vendor",
                            "webGL.vendor", "graphic.vendor", "webgl_vendor",
                            "unmaskedVendor", "fingerprint.webgl_vendor",
                            default=base.webgl_vendor),
        webgl_renderer=_first(d, "webGLMetadata.renderer", "webgl.renderer",
                              "webGL.renderer", "graphic.renderer", "webgl_renderer",
                              "unmaskedRenderer", "fingerprint.webgl_renderer",
                              default=base.webgl_renderer),
        locale=locale,
        timezone=tz,
        languages=devices.LOCALES[locale][1],
        is_mobile=os_key in devices.MOBILE_OS,
        seed=base.seed,
        fonts=base.fonts, cameras=base.cameras, microphones=base.microphones,
        # A phone whose touch and pixel-ratio fields keep the dataclass
        # defaults (0 and 1.0) reports a touch screen it does not have on a
        # display no handset ships. Both are stored and reach the launch.
        max_touch_points=base.max_touch_points,
        device_pixel_ratio=base.device_pixel_ratio,
    )

    tags = _first(d, "tags", default=[]) or []
    if isinstance(tags, str):
        tags = [t.strip() for t in tags.split(",") if t.strip()]
    # After the string branch, so a comma-separated list is still read as one.
    # A tag list that is not a list is not iterable, and the `group not in tags`
    # below iterated it.
    tags = _usable(tags, [], "the tag list", issues)
    group = _first(d, "group_name", "group", "folder")
    if group and group not in tags:
        tags.append(str(group))

    prof = Profile(
        name=_first(d, "name", "title", "profile_name", default="imported profile"),
        fingerprint=fp,
        proxy=_parse_proxy(d),
        notes=_first(d, "notes", "description", default="") or "",
        tags=tags,
    )
    return prof, issues + validate(fp)


# --------------------------------------------------------------------------
# Numera's own export format
# --------------------------------------------------------------------------
# Everything above reads *somebody else's* file, and being tolerant about a
# format you have never seen is the right rule for that. Fed Numera's own
# export it is the wrong one: the heuristics read a fraction of the dashboard
# shape, generate a fresh fingerprint for the rest, and report a faithful
# import. So a native file says what it is, and is read as configuration
# instead of guessed at.
#
# Configuration, not a session: the browser profile directory - cookies,
# logins, local storage - is not in this file. `numera backup` is that.

#: The key that marks a Numera export, and the version this build writes.
NATIVE_KEY = "numeraProfiles"
NATIVE_VERSION = 1

#: How many markers below make an unversioned file a Numera export.
#:
#: A native export carries every one of them at the top level; a vendor export
#: nests its fingerprint under `navigator`, `fingerprint` or `webGLMetadata` -
#: which is why `_first` reads dotted paths - so its top level has almost none.
#: A few of the markers are ordinary DOM property names (`mediaDevices`,
#: `maxTouchPoints`, `devicePixelRatio`) that a flat vendor dump could plausibly
#: use, and refusing somebody's real vendor export is the worse mistake here, so
#: the bar is well above a coincidence and well below a native file's 19.
_NATIVE_MARKERS_NEEDED = 6

#: Keys a Numera dashboard profile has at its top level. Used only to recognise
#: an *unversioned* native file - one written before the format was versioned -
#: so it can be refused by name rather than silently mangled by the heuristics
#: above.
_NATIVE_MARKERS = frozenset({
    "webglVendor", "webglRenderer", "clientRects", "webglImage",
    "mediaDevices", "mediaVideo", "mediaAudio", "mediaSpeakers", "fontList",
    "startupUrls", "launchArgs", "clearOnStart", "switchedOff", "ipBased",
    "devicePixelRatio", "maxTouchPoints", "osVersion", "deviceModel",
    "lastActive",
})


def native_export(data) -> Optional[list]:
    """The profile list from a versioned Numera export, or None if it is not one.

    Raises ``ValueError`` when the file *claims* to be one and this build
    cannot read it - a version from the future or a broken envelope. Refusing
    is the point: the alternative is reading a newer file with older rules and
    reporting that as success.
    """
    if not isinstance(data, dict) or NATIVE_KEY not in data:
        return None
    version = data.get(NATIVE_KEY)
    if version != NATIVE_VERSION:
        raise ValueError(
            f"this file says it is Numera export format {version!r}; this build "
            f"reads format {NATIVE_VERSION}. "
            + ("It was written by a newer Numera - update this install, or "
               "re-export from that one."
               if isinstance(version, int) and version > NATIVE_VERSION else
               "Re-export it from the install that wrote it."))
    profiles = data.get("profiles")
    if not isinstance(profiles, list) or not all(isinstance(p, dict) for p in profiles):
        raise ValueError("a Numera export must carry 'profiles' as a list of objects")
    return profiles


def looks_native(entry) -> bool:
    """Is this a Numera dashboard profile with no format version on it?"""
    if not isinstance(entry, dict):
        return False
    return len(_NATIVE_MARKERS & set(entry)) >= _NATIVE_MARKERS_NEEDED


def _unwrap(data) -> list[dict]:
    """Pull the list of profiles out of whatever the export wraps them in."""
    if isinstance(data, list):
        return [x for x in data if isinstance(x, dict)]
    if isinstance(data, dict):
        for key in ("data", "profiles", "list", "items", "rows"):
            inner = data.get(key)
            if isinstance(inner, list):
                return [x for x in inner if isinstance(x, dict)]
        return [data]   # a single profile object
    return []


def load_export(path: str | Path) -> list[dict]:
    """The vendor profiles in ``path``.

    Refuses Numera's own export, versioned or not. This is the door the CLI's
    `import-from` uses, and without the check it read a native file with the
    heuristics above and reported a lossy reinterpretation as a successful
    import - the same defect the API import route had, through a different
    door. `cmd_import_from` turns this into "Couldn't read <file>: ...".
    """
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    entries = _unwrap(data)
    if native_export(data) is not None or any(looks_native(e) for e in entries):
        raise ValueError(
            "this is a Numera profile export, not a vendor one, and reading it "
            "with the vendor importer would drop its fingerprint seed, signal "
            "modes, startup URLs, extensions and folder. Import it from the "
            "dashboard, which reads the native format field for field.")
    return entries


def import_file(path: str | Path, store) -> tuple:
    """Import every profile in ``path`` into ``store``.

    Returns ``(imported, skipped)``: (profile, issues) pairs for what was
    written, so the caller can report what came in and what needs a look, and
    the names that were already taken.

    Whole file or nothing, for failures. Saving as it went left a partial
    import when one entry failed - and a partial import is the worst shape to
    recover from, because re-running it duplicates whatever did land.

    Names are the stated exception, and the policy is skip-and-report. A vendor
    export arrives with names of its own; creating a second profile under a
    name this store already uses would make ``numera <name>`` resolve to one of
    two profiles with nothing ever saying which. Skipping means importing the
    same file twice adds nothing the second time, and one clash in fifty
    profiles does not refuse the other forty-nine. Rename the existing profile
    and re-import if the incoming one was the one you wanted.
    """
    from .repo import repo

    parsed = [profile_from_export(raw) for raw in load_export(path)]
    written = {prof.id for prof in
               repo(store).save_many([prof for prof, _issues in parsed],
                                     on_name_clash="skip")}
    imported = [(prof, issues) for prof, issues in parsed if prof.id in written]
    skipped = [prof.name for prof, _issues in parsed if prof.id not in written]
    return imported, skipped
