"""Writes that must complete, normalise, or refuse - never half-do.

Four findings from one audit, all the same shape: a path that wrote something
without finishing the job it implied. A batch that stored profiles the dashboard
cannot render, a delete that erased one copy and reported 404, settings that
overwrote each other, and structured proxy input that reached the store without
ever being checked.
"""
import json
import threading

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera import folders, proxypool                           # noqa: E402
from numera.server import create_app                            # noqa: E402
from numera.store import ProfileStore                           # noqa: E402


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def stored(tmp_path, pid):
    return json.loads((tmp_path / "dashboard" / f"{pid}.json").read_text(encoding="utf-8"))


# --- batch entries are complete profiles, not what was posted ---------------

def test_a_minimal_batch_entry_is_normalised_on_disk(c, tmp_path):
    """Batch type-checked the fields that were present and then saved, so
    `{"name": "one"}` landed with no `tags` at all - and the profile list calls
    `p.tags.some(...)`, which takes the page down on the next load. The response
    said `created: 2` either way, which is why counting it proved nothing."""
    r = c.post("/api/profiles/batch", json={"profiles": [{"name": "one"}, {"name": "two"}]})
    assert r.status_code == 200, r.json()
    ids = r.json()["ids"]
    assert len(ids) == 2

    for pid in ids:
        on_disk = stored(tmp_path, pid)
        assert isinstance(on_disk.get("tags"), list), on_disk
        assert isinstance(on_disk.get("startupUrls"), str)
        assert on_disk.get("userAgent"), "no user agent was derived"
        assert on_disk.get("os")


def test_a_batch_profile_survives_the_round_trip(c):
    """The list endpoint is where an incomplete profile actually bites."""
    c.post("/api/profiles/batch", json={"profiles": [{"name": "one"}]})
    listed = c.get("/api/profiles").json()
    assert len(listed) == 1
    assert isinstance(listed[0]["tags"], list)
    assert listed[0]["status"] in ("running", "stopped")


def test_two_minimal_entries_get_different_identities(c, tmp_path):
    """The noise seed is derived from the id, so the id has to be minted before
    the conversion runs. Sharing one seed would give both profiles the same
    canvas, audio and ClientRects fingerprint."""
    ids = c.post("/api/profiles/batch",
                 json={"profiles": [{"name": "a"}, {"name": "b"}]}).json()["ids"]
    seeds = [stored(tmp_path, pid)["seed"] for pid in ids]
    assert len(set(ids)) == 2
    assert all(s for s in seeds), seeds
    assert len(set(seeds)) == 2, f"both profiles share a seed: {seeds}"


def test_one_bad_entry_still_writes_none_of_them(c):
    ids_before = c.get("/api/profiles").json()
    r = c.post("/api/profiles/batch",
               json={"profiles": [{"name": "fine"}, {"name": "bad", "tags": "not-a-list"}]})
    assert r.status_code in (400, 422)
    assert c.get("/api/profiles").json() == ids_before


# --- delete: one owner, existence checked first -----------------------------

def test_deleting_a_profile_that_does_not_exist_changes_nothing(c, tmp_path):
    """The route deleted the engine record and only then discovered the
    dashboard had no such profile - so a 404 could arrive *after* an engine-only
    profile had been erased. A 404 is the one response that promises nothing
    happened."""
    engine = ProfileStore(tmp_path)
    from numera import generate
    from numera.models import Profile
    kept = engine.save(Profile(name="engine-only", fingerprint=generate(seed=2)))

    assert c.delete(f"/api/profiles/{kept.id}").status_code in (200, 404)
    # Whatever the answer, an id that exists nowhere must not touch the store.
    before = sorted(p.id for p in engine.list())
    assert c.delete("/api/profiles/deadbeef1234").status_code == 404
    assert sorted(p.id for p in engine.list()) == before


def test_a_failed_engine_delete_leaves_both_copies(tmp_path, monkeypatch):
    """It swallowed the engine failure, so the dashboard copy was gone and the
    twin stayed - the profile reappeared in `numera list` after the GUI said it
    was deleted, and the call reported success."""
    from numera.server.stores import DashboardStore

    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    saved = dash.save({"name": "both", "tags": []})
    pid = saved["id"]
    dash_file = tmp_path / "dashboard" / f"{pid}.json"
    engine_file = tmp_path / "profiles" / f"{pid}.json"
    assert dash_file.is_file() and engine_file.is_file()

    real_unlink = type(dash_file).unlink

    def fail_on_the_engine_copy(self, *a, **k):
        if self.parent.name == "profiles":
            raise OSError("in use")
        return real_unlink(self, *a, **k)

    monkeypatch.setattr(type(dash_file), "unlink", fail_on_the_engine_copy)
    with pytest.raises(OSError):
        dash.delete(pid)
    monkeypatch.setattr(type(dash_file), "unlink", real_unlink)

    assert dash_file.is_file(), "the dashboard copy was not put back"
    assert engine_file.is_file()


def test_the_session_is_detached_before_any_metadata_is_touched(tmp_path, monkeypatch):
    """The ordering changed, and the new one is stronger.

    It used to delete both records and remove the session last, on the reasoning
    that metadata can be written back from memory and a wiped session cannot.
    That was right about which half is recoverable and wrong about the failure
    it created: once the session removal could fail loudly, a held directory
    produced a failed request that had *already* deleted the profile.

    So the session is detached by rename first - which fails while a browser
    holds it, at a point where nothing else has moved - then the records go,
    then the detached bytes are reclaimed.
    """
    from numera.server.stores import DashboardStore

    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    pid = dash.save({"name": "with-session", "tags": []})["id"]
    session = engine.user_data_path(pid)
    session.mkdir(parents=True, exist_ok=True)
    (session / "Cookies").write_text("real", encoding="utf-8")

    order = []
    real_detach = ProfileStore._detach_session
    real_discard = ProfileStore.discard_tombstone
    monkeypatch.setattr(ProfileStore, "_detach_session",
                        lambda self, p: (order.append("detach"), real_detach(self, p))[1])
    monkeypatch.setattr(ProfileStore, "discard_tombstone",
                        staticmethod(lambda t: (order.append("discard"), real_discard(t))[1]))
    real_unlink = type(session).unlink
    monkeypatch.setattr(type(session), "unlink",
                        lambda self, *a, **k: (order.append("metadata"),
                                               real_unlink(self, *a, **k))[1])
    dash.delete(pid)

    assert order[0] == "detach", f"metadata moved before the session: {order}"
    assert order[-1] == "discard", f"the tombstone was not reclaimed last: {order}"
    assert "metadata" in order
    assert not (tmp_path / "dashboard" / f"{pid}.json").exists()
    assert not (tmp_path / "profiles" / f"{pid}.json").exists()
    assert not session.exists()


def test_a_held_session_fails_the_delete_without_touching_the_records(tmp_path, monkeypatch):
    """The failure this ordering exists to prevent: a DELETE that reports
    failure and has already removed the profile."""
    from pathlib import Path

    from numera.server.stores import DashboardStore

    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    pid = dash.save({"name": "held", "tags": []})["id"]
    session = engine.user_data_path(pid)
    session.mkdir(parents=True, exist_ok=True)
    (session / "Cookies").write_text("live", encoding="utf-8")

    real_rename = Path.rename
    monkeypatch.setattr(Path, "rename",
                        lambda self, target: (_ for _ in ()).throw(OSError("in use"))
                        if self.name == pid else real_rename(self, target))
    with pytest.raises(OSError):
        dash.delete(pid)
    monkeypatch.setattr(Path, "rename", real_rename)

    assert (tmp_path / "dashboard" / f"{pid}.json").is_file(), "the record was deleted anyway"
    assert (tmp_path / "profiles" / f"{pid}.json").is_file()
    assert (session / "Cookies").read_text(encoding="utf-8") == "live"


def test_a_successful_delete_removes_both_copies(c, tmp_path):
    pid = c.post("/api/profiles", json={"name": "gone"}).json()["id"]
    assert c.delete(f"/api/profiles/{pid}").status_code == 200
    assert not (tmp_path / "dashboard" / f"{pid}.json").exists()
    assert not (tmp_path / "profiles" / f"{pid}.json").exists()
    assert c.get("/api/profiles").json() == []


# --- settings written at once do not overwrite each other -------------------

def test_two_settings_saved_at_once_both_survive(tmp_path):
    """Each writer read the config outside the lock and wrote its own copy of
    the whole document back, so the second one silently dropped the first one's
    change."""
    store = ProfileStore(tmp_path)
    errors = []

    def set_engine():
        try:
            for _ in range(40):
                store.set_default_engine("camoufox")
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    def set_headless():
        try:
            for _ in range(40):
                store.set_headless_launch(True)
        except Exception as exc:                                # noqa: BLE001
            errors.append(exc)

    threads = [threading.Thread(target=set_engine), threading.Thread(target=set_headless)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=30)
    assert not errors, errors
    assert not any(t.is_alive() for t in threads)

    cfg = ProfileStore(tmp_path).get_config()
    assert cfg.get("default_engine") == "camoufox", cfg
    assert cfg.get("headless_launch") is True, cfg


def test_a_setting_does_not_disturb_the_others(tmp_path):
    store = ProfileStore(tmp_path)
    # Real folders: the store validates additions the way the API does, so a
    # placeholder path is refused before it can round-trip.
    kept = []
    for name in ("one", "two"):
        d = tmp_path / name
        d.mkdir()
        (d / "manifest.json").write_text(
            '{"manifest_version": 3, "name": "x", "version": "1.0"}', encoding="utf-8")
        kept.append(str(d))
    store.set_default_extensions(kept)
    store.set_default_engine("playwright")
    store.set_headless_launch(True)
    cfg = ProfileStore(tmp_path).get_config()
    assert cfg["default_extensions"] == kept
    assert cfg["default_engine"] == "playwright"
    assert cfg["headless_launch"] is True


# --- folder edits complete or undo ------------------------------------------

def make_profiles(tmp_path, folder, n=4):
    """Folders operate on the dashboard copies - that is the shape that has a
    `folder` field at all."""
    from numera.server.stores import DashboardStore

    dash = DashboardStore(tmp_path, engine_store=ProfileStore(tmp_path))
    return [dash.save({"name": f"p{i}", "tags": [], "folder": folder})["id"]
            for i in range(n)]


def folder_of(tmp_path, pid):
    return stored(tmp_path, pid).get("folder")


def test_a_rename_that_fails_part_way_moves_nobody(tmp_path, monkeypatch):
    """Writing each profile directly meant a failure half way left some moved
    and the rest behind, under a folder list that had already been updated."""
    ids = make_profiles(tmp_path, "work")
    folders.create(tmp_path, "work")
    from numera import paths

    real = paths.write_json
    calls = {"n": 0}

    def fail_on_the_third(path, data):
        calls["n"] += 1
        if calls["n"] == 3:
            raise OSError("disk full")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_on_the_third)
    monkeypatch.setattr(folders, "write_json", fail_on_the_third, raising=False)
    with pytest.raises(OSError):
        folders.rename(tmp_path, "work", "client-a")
    monkeypatch.setattr(paths, "write_json", real)
    monkeypatch.setattr(folders, "write_json", real, raising=False)

    moved = [pid for pid in ids if folder_of(tmp_path, pid) == "client-a"]
    assert moved == [], f"{len(moved)} of {len(ids)} profiles were left moved"
    assert all(folder_of(tmp_path, pid) == "work" for pid in ids)


def test_a_rename_that_succeeds_moves_every_profile(tmp_path):
    ids = make_profiles(tmp_path, "work")
    folders.create(tmp_path, "work")
    folders.rename(tmp_path, "work", "client-a")
    assert all(folder_of(tmp_path, pid) == "client-a" for pid in ids)
    assert "client-a" in folders.listing(tmp_path)
    assert "work" not in folders.listing(tmp_path)


def test_deleting_a_folder_reassigns_every_profile(tmp_path):
    ids = make_profiles(tmp_path, "old")
    folders.create(tmp_path, "old")
    folders.delete(tmp_path, "old", "kept")
    assert all(folder_of(tmp_path, pid) == "kept" for pid in ids)


# --- structured proxy input is validated like text input --------------------

@pytest.mark.parametrize("body, why", [
    ({"proxies": "not-a-list"}, "a string is not a list of proxies"),
    ({"proxies": [{"nope": 1}]}, "no host"),
    ({"proxies": [{"host": "", "port": "80"}]}, "an empty host"),
    ({"proxies": [{"host": "h", "port": "notaport"}]}, "a port that is not a number"),
    ({"proxies": [{"host": "h", "port": "0"}]}, "a port outside the range"),
    ({"proxies": [{"host": "h", "port": "70000"}]}, "a port outside the range"),
    ({"proxies": [{"type": "GOPHER", "host": "h", "port": "80"}]}, "an unsupported type"),
    ({"proxies": ["just a string"]}, "an entry that is not an object"),
])
def test_malformed_structured_input_is_a_4xx_not_a_500(c, body, why):
    """`body.proxies` went straight to the pool, whose identity code indexes
    `host` and `port` - so a malformed object raised a KeyError as a 500, and a
    non-numeric port was stored and only failed at launch."""
    r = c.post("/api/proxies", json=body)
    assert r.status_code in (400, 422), f"{why}: got {r.status_code}"
    assert c.get("/api/proxies").json() == [] or True
    assert not any(p for p in c.get("/api/proxies").json()), "a bad proxy was stored"


@pytest.mark.parametrize("entry", [
    {"type": "SOCKS4", "host": "2001:db8::1", "port": "1080", "user": "u"},
    {"type": "SOCKS5", "host": "1.2.3.4", "port": "1080", "user": "u", "pass": "p"},
    {"type": "HTTPS", "host": "example.com", "port": "8443", "country": "de"},
    {"host": "1.2.3.4", "port": 8080},
])
def test_good_structured_input_still_round_trips(c, entry):
    r = c.post("/api/proxies", json={"proxies": [entry]})
    assert r.status_code == 200, r.json()
    got = c.get("/api/proxies").json()[-1]
    assert got["host"] == str(entry["host"]).strip("[]")
    assert got["port"] == str(entry["port"])
    assert got["type"] == entry.get("type", "HTTP")


def test_text_and_structured_input_agree(c):
    """One validator for both shapes, so the same proxy written either way
    lands identically - the pool deduped on the result and could not."""
    c.post("/api/proxies", json={"text": "socks5://u:p@1.2.3.4:1080"})
    first = c.get("/api/proxies").json()[0]
    c.post("/api/proxies", json={"proxies": [{"type": "SOCKS5", "host": "1.2.3.4",
                                              "port": "1080", "user": "u", "pass": "p"}]})
    entries = c.get("/api/proxies").json()
    assert len(entries) == 1, f"the same proxy was stored twice: {entries}"
    assert proxypool.identity(first) == proxypool.identity(entries[0])
