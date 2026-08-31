"""Batch writes, session deletion, folder edits and proxy identity.

Four defects that share a theme: an operation that was *nearly* transactional or
*nearly* canonical, and reported success from the gap.
"""
import json
import threading

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera import folders, proxypool                           # noqa: E402
from numera.server import create_app                            # noqa: E402
from numera.server.stores import DashboardStore, ProxyPool      # noqa: E402
from numera.store import ProfileStore                           # noqa: E402


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def files_in(tmp_path, sub):
    d = tmp_path / sub
    return sorted(f.name for f in d.glob("*.json")) if d.is_dir() else []


# --- batch create is a transaction, not just a validated loop --------------

def test_a_batch_that_fails_on_the_third_write_stores_none_of_them(tmp_path, monkeypatch):
    """Validation was already all-or-nothing; the writes were not. A storage
    failure part-way left the earlier profiles on disk while the request
    reported failure - and the obvious retry then produced duplicates."""
    from numera import paths

    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    real = paths.write_json
    calls = {"n": 0}

    def fail_on_the_third(path, data):
        calls["n"] += 1
        if calls["n"] == 3:
            raise OSError("disk full")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_on_the_third)
    with pytest.raises(OSError):
        c.post("/api/profiles/batch",
               json={"profiles": [{"name": "a"}, {"name": "b"}, {"name": "d"}]})
    monkeypatch.setattr(paths, "write_json", real)

    assert files_in(tmp_path, "dashboard") == [], "part of the batch survived"
    assert files_in(tmp_path, "profiles") == [], "engine twins survived"


def test_a_successful_batch_still_writes_both_copies(c, tmp_path):
    r = c.post("/api/profiles/batch", json={"profiles": [{"name": "a"}, {"name": "b"}]})
    assert r.status_code == 200, r.json()
    assert len(files_in(tmp_path, "dashboard")) == 2
    assert len(files_in(tmp_path, "profiles")) == 2
    assert len({p["seed"] for p in c.get("/api/profiles").json()}) == 2


# --- a session that cannot be removed is reported, never assumed -----------

def test_a_session_that_cannot_be_detached_is_reported(tmp_path, monkeypatch):
    """`rmtree(..., ignore_errors=True)` hid the failure completely: on Windows
    a running Chromium holds its files, so the delete quietly did nothing and
    the caller was told the session was gone while it was all still there."""
    from pathlib import Path

    store = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=store)
    pid = dash.save({"name": "held", "tags": []})["id"]
    session = store.user_data_path(pid)
    session.mkdir(parents=True, exist_ok=True)
    (session / "Cookies").write_text("live", encoding="utf-8")

    real_rename = Path.rename
    monkeypatch.setattr(Path, "rename",
                        lambda self, target: (_ for _ in ()).throw(OSError("in use"))
                        if self.name == pid else real_rename(self, target))
    with pytest.raises(OSError):
        store.delete_session(pid)
    monkeypatch.setattr(Path, "rename", real_rename)

    assert (session / "Cookies").exists(), "the session vanished despite the failure"


def test_a_removable_session_is_actually_removed(tmp_path):
    store = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=store)
    pid = dash.save({"name": "ok", "tags": []})["id"]
    session = store.user_data_path(pid)
    session.mkdir(parents=True, exist_ok=True)
    (session / "Cookies").write_text("x", encoding="utf-8")

    store.delete_session(pid)
    assert not session.exists()
    leftovers = [d.name for d in session.parent.iterdir() if ".deleting-" in d.name]
    assert not leftovers, f"a tombstone was left behind: {leftovers}"


# --- folder edits serialise ------------------------------------------------

def test_concurrent_folder_creates_both_survive(tmp_path):
    """Each operation read the folder list, changed it and wrote it back with
    no lock, so the second write silently dropped the first one's change."""
    errors = []

    def make(name):
        try:
            for i in range(15):
                folders.create(tmp_path, f"{name}{i}")
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=make, args=(n,)) for n in ("a", "b", "c")]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=30)
    assert not errors, errors
    assert not any(t.is_alive() for t in threads)

    listed = folders.listing(tmp_path)
    for name in ("a", "b", "c"):
        got = [f for f in listed if f.startswith(name)]
        assert len(got) == 15, f"{name}: kept {len(got)} of 15 - a write was lost"


def test_a_rename_and_a_create_do_not_lose_each_other(tmp_path):
    folders.create(tmp_path, "work")
    errors = []

    def rename():
        try:
            folders.rename(tmp_path, "work", "client-a")
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    def create():
        try:
            folders.create(tmp_path, "fresh")
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=rename), threading.Thread(target=create)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=30)
    assert not errors, errors
    listed = folders.listing(tmp_path)
    assert "client-a" in listed and "fresh" in listed, listed


# --- one proxy identity, one validator, one conversion ---------------------

def test_protocol_is_part_of_proxy_identity(tmp_path):
    """`ProxyPool._key` left the type out while `identity()` included it, so an
    HTTP and a SOCKS5 entry with the same host/port/credentials were one proxy
    to the deduper and two to the usage count."""
    pool = ProxyPool(tmp_path, ProfileStore(tmp_path))
    base = {"host": "1.2.3.4", "port": "1080", "user": "u", "pass": "p"}
    pool.add_many([{**base, "type": "HTTP"}])
    pool.add_many([{**base, "type": "SOCKS5"}])
    assert len(pool.list()) == 2, "a different protocol was deduped away"

    ids = {proxypool.identity(p) for p in pool.list()}
    assert len(ids) == 2


def test_the_same_proxy_twice_is_still_deduped(tmp_path):
    pool = ProxyPool(tmp_path, ProfileStore(tmp_path))
    entry = {"type": "HTTP", "host": "1.2.3.4", "port": "8080", "user": "u", "pass": "p"}
    pool.add_many([entry])
    assert pool.add_many([dict(entry)]) == []
    assert len(pool.list()) == 1


@pytest.mark.parametrize("patch, why", [
    ({"port": "70000"}, "a port outside the range"),
    ({"port": "notaport"}, "a port that is not a number"),
    ({"type": "GOPHER"}, "an unsupported type"),
    ({"host": ""}, "an empty host"),
    ({"country": "xyz"}, "a malformed country"),
])
def test_editing_a_proxy_is_validated_like_creating_one(c, patch, why):
    """Edit went straight to the store, so these were persisted and only failed
    later at launch."""
    made = c.post("/api/proxies", json={"proxies": [
        {"type": "HTTP", "host": "1.2.3.4", "port": "8080"}]}).json()["proxies"][0]
    before = c.get("/api/proxies").json()[0]

    r = c.put(f"/api/proxies/{made['id']}", json=patch)
    assert r.status_code in (400, 422), f"{why}: got {r.status_code}"
    assert c.get("/api/proxies").json()[0] == before, f"{why} was persisted"


def test_a_valid_edit_still_applies(c):
    made = c.post("/api/proxies", json={"proxies": [
        {"type": "HTTP", "host": "1.2.3.4", "port": "8080"}]}).json()["proxies"][0]
    r = c.put(f"/api/proxies/{made['id']}", json={"type": "SOCKS5", "port": "1080"})
    assert r.status_code == 200, r.json()
    got = c.get("/api/proxies").json()[0]
    assert got["type"] == "SOCKS5" and got["port"] == "1080"


def test_an_unsupported_text_scheme_is_refused_not_rewritten(c):
    """`parse_line` mapped an unknown scheme to HTTP, so `normalize` downstream
    saw a valid-looking entry and could never reject the input - the user got an
    HTTP proxy they never asked for."""
    r = c.post("/api/proxies", json={"text": "gopher://1.2.3.4:70"})
    assert r.status_code in (400, 422), r.status_code
    assert c.get("/api/proxies").json() == []


def test_a_bare_host_port_line_is_still_http(c):
    """No scheme at all is HTTP by convention and must keep working."""
    r = c.post("/api/proxies", json={"text": "1.2.3.4:8080"})
    assert r.status_code == 200, r.json()
    assert c.get("/api/proxies").json()[0]["type"] == "HTTP"


@pytest.mark.parametrize("kind, scheme", [
    ("HTTP", "http"), ("HTTPS", "https"), ("SOCKS4", "socks4"), ("SOCKS5", "socks5"),
])
def test_every_protocol_survives_the_cli_assignment(kind, scheme):
    """The CLI had its own scheme map that sent HTTPS to "http", so a profile
    claimed one protocol while the browser spoke another."""
    assert proxypool.server_url(kind, "1.2.3.4", "8080") == f"{scheme}://1.2.3.4:8080"


def test_an_ipv6_host_stays_bracketed():
    url = proxypool.server_url("SOCKS5", "2001:db8::1", "1080")
    assert url == "socks5://[2001:db8::1]:1080", url
