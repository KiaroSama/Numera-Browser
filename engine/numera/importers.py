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


def profile_from_export(d: dict) -> tuple[Profile, list[str]]:
    """Map one exported profile dict to a Numera :class:`Profile`.

    Returns the profile plus the list of coherence issues ``validate()`` found,
    so the caller can surface what needs a look.
    """
    ua = _first(d, "navigator.userAgent", "navigator.user_agent", "user_agent",
                "ua", "useragent", "fingerprint.ua", "fingerprint.userAgent",
                "browser.userAgent", "userAgent", default="") or ""
    os_key = _norm_os(_first(d, "os", "os_type", "osType", "navigator.platform"), ua)

    # Start from a coherent base for this OS, then overlay whatever the export
    # actually specified. Gaps stay coherent instead of becoming holes.
    base = generate(os=os_key)

    sw, sh = _parse_resolution(_first(d, "navigator.resolution", "screen_resolution",
                                      "resolution", "screen", default=""))
    locale = _first(d, "navigator.language", "language", "locale", default=base.locale)
    if locale not in devices.LOCALES:
        locale = base.locale
    tz = _first(d, "timezone.timezone", "timezone", "time_zone",
                "fingerprint.timezone", default=devices.LOCALES[locale][0])

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
    return prof, validate(fp)


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
    return _unwrap(json.loads(Path(path).read_text(encoding="utf-8")))


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
