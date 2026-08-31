"""The launch-time profile settings actually reach Chromium.

Bookmarks, extra flags, DNS and "based on IP" are the four things the editor
grew for parity with the commercial tools. Each is only worth having if it
changes what the browser does, so each is asserted at the point it takes effect
rather than at the point it is stored.
"""
import json
from dataclasses import replace
from pathlib import Path

import pytest

from numera import devices, drivers
from numera.fingerprint import generate
from numera.models import Profile, Proxy


def profile(**kw) -> Profile:
    return Profile(name="t", fingerprint=generate("windows"), **kw)


# --- DNS -------------------------------------------------------------------

def test_a_known_resolver_ip_becomes_its_doh_endpoint():
    args = drivers._dns_args(profile(dns="1.1.1.1"))
    assert "--dns-over-https-templates=https://cloudflare-dns.com/dns-query" in args
    assert "--dns-over-https-mode=secure" in args


def test_a_doh_url_is_passed_through():
    args = drivers._dns_args(profile(dns="https://doh.example/dns-query"))
    assert "--dns-over-https-templates=https://doh.example/dns-query" in args


def test_no_dns_configured_adds_no_flags():
    assert drivers._dns_args(profile()) == []


def test_a_nameserver_chromium_cannot_use_is_refused_not_ignored():
    """The whole point is closing a leak; silently launching without it is worse
    than a failure the user can see."""
    with pytest.raises(ValueError) as e:
        drivers._dns_args(profile(dns="192.168.1.1"))
    assert "DNS-over-HTTPS" in str(e.value)


# --- bookmarks -------------------------------------------------------------

def read_bar(root: Path):
    doc = json.loads((root / "Default" / "Bookmarks").read_text(encoding="utf-8"))
    return doc["roots"]["bookmark_bar"]["children"]


def test_bookmarks_are_written_where_chromium_reads_them(tmp_path):
    p = profile(bookmarks=[{"name": "GitHub", "url": "https://github.com"}])
    drivers._seed_bookmarks(p, tmp_path)
    bar = read_bar(tmp_path)
    assert [(b["name"], b["url"], b["type"]) for b in bar] == [
        ("GitHub", "https://github.com", "url")]
    assert int(bar[0]["date_added"]) > 13_000_000_000_000_000  # 1601 epoch, µs


def test_relaunching_does_not_stack_duplicates(tmp_path):
    p = profile(bookmarks=[{"name": "GitHub", "url": "https://github.com"}])
    drivers._seed_bookmarks(p, tmp_path)
    drivers._seed_bookmarks(p, tmp_path)
    assert len(read_bar(tmp_path)) == 1


def test_a_bookmark_the_user_made_in_the_browser_survives(tmp_path):
    drivers._seed_bookmarks(profile(bookmarks=[{"name": "A", "url": "https://a.test"}]),
                            tmp_path)
    # Simulate the user adding one from inside Chromium, then editing the list
    # in Numera. Overwriting instead of merging would destroy their work.
    path = tmp_path / "Default" / "Bookmarks"
    doc = json.loads(path.read_text(encoding="utf-8"))
    doc["roots"]["bookmark_bar"]["children"].append(
        {"date_added": "0", "id": "99", "type": "url",
         "name": "Mine", "url": "https://mine.test"})
    path.write_text(json.dumps(doc), encoding="utf-8")

    drivers._seed_bookmarks(profile(bookmarks=[{"name": "A", "url": "https://a.test"},
                                               {"name": "B", "url": "https://b.test"}]),
                            tmp_path)
    urls = [b["url"] for b in read_bar(tmp_path)]
    assert urls == ["https://a.test", "https://mine.test", "https://b.test"]
    ids = [b["id"] for b in read_bar(tmp_path)]
    assert len(set(ids)) == len(ids), f"duplicate bookmark ids {ids}"


def test_no_bookmarks_leaves_the_file_alone(tmp_path):
    drivers._seed_bookmarks(profile(), tmp_path)
    assert not (tmp_path / "Default").exists()


def test_a_corrupt_bookmarks_file_is_replaced_rather_than_crashing_the_launch(tmp_path):
    (tmp_path / "Default").mkdir()
    (tmp_path / "Default" / "Bookmarks").write_text("{ not json", encoding="utf-8")
    drivers._seed_bookmarks(profile(bookmarks=[{"name": "A", "url": "https://a.test"}]),
                            tmp_path)
    assert [b["url"] for b in read_bar(tmp_path)] == ["https://a.test"]


def test_no_checksum_is_written(tmp_path):
    """A stale checksum makes Chromium discard the file and everything in it."""
    drivers._seed_bookmarks(profile(bookmarks=[{"name": "A", "url": "https://a.test"}]),
                            tmp_path)
    doc = json.loads((tmp_path / "Default" / "Bookmarks").read_text(encoding="utf-8"))
    assert "checksum" not in doc


# --- based on IP -----------------------------------------------------------

def test_timezone_and_languages_follow_the_real_exit_country(monkeypatch):
    monkeypatch.setattr("numera.proxypool.exit_country", lambda *a, **k: "DE")
    p = profile(proxy=Proxy(server="http://x:1", country="US"),
                ip_based=["timezone", "languages"])
    p = replace(p, fingerprint=replace(p.fingerprint, timezone="America/New_York",
                                       locale="en-US", languages=["en-US", "en"]))
    out = drivers._resolve_ip_based(p)
    assert out.fingerprint.timezone == devices.LOCALES["de-DE"][0]
    assert out.fingerprint.locale == "de-DE"
    assert out.fingerprint.languages[0] == "de-DE"


def test_only_the_marked_signals_move(monkeypatch):
    monkeypatch.setattr("numera.proxypool.exit_country", lambda *a, **k: "DE")
    p = profile(proxy=Proxy(server="http://x:1"), ip_based=["timezone"])
    p = replace(p, fingerprint=replace(p.fingerprint, locale="en-US",
                                       languages=["en-US", "en"]))
    out = drivers._resolve_ip_based(p)
    assert out.fingerprint.timezone == devices.LOCALES["de-DE"][0]
    assert out.fingerprint.locale == "en-US", "languages was not marked"


def test_geolocation_follows_the_country_the_proxy_actually_exits_from(monkeypatch):
    monkeypatch.setattr("numera.proxypool.exit_country", lambda *a, **k: "TR")
    p = profile(proxy=Proxy(server="http://x:1", country="US"),
                ip_based=["geolocation"])
    out = drivers._resolve_ip_based(p)
    assert out.proxy.country == "TR"
    lat, lon = devices.COUNTRY_GEO["TR"]
    assert drivers._geolocation_for(out)["latitude"] == pytest.approx(lat)
    assert drivers._geolocation_for(out)["longitude"] == pytest.approx(lon)


def test_nothing_marked_means_no_lookup(monkeypatch):
    """Otherwise every launch pays for a request nobody asked for."""
    def boom(*a, **k):
        raise AssertionError("looked up the exit IP with nothing marked")
    monkeypatch.setattr("numera.proxypool.exit_country", boom)
    p = profile(proxy=Proxy(server="http://x:1"))
    assert drivers._resolve_ip_based(p) is p


def test_an_unreachable_proxy_leaves_the_stored_identity_alone(monkeypatch):
    """A lookup that fails must not strip the profile of a timezone entirely."""
    monkeypatch.setattr("numera.proxypool.exit_country",
                        lambda *a, **k: (_ for _ in ()).throw(OSError("no route")))
    p = profile(proxy=Proxy(server="http://x:1"), ip_based=["timezone"])
    out = drivers._resolve_ip_based(p)
    assert out.fingerprint.timezone == p.fingerprint.timezone


# --- extra flags -----------------------------------------------------------

def test_the_profiles_own_flags_are_on_the_command_line(monkeypatch, tmp_path):
    """`extra_args` was internal-only; a user's flags have to land there too."""
    seen = {}

    class FakeCtx:
        def __init__(self): self.pages = []
        def add_init_script(self, *a, **k): pass
        def new_page(self): raise RuntimeError("stop here")

    class FakeChromium:
        def launch_persistent_context(self, **kw):
            seen.update(kw)
            raise RuntimeError("stop here")

    class FakePW:
        chromium = FakeChromium()
        def stop(self): pass

    monkeypatch.setattr(drivers, "_seed_bookmarks", lambda *a, **k: None)
    p = profile(launch_args=["--mute-audio"], dns="9.9.9.9")

    class Store:
        def user_data_path(self, _id): return tmp_path

    with pytest.raises(RuntimeError):
        drivers._launch_playwright_like(lambda: type("S", (), {"start": lambda s: FakePW()})(),
                                        p, Store(), headless=True, keep_open=False,
                                        extra_args=["--internal"])
    args = seen["args"]
    assert "--mute-audio" in args and "--internal" in args
    assert "--dns-over-https-templates=https://dns.quad9.net/dns-query" in args


# --- storage ---------------------------------------------------------------

def test_the_new_fields_survive_export_and_import():
    """`to_dict`/`from_dict` are hand-written, so a field added to the dataclass
    and forgotten in one of them is dropped silently. Export -> edit -> import is
    the CLI's whole-profile edit path (see test_agent_parity), so a field that
    does not survive it is unreachable from the CLI."""
    p = profile(launch_args=["--mute-audio"],
                bookmarks=[{"name": "A", "url": "https://a.test"}],
                dns="1.1.1.1", ip_based=["timezone"])
    back = Profile.from_dict(json.loads(json.dumps(p.to_dict())))
    assert back.launch_args == p.launch_args
    assert back.bookmarks == p.bookmarks
    assert back.dns == p.dns
    assert back.ip_based == p.ip_based


def test_a_profile_saved_before_these_fields_existed_still_loads():
    d = profile().to_dict()
    for key in ("launch_args", "bookmarks", "dns", "ip_based"):
        d.pop(key)
    back = Profile.from_dict(d)
    assert (back.launch_args, back.bookmarks, back.dns, back.ip_based) == ([], [], "", [])
