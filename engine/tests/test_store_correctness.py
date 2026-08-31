"""Storage invariants the audit found broken.

Each of these is a value that looked right in the UI and was wrong on disk, so
each is asserted against what was actually persisted rather than what a route
returned.
"""
import json
import threading

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient  # noqa: E402

from numera.server import create_app  # noqa: E402
from numera.server.stores import ProxyPool  # noqa: E402


@pytest.fixture
def c(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def stored(tmp_path, pid):
    return json.loads((tmp_path / "dashboard" / f"{pid}.json").read_text(encoding="utf-8"))


# --- seeds -----------------------------------------------------------------

def test_two_seedless_profiles_get_different_seeds(c, tmp_path):
    """The seed drives canvas, audio and ClientRects noise. It was derived from
    the profile id - but before the id existed, so every profile created without
    one hashed the literal string "unsaved" and they all shared a fingerprint."""
    a = c.post("/api/profiles", json={"name": "a"}).json()
    b = c.post("/api/profiles", json={"name": "b"}).json()
    sa, sb = a.get("seed"), b.get("seed")
    assert sa and sb, f"a seed of 0/None means noise is not stable: {sa}, {sb}"
    assert sa != sb, "two profiles were given the same canvas noise seed"


def test_a_seed_is_stable_across_a_round_trip(c, tmp_path):
    made = c.post("/api/profiles", json={"name": "a"}).json()
    again = c.get("/api/profiles").json()
    mine = [p for p in again if p["id"] == made["id"]][0]
    assert mine["seed"] == made["seed"]


def test_an_explicit_seed_is_never_overwritten(c):
    made = c.post("/api/profiles", json={"name": "a", "seed": 4242}).json()
    assert made["seed"] == 4242


# --- the dashboard-only _new flag ------------------------------------------

def test_new_is_never_written_to_disk(c, tmp_path):
    """`_new` is how the editor knows it is on a create form. Persisted, the
    profile reloads as a form: it saves as a duplicate instead of an update and
    cookie actions stay disabled."""
    made = c.post("/api/profiles", json={"name": "a", "_new": True}).json()
    assert "_new" not in made
    assert "_new" not in stored(tmp_path, made["id"])


def test_new_is_stripped_on_update_too(c, tmp_path):
    made = c.post("/api/profiles", json={"name": "a"}).json()
    c.put(f"/api/profiles/{made['id']}", json={**made, "_new": True})
    assert "_new" not in stored(tmp_path, made["id"])


def test_an_imported_profile_reloads_as_an_existing_one(c, tmp_path):
    r = c.post("/api/profiles/import", json={"profiles": [
        {"name": "imported", "os": "Windows", "_new": True}]})
    if r.status_code == 404:
        pytest.skip("this build has no import route")
    listed = c.get("/api/profiles").json()
    assert all("_new" not in p for p in listed)
    assert len({p["id"] for p in listed}) == len(listed), "an import minted two ids"


# --- the default engine ----------------------------------------------------

def test_a_minimal_create_uses_the_default_without_rewriting_it(c):
    """Filling the engine in from the current default is not a user choice, and
    persisting it as one lets a client's blank field become everyone's default."""
    c.put("/api/config", json={"default_engine": "camoufox"})
    c.post("/api/profiles", json={"name": "no engine named"})
    assert c.get("/api/config").json()["default_engine"] == "camoufox"


def test_a_real_choice_does_move_the_default(c):
    c.put("/api/config", json={"default_engine": "camoufox"})
    c.post("/api/profiles", json={"name": "a", "engine": "playwright"})
    assert c.get("/api/config").json()["default_engine"] == "playwright"


# --- bulk and batch validation ---------------------------------------------

def test_a_bulk_patch_cannot_rewrite_an_id(c, tmp_path):
    """`id` is what every file, session directory and proxy assignment hangs
    off. A patch that sets it orphans all three."""
    made = c.post("/api/profiles", json={"name": "a"}).json()
    r = c.post("/api/profiles/bulk", json={"ids": [made["id"]],
                                           "patch": {"id": "hijacked"}})
    assert r.status_code in (400, 422), r.status_code
    assert (tmp_path / "dashboard" / f"{made['id']}.json").is_file()
    assert not (tmp_path / "dashboard" / "hijacked.json").exists()


def test_a_bulk_patch_cannot_break_a_list_field(c, tmp_path):
    made = c.post("/api/profiles", json={"name": "a"}).json()
    r = c.post("/api/profiles/bulk", json={"ids": [made["id"]],
                                           "patch": {"tags": "not-a-list"}})
    assert r.status_code in (400, 422)
    assert isinstance(stored(tmp_path, made["id"])["tags"], list)


def test_a_malformed_batch_writes_nothing_at_all(c, tmp_path):
    """Validate the whole batch first: half a batch on disk is worse than none,
    because nothing tells the user which half."""
    before = len(c.get("/api/profiles").json())
    r = c.post("/api/profiles/batch", json={"profiles": [
        {"name": "fine"},
        {"name": 12345},                      # not a string
    ]})
    assert r.status_code in (400, 422), r.status_code
    assert len(c.get("/api/profiles").json()) == before, "part of the batch was saved"


def test_a_batch_cannot_carry_its_own_ids(c):
    r = c.post("/api/profiles/batch", json={"profiles": [
        {"name": "a", "id": "chosen-by-client"}]})
    if r.status_code >= 400:
        return                                  # refusing is also correct
    assert all(p["id"] != "chosen-by-client" for p in c.get("/api/profiles").json())


def test_a_valid_batch_still_works(c):
    r = c.post("/api/profiles/batch", json={"profiles": [
        {"name": "one"}, {"name": "two"}]})
    assert r.status_code == 200, r.text
    names = {p["name"] for p in c.get("/api/profiles").json()}
    assert {"one", "two"} <= names


# --- concurrent writes to one JSON file ------------------------------------

def test_two_concurrent_pool_updates_both_survive(tmp_path):
    """`/api/proxies/test` runs eight checks at once and each one did an
    unsynchronised read-modify-write of the same file, so results were lost -
    the last writer's snapshot was already stale when it read it."""
    pool = ProxyPool(tmp_path)
    added = pool.add_many([{"type": "HTTP", "host": f"10.0.0.{i}", "port": 8080}
                           for i in range(2)])
    a, b = added[0]["id"], added[1]["id"]

    # Force the interleaving: both threads read, then both write.
    read = threading.Barrier(2)

    def mark(pid, status):
        pool.list()          # take a snapshot
        read.wait(timeout=5)
        pool.update(pid, {"status": status})

    t1 = threading.Thread(target=mark, args=(a, "ok"))
    t2 = threading.Thread(target=mark, args=(b, "failed"))
    t1.start(); t2.start(); t1.join(10); t2.join(10)

    by_id = {p["id"]: p for p in pool.list()}
    assert by_id[a]["status"] == "ok"
    assert by_id[b]["status"] == "failed", "one update was overwritten by the other"


def test_eight_concurrent_updates_all_persist(tmp_path):
    pool = ProxyPool(tmp_path)
    added = pool.add_many([{"type": "HTTP", "host": f"10.1.0.{i}", "port": 8080}
                           for i in range(8)])
    ready = threading.Barrier(8)

    def mark(entry):
        pool.list()
        ready.wait(timeout=10)
        pool.update(entry["id"], {"status": "ok", "note": entry["host"]})

    threads = [threading.Thread(target=mark, args=(e,)) for e in added]
    for t in threads:
        t.start()
    for t in threads:
        t.join(20)

    final = pool.list()
    assert len(final) == 8, "entries were lost entirely"
    assert all(p.get("status") == "ok" for p in final), \
        [p.get("status") for p in final]


def test_a_failed_write_leaves_the_previous_file_intact(tmp_path, monkeypatch):
    """A half-written JSON file is an unreadable pool. Write to a temporary file
    and replace, so an interrupted write loses the change, not the data."""
    pool = ProxyPool(tmp_path)
    pool.add_many([{"type": "HTTP", "host": "10.2.0.1", "port": 8080}])
    good = (tmp_path / "proxies.json").read_text(encoding="utf-8")

    import numera.server.stores as st
    real = st.json.dumps

    def boom(*a, **k):
        raise RuntimeError("disk full")

    monkeypatch.setattr(st.json, "dumps", boom)
    with pytest.raises(RuntimeError):
        pool.add_many([{"type": "HTTP", "host": "10.2.0.2", "port": 8080}])
    monkeypatch.setattr(st.json, "dumps", real)

    assert (tmp_path / "proxies.json").read_text(encoding="utf-8") == good
    assert len(pool.list()) == 1
