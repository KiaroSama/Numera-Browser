"""Work that must not be repeated, asserted as read counts rather than clocks.

A timing assertion is flaky and tells you nothing about *why*. These count file
reads and conversions instead - the thing that actually regressed, and the thing
a future change would silently undo.

Measured on a 60-profile, 60-proxy root with the vault on, before and after:

| path                      | reads before | reads after |
| ------------------------- | ------------ | ----------- |
| resolve 60 proxy ids      | 7260         | 60          |
| update 20 proxies         | 6040         | 20          |
| `GET /api/profiles`       | 240          | 120         |

The pool numbers were not mostly PBKDF2 - `vault()` and `unlocked()` each
re-read `config.json`, so decrypting one secret cost two full config reads.
"""
import json
from pathlib import Path

import pytest

pytest.importorskip("fastapi")
pytest.importorskip("cryptography")

from fastapi.testclient import TestClient                       # noqa: E402

from numera import crypto                                       # noqa: E402
from numera.server import create_app                            # noqa: E402
from numera.server.stores import ProxyPool                      # noqa: E402
from numera.store import ProfileStore                           # noqa: E402


class CountReads:
    """Count JSON reads, so a regression names itself."""

    def __enter__(self):
        self.n = 0
        self._real = Path.read_text
        outer = self

        def counted(inner_self, *a, **k):
            if inner_self.suffix == ".json":
                outer.n += 1
            return outer._real(inner_self, *a, **k)

        Path.read_text = counted
        return self

    def __exit__(self, *exc):
        Path.read_text = self._real


def vaulted_pool(tmp_path, n=8):
    engine = ProfileStore(tmp_path)
    pool = ProxyPool(tmp_path, engine)
    pool.add_many([{"type": "HTTP", "host": f"10.0.0.{i + 1}", "port": str(9000 + i),
                    "user": "u", "pass": f"secret-{i}"} for i in range(n)])
    engine.enable_vault("master")
    return ProxyPool(tmp_path, ProfileStore(tmp_path, password="master")), n


def test_reading_the_pool_does_not_reread_the_config_per_entry(tmp_path):
    """`vault()` and `unlocked()` both read `config.json`, and
    `decrypt_for_read` calls both - so one `list()` of N proxies did over 2N
    config reads on top of the pool file itself."""
    pool, n = vaulted_pool(tmp_path)
    with CountReads() as c:
        pool.list()
    assert c.n <= 3, f"{c.n} reads to list {n} proxies - the config is being re-read per entry"


def test_resolving_many_ids_costs_one_read_each_at_most(tmp_path):
    pool, n = vaulted_pool(tmp_path)
    ids = [p["id"] for p in pool.list()]
    with CountReads() as c:
        for i in ids:
            pool.get(i)
    assert c.n <= n + 2, f"{c.n} reads for {n} lookups"


def test_a_single_update_reads_the_pool_once(tmp_path):
    """`update` read the file through `list()` and again inside
    `_save_entries()` to rebuild the map needed to re-encrypt."""
    pool, _ = vaulted_pool(tmp_path)
    pid = pool.list()[0]["id"]
    with CountReads() as c:
        pool.update(pid, {"country": "DE"})
    assert c.n <= 2, f"{c.n} reads for one update"


def test_update_many_writes_once_for_the_whole_sweep(tmp_path):
    """A pool sweep updated each proxy separately - N locked read-modify-write
    cycles over the whole file for one button press."""
    pool, n = vaulted_pool(tmp_path)
    ids = [p["id"] for p in pool.list()]
    with CountReads() as c:
        touched = pool.update_many({i: {"country": "FR"} for i in ids})
    assert len(touched) == n
    assert c.n <= 2, f"{c.n} reads to update {n} proxies"
    assert all(p["country"] == "FR" for p in pool.list())


def test_update_many_preserves_the_secrets_it_did_not_touch(tmp_path):
    pool, _ = vaulted_pool(tmp_path)
    before = {p["id"]: p["pass"] for p in pool.list()}
    pool.update_many({i: {"country": "IT"} for i in list(before)[:3]})
    assert {p["id"]: p["pass"] for p in pool.list()} == before


def test_listing_profiles_reads_each_file_once(tmp_path):
    """The route read the dashboard store, read the engine store, and then read
    the dashboard store *again* for the response - on an endpoint the UI polls
    every three seconds."""
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    c.post("/api/profiles/batch", json={"profiles": [{"name": f"p{i}"} for i in range(6)]})

    with CountReads() as counter:
        c.get("/api/profiles")
    # 6 dashboard + 6 engine + config. Three listings would be 18 plus.
    assert counter.n <= 15, f"{counter.n} reads to list 6 profiles"


def test_health_converts_each_profile_once(tmp_path, monkeypatch):
    """`to_engine_profile` ran once for collision detection and again inside
    `dashboard_coherence` for every profile."""
    from numera.server import routes_profiles

    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    c.post("/api/profiles/batch", json={"profiles": [{"name": f"p{i}"} for i in range(5)]})

    calls = {"n": 0}
    real = routes_profiles.to_engine_profile
    monkeypatch.setattr(routes_profiles, "to_engine_profile",
                        lambda d: (calls.__setitem__("n", calls["n"] + 1), real(d))[1])
    c.get("/api/profiles/health")
    assert calls["n"] == 5, f"converted {calls['n']} times for 5 profiles"


def test_verifying_a_token_does_not_rewrite_the_file_every_time(tmp_path):
    """Every authenticated request rewrote the whole token file - atomic temp
    write, rename and chmod under the lock - to move `lastUsed` by
    milliseconds."""
    from numera.server.stores import TokenStore

    store = TokenStore(tmp_path)
    made = store.create("CI")
    path = tmp_path / "tokens.json"

    store.verify(made["token"])
    stamp = path.stat().st_mtime_ns
    for _ in range(20):
        assert store.verify(made["token"]) is not None
    assert path.stat().st_mtime_ns == stamp, "the token file was rewritten per request"


def test_a_revoked_token_still_stops_working_immediately(tmp_path):
    """The throttle must not delay revocation - that is a security property,
    not a bookkeeping one."""
    from numera.server.stores import TokenStore

    store = TokenStore(tmp_path)
    keep = store.create("keep")
    drop = store.create("drop")
    assert store.verify(drop["token"]) is not None
    store.revoke(drop["id"])
    assert store.verify(drop["token"]) is None
    assert store.verify(keep["token"]) is not None


# --- the versioned KDF ------------------------------------------------------

def test_a_v1_secret_still_opens_after_the_round_count_rose(tmp_path):
    """The round count is part of what the key was derived from, so raising it
    without a version would have made every existing vault unreadable."""
    salt = crypto.new_salt()
    legacy = "enc:v1:" + crypto._fernet("pw", salt, 200_000).encrypt(b"legacy").decode("ascii")
    assert crypto.is_encrypted(legacy)
    assert crypto.decrypt("pw", salt, legacy) == "legacy"


def test_new_secrets_are_written_at_the_current_cost():
    salt = crypto.new_salt()
    token = crypto.encrypt("pw", salt, "fresh")
    assert token.startswith(f"enc:{crypto.CURRENT_VERSION}:")
    assert crypto.ROUNDS_BY_VERSION[crypto.CURRENT_VERSION] == 600_000
    assert crypto.decrypt("pw", salt, token) == "fresh"


def test_a_secret_from_a_newer_version_says_so(tmp_path):
    """Better a clear message than 'wrong master password' for a file this
    engine simply cannot read yet."""
    salt = crypto.new_salt()
    with pytest.raises(crypto.VaultLocked, match="newer version"):
        crypto.decrypt("pw", salt, "enc:v9:whatever")


def test_a_whole_vault_round_trips_across_the_version_change(tmp_path):
    from numera import generate
    from numera.models import Profile, Proxy

    store = ProfileStore(tmp_path)
    store.save(Profile(name="p", fingerprint=generate(seed=1),
                       proxy=Proxy(server="http://1.2.3.4:8080",
                                   username="u", password="SECRET")))
    store.enable_vault("master")
    raw = json.loads((tmp_path / "profiles").glob("*.json").__next__()
                     .read_text(encoding="utf-8"))
    assert raw["proxy"]["password"].startswith("enc:v2:")
    assert ProfileStore(tmp_path, password="master").resolve("p").proxy.password == "SECRET"


# --- bulk delete ------------------------------------------------------------

def test_deleting_many_profiles_takes_one_request(tmp_path):
    """The dashboard looped `DELETE` per selection while create and edit were
    both batched."""
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    ids = c.post("/api/profiles/batch",
                 json={"profiles": [{"name": f"p{i}"} for i in range(5)]}).json()["ids"]

    r = c.post("/api/profiles/delete", json={"ids": ids[:3]})
    assert r.status_code == 200, r.json()
    assert sorted(r.json()["removed"]) == sorted(ids[:3])
    assert r.json()["failed"] == []
    assert len(c.get("/api/profiles").json()) == 2


def test_bulk_delete_reports_ids_that_were_not_there(tmp_path):
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    made = c.post("/api/profiles", json={"name": "real"}).json()
    r = c.post("/api/profiles/delete", json={"ids": [made["id"], "deadbeef1234"]})
    assert r.status_code == 200, r.json()
    assert r.json()["removed"] == [made["id"]]
    assert r.json()["missing"] == ["deadbeef1234"]


def test_a_malformed_bulk_delete_is_a_4xx(tmp_path):
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    for body in ({"ids": "nope"}, {"ids": [1, 2]}, {}):
        assert c.post("/api/profiles/delete", json=body).status_code in (400, 422), body
