"""What a locked vault must refuse, and what it must never silently swallow.

Three defects with one shape: the code could not tell "there is no secret" from
"there is a secret I cannot read right now", so it treated the second as the
first and carried on. A proxy password vanished from an assignment, a password
change reported success without changing anything, and an envelope this build
does not understand was handed to a launcher as if it were a plaintext password.

The tests here drive the real entry points - `cli.main`, the stores, the API -
because every one of these bugs lived in the wiring rather than in the helper
the wiring was supposed to call. `_refuse_if_locked()` was correct and unit
tests of it passed while `pool assign` never reached it.
"""
import io
import json

import pytest

from numera import cli, crypto, generate, vault
from numera.models import Profile, Proxy
from numera.server.stores import DashboardStore, ProxyPool
from numera.store import ProfileStore

pytest.importorskip("cryptography")

SECRET = "PROXY-PASSWORD"
MASTER = "master"


def vaulted(root):
    """A root holding one profile and one pool proxy, then locked."""
    engine = ProfileStore(root)
    pool = ProxyPool(root, engine)
    engine.save(Profile(name="p1", fingerprint=generate(seed=1)))
    engine.save(Profile(name="p2", fingerprint=generate(seed=2)))
    pool.add_many([{"type": "HTTP", "host": "5.6.7.8", "port": "8080",
                    "user": "u", "pass": SECRET}])
    engine.enable_vault(MASTER)
    return engine


# --- 1. locked `pool assign` refuses before touching a profile --------------

def test_locked_pool_assign_refuses_and_changes_nothing(tmp_path, capsys):
    """Through `cli.main`, not through `_refuse_if_locked`.

    The guard existed and was correct; it was written inside the empty-pool
    branch *after* its `return`, so the live path never reached it. A helper
    test passed the whole time.
    """
    vaulted(tmp_path)
    before = {p.id: p.proxy for p in ProfileStore(tmp_path, password=MASTER).list()}
    assert all(v is None for v in before.values()), "fixture already assigned a proxy"

    code = cli.main(["--data-dir", str(tmp_path), "pool", "assign", "--all"])

    assert code == 1, "locked assign reported success"
    assert "master password" in capsys.readouterr().out.lower()
    after = ProfileStore(tmp_path, password=MASTER).list()
    assert all(p.proxy is None for p in after), \
        "a profile was assigned a proxy while the vault was locked"


def test_unlocked_pool_assign_carries_the_real_credential(tmp_path):
    """The other direction: unlocked, the password must survive the round trip."""
    vaulted(tmp_path)

    code = cli.main(["--data-dir", str(tmp_path), "--password", MASTER,
                     "pool", "assign", "--all"])

    assert code == 0
    profiles = ProfileStore(tmp_path, password=MASTER).list()
    assert profiles, "no profiles to check"
    for prof in profiles:
        assert prof.proxy is not None, f"{prof.name} was not assigned"
        assert prof.proxy.password == SECRET, \
            f"{prof.name} got {prof.proxy.password!r} instead of the real password"


# --- 2a. a locked caller may not replace a secret ---------------------------

def test_a_locked_plaintext_replacement_is_refused(tmp_path):
    """`encrypt_for_write` returned the *stored* ciphertext for a new plaintext.

    An empty or redacted value legitimately means "keep what is there". A
    non-empty new password does not, and returning the old ciphertext let a
    password change report success while the password never changed.
    """
    vaulted(tmp_path)
    locked = ProfileStore(tmp_path)                 # never given the password
    assert not locked.unlocked()
    stored = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    ciphertext = stored[0]["pass"]
    assert crypto.is_encrypted(ciphertext)

    with pytest.raises(crypto.VaultLocked):
        vault.encrypt_for_write(locked, "NEW-PASSWORD", stored=ciphertext)

    again = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    assert again[0]["pass"] == ciphertext, "the stored secret changed"


def test_a_locked_empty_value_still_preserves_the_secret(tmp_path):
    """The redaction path stays working - this is how a locked edit of a
    non-secret field round-trips the secret it cannot read."""
    vaulted(tmp_path)
    locked = ProfileStore(tmp_path)
    stored = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    ciphertext = stored[0]["pass"]

    assert vault.encrypt_for_write(locked, "", stored=ciphertext) == ciphertext
    assert vault.encrypt_for_write(locked, None, stored=ciphertext) == ciphertext


def test_a_locked_non_secret_edit_still_works(tmp_path):
    """Refusing a secret replacement must not refuse ordinary edits."""
    vaulted(tmp_path)
    locked = ProfileStore(tmp_path)
    pool = ProxyPool(tmp_path, locked)
    pid = pool.list()[0]["id"]

    pool.update(pid, {"country": "DE"})

    assert pool.get(pid)["country"] == "DE"
    stored = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    assert crypto.is_encrypted(stored[0]["pass"]), "the secret was damaged"


# --- 2b. an envelope we cannot open is not a password -----------------------

FUTURE = "enc:v9:" + "Z" * 40
CORRUPT = "enc:v2:not-valid-base64-!!!"


def test_an_unreadable_envelope_is_never_handed_out_as_a_password(tmp_path):
    """`is_encrypted()` knew only the current prefixes, so `enc:v9:` read back
    as ordinary plaintext and would have been used as the password itself."""
    engine = vaulted(tmp_path)
    raw = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    raw[0]["pass"] = FUTURE
    (tmp_path / "proxies.json").write_text(json.dumps(raw), encoding="utf-8")

    unlocked = ProfileStore(tmp_path, password=MASTER)
    pool = ProxyPool(tmp_path, unlocked)

    assert crypto.is_envelope(FUTURE), "a future version is still an envelope"
    assert not crypto.is_supported_envelope(FUTURE)
    assert pool.get(pool.list()[0]["id"])["pass"] != FUTURE, \
        "an unopened envelope was returned as the password"
    assert pool.secret_withheld(pool.list()[0]["id"]), \
        "an unreadable secret must count as withheld, unlocked vault or not"


def test_corrupt_ciphertext_counts_as_withheld_not_absent(tmp_path):
    """An unlocked vault can still hold one record it cannot decrypt - a foreign
    key, a truncated write. `is_withheld()` answered "not withheld" purely
    because the *verifier* opened, so export/test/assign dropped the credential.
    """
    engine = vaulted(tmp_path)
    raw = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    raw[0]["pass"] = CORRUPT
    (tmp_path / "proxies.json").write_text(json.dumps(raw), encoding="utf-8")

    unlocked = ProfileStore(tmp_path, password=MASTER)
    pool = ProxyPool(tmp_path, unlocked)
    pid = pool.list()[0]["id"]

    assert pool.secret_withheld(pid), "corrupt ciphertext reported as readable"


def test_cli_export_refuses_when_one_secret_is_unreadable(tmp_path, capsys):
    """The store-level verdict has to reach a real secret-dependent command."""
    engine = vaulted(tmp_path)
    raw = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    raw[0]["pass"] = FUTURE
    (tmp_path / "proxies.json").write_text(json.dumps(raw), encoding="utf-8")

    code = cli.main(["--data-dir", str(tmp_path), "--password", MASTER,
                     "pool", "export"])

    assert code == 1, "export blanked a secret it could not read"
    assert "cannot be read" in capsys.readouterr().out.lower()
