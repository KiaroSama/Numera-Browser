"""Named API tokens really gate the HTTP surface.

`NUMERA_API_TOKEN` is one shared secret: rotating it locks out every client at
once and nothing records what a token was for. Named tokens only earn their
keep if minting one turns auth on, revoking one closes that door alone, and the
env var keeps working for whatever already uses it.
"""
import json

import pytest

from numera.server.stores import TokenStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient  # noqa: E402

from numera.server import create_app  # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def auth(token):
    return {"Authorization": f"Bearer {token}"}


# --- the store -------------------------------------------------------------

def test_the_value_is_never_written_to_disk(tmp_path):
    """A token readable out of a file is one the file must be guarded like the
    secret itself, and verification only ever needs the digest."""
    store = TokenStore(tmp_path)
    made = store.create("CI runner")
    on_disk = (tmp_path / "tokens.json").read_text(encoding="utf-8")
    assert made["token"] not in on_disk
    assert "hash" not in made or made["hash"] in on_disk
    assert set(store.list()[0]) == {"id", "name", "hash", "created", "lastUsed"}


def test_verify_matches_only_the_right_token(tmp_path):
    store = TokenStore(tmp_path)
    made = store.create("one")
    assert store.verify(made["token"])["name"] == "one"
    assert store.verify(made["token"] + "x") is None
    assert store.verify("") is None


def test_using_a_token_records_when(tmp_path):
    store = TokenStore(tmp_path)
    made = store.create("one")
    assert store.list()[0]["lastUsed"] is None
    store.verify(made["token"])
    assert store.list()[0]["lastUsed"] is not None


def test_a_corrupt_token_file_does_not_lock_everything_out(tmp_path):
    (tmp_path / "tokens.json").write_text("{ not json", encoding="utf-8")
    assert TokenStore(tmp_path).list() == []


# --- the API ---------------------------------------------------------------

def test_with_no_tokens_and_no_env_var_the_api_is_open(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    assert client(tmp_path).get("/api/health").status_code == 200


def test_minting_a_token_turns_auth_on(tmp_path, monkeypatch):
    """Otherwise the table is decoration: tokens that exist and gate nothing."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    TokenStore(tmp_path).create("CI runner")
    c = client(tmp_path)
    assert c.get("/api/health").status_code == 401
    assert c.get("/api/health", headers=auth("nmr_wrong")).status_code == 401


def test_a_named_token_opens_the_api(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    made = TokenStore(tmp_path).create("CI runner")
    assert client(tmp_path).get("/api/health", headers=auth(made["token"])).status_code == 200


def test_revoking_one_token_leaves_the_others_working(tmp_path, monkeypatch):
    """The reason for having names at all."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    store = TokenStore(tmp_path)
    keep, drop = store.create("keep"), store.create("drop")
    store.revoke(drop["id"])
    c = client(tmp_path)
    assert c.get("/api/health", headers=auth(keep["token"])).status_code == 200
    assert c.get("/api/health", headers=auth(drop["token"])).status_code == 401


def test_the_env_var_keeps_working_after_a_named_token_exists(tmp_path, monkeypatch):
    """It is the credential the dashboard and any existing script already use;
    pulling it out from under them on the first named token would be a trap."""
    monkeypatch.setenv("NUMERA_API_TOKEN", "legacy-secret")
    made = TokenStore(tmp_path).create("CI runner")
    c = client(tmp_path)
    assert c.get("/api/health", headers=auth("legacy-secret")).status_code == 200
    assert c.get("/api/health", headers=auth(made["token"])).status_code == 200


def test_revoking_every_named_token_does_not_reopen_a_locked_server(tmp_path, monkeypatch):
    monkeypatch.setenv("NUMERA_API_TOKEN", "legacy-secret")
    store = TokenStore(tmp_path)
    store.revoke(store.create("one")["id"])
    assert client(tmp_path).get("/api/health").status_code == 401


def test_the_listing_never_returns_a_hash_or_a_value(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    made = TokenStore(tmp_path).create("CI runner")
    c = client(tmp_path)
    body = c.get("/api/tokens", headers=auth(made["token"])).json()
    assert body["envToken"] is False
    assert [t["name"] for t in body["tokens"]] == ["CI runner"]
    text = json.dumps(body)
    assert "hash" not in text and made["token"] not in text


def test_the_listing_says_whether_the_env_var_is_also_in_play(tmp_path, monkeypatch):
    """Someone who revokes every named token and still gets 401s needs to be
    told why."""
    monkeypatch.setenv("NUMERA_API_TOKEN", "legacy-secret")
    c = client(tmp_path)
    assert c.get("/api/tokens", headers=auth("legacy-secret")).json()["envToken"] is True


def test_a_token_must_be_named(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    c = client(tmp_path)
    assert c.post("/api/tokens", json={"name": "  "}).status_code == 400


def test_a_duplicate_name_is_refused(tmp_path, monkeypatch):
    """Two tokens called the same thing cannot be told apart when revoking."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    c = client(tmp_path)
    first = c.post("/api/tokens", json={"name": "CI"}).json()
    assert c.post("/api/tokens", json={"name": "CI"},
                  headers=auth(first["token"])).status_code == 409


def test_the_value_comes_back_exactly_once(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    c = client(tmp_path)
    made = c.post("/api/tokens", json={"name": "CI"}).json()
    assert made["token"].startswith("nmr_")
    listed = c.get("/api/tokens", headers=auth(made["token"])).json()["tokens"][0]
    assert "token" not in listed


def test_the_config_reports_how_many_proxies_are_stored(tmp_path, monkeypatch):
    """Settings showed "0 proxies" beside a pool of 27: the only number the
    dashboard had was how many *profiles were using* one, which is a different
    question from what is on disk."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    from numera.server.stores import ProxyPool

    ProxyPool(tmp_path).add_many([
        {"type": "HTTP", "host": "1.1.1.1", "port": 8080},
        {"type": "HTTP", "host": "2.2.2.2", "port": 8080},
    ])
    assert client(tmp_path).get("/api/config").json()["proxyCount"] == 2
