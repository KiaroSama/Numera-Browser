"""Translation between the dashboard's profile shape and the engine's.

The dashboard edits a flat, form-shaped object; the engine wants a
`Profile` with a coherent `Fingerprint` inside it. Everything that maps
one to the other lives here, in one place, because a field that only one
direction knows about is a control that silently does nothing - the
single most repeated bug in this project."""

from __future__ import annotations

import hashlib
import math
import os
import time
from typing import Optional
from . import devices
from .fingerprint import _build_user_agent, validate
from .models import Fingerprint, Profile, Proxy


# --------------------------------------------------------------------------
# OS name mapping between the dashboard ("Windows") and engine ("windows").
# --------------------------------------------------------------------------
OS_TO_ENGINE = {"Windows": "windows", "macOS": "macos", "Linux": "linux",
                "Android": "android", "iOS": "ios"}
OS_TO_DASH = {v: k for k, v in OS_TO_ENGINE.items()}

def _vendor_for(renderer: str) -> str:
    """Guess a WebGL vendor string from a renderer string's GPU family."""
    r = (renderer or "").lower()
    for token, name in (("nvidia", "NVIDIA"), ("radeon", "AMD"), ("amd", "AMD"),
                        ("intel", "Intel"), ("apple", "Apple"),
                        ("adreno", "Qualcomm"), ("qualcomm", "Qualcomm"),
                        ("mali", "ARM"), ("arm", "ARM")):
        if token in r:
            return f"Google Inc. ({name})"
    return "Google Inc."


def _engine_webgl(os_key: str, dash_renderer: str, dash_vendor: str = "") -> tuple[str, str]:
    """Resolve a dashboard profile's GPU into an engine WebGL vendor/renderer.

    The dashboard string is passed through as-is (not remapped to the curated
    pool), so a real GPU written by `numera align` survives the round-trip and
    the profile keeps matching its browser. Only a blank renderer falls back to
    a curated default."""
    renderer = (dash_renderer or "").strip()
    vendor = (dash_vendor or "").strip()
    if not renderer:
        return (devices.WEBGL.get(os_key) or devices.WEBGL["windows"])[0]
    return (vendor or _vendor_for(renderer)), renderer


def _parse_lines(raw) -> list[str]:
    """Accept a newline-separated string (or a list) and return clean entries.

    Anything else is *no* entries rather than an exception. The API refuses a
    `fontList: 17` on the way in, but a record written before it did is still
    on disk, and this function is what every read of that record runs through -
    the Health page, the collision report and the launcher all called it and
    all raised `AttributeError` on the same record, so one bad profile made
    those pages unreachable for every other profile too.
    """
    if isinstance(raw, str):
        parts = raw.splitlines()
    elif isinstance(raw, list):
        parts = raw
    else:
        return []
    return [p.strip() for p in parts if isinstance(p, str) and p.strip()]


def _str_list(raw) -> list[str]:
    """The string entries of a list field, and nothing else.

    Same reason as `_parse_lines`: `k in raw` raises for a legacy `ipBased: 17`
    and `list(tags)` over `[{}]` carries an object into every consumer.
    """
    return [v for v in raw if isinstance(v, str)] if isinstance(raw, list) else []


def _exact(value):
    """`int` when the number is whole, `float` when it genuinely is not.

    For a field that is a number rather than a count. `int` was the only cast
    here, and `int(0.25)` is `0` - so a dashboard reporting a quarter-gigabyte
    device stored zero, passed every later guard because zero is a legal
    amount, and round-tripped as zero. Silent truncation to a value the caller
    never sent is not normalization.

    Whole values stay `int` so nothing that already round-trips starts writing
    `16.0` where it wrote `16`.
    """
    out = float(value)
    return int(out) if out.is_integer() else out


def _num(value, default, cast=int):
    """A finite number of the wanted kind, or `default`.

    `int(None)`, `int("")` and `int(float("nan"))` all raise, and
    `int(float("inf"))` raises `OverflowError` - which the one `try` in this
    module did not catch. Non-finite values get the default rather than a
    conversion, because `NaN` is not JSON and cannot be written back out.
    """
    if isinstance(value, bool) or value is None:
        return default
    try:
        out = cast(value)
    except (TypeError, ValueError, OverflowError):
        return default
    # `math.isfinite` converts to float first, so it raises on an int too large
    # for one - and it raises *here*, outside the guard above, because the cast
    # itself succeeded. An int has no infinities to test for, so asking is both
    # unnecessary and the thing that breaks.
    if isinstance(out, int):
        return out
    return out if math.isfinite(out) else default


# Kept in step with drivers.CLEARABLE by test_launch_extras, so an unknown key
# from a client cannot reach the profile and silently do nothing.
CLEARABLE_KEYS = ("history", "passwords", "cookies", "localStorage",
                  "indexedDb", "sessions", "extensionData", "bookmarks")
SWITCHABLE_KEYS = ("googleServices", "systemExtensions")


def _parse_bookmarks(raw) -> list[dict]:
    """One bookmark per line, as ``Name | https://url`` or a bare URL.

    A textarea rather than a row editor because the list is usually pasted, and
    the separator is a pipe because a title may contain a comma but rarely one
    of these.
    """
    out = []
    for line in _parse_lines(raw):
        name, sep, url = line.rpartition("|")
        url = (url if sep else line).strip()
        if url:
            out.append({"name": name.strip() or url, "url": url})
    return out


def _split_legacy_commas(line: str) -> list[str]:
    """Split one line only where a comma cannot belong to the URL before it.

    There is exactly one such place: the entry so far is still a scheme and a
    host, because no host name contains a comma. Past that point the URL is
    opaque - a comma in a path, query or fragment is data, and so is a colon,
    which is why what the *next* chunk looks like decides nothing. Reading a
    scheme-shaped chunk as a separator truncated every URL carrying another URL
    in its query (``...?targets=https://a.test,https://b.test``) and opened its
    tail as a second tab, and made a host out of the path segment in
    ``/red,blue:green``.
    """
    entries: list[str] = []
    for chunk in line.split(","):
        still_a_host = entries and not any(
            c in entries[-1].split("://", 1)[-1] for c in "/?#")
        if entries and not still_a_host:
            entries[-1] += "," + chunk
        else:
            entries.append(chunk)
    return entries


def _parse_startup_urls(raw) -> list[str]:
    """One URL per line (or per list entry), with a scheme added where one is
    missing.

    Every comma used to become a newline before splitting. That reads the
    legacy ``a.test,b.test`` this field once accepted, and corrupts every URL
    carrying a comma of its own: ``?q=red,blue`` became two startup tabs, the
    second of them ``https://blue``. The legacy form still splits where it
    cannot be one URL; see :func:`_split_legacy_commas`.

    That legacy form belongs to the textarea alone. A caller that sent a *list*
    has already said where each entry ends, so its elements are taken whole -
    splitting one overrides an answer this function was given rather than
    filling in one it was not.
    """
    lines = _parse_lines(raw)
    if isinstance(raw, str):
        lines = [u for line in lines for u in _split_legacy_commas(line)]
    urls = []
    for u in lines:
        u = u.strip()
        if not u:
            continue
        if "://" not in u and not u.startswith("about:"):
            u = "https://" + u
        urls.append(u)
    return urls


def _stable_seed(pid: str) -> int:
    """A seed derived from the profile id, for a profile that carries none.

    The seed drives canvas, audio, ClientRects and media-device noise, so it has
    to be the *same* on every launch — that is the whole promise of a profile
    looking identical each time it opens. The dashboard has never sent one, and
    the old fallback minted a fresh random seed on every conversion, which meant
    every launch of a dashboard profile presented a different canvas hash.
    Deriving it from the id fixes that for profiles already on disk without a
    migration, and an explicit ``seed`` still wins when one is present.
    """
    return int(hashlib.sha256(pid.encode("utf-8")).hexdigest()[:8], 16) or 1


def to_engine_profile(d: dict) -> Profile:
    """Convert a stored dashboard profile dict into an engine :class:`Profile`."""
    os_key = OS_TO_ENGINE.get(d.get("os", "Windows"), "windows")
    is_mobile = os_key in devices.MOBILE_OS

    try:
        sw, sh = (int(x) for x in str(d.get("screen", "1920x1080")).lower().split("x"))
    except ValueError:
        sw, sh = 1920, 1080
    if is_mobile:
        vw, vh = sw, max(1, sh - (113 if os_key == "ios" else 80))
    else:
        vw, vh = sw, max(1, sh - 88)

    locale = d.get("locale", "en-US")
    tz, langs = devices.LOCALES.get(locale, devices.LOCALES["en-US"])
    vendor, renderer = _engine_webgl(os_key, d.get("webglRenderer", ""), d.get("webglVendor", ""))
    os_version = str(d.get("osVersion") or devices.OS_VERSIONS[os_key][0][0])
    # iOS has no navigator.deviceMemory to report, whatever the profile says.
    # `_exact`, not `int`: `navigator.deviceMemory` is a `double` and the
    # values a real browser reports include 0.25 and 0.5. Casting to `int`
    # turned both into 0.
    memory = 0 if os_key == "ios" else _num(d.get("memory"), 16, cast=_exact)

    fp = Fingerprint(
        os=os_key,
        user_agent=(d.get("userAgent")
                    or _build_user_agent(os_key, str(d.get("browserVersion", "128.0.0.0")),
                                         os_version, str(d.get("deviceModel") or ""))),
        platform=devices.NAV_PLATFORM[os_key],
        ch_platform=devices.CH_PLATFORM[os_key],
        chrome_version=d.get("browserVersion", "128.0.0.0"),
        screen_width=sw,
        screen_height=sh,
        viewport_width=vw,
        viewport_height=vh,
        hardware_concurrency=_num(d.get("cores"), 8),
        device_memory=memory,
        # An edited font list is the profile's own; only fall back to the whole
        # OS set when the dashboard sent nothing.
        fonts=_parse_lines(d.get("fontList")) or devices.FONTS.get(os_key, []),
        cameras=_num(d.get("mediaVideo"), 1),
        microphones=_num(d.get("mediaAudio"), 1),
        speakers=_num(d.get("mediaSpeakers"), 1),
        # These pickers used to be cosmetic - every one is carried through now.
        webrtc=str(d.get("webrtc") or "Altered").strip().lower(),
        canvas=str(d.get("canvas") or "Noise").strip().lower(),
        audio=str(d.get("audio") or "Noise").strip().lower(),
        client_rects=str(d.get("clientRects") or "Noise").strip().lower(),
        webgl_image=str(d.get("webglImage") or "Noise").strip().lower(),
        media_mode=str(d.get("mediaDevices") or "Masked").strip().lower(),
        font_mode=str(d.get("fonts") or "Masked").strip().lower(),
        geolocation=str(d.get("geo") or "Prompt").strip().lower(),
        do_not_track=bool(d.get("dnt")),
        webgl_vendor=vendor,
        webgl_renderer=renderer,
        locale=locale,
        timezone=d.get("timezone") or tz,
        languages=langs,
        is_mobile=is_mobile,
        os_version=os_version,
        device_model=str(d.get("deviceModel") or ""),
        max_touch_points=_num(d.get("maxTouchPoints"),
                              devices.TOUCH_POINTS[os_key][0]),
        device_pixel_ratio=_num(d.get("devicePixelRatio"),
                                devices.PIXEL_RATIOS[os_key][0], float)
        or devices.PIXEL_RATIOS[os_key][0],
        seed=_num(d.get("seed"), 0) or _stable_seed(str(d.get("id") or "unsaved")),
    )

    proxy = None
    px = d.get("proxy")
    if isinstance(px, dict) and px.get("host"):
        # One mapping, and it refuses rather than guesses. The old dict had no
        # SOCKS4 entry and fell back to "http", so a SOCKS4 profile launched a
        # browser speaking a different protocol than the profile claimed.
        from .proxypool import server_url

        # `from_storage`: this password came out of the dashboard record, so it
        # is the stored one until a caller assigns to it. Without that, every
        # profile converted for a read looked like a deliberate replacement.
        proxy = Proxy(
            server=server_url(px.get("type", "HTTP"), px["host"], px.get("port", "")),
            username=px.get("user") or None,
            password=px.get("pass") or None,
            country=px.get("country") or None,
        ).from_storage()

    return Profile(
        id=d.get("id") or "unsaved",
        name=d.get("name", "profile"),
        fingerprint=fp,
        proxy=proxy,
        notes=d.get("notes", ""),
        tags=_str_list(d.get("tags")),
        engine=d.get("engine", "playwright"),
        startup_urls=_parse_startup_urls(d.get("startupUrls")),
        extensions=_parse_lines(d.get("extensions")),
        launch_args=_parse_lines(d.get("launchArgs")),
        bookmarks=_parse_bookmarks(d.get("bookmarks")),
        dns=str(d.get("dns") or "").strip(),
        ip_based=[k for k in ("timezone", "geolocation", "languages")
                  if k in _str_list(d.get("ipBased"))],
        clear_on_start=[k for k in CLEARABLE_KEYS
                        if k in _str_list(d.get("clearOnStart"))],
        switched_off=[k for k in SWITCHABLE_KEYS
                      if k in _str_list(d.get("switchedOff"))],
    )


def _proxy_to_dash(proxy) -> Optional[dict]:
    """Turn an engine Proxy into the dashboard's proxy dict."""
    if not proxy or not proxy.server:
        return None
    from .proxypool import _SCHEME_TYPE, split_server

    scheme, host, port = split_server(proxy.server)
    kind = _SCHEME_TYPE.get(scheme, "HTTP")
    return {"type": kind, "host": host, "port": port,
            "user": proxy.username or "", "pass": proxy.password or "",
            "country": proxy.country or ""}


def to_dashboard_profile(prof: Profile) -> dict:
    """Convert an engine :class:`Profile` into a dashboard-shaped dict.

    The reverse of :func:`to_engine_profile` — used when the engine produces a
    profile (an import) that the dashboard then owns and displays."""
    fp = prof.fingerprint
    return {
        "name": prof.name,
        "folder": "Facebook Ads",
        "status": "stopped",
        "os": OS_TO_DASH.get(fp.os, "Windows"),
        "engine": prof.engine or "playwright",
        "browser": "Chrome",
        "browserVersion": fp.chrome_version,
        "userAgent": fp.user_agent,
        "screen": f"{fp.screen_width}x{fp.screen_height}",
        "cores": fp.hardware_concurrency,
        "memory": fp.device_memory,
        "webglVendor": fp.webgl_vendor,
        "webglRenderer": fp.webgl_renderer,
        "canvas": fp.canvas.capitalize(), "webrtc": fp.webrtc.capitalize(),
        "audio": fp.audio.capitalize(), "fonts": fp.font_mode.capitalize(),
        "clientRects": fp.client_rects.capitalize(),
        "webglImage": fp.webgl_image.capitalize(),
        "mediaDevices": fp.media_mode.capitalize(),
        "locale": fp.locale, "timezone": fp.timezone,
        "geo": fp.geolocation.capitalize(),
        "mediaVideo": fp.cameras, "mediaAudio": fp.microphones,
        "mediaSpeakers": fp.speakers,
        "fontList": "\n".join(fp.fonts or []),
        "osVersion": fp.os_version,
        "deviceModel": fp.device_model,
        "maxTouchPoints": fp.max_touch_points,
        "devicePixelRatio": fp.device_pixel_ratio,
        # Without this the fingerprint's noise seed was dropped on import and a
        # fresh one was invented at every launch, so an imported profile never
        # looked the same twice.
        "seed": fp.seed,
        "dnt": fp.do_not_track,
        "proxy": _proxy_to_dash(prof.proxy),
        "notes": prof.notes or "",
        "tags": list(prof.tags),
        "startupUrls": "\n".join(prof.startup_urls or []),
        "extensions": "\n".join(prof.extensions or []),
        "launchArgs": "\n".join(prof.launch_args or []),
        "bookmarks": "\n".join(
            f"{b['name']} | {b['url']}" if b.get("name") not in (None, "", b["url"]) else b["url"]
            for b in (prof.bookmarks or []) if b.get("url")),
        "dns": prof.dns or "",
        "ipBased": list(prof.ip_based or []),
        "clearOnStart": list(prof.clear_on_start or []),
        "switchedOff": list(prof.switched_off or []),
        "lastActive": "never",
    }


def _rel_time(ts: Optional[float]) -> str:
    """Human 'last active' label from a unix timestamp."""
    if not ts:
        return "never"
    secs = max(0.0, time.time() - ts)
    if secs < 60:
        return "just now"
    mins = secs / 60
    if mins < 60:
        return f"{int(mins)}m ago"
    hours = mins / 60
    if hours < 24:
        return f"{int(hours)}h ago"
    return f"{int(hours / 24)}d ago"


def dashboard_coherence(d: dict) -> list[str]:
    """Run the real engine validator against a dashboard profile."""
    try:
        return validate(to_engine_profile(d).fingerprint)
    except Exception as e:  # pragma: no cover - defensive
        return [f"could not validate: {e}"]
