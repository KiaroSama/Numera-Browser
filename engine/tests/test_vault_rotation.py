"""Rotating the master password, and what must survive a failed rotation.

`enable_vault` told users to run `vault change` and no such command existed, so
there was no way to rotate a master password at all - which matters more here
than usual, because there is no recovery: a password you suspect is compromised
was previously permanent.

A rotation touches every secret under the root. Half of one is unrecoverable in
a way a half-finished ordinary write is not: some secrets under the old key,
some under the new, and no single password that opens the store. So the
interesting tests are the failures.
"""
import json

import pytest

from numera import cli, crypto, generate, vault
from numera.models import Profile, Proxy
from numera.server.stores import ProxyPool
from numera.store import ProfileStore

pytest.importorskip("cryptography")

SECRET = "PROXY-PASSWORD"
OLD, NEW = "old-master", "new-master"


def vaulted(root):
    engine = ProfileStore(root)
    pool = ProxyPool(root, engine)
    engine.save(Profile(name="p1", fingerprint=generate(seed=1),
                        proxy=Proxy(server="http://1.2.3.4:8080",
                                    username="u", password=SECRET)))
    pool.add_many([{"type": "HTTP", "host": "5.6.7.8", "port": "8080",
                    "user": "u", "pass": SECRET}])
    engine.enable_vault(OLD)
    return engine


def salt_of(root):
    return json.loads((root / "config.json").read_text(encoding="utf-8"))["vault"]["salt"]


def ciphertexts(root):
    return [v for _, v in
            [(p, h[s.field]) for p, s in vault.secret_files(root)
             for h in vault.holders(json.loads(p.read_text(encoding="utf-8")), s)]]


# --- the happy path, end to end ---------------------------------------------

def test_rotation_moves_every_secret_and_retires_the_old_password(tmp_path):
    vaulted(tmp_path)
    before_salt = salt_of(tmp_path)

    moved = ProfileStore(tmp_path).change_vault_password(OLD, NEW)

    assert moved >= 2, f"only {moved} secrets moved; the pool and the profile both hold one"
    assert salt_of(tmp_path) != before_salt, \
        "the salt was reused, so both passwords derive from the same input"

    # The new password opens everything.
    reopened = ProfileStore(tmp_path, password=NEW)
    assert reopened.unlocked()
    assert reopened.resolve("p1").proxy.password == SECRET
    assert ProxyPool(tmp_path, reopened).list()[0]["pass"] == SECRET

    # The old one opens nothing.
    assert not ProfileStore(tmp_path, password=OLD).unlocked(), \
        "the old master password still works after a rotation"


def test_the_cli_rotates(tmp_path):
    vaulted(tmp_path)

    code = cli.main(["--data-dir", str(tmp_path), "--password", OLD,
                     "vault", "change", "--new-password", NEW])

    assert code == 0
    assert ProfileStore(tmp_path, password=NEW).resolve("p1").proxy.password == SECRET


def test_the_wrong_current_password_changes_nothing(tmp_path):
    vaulted(tmp_path)
    before = ciphertexts(tmp_path)

    with pytest.raises(crypto.VaultLocked):
        ProfileStore(tmp_path).change_vault_password("not-the-password", NEW)

    assert ciphertexts(tmp_path) == before
    assert ProfileStore(tmp_path, password=OLD).unlocked(), "the old password stopped working"


# --- the failures are the point ---------------------------------------------

def test_a_failure_part_way_leaves_the_old_password_working(tmp_path, monkeypatch):
    """Every file back, vault descriptor unchanged. A half-rotated store has no
    password that opens all of it."""
    vaulted(tmp_path)
    before = ciphertexts(tmp_path)
    before_salt = salt_of(tmp_path)

    real = crypto.encrypt
    calls = {"n": 0}

    def fail_on_the_second(password, salt, plaintext):
        calls["n"] += 1
        if calls["n"] == 2:
            raise OSError("no space left on device")
        return real(password, salt, plaintext)

    monkeypatch.setattr(crypto, "encrypt", fail_on_the_second)
    with pytest.raises(OSError):
        ProfileStore(tmp_path).change_vault_password(OLD, NEW)
    monkeypatch.undo()

    assert salt_of(tmp_path) == before_salt, "the vault descriptor moved anyway"
    assert ciphertexts(tmp_path) == before, "secrets were left re-encrypted"
    store = ProfileStore(tmp_path, password=OLD)
    assert store.unlocked(), "the old password no longer opens the vault"
    assert store.resolve("p1").proxy.password == SECRET


def test_a_secret_this_build_cannot_open_stops_the_rotation(tmp_path):
    """Rotating past an unreadable record would leave it encrypted under a key
    nothing will ever have."""
    vaulted(tmp_path)
    raw = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    raw[0]["pass"] = "enc:v9:" + "Z" * 40
    (tmp_path / "proxies.json").write_text(json.dumps(raw), encoding="utf-8")
    before_salt = salt_of(tmp_path)

    with pytest.raises(ValueError):
        ProfileStore(tmp_path).change_vault_password(OLD, NEW)

    assert salt_of(tmp_path) == before_salt
    assert ProfileStore(tmp_path, password=OLD).unlocked()


def test_rotating_without_a_vault_is_refused(tmp_path):
    ProfileStore(tmp_path).save(Profile(name="p", fingerprint=generate(seed=3)))
    with pytest.raises(RuntimeError):
        ProfileStore(tmp_path).change_vault_password(OLD, NEW)


def test_an_empty_new_password_is_refused(tmp_path):
    vaulted(tmp_path)
    with pytest.raises(ValueError):
        ProfileStore(tmp_path).change_vault_password(OLD, "")
    assert ProfileStore(tmp_path, password=OLD).unlocked()
