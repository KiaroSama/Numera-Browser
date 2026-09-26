"""HTTP and mirrored-store writes reject lossy counts before any file changes."""
import copy
import json

import pytest
from fastapi.testclient import TestClient

from numera.importers import NATIVE_KEY, NATIVE_VERSION
from numera.server import create_app
from numera.server.stores import DashboardStore, UnconvertibleProfile
from numera.store import ProfileStore
from numera.write_contract import WriteRefused

BAD_COUNTS = [(field, 1.5) for field in
              ("cores", "mediaVideo", "mediaAudio", "mediaSpeakers",
               "maxTouchPoints", "seed")] + [("memory", 3.5)]


def snapshot(root):
    """Include both copies, config and metadata; lockfiles hold no JSON state."""
    return {str(path.relative_to(root)): path.read_bytes()
            for path in root.rglob("*.json")}


@pytest.fixture
def client(tmp_path):
    (tmp_path / ".imported").write_text("isolated numeric fixture\n", encoding="utf-8")
    with TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False)) as app:
        yield app


def make(client, name):
    response = client.post("/api/profiles", json={"name": name, "engine": "playwright"})
    assert response.status_code < 300, response.text
    return response.json()["id"]


@pytest.mark.parametrize("field,value", BAD_COUNTS)
@pytest.mark.parametrize("route", ["create", "update", "batch", "bulk", "native-import"])
def test_every_http_write_rejects_a_fractional_count_atomically(client, tmp_path, field, value, route):
    ids = [make(client, "existing-a"), make(client, "existing-b")]
    before = snapshot(tmp_path)
    bad = {"name": "invalid-new", "engine": "playwright", field: value}
    if route == "create":
        response = client.post("/api/profiles", json=bad)
    elif route == "update":
        response = client.put(f"/api/profiles/{ids[0]}", json={field: value})
    elif route == "batch":
        response = client.post("/api/profiles/batch", json={"profiles": [
            {"name": "valid-new", "engine": "playwright"}, bad]})
    elif route == "bulk":
        response = client.post("/api/profiles/bulk", json={"ids": ids, "patch": {field: value}})
    else:
        exported = {NATIVE_KEY: NATIVE_VERSION, "profiles": [
            {"name": "valid-import", "engine": "playwright"}, bad]}
        response = client.post("/api/profiles/import", json={"text": json.dumps(exported)})
    assert 400 <= response.status_code < 500, response.text
    assert field in response.text, response.text
    assert snapshot(tmp_path) == before, "a refused write changed persistent state"


@pytest.mark.parametrize("field,value", BAD_COUNTS)
@pytest.mark.parametrize("many", [False, True])
def test_direct_mirrored_store_cannot_bypass_numeric_policy(tmp_path, field, value, many):
    (tmp_path / ".imported").write_text("isolated mirrored-store fixture\n", encoding="utf-8")
    engine = ProfileStore(tmp_path)
    store = DashboardStore(tmp_path, engine_store=engine)
    before = snapshot(tmp_path)
    bad = {"id": "bad-number", "name": "bad-number", field: value}
    with pytest.raises((UnconvertibleProfile, WriteRefused), match=field):
        if many:
            store.save_many([{"id": "good-number", "name": "good-number"}, copy.deepcopy(bad)])
        else:
            store.save(copy.deepcopy(bad))
    assert snapshot(tmp_path) == before


@pytest.mark.parametrize("memory", [.25, .5, 1, 16])
def test_http_allowed_numbers_match_both_persisted_copies(client, tmp_path, memory):
    response = client.post("/api/profiles", json={
        "name": "precise", "engine": "playwright", "memory": memory,
        "devicePixelRatio": 1.5, "cores": 8.0})
    assert response.status_code < 300, response.text
    pid = response.json()["id"]
    dash = json.loads((tmp_path / "dashboard" / f"{pid}.json").read_text(encoding="utf-8"))
    engine = json.loads((tmp_path / "profiles" / f"{pid}.json").read_text(encoding="utf-8"))
    assert dash["memory"] == engine["fingerprint"]["device_memory"] == memory
    assert dash["devicePixelRatio"] == engine["fingerprint"]["device_pixel_ratio"] == 1.5
    assert dash["cores"] == engine["fingerprint"]["hardware_concurrency"] == 8
    returned = next(p for p in client.get("/api/profiles").json() if p["id"] == pid)
    assert returned["memory"] == memory
