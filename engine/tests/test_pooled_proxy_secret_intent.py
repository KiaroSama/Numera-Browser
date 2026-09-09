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


# --- intent is one question, the value's type is another --------------------
#
# The route keeps a raw `pass` so an explicit `""` survives normalisation as a
# CLEAR - but it kept every other raw value too, so `{"pass": 123}` reached the
# vault writer as an int. Without a vault it landed on disk as a number; with
# one it failed inside the encryptor rather than as a controlled 400.

BAD_SECRETS = {"a number": 123, "a boolean": True, "an object": {"a": 1},
               "a list": ["x"], "a float": 1.5}


@pytest.mark.parametrize("shape", sorted(BAD_SECRETS))
def test_an_edit_refuses_a_password_that_is_not_text(store, shape):
    client = api(store)
    pid = added(client)
    before = raw(store)

    answer = client.put(f"/api/proxies/{pid}", json={"pass": BAD_SECRETS[shape]})

    assert answer.status_code == 400, answer.text
    assert "password must be text" in answer.json()["detail"]
    assert raw(store) == before, "a malformed password reached the file"


@pytest.mark.parametrize("shape", sorted(BAD_SECRETS))
def test_create_refuses_the_same_shapes(store, shape):
    """Parity: the two ways a proxy gets a password answer alike."""
    client = api(store)

    answer = client.post("/api/proxies", json={"proxies": [
        {"type": "HTTP", "host": "h.example", "port": 8080,
         "user": "u", "pass": BAD_SECRETS[shape]}]})

    assert answer.status_code == 400, answer.text
    assert not (store.root / "proxies.json").exists() or raw(store) == [], \
        "a malformed password was stored on create"


def test_the_shapes_that_are_meant_to_work_still_do(store):
    """The control: refusing a type must not refuse an intent."""
    client = api(store)
    pid = added(client)

    for body, expected in (({"pass": "other"}, "other"),
                           ({"label": "renamed"}, "other"),
                           ({"pass": ""}, None)):
        assert client.put(f"/api/proxies/{pid}", json=body).status_code == 200
        got = client.get("/api/proxies").json()[0]["pass"]
        assert (got or None) == expected, f"{body} gave {got!r}"
