"""Data models for fingerprints and profiles."""

from __future__ import annotations

import time
import uuid
from dataclasses import dataclass, field, asdict, fields
from typing import Optional


@dataclass
class Fingerprint:
    """A complete, internally-consistent browser fingerprint.

    Every field is chosen so the parts agree with one another (see
    ``fingerprint.generate``). The ``seed`` makes canvas/audio noise
    deterministic, so the same profile looks identical across launches.
    """

    os: str
    user_agent: str
    platform: str
    ch_platform: str
    chrome_version: str
    screen_width: int
    screen_height: int
    viewport_width: int
    viewport_height: int
    hardware_concurrency: int
    device_memory: int
    webgl_vendor: str
    webgl_renderer: str
    locale: str
    timezone: str
    languages: list[str]
    is_mobile: bool = False
    seed: int = 0
    # Secondary signals. They have defaults so profiles saved by older versions
    # still load; ``generate()`` fills them from the device archetype.
    fonts: list[str] = field(default_factory=list)   # installed-font probe answers
    cameras: int = 1                                  # enumerateDevices() videoinput
    microphones: int = 1                              # enumerateDevices() audioinput
    # How WebRTC may behave. Chromium's own proxy flag does not reliably stop
    # ICE from handing out a non-proxied public address, so the in-page layer
    # has to finish the job: "altered" drops those candidates, "disabled"
    # removes RTCPeerConnection outright, "real" leaves WebRTC untouched.
    webrtc: str = "altered"
    # The rest of the anti-detect surfaces the dashboard exposes. Each one used
    # to be hardcoded in the patch layer, so picking a mode changed nothing.
    canvas: str = "noise"        # noise | block | real
    audio: str = "noise"         # noise | real
    font_mode: str = "masked"    # masked | real -- distinct from `fonts`, the list
    geolocation: str = "prompt"  # prompt | allow | block
    do_not_track: bool = False
    client_rects: str = "noise"  # noise | real
    webgl_image: str = "noise"   # noise | real -- the rendered pixels, not the
                                 # vendor/renderer strings above
    media_mode: str = "masked"   # masked | real -- whether enumerateDevices is
                                 # answered from the profile at all
    speakers: int = 1            # enumerateDevices() audiooutput
    # Which release of `os` this profile claims. Desktop Chrome freezes the OS
    # token in the UA string, so on Windows and macOS this only shows up in the
    # UA-CH platformVersion hint; on Android and iOS it is in the UA itself.
    os_version: str = ""
    device_model: str = ""       # Android handset name in the UA string
    max_touch_points: int = 0
    device_pixel_ratio: float = 1.0

    def to_dict(self) -> dict:
        return asdict(self)

    @classmethod
    def from_dict(cls, d: dict) -> "Fingerprint":
        known = {f.name for f in fields(cls)}
        return cls(**{k: v for k, v in d.items() if k in known})


@dataclass
class Proxy:
    """Proxy configuration for a profile."""

    server: str  # e.g. "http://host:port" or "socks5://host:port"
    username: Optional[str] = None
    password: Optional[str] = None
    country: Optional[str] = None  # ISO code, used to align locale/timezone

    def to_playwright(self) -> dict:
        cfg: dict = {"server": self.server}
        if self.username:
            cfg["username"] = self.username
        if self.password:
            cfg["password"] = self.password
        return cfg

    def to_dict(self) -> dict:
        return asdict(self)

    @classmethod
    def from_dict(cls, d: dict) -> "Proxy":
        # Filter to known fields, exactly as `Fingerprint.from_dict` does. This
        # was the asymmetric one: an extra key - from a newer version, a hand
        # edit, or a foreign backup - reached the constructor and raised
        # TypeError inside `ProfileStore.get()`, turning one odd record into a
        # 500 for the whole request.
        known = {f.name for f in fields(cls)}
        return cls(**{k: v for k, v in d.items() if k in known})


@dataclass
class Profile:
    """A named browser identity: fingerprint + proxy + metadata + storage."""

    name: str
    fingerprint: Fingerprint
    proxy: Optional[Proxy] = None
    notes: str = ""
    tags: list[str] = field(default_factory=list)
    engine: str = "playwright"  # which launch backend to use (see drivers.py)
    startup_urls: list[str] = field(default_factory=list)  # opened on launch
    extensions: list[str] = field(default_factory=list)  # unpacked extension dirs
    launch_args: list[str] = field(default_factory=list)  # extra Chromium flags
    bookmarks: list[dict] = field(default_factory=list)   # {"name", "url"} pairs
    dns: str = ""            # DNS-over-HTTPS resolver; see drivers._dns_args
    # Which identity signals follow the proxy's real exit IP instead of the
    # stored fingerprint. Any of "timezone", "geolocation", "languages".
    # Resolved at launch, because an IP's country is not knowable before then.
    ip_based: list[str] = field(default_factory=list)
    # Stores this profile does NOT carry between launches; see drivers.CLEARABLE.
    # Empty means full persistence, which is what every existing profile has.
    clear_on_start: list[str] = field(default_factory=list)
    # Chromium subsystems switched off at launch; see drivers.SWITCHABLE.
    switched_off: list[str] = field(default_factory=list)
    id: str = field(default_factory=lambda: uuid.uuid4().hex[:12])
    created_at: float = field(default_factory=time.time)
    updated_at: float = field(default_factory=time.time)

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "name": self.name,
            "notes": self.notes,
            "tags": self.tags,
            "engine": self.engine,
            "startup_urls": self.startup_urls,
            "extensions": self.extensions,
            "launch_args": self.launch_args,
            "bookmarks": self.bookmarks,
            "dns": self.dns,
            "ip_based": self.ip_based,
            "clear_on_start": self.clear_on_start,
            "switched_off": self.switched_off,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "fingerprint": self.fingerprint.to_dict(),
            "proxy": self.proxy.to_dict() if self.proxy else None,
        }

    @classmethod
    def from_dict(cls, d: dict) -> "Profile":
        return cls(
            id=d["id"],
            name=d["name"],
            notes=d.get("notes", ""),
            tags=d.get("tags", []),
            engine=d.get("engine", "playwright"),
            startup_urls=d.get("startup_urls", []),
            extensions=d.get("extensions", []),
            launch_args=d.get("launch_args", []),
            bookmarks=d.get("bookmarks", []),
            dns=d.get("dns", ""),
            ip_based=d.get("ip_based", []),
            clear_on_start=d.get("clear_on_start", []),
            switched_off=d.get("switched_off", []),
            created_at=d.get("created_at", time.time()),
            updated_at=d.get("updated_at", time.time()),
            fingerprint=Fingerprint.from_dict(d["fingerprint"]),
            proxy=Proxy.from_dict(d["proxy"]) if d.get("proxy") else None,
        )
