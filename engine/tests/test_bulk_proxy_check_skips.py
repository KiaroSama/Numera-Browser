"""`POST /api/proxies/test` when some — or every — credential is withheld.

A proxy whose password this process cannot read is reported as *skipped*: the
check would authenticate with nothing and record a real "failed" against a
proxy that is fine. That filter left `targets` empty while `withheld` was not,
and the sweep still built `ThreadPoolExecutor(max_workers=min(8, 0))`, which
raises `ValueError: max_workers must be greater than 0` before the skipped
results the request exists to return are ever assembled. So selecting a locked
pool and pressing Check answered 500 instead of "these are locked".

Everything here is asserted through the real route, against the real pool file:
the point is not only the status code but that a withheld credential is never
handed to the checker and never has its health state rewritten.
"""

import json

import pytest

from numera import proxypool as pp
from numera.store import ProfileStore


@pytest.fixture
def store(tmp_path):
    return ProfileStore(tmp_path)


@pytest.fixture
def checked(monkeypatch):
    """Record every proxy the checker is handed, and answer "ok".

    The suite never leaves this machine, so a real check is out; what matters
    is the call list, because "no checker call for a withheld credential" is
    half of this contract.
    """
    seen = []

    def fake_check(proxy):
        seen.append(proxy["host"])
        return {"ok": True, "ip": "203.0.113.7", "ms": 12}

    monkeypatch.setattr(pp, "check", fake_check)
    return seen


def api(store, password=None):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    return TestClient(create_app(data_dir=str(store.root), start_scheduler=False,
                                 password=password))


def raw(store) -> list:
    return json.loads((store.root / "proxies.json").read_text(encoding="utf-8"))


def added(client, *hosts, password="secret") -> list:
    answer = client.post("/api/proxies", json={"proxies": [
        {"type": "HTTP", "host": h, "port": 8080, "user": "u", "pass": password}
        for h in hosts]})
    assert answer.status_code == 200, answer.text
    return [p["id"] for p in answer.json()["proxies"]]


def sealed(store, *hosts, password="secret") -> list:
    """Ids for `hosts`, added and then sealed behind a vault nobody unlocked."""
    pytest.importorskip("cryptography")
    ids = added(api(store), *hosts, password=password)
    store.enable_vault("master")
    assert raw(store)[0]["pass"] != password, "the vault did not seal the pool"
    return ids


def test_a_wholly_locked_selection_reports_skipped_instead_of_failing(store, checked):
    """The crash: nothing runnable, something withheld, and no executor to build."""
    ids = sealed(store, "a.example", "b.example")

    answer = api(store).post("/api/proxies/test", json={"ids": ids})

    assert answer.status_code == 200, answer.text
    body = answer.json()
    assert body["skipped"] == 2 and body["tested"] == 2
    assert [r["status"] for r in body["results"]] == ["skipped", "skipped"]
    assert {r["id"] for r in body["results"]} == set(ids)
    assert checked == [], "a withheld credential was handed to the checker"


def test_a_locked_sweep_leaves_every_health_field_alone(store, checked):
    """Skipped is not a result: nothing about those rows may be rewritten."""
    sealed(store, "a.example", "b.example")
    before = raw(store)

    answer = api(store).post("/api/proxies/test", json={})

    assert answer.status_code == 200, answer.text
    assert raw(store) == before, "a skipped proxy had its health state written"


def test_a_mixed_selection_tests_what_it_can_and_skips_the_rest(store, checked):
    """The readable half still gets a real check; the locked half never does.

    A proxy with no password at all is readable under a locked vault - there is
    nothing sealed to withhold - so one of each is the real mixed pool.
    """
    pytest.importorskip("cryptography")
    setup = api(store)
    locked = added(setup, "locked.example")
    open_id = added(setup, "open.example", password="")[0]
    store.enable_vault("master")
    client = api(store)                     # locked: no master password
    before = {p["host"]: p for p in raw(store)}["locked.example"]

    answer = client.post("/api/proxies/test", json={"ids": locked + [open_id]})

    assert answer.status_code == 200, answer.text
    body = answer.json()
    assert checked == ["open.example"], "the checker saw the wrong set"
    by_id = {r["id"]: r for r in body["results"]}
    assert by_id[open_id]["status"] == "ok"
    assert by_id[locked[0]]["status"] == "skipped"
    assert body["skipped"] == 1 and body["tested"] == 2
    stored = {p["host"]: p for p in raw(store)}
    assert stored["open.example"]["status"] == "ok"
    assert stored["locked.example"] == before, \
        "a withheld credential had its health state written"


def test_an_all_readable_sweep_is_unchanged(store, checked):
    ids = added(api(store), "one.example", "two.example")

    answer = api(store).post("/api/proxies/test", json={"ids": ids})

    assert answer.status_code == 200, answer.text
    body = answer.json()
    assert (body["tested"], body["ok"], body["skipped"]) == (2, 2, 0)
    assert sorted(checked) == ["one.example", "two.example"]
    assert {p["status"] for p in raw(store)} == {"ok"}


def test_ids_nothing_in_the_pool_matches_are_refused(store, checked):
    added(api(store), "one.example")

    answer = api(store).post("/api/proxies/test", json={"ids": ["no-such-proxy"]})

    assert answer.status_code == 400, answer.text
    assert checked == []


def test_a_missing_id_beside_a_locked_one_still_reports_the_locked_one(store, checked):
    """A stale selection is the ordinary case after a delete, and it must not
    turn the locked half of the answer into an error."""
    ids = sealed(store, "a.example")

    answer = api(store).post("/api/proxies/test",
                             json={"ids": ids + ["gone-from-the-pool"]})

    assert answer.status_code == 200, answer.text
    body = answer.json()
    assert [r["id"] for r in body["results"]] == ids
    assert body["skipped"] == 1
    assert checked == []


def test_an_empty_pool_is_refused_rather_than_swept(store, checked):
    answer = api(store).post("/api/proxies/test", json={"ids": []})

    assert answer.status_code == 400, answer.text
    assert checked == []
