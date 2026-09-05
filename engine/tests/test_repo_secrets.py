"""The coordinator must not be the hole in the vault.

`ProfileRepo` wrote both records straight into the batch:

    batch.write(engine_path, store._apply_secrets(profile.to_dict(), "enc"))
    batch.write(dash_path, merged)

which skipped `vault.encrypt_for_write` - the one function that gets all four
secret cases right. Two ways that lost data, and they pull in opposite
directions:

* **Locked.** `resolve()` hands back `proxy.password = None`, because a locked
  read refuses to yield ciphertext. Writing that object back put `None` over
  the stored ciphertext in *both* copies. The credential was gone, and nothing
  said so.
* **Unlocked.** The engine copy was encrypted, but `to_dashboard_profile()`
  carries `proxy.pass` in the clear, and it went to disk that way - a readable
  credential reintroduced under an enabled vault by an ordinary CLI edit.

The invariant these tests hold: after any successful repo write,
`vault.plaintext_secrets(root)` is empty, and every stored credential still
opens with the current password.
"""
import json

import pytest

from numera import cli, crypto, generate, vault
from numera.models import Profile, Proxy
from numera.repo import repo
from numera.store import ProfileStore

pytest.importorskip("cryptography")

MASTER = "master"


def credentialed(root, name="creds", password="SECRET") -> Profile:
    prof = Profile(name=name, fingerprint=generate(seed=1),
                   proxy=Proxy(server="http://1.2.3.4:8080", username="u",
                               password=password))
    repo(ProfileStore(root)).create(prof)
    return prof


def dash_raw(root, pid) -> dict:
    return json.loads((root / "dashboard" / f"{pid}.json").read_text(encoding="utf-8"))


def engine_raw(root, pid) -> dict:
    return json.loads((root / "profiles" / f"{pid}.json").read_text(encoding="utf-8"))


# --- unlocked: both schemas encrypted ---------------------------------------

def test_a_repo_create_under_an_unlocked_vault_leaves_no_plaintext(tmp_path):
    ProfileStore(tmp_path).enable_vault(MASTER)
    store = ProfileStore(tmp_path, password=MASTER)
    prof = Profile(name="fresh", fingerprint=generate(seed=1),
                   proxy=Proxy(server="http://1.2.3.4:8080", username="u",
                               password="SECRET"))
    repo(store).create(prof)

    assert not vault.plaintext_secrets(tmp_path), \
        "the dashboard copy went to disk in the clear"
    assert crypto.is_encrypted(engine_raw(tmp_path, prof.id)["proxy"]["password"])
    assert crypto.is_encrypted(dash_raw(tmp_path, prof.id)["proxy"]["pass"])
    # And both views decrypt to the same thing.
    assert ProfileStore(tmp_path, password=MASTER).resolve("fresh").proxy.password == "SECRET"


def test_a_repo_edit_under_an_unlocked_vault_leaves_no_plaintext(tmp_path):
    prof = credentialed(tmp_path)
    ProfileStore(tmp_path).enable_vault(MASTER)

    store = ProfileStore(tmp_path, password=MASTER)
    repo(store).edit("creds", lambda p: setattr(p, "notes", "edited") or p)

    assert not vault.plaintext_secrets(tmp_path)
    assert ProfileStore(tmp_path, password=MASTER).resolve("creds").proxy.password == "SECRET"


def test_the_cli_edit_path_leaves_no_plaintext(tmp_path):
    """The whole point is that the surface does not matter."""
    prof = credentialed(tmp_path)
    ProfileStore(tmp_path).enable_vault(MASTER)

    assert cli.main(["--data-dir", str(tmp_path), "--password", MASTER,
                     "edit", "creds", "--notes", "from the cli"]) == 0
    assert not vault.plaintext_secrets(tmp_path)
    assert dash_raw(tmp_path, prof.id)["notes"] == "from the cli"


# --- locked: the ciphertext survives ----------------------------------------

def test_a_locked_notes_edit_keeps_both_ciphertexts_untouched(tmp_path):
    prof = credentialed(tmp_path)
    ProfileStore(tmp_path).enable_vault(MASTER)
    before_engine = engine_raw(tmp_path, prof.id)["proxy"]["password"]
    before_dash = dash_raw(tmp_path, prof.id)["proxy"]["pass"]
    assert crypto.is_encrypted(before_engine)

    locked = ProfileStore(tmp_path)                     # no password
    repo(locked).edit("creds", lambda p: setattr(p, "notes", "while locked") or p)

    assert engine_raw(tmp_path, prof.id)["proxy"]["password"] == before_engine, \
        "a locked edit wrote None over the stored credential"
    # The dashboard's blob may legitimately CHANGE here without being rewritten
    # in the crypto sense: `enable_vault` seals each file independently, and
    # Fernet's IV is random, so the two copies started with different
    # ciphertexts for the same secret. `_seal` gives both the engine's, which
    # is the stronger invariant - one credential, one stored value.
    after_dash = dash_raw(tmp_path, prof.id)["proxy"]["pass"]
    assert crypto.is_encrypted(after_dash), "the dashboard copy lost its ciphertext"
    assert after_dash == before_engine, "the two copies no longer hold one value"
    assert before_dash != "" and crypto.is_encrypted(before_dash)
    assert dash_raw(tmp_path, prof.id)["notes"] == "while locked"
    # Still readable with the real password.
    assert ProfileStore(tmp_path, password=MASTER).resolve("creds").proxy.password == "SECRET"


def test_a_locked_password_replacement_is_refused_and_changes_nothing(tmp_path):
    prof = credentialed(tmp_path)
    ProfileStore(tmp_path).enable_vault(MASTER)
    before_engine = engine_raw(tmp_path, prof.id)
    before_dash = dash_raw(tmp_path, prof.id)

    locked = ProfileStore(tmp_path)

    def replace(p):
        p.proxy.password = "NEW-PLAINTEXT"
        return p

    with pytest.raises(crypto.VaultLocked):
        repo(locked).edit("creds", replace)

    assert engine_raw(tmp_path, prof.id) == before_engine, "a refused write still landed"
    assert dash_raw(tmp_path, prof.id) == before_dash


# --- save_many has the same guarantees --------------------------------------

def test_save_many_seals_every_secret(tmp_path):
    ProfileStore(tmp_path).enable_vault(MASTER)
    store = ProfileStore(tmp_path, password=MASTER)
    profiles = [Profile(name=f"b{i}", fingerprint=generate(seed=i),
                        proxy=Proxy(server=f"http://1.2.3.{i}:8080", username="u",
                                    password="SECRET"))
                for i in range(3)]
    repo(store).save_many(profiles)

    assert not vault.plaintext_secrets(tmp_path)
    reader = ProfileStore(tmp_path, password=MASTER)
    for i in range(3):
        assert reader.resolve(f"b{i}").proxy.password == "SECRET"


def test_one_locked_replacement_refuses_the_whole_batch(tmp_path):
    first = credentialed(tmp_path, "one")
    second = credentialed(tmp_path, "two")
    ProfileStore(tmp_path).enable_vault(MASTER)
    before = engine_raw(tmp_path, first.id)

    locked = ProfileStore(tmp_path)
    a = locked.resolve("one")
    b = locked.resolve("two")
    a.notes = "fine"
    b.proxy.password = "NEW-PLAINTEXT"          # the one that must refuse

    with pytest.raises(crypto.VaultLocked):
        repo(locked).save_many([a, b])

    assert engine_raw(tmp_path, first.id) == before, \
        "the batch wrote the acceptable half before hitting the refusal"


# --- rotation still opens everything the repo wrote --------------------------

def test_a_password_rotation_after_repo_writes_can_read_every_credential(tmp_path):
    ProfileStore(tmp_path).enable_vault(MASTER)
    store = ProfileStore(tmp_path, password=MASTER)
    for i in range(3):
        repo(store).create(Profile(
            name=f"p{i}", fingerprint=generate(seed=i),
            proxy=Proxy(server=f"http://1.2.3.{i}:8080", username="u",
                        password="SECRET")))

    ProfileStore(tmp_path, password=MASTER).change_vault_password(MASTER, "second")

    reader = ProfileStore(tmp_path, password="second")
    assert reader.unlocked()
    for i in range(3):
        assert reader.resolve(f"p{i}").proxy.password == "SECRET", \
            f"p{i} was sealed with something the rotation could not re-key"
    assert not vault.plaintext_secrets(tmp_path)


# --- store metadata invariants ----------------------------------------------

def test_a_new_twin_is_born_with_created_and_an_edit_advances_updated_at(tmp_path):
    import time

    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="meta", fingerprint=generate(seed=1)))
    twin = dash_raw(tmp_path, prof.id)
    assert twin.get("_created"), "the dashboard twin has no creation stamp"
    assert twin["_rev"] == 1

    first_updated = engine_raw(tmp_path, prof.id)["updated_at"]
    time.sleep(0.01)
    repo(store).edit("meta", lambda p: setattr(p, "notes", "n") or p)

    assert engine_raw(tmp_path, prof.id)["updated_at"] > first_updated
    after = dash_raw(tmp_path, prof.id)
    assert after["_created"] == twin["_created"], "the creation stamp moved"
    assert after["_rev"] == 2
