"""One bad record must not sink the operation; two guards must not have holes.

Three findings that share a shape: something strict enough to be right in the
happy case and brittle enough to fail the whole request in an ordinary one -
plus the key-derivation cache, which is the same code path made affordable.
"""
import json

import pytest

from numera import crypto, generate
from numera.models import Profile, Proxy
from numera.store import ProfileStore

pytest.importorskip("cryptography")


# --- one unreadable secret does not hide every other profile ---------------

def test_a_profile_encrypted_under_another_key_does_not_hide_the_rest(tmp_path):
    """The realistic trigger is a restore from another machine or a rotated
    vault: `unlocked()` says the *current* verifier matches, not that this
    record was encrypted with it. The decrypt then raised and took the entire
    listing with it - not one broken row, an empty dashboard."""
    store = ProfileStore(tmp_path)
    good = store.save(Profile(name="good", fingerprint=generate(seed=1)))
    other = store.save(Profile(name="other", fingerprint=generate(seed=2),
                               proxy=Proxy(server="http://1.2.3.4:8080",
                                           username="u", password="SECRET")))
    store.enable_vault("master")

    # Hand-write a token from a different vault over the second profile.
    foreign = crypto.encrypt("a-different-password", crypto.new_salt(), "SECRET")
    path = tmp_path / "profiles" / f"{other.id}.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    data["proxy"]["password"] = foreign
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    unlocked = ProfileStore(tmp_path, password="master")
    names = [p.name for p in unlocked.list()]
    assert "good" in names, f"the whole listing was lost: {names}"
    assert unlocked.get(good.id) is not None


def test_an_unreadable_secret_reads_as_none_not_ciphertext(tmp_path):
    """Degrading must not leak the blob onwards - the launcher would take it
    straight to the browser as a password."""
    store = ProfileStore(tmp_path)
    prof = store.save(Profile(name="p", fingerprint=generate(seed=1),
                              proxy=Proxy(server="http://1.2.3.4:8080",
                                          username="u", password="SECRET")))
    store.enable_vault("master")
    path = tmp_path / "profiles" / f"{prof.id}.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    data["proxy"]["password"] = crypto.encrypt("other", crypto.new_salt(), "SECRET")
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    got = ProfileStore(tmp_path, password="master").get(prof.id)
    assert got is not None
    assert got.proxy.password is None
    assert not crypto.is_encrypted(got.proxy.password or "")


# --- a proxy field this version does not know about ------------------------

def test_a_proxy_with_an_unknown_field_still_loads(tmp_path):
    """`Fingerprint.from_dict` filters unknown keys so an older engine can read
    a newer file. `Proxy.from_dict` did not, so any added proxy field became a
    500 in `get()`."""
    store = ProfileStore(tmp_path)
    prof = store.save(Profile(name="p", fingerprint=generate(seed=1),
                              proxy=Proxy(server="http://1.2.3.4:8080")))
    path = tmp_path / "profiles" / f"{prof.id}.json"
    data = json.loads(path.read_text(encoding="utf-8"))
    data["proxy"]["somethingNewerAdded"] = "value"
    path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    got = ProfileStore(tmp_path).get(prof.id)
    assert got is not None, "an unknown proxy field made the profile unreadable"
    assert got.proxy.server == "http://1.2.3.4:8080"


# --- the importer tolerates what vendors actually put in files -------------

@pytest.mark.parametrize("value", ["8 cores", "N/A", "", None, "8.5"])
def test_a_non_numeric_core_count_does_not_raise(value):
    """A bare `int()` raised, and the import route saves as it goes - so one odd
    value aborted the request and left the earlier profiles already written."""
    from numera.importers import profile_from_export

    prof, _issues = profile_from_export({"name": "x",
                                        "navigator": {"hardwareConcurrency": value}})
    assert isinstance(prof.fingerprint.hardware_concurrency, int)
    assert prof.fingerprint.hardware_concurrency > 0


def test_a_numeric_core_count_is_still_honoured():
    from numera.importers import profile_from_export

    prof, _issues = profile_from_export({"name": "x",
                                        "navigator": {"hardwareConcurrency": 12}})
    assert prof.fingerprint.hardware_concurrency == 12


def test_importing_a_file_with_one_bad_entry_is_a_4xx_not_a_500(tmp_path):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    payload = json.dumps([{"name": "one", "navigator": {"hardwareConcurrency": "8 cores"}}])
    r = c.post("/api/profiles/import", json={"text": payload})
    assert r.status_code < 500, f"a vendor quirk became a server error: {r.status_code}"


# --- the key is derived once ------------------------------------------------

def test_the_vault_key_is_derived_once_per_password_and_salt(monkeypatch):
    """200,000 PBKDF2 rounds ran on *every* encrypt and decrypt, for a key that
    is identical every time - a vault has one salt. Listing the pool decrypts
    every entry and saving it re-encrypts every entry, so a single sweep paid
    this on the order of N-squared."""
    import hashlib

    crypto._derive_key.cache_clear()
    calls = {"n": 0}
    real = hashlib.pbkdf2_hmac

    def counted(*a, **k):
        calls["n"] += 1
        return real(*a, **k)

    monkeypatch.setattr(hashlib, "pbkdf2_hmac", counted)
    salt = crypto.new_salt()
    for _ in range(5):
        token = crypto.encrypt("pw", salt, "secret")
        assert crypto.decrypt("pw", salt, token) == "secret"
    assert calls["n"] == 1, f"derived the key {calls['n']} times, expected 1"


def test_a_different_password_or_salt_derives_again(monkeypatch):
    """The cache is keyed, not global - two vaults must not share a key."""
    import hashlib

    crypto._derive_key.cache_clear()
    calls = {"n": 0}
    real = hashlib.pbkdf2_hmac

    monkeypatch.setattr(hashlib, "pbkdf2_hmac",
                        lambda *a, **k: (calls.__setitem__("n", calls["n"] + 1), real(*a, **k))[1])
    salt_a, salt_b = crypto.new_salt(), crypto.new_salt()
    crypto.encrypt("pw", salt_a, "x")
    crypto.encrypt("pw", salt_b, "x")
    crypto.encrypt("other", salt_a, "x")
    assert calls["n"] == 3


def test_the_round_trip_still_works_across_store_instances(tmp_path):
    """The cache must be invisible to behaviour."""
    store = ProfileStore(tmp_path)
    store.save(Profile(name="p", fingerprint=generate(seed=1),
                       proxy=Proxy(server="http://1.2.3.4:8080",
                                   username="u", password="SECRET")))
    store.enable_vault("master")
    assert ProfileStore(tmp_path, password="master").resolve("p").proxy.password == "SECRET"


# --- the rotate fetch stays on the public internet -------------------------

@pytest.mark.parametrize("url", [
    "http://127.0.0.1/rotate",
    "http://localhost:8787/rotate",
    "http://169.254.169.254/latest/meta-data/",
    "http://10.0.0.5/rotate",
    "http://192.168.1.1/rotate",
    "http://[::1]/rotate",
])
def test_rotate_refuses_a_private_destination(url):
    """The one place the engine fetches a caller-supplied address, and it runs
    from the server's own network position. Only the scheme was checked."""
    from numera import proxypool

    problem = proxypool._public_destination(url)
    assert problem, f"{url} was allowed"

    res = proxypool.rotate({"rotateUrl": url, "host": "1.2.3.4", "port": "8080"})
    assert res["rotated"] is False
    assert res["ok"] is None


def test_rotate_still_allows_a_real_provider_url():
    """The guard must not break the feature: a public host is fine."""
    from numera import proxypool

    assert proxypool._public_destination("https://example.com/api/rotate") is None


@pytest.mark.parametrize("url", ["ftp://example.com/x", "file:///etc/passwd", "gopher://x/"])
def test_rotate_still_refuses_a_non_http_scheme(url):
    from numera import proxypool

    assert proxypool._public_destination(url)


# --- an empty bind host is a bind-all, not loopback ------------------------

def test_an_empty_host_is_not_treated_as_loopback(tmp_path):
    """`--host ""` binds every interface. Listing `""` as loopback waved the
    fully-capable unauthenticated API straight through the guard that exists to
    stop exactly that."""
    pytest.importorskip("fastapi")
    from numera import server

    assert "" not in server.LOOPBACK
    assert server._refuse_insecure_bind("", tmp_path) is not None


@pytest.mark.parametrize("host", ["127.0.0.1", "localhost", "::1"])
def test_real_loopback_still_needs_no_credential(tmp_path, host):
    pytest.importorskip("fastapi")
    from numera import server

    assert server._refuse_insecure_bind(host, tmp_path) is None
