"""One request, one commit - and a stale writer cannot erase a fresh one.

Two failure shapes the store used to allow:

* **Lost update.** The route read the stored profile, merged its patch,
  validated it, and only then called `save()`, where the lock was taken. Two
  requests could read the same record, prepare independent edits, and serialise
  their writes; the second carried stale values for every field it had not
  touched and erased the first.

* **Split commit.** Profile data and `config.json` were separate writes. Config
  first meant a failed profile write left the global default changed by a
  request that returned an error. Config second meant a failed config write
  reported failure over profile changes that had already landed, and the obvious
  retry applied them twice.
"""
import json

import pytest

from numera import paths

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient          # noqa: E402

from numera.server import create_app               # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def one(c, pid):
    for prof in c.get("/api/profiles").json():
        if prof["id"] == pid:
            return prof
    return None


def default_engine(tmp_path):
    p = tmp_path / "config.json"
    return json.loads(p.read_text(encoding="utf-8")).get("default_engine") if p.is_file() else None


def failing_write(after: int):
    """A `write_json` that fails on the Nth call, to cut a request in half."""
    real = paths.write_json
    seen = {"n": 0}

    def wrapped(path, data):
        seen["n"] += 1
        if seen["n"] == after:
            raise OSError("no space left on device")
        return real(path, data)

    return wrapped


# --- lost updates -----------------------------------------------------------

def test_two_field_edits_both_survive(tmp_path):
    """A changes `notes`, B changes `tags`. Both acknowledged, both kept."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "p"}).json()["id"]

    assert c.put(f"/api/profiles/{pid}", json={"notes": "from-A"}).status_code == 200
    assert c.put(f"/api/profiles/{pid}", json={"tags": ["from-B"]}).status_code == 200

    after = one(c, pid)
    assert after["notes"] == "from-A", "the second edit erased the first"
    assert after["tags"] == ["from-B"]


def test_a_stale_full_profile_is_refused(tmp_path):
    """The dashboard's inline rename sends `{...p, name}` - a whole profile
    that may already be out of date. Without a revision the later writer wins
    and silently reverts every field it was not editing."""
    c = client(tmp_path)
    made = c.post("/api/profiles", json={"name": "p"}).json()
    pid = made["id"]

    stale = dict(one(c, pid))                       # a client's copy
    assert stale.get("_rev") is not None, "profiles carry no revision"

    # Somebody else edits it first.
    assert c.put(f"/api/profiles/{pid}", json={"notes": "fresh"}).status_code == 200

    # Now the stale holder submits its whole profile with one field changed.
    r = c.put(f"/api/profiles/{pid}", json={**stale, "name": "renamed"})

    assert r.status_code == 409, f"a stale full profile was accepted: {r.status_code}"
    assert one(c, pid)["notes"] == "fresh", "the fresh edit was erased"


def test_a_fresh_full_profile_still_saves(tmp_path):
    """The check must not refuse an up-to-date client."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "p"}).json()["id"]
    fresh = dict(one(c, pid))

    r = c.put(f"/api/profiles/{pid}", json={**fresh, "name": "renamed"})

    assert r.status_code == 200, r.text
    assert one(c, pid)["name"] == "renamed"


def test_a_client_without_a_revision_is_not_refused(tmp_path):
    """A patch that names only the fields it changes needs no revision - it
    cannot erase anything it did not send."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "p", "notes": "keep"}).json()["id"]

    r = c.put(f"/api/profiles/{pid}", json={"name": "renamed"})

    assert r.status_code == 200, r.text
    assert one(c, pid)["notes"] == "keep"


# --- config and data commit together, in both failure directions ------------

def test_a_failed_profile_write_leaves_the_default_engine_alone(tmp_path, monkeypatch):
    """`_remember_engine` ran *before* the profile write on create."""
    c = client(tmp_path)
    c.post("/api/profiles", json={"name": "first"})
    before = default_engine(tmp_path)

    monkeypatch.setattr(paths, "write_json", failing_write(1))
    monkeypatch.setattr("numera.server.stores.write_json", failing_write(1))
    with pytest.raises(OSError):
        c.post("/api/profiles", json={"name": "second", "engine": "camoufox"})
    monkeypatch.undo()

    assert default_engine(tmp_path) == before, \
        "a failed create still changed the global default engine"
    assert [p["name"] for p in c.get("/api/profiles").json()] == ["first"]


def test_a_failed_config_write_rolls_the_profile_back(tmp_path, monkeypatch):
    """The other direction: config was written *after* the data, so its failure
    reported an error over changes that had already committed."""
    c = client(tmp_path)
    c.post("/api/profiles", json={"name": "first"})
    before_engine = default_engine(tmp_path)
    before_names = sorted(p["name"] for p in c.get("/api/profiles").json())

    # The config file is the last write of the request, after the profile and
    # its engine twin.
    monkeypatch.setattr(paths, "write_json", failing_write(3))
    monkeypatch.setattr("numera.server.stores.write_json", failing_write(3))
    with pytest.raises(OSError):
        c.post("/api/profiles", json={"name": "second", "engine": "camoufox"})
    monkeypatch.undo()

    assert default_engine(tmp_path) == before_engine
    assert sorted(p["name"] for p in c.get("/api/profiles").json()) == before_names, \
        "the profile survived a request that reported failure"


def test_a_successful_request_commits_both(tmp_path):
    c = client(tmp_path)
    r = c.post("/api/profiles", json={"name": "p", "engine": "camoufox"})
    assert r.status_code < 400, r.text
    assert default_engine(tmp_path) == "camoufox"
    assert one(c, r.json()["id"])["engine"] == "camoufox"


def test_a_failed_bulk_leaves_neither_profiles_nor_config_changed(tmp_path, monkeypatch):
    c = client(tmp_path)
    ids = [c.post("/api/profiles", json={"name": n}).json()["id"] for n in ("a", "b", "c")]
    before_engine = default_engine(tmp_path)
    before = {i: one(c, i) for i in ids}

    monkeypatch.setattr(paths, "write_json", failing_write(4))
    monkeypatch.setattr("numera.server.stores.write_json", failing_write(4))
    with pytest.raises(OSError):
        c.post("/api/profiles/bulk",
               json={"ids": ids, "patch": {"engine": "camoufox", "notes": "x"}})
    monkeypatch.undo()

    assert default_engine(tmp_path) == before_engine
    for i in ids:
        assert one(c, i) == before[i], f"profile {i} kept part of a failed bulk edit"
