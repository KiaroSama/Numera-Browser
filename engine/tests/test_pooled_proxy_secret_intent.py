"""A pooled proxy's password: withheld, cleared, or replaced - through HTTP.

`proxypool.normalize()` maps both `None` and `""` to `None`, which is right for
the canonical *data* shape and wrong for a secret. `PUT /api/proxies/{pid}`
validated the merged record and copied the normalised values back over the
caller's patch, so an explicit

    {"pass": ""}

reached `vault.encrypt_for_write` as `None` - withheld - and the route answered
200 while the old password stayed exactly where it was. A control that reports
instead of acting is worse than one that refuses.

So normalisation and intent are separated: the route takes the canonical values
for the fields that describe the address, and leaves `pass` as the caller wrote
it. These are the five things a caller can mean, tested through the real route.
"""

import json

import pytest

from numera.store import ProfileStore


@pytest.fixture
def store(tmp_path):
    return ProfileStore(tmp_path)


def api(store, password=None):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    return TestClient(create_app(data_dir=str(store.root), start_scheduler=False,
                                 password=password))


def raw(store) -> list:
    return json.loads((store.root / "proxies.json").read_text(encoding="utf-8"))


def added(client, password="secret") -> str:
    answer = client.post("/api/proxies", json={"proxies": [
        {"type": "HTTP", "host": "h.example", "port": 8080,
         "user": "u", "pass": password}]})
    assert answer.status_code == 200, answer.text
    return answer.json()["proxies"][0]["id"]


def test_an_explicit_clear_removes_the_password_and_keeps_the_proxy(store):
    """The bug, at its plainest: no vault involved, and the password stayed."""
    client = api(store)
    pid = added(client)

    answer = client.put(f"/api/proxies/{pid}", json={"pass": ""})

    assert answer.status_code == 200, answer.text
    entry = client.get("/api/proxies").json()[0]
    assert not entry["pass"], "an explicit clear left the old password in place"
    assert (entry["host"], entry["port"], entry["type"], entry["user"]) == \
        ("h.example", "8080", "HTTP", "u"), \
        "clearing the password took the address with it"


def test_an_omitted_password_is_left_alone(store):
    """The other half of the same distinction: saying nothing changes nothing."""
    client = api(store)
    pid = added(client)

    answer = client.put(f"/api/proxies/{pid}", json={"label": "renamed"})

    assert answer.status_code == 200, answer.text
    entry = client.get("/api/proxies").json()[0]
    assert entry["pass"] == "secret"
    assert entry["label"] == "renamed"


def test_a_new_password_replaces_the_old_one(store):
    client = api(store)
    pid = added(client)

    answer = client.put(f"/api/proxies/{pid}", json={"pass": "other"})

    assert answer.status_code == 200, answer.text
    assert client.get("/api/proxies").json()[0]["pass"] == "other"


def test_an_unlocked_clear_removes_the_ciphertext(store):
    pytest.importorskip("cryptography")
    client = api(store)
    pid = added(client)
    store.enable_vault("master")
    assert raw(store)[0]["pass"] != "secret", "the vault did not seal the pool"

    unlocked = api(store, password="master")
    answer = unlocked.put(f"/api/proxies/{pid}", json={"pass": ""})

    assert answer.status_code == 200, answer.text
    assert not raw(store)[0]["pass"], "the ciphertext survived an explicit clear"
    assert raw(store)[0]["host"] == "h.example"


def test_a_locked_clear_is_refused_rather_than_guessed(store):
    """Destroying a credential this process cannot read is the one irreversible
    thing to get wrong, so it refuses - and changes nothing on the way out."""
    pytest.importorskip("cryptography")
    client = api(store)
    pid = added(client)
    store.enable_vault("master")
    before = raw(store)

    answer = api(store).put(f"/api/proxies/{pid}", json={"pass": ""})

    assert answer.status_code == 423, answer.text
    assert raw(store) == before, "a refused clear still wrote to the pool"
