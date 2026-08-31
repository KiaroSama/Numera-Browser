"""Encrypt-at-rest for proxy passwords.

Skipped wholesale when the optional crypto backend isn't installed, so the core
test run stays dependency-free.
"""

import json

import pytest

from numera import crypto
from numera.models import Profile, Proxy
from numera.store import ProfileStore
from numera.fingerprint import generate

pytestmark = pytest.mark.skipif(not crypto.available(),
                                reason="cryptography not installed")


def mk(store, name="acct", password="s3cret"):
    prof = Profile(name=name, fingerprint=generate(seed=1),
                   proxy=Proxy(server="http://h:8080", username="u", password=password))
    return store.save(prof)


def test_round_trip():
    salt = crypto.new_salt()
    token = crypto.encrypt("pw", salt, "hello")
    assert crypto.is_encrypted(token)
    assert crypto.decrypt("pw", salt, token) == "hello"


def test_wrong_password_raises():
    salt = crypto.new_salt()
    token = crypto.encrypt("right", salt, "hello")
    with pytest.raises(crypto.VaultLocked):
        crypto.decrypt("wrong", salt, token)


def test_verifier_checks_password():
    salt = crypto.new_salt()
    v = crypto.make_verifier("pw", salt)
    assert crypto.check_password("pw", salt, v)
    assert not crypto.check_password("nope", salt, v)


def test_enable_encrypts_existing_secret(tmp_path):
    store = ProfileStore(tmp_path, password="master")
    prof = mk(store)
    store.enable_vault("master")

    # On disk the password is a ciphertext token, not the plaintext.
    raw = json.loads((store.profiles_dir / f"{prof.id}.json").read_text())
    assert crypto.is_encrypted(raw["proxy"]["password"])
    assert "s3cret" not in json.dumps(raw)


def test_unlocked_store_reads_the_secret_back(tmp_path):
    store = ProfileStore(tmp_path, password="master")
    mk(store)
    store.enable_vault("master")

    reopened = ProfileStore(tmp_path, password="master")
    assert reopened.unlocked()
    assert reopened.resolve("acct").proxy.password == "s3cret"


def test_locked_store_hides_the_secret(tmp_path):
    store = ProfileStore(tmp_path, password="master")
    mk(store)
    store.enable_vault("master")

    locked = ProfileStore(tmp_path)  # no password
    assert locked.vault_enabled()
    assert not locked.unlocked()
    # Everything else still loads; only the secret is withheld.
    prof = locked.resolve("acct")
    assert prof.name == "acct"
    assert prof.proxy.password is None


def test_wrong_password_leaves_store_locked(tmp_path):
    store = ProfileStore(tmp_path, password="master")
    mk(store)
    store.enable_vault("master")
    assert not ProfileStore(tmp_path, password="nope").unlocked()


def test_saving_a_secret_while_locked_is_refused(tmp_path):
    store = ProfileStore(tmp_path, password="master")
    mk(store)
    store.enable_vault("master")

    locked = ProfileStore(tmp_path)
    prof = locked.resolve("acct")           # password comes back None
    prof.proxy.password = "new-plaintext"   # user tries to change it while locked
    with pytest.raises(crypto.VaultLocked):
        locked.save(prof)                   # must not silently write plaintext


def test_non_secret_profiles_save_fine_while_locked(tmp_path):
    store = ProfileStore(tmp_path, password="master")
    store.enable_vault("master")
    locked = ProfileStore(tmp_path)
    # A profile with no proxy password has no secret, so the vault doesn't block it.
    p = Profile(name="noproxy", fingerprint=generate(seed=2))
    assert locked.save(p).name == "noproxy"


def test_enable_is_idempotent_guard(tmp_path):
    store = ProfileStore(tmp_path, password="master")
    store.enable_vault("master")
    with pytest.raises(RuntimeError):
        store.enable_vault("master")


def test_a_non_secret_edit_while_locked_keeps_the_password(tmp_path):
    """The destruction path the audit found, asserted against `ProfileStore`
    itself rather than through the dashboard store.

    A locked `get()` hands back None for the secret - deliberately, so the
    launcher never tries to authenticate with ciphertext. An ordinary edit then
    came back through `save()` carrying that None and wrote it straight over the
    encrypted value. The password was gone and nothing said so.
    """
    store = ProfileStore(tmp_path, password="master")
    mk(store)
    store.enable_vault("master")
    ciphertext = json.loads(
        (tmp_path / "profiles" / store.resolve("acct").id).with_suffix(".json")
        .read_text(encoding="utf-8"))["proxy"]["password"]
    assert crypto.is_encrypted(ciphertext)

    locked = ProfileStore(tmp_path)
    prof = locked.resolve("acct")
    assert prof.proxy.password is None          # withheld, as it should be
    prof.name = "renamed while locked"
    locked.save(prof)                            # a perfectly ordinary edit

    on_disk = json.loads(
        (tmp_path / "profiles" / prof.id).with_suffix(".json").read_text(encoding="utf-8"))
    assert on_disk["proxy"]["password"] == ciphertext, "the encrypted password was destroyed"
    assert on_disk["name"] == "renamed while locked"

    # And it is still the real password once unlocked.
    unlocked = ProfileStore(tmp_path, password="master")
    assert unlocked.resolve("renamed while locked").proxy.password == "s3cret"


def test_an_interrupted_save_does_not_truncate_a_profile(tmp_path, monkeypatch):
    """Profiles were written with a direct `write_text`, so a failure halfway
    left an unreadable file where a profile used to be."""
    store = ProfileStore(tmp_path)
    prof = mk(store)
    good = (tmp_path / "profiles" / prof.id).with_suffix(".json").read_text(encoding="utf-8")

    import numera.paths as _paths
    real = _paths.json.dumps
    monkeypatch.setattr(_paths.json, "dumps",
                        lambda *a, **k: (_ for _ in ()).throw(RuntimeError("disk full")))
    with pytest.raises(RuntimeError):
        prof.notes = "this write fails"
        store.save(prof)
    monkeypatch.setattr(_paths.json, "dumps", real)

    assert (tmp_path / "profiles" / prof.id).with_suffix(".json").read_text(
        encoding="utf-8") == good
