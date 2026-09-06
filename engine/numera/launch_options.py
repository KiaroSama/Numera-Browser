"""What a profile *means* to a browser, before any backend is chosen.

A `Profile` is an identity; a browser takes command-line flags, a proxy, a
geolocation, a user-data directory. Everything that performs that translation
lives here - the DNS resolver as a DoH endpoint, WebRTC policy, the switches
that stop Chromium phoning home, the stores a profile is set not to carry
between launches, its bookmarks, its unpacked extensions, and the fields that
follow the proxy's country rather than the profile's own claim.

None of it knows which engine will be launched, which is the point: `drivers.py`
picks the backend and drives it, and asks this module what to hand it. The two
were one file and read as one job; they are not, and the seam is that nothing
here imports a backend.

`drivers` re-imports these names, so `numera.drivers._dns_args` and a test that
patches `drivers._seed_bookmarks` both still resolve - the call sites are in
`drivers`' own namespace and Python binds them at call time.
"""

from __future__ import annotations

import json
import time
from dataclasses import replace
from pathlib import Path
from typing import Optional

from . import devices as fp_mod_devices
from .models import Profile
from .relay import ProxyRelay


def _open_startup_urls(context, page, profile: Profile) -> None:
    """Navigate the first tab to the profile's first startup URL and open the
    rest in new tabs. A bad URL is skipped rather than aborting the launch."""
    urls = getattr(profile, "startup_urls", None) or []
    if not urls:
        return
    for i, url in enumerate(urls):
        try:
            target = page if i == 0 else context.new_page()
            target.goto(url, wait_until="domcontentloaded", timeout=30000)
        except Exception:
            continue  # unreachable site / typo — don't crash the session


def _geolocation_for(profile: Profile) -> Optional[dict]:
    """Coordinates that agree with the profile's proxy country (or its locale
    when there's no proxy), so navigator.geolocation doesn't contradict the IP."""
    country = None
    if profile.proxy and profile.proxy.country:
        country = profile.proxy.country.upper()
    if not country:
        country = fp_mod_devices.LOCALE_TO_COUNTRY.get(profile.fingerprint.locale)
    coords = fp_mod_devices.COUNTRY_GEO.get(country or "")
    if not coords:
        return None
    lat, lon = coords
    return {"latitude": lat, "longitude": lon, "accuracy": 60}


def _webrtc_arg(profile: Profile) -> list[str]:
    """Stop WebRTC from leaking the real IP. With a proxy, force everything
    through it (or don't send UDP at all); otherwise expose only the public
    interface so local addresses stay hidden."""
    if profile.proxy:
        return ["--force-webrtc-ip-handling-policy=disable_non_proxied_udp"]
    return ["--force-webrtc-ip-handling-policy=default_public_interface_only"]


def _browser_proxy(profile: Profile):
    """The proxy config to hand the browser, and the relay behind it (or ``None``).

    A proxy with credentials is forwarded through a local unauthenticated relay
    rather than given to the browser directly. Every engine can point at
    ``http://127.0.0.1:<port>`` with nothing to authenticate, which is the only
    thing they all agree on: fingerprint-chromium has no password support on
    ``--proxy-server``, Firefox cannot do authenticated SOCKS, and Chromium
    refuses SOCKS authentication outright (BUGS.md B4). See relay.py.
    """
    if not profile.proxy:
        return None, None
    if not (profile.proxy.username or profile.proxy.password):
        return profile.proxy.to_playwright(), None
    relay = ProxyRelay(profile.proxy).start()
    return {"server": relay.url}, relay


# Well-known resolver addresses mapped to their DNS-over-HTTPS endpoint.
# Chromium has no "use this nameserver" flag - the OS resolver owns that - but
# it does speak DoH, which is the stronger answer anyway: it takes DNS off the
# local network entirely, so the lookups can't leak beside the proxy.
DOH_TEMPLATES = {
    "1.1.1.1": "https://cloudflare-dns.com/dns-query",
    "1.0.0.1": "https://cloudflare-dns.com/dns-query",
    "8.8.8.8": "https://dns.google/dns-query",
    "8.8.4.4": "https://dns.google/dns-query",
    "9.9.9.9": "https://dns.quad9.net/dns-query",
    "149.112.112.112": "https://dns.quad9.net/dns-query",
    "94.140.14.14": "https://dns.adguard-dns.com/dns-query",
    "208.67.222.222": "https://doh.opendns.com/dns-query",
}


# Chromium subsystems a profile can switch off, and the flags that do it.
#
# Both are reachable by typing into the launch-arguments box; they are named
# switches because "stop Chrome phoning Google" is a thing people want, not a
# flag people remember. Deliberately no `--disable-features=` here: Chromium
# keeps only the *last* occurrence of that switch, so adding one would silently
# cancel the DNS-over-HTTPS flag.
SWITCHABLE = {
    # Sync, Safe Browsing updates, field trials, the extension updater and the
    # usage pings - all traffic to Google from the proxy's IP that has nothing to
    # do with what the profile is being used for.
    "googleServices": ["--disable-sync", "--disable-background-networking",
                       "--disable-client-side-phishing-detection", "--no-pings",
                       "--disable-domain-reliability"],
    # Chromium's own hidden extensions (the Web Store connector, Media Router,
    # Cloud Print). They run in the background whether or not anything uses them.
    "systemExtensions": ["--disable-component-extensions-with-background-pages",
                         "--disable-component-update"],
}


def _switched_off_args(profile: Profile) -> list[str]:
    return [flag for key in (getattr(profile, "switched_off", None) or [])
            for flag in SWITCHABLE.get(key, ())]


def _dns_args(profile: Profile) -> list[str]:
    """Force this session's DNS through the profile's resolver.

    Raises rather than launching without it: a profile configured to resolve
    through Cloudflare that quietly resolves through the ISP is a leak the user
    believes they closed, which is worse than a failed launch they can see.
    """
    dns = (getattr(profile, "dns", "") or "").strip()
    if not dns:
        return []
    template = dns if dns.startswith("https://") else DOH_TEMPLATES.get(dns)
    if not template:
        raise ValueError(
            f"{dns!r} is not a DNS-over-HTTPS resolver. Chromium cannot be pointed "
            f"at a plain nameserver; give a DoH URL (https://...) or one of: "
            f"{', '.join(sorted(DOH_TEMPLATES))}.")
    return ["--enable-features=DnsOverHttps",
            f"--dns-over-https-templates={template}",
            "--dns-over-https-mode=secure"]


# What each "don't carry this over" switch has to remove. Paths are relative to
# the profile directory Chromium actually uses; the journal/companion files come
# too, because leaving one behind gets the store rebuilt from it.
CLEARABLE = {
    "history": ["Default/History", "Default/History-journal",
                "Default/Top Sites", "Default/Top Sites-journal",
                "Default/Visited Links"],
    "passwords": ["Default/Login Data", "Default/Login Data-journal",
                  "Default/Login Data For Account",
                  "Default/Login Data For Account-journal"],
    "cookies": ["Default/Network/Cookies", "Default/Network/Cookies-journal"],
    "localStorage": ["Default/Local Storage"],
    "indexedDb": ["Default/IndexedDB"],
    "sessions": ["Default/Sessions", "Default/Current Session",
                 "Default/Current Tabs", "Default/Last Session",
                 "Default/Last Tabs"],
    "extensionData": ["Default/Local Extension Settings",
                      "Default/Extension State", "Default/Sync Extension Settings"],
    "bookmarks": ["Default/Bookmarks", "Default/Bookmarks.bak"],
}


def _clear_unkept(profile: Profile, user_data_dir) -> None:
    """Drop the stores this profile is set not to carry between launches.

    Cleared on the way *in* rather than on the way out: there is exactly one
    place to do it, and a session that ends by crashing still starts clean next
    time. Chromium recreates each of these on demand, so removing them is a
    reset, not damage.
    """
    import shutil

    root = Path(user_data_dir)
    for key in (getattr(profile, "clear_on_start", None) or []):
        for rel in CLEARABLE.get(key, ()):
            target = root / rel
            if target.is_dir():
                shutil.rmtree(target, ignore_errors=True)
            elif target.exists():
                try:
                    target.unlink()
                except OSError:
                    pass  # locked by a session still shutting down; next launch gets it


def _seed_bookmarks(profile: Profile, user_data_dir) -> None:
    """Merge the profile's bookmarks into Chromium's bookmark bar.

    Merge, not overwrite: anything the user bookmarked inside the browser stays,
    and editing the list in Numera still takes effect. Matched by URL, so
    re-launching doesn't stack duplicates.

    Written before launch because Chromium reads this file at startup and
    rewrites it on exit - touching it while the browser is open loses the edit.
    """
    wanted = [b for b in (getattr(profile, "bookmarks", None) or [])
              if isinstance(b, dict) and b.get("url")]
    if not wanted:
        return
    path = Path(user_data_dir) / "Default" / "Bookmarks"
    path.parent.mkdir(parents=True, exist_ok=True)

    try:
        doc = json.loads(path.read_text(encoding="utf-8"))
        bar = doc["roots"]["bookmark_bar"]
        children = bar.setdefault("children", [])
    except Exception:  # noqa: BLE001 - absent or unreadable: start a fresh file
        children = []
        bar = {"children": children, "date_added": "0", "id": "1",
               "name": "Bookmarks bar", "type": "folder"}
        doc = {"roots": {"bookmark_bar": bar,
                         "other": {"children": [], "date_added": "0", "id": "2",
                                   "name": "Other bookmarks", "type": "folder"},
                         "synced": {"children": [], "date_added": "0", "id": "3",
                                    "name": "Mobile bookmarks", "type": "folder"}},
               "version": 1}

    have = {c.get("url") for c in children if isinstance(c, dict)}
    # Chromium timestamps are microseconds since 1601-01-01, not the Unix epoch.
    stamp = str(int((time.time() + 11644473600) * 1_000_000))
    next_id = max((int(c.get("id", 0)) for c in children if str(c.get("id", "")).isdigit()),
                  default=int(bar.get("id", 1)))
    for b in wanted:
        if b["url"] in have:
            continue
        next_id += 1
        children.append({"date_added": stamp, "id": str(next_id), "type": "url",
                         "name": b.get("name") or b["url"], "url": b["url"]})
    # No "checksum" key: Chromium recomputes a missing one, but rejects a wrong
    # one and resets the file - which would silently drop the user's own marks.
    doc.pop("checksum", None)
    path.write_text(json.dumps(doc, indent=2), encoding="utf-8")


def _resolve_ip_based(profile: Profile) -> Profile:
    """Point the signals the user marked "based on IP" at the real exit country.

    A stored timezone is a guess made when the profile was created; the proxy it
    ends up using may exit somewhere else entirely, and a browser whose clock
    disagrees with its IP is the cheapest possible tell. Costs one request
    through the proxy at launch, and only when something is actually marked.
    """
    wanted = set(getattr(profile, "ip_based", None) or [])
    if not wanted or not profile.proxy:
        return profile
    from . import proxypool

    try:
        country = proxypool.exit_country(profile.proxy)
    except Exception:  # noqa: BLE001 - a launch must not fail on a lookup
        country = None
    if not country:
        return profile

    fp = profile.fingerprint
    changes: dict = {}
    locale = fp_mod_devices.COUNTRY_TO_LOCALE.get(country)
    if locale and locale in fp_mod_devices.LOCALES:
        tz, langs = fp_mod_devices.LOCALES[locale]
        if "timezone" in wanted:
            changes["timezone"] = tz
        if "languages" in wanted:
            changes["locale"] = locale
            changes["languages"] = list(langs)
    if "geolocation" in wanted and country in fp_mod_devices.COUNTRY_GEO:
        # The coordinates themselves live on the proxy, which is what
        # `_geolocation_for` already reads first.
        profile = replace(profile, proxy=replace(profile.proxy, country=country))
    return replace(profile, fingerprint=replace(fp, **changes)) if changes else profile


def _extension_args(profile: Profile) -> list[str]:
    """Chromium flags that load the profile's unpacked extensions.

    Both flags are required: ``--load-extension`` alone still leaves every other
    extension enabled, which would leak identity between profiles.
    """
    paths = [str(Path(p).expanduser().resolve())
             for p in (getattr(profile, "extensions", None) or [])]
    paths = [p for p in paths if Path(p).is_dir()]
    if not paths:
        return []
    joined = ",".join(paths)
    return [f"--disable-extensions-except={joined}", f"--load-extension={joined}"]
