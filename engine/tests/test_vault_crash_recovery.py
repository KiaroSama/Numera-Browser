"""A vault migration that is killed must not strand ciphertext without its salt.

`enable_vault` encrypts every secret file and *then* writes the vault
descriptor - the salt and verifier - into the config. `_Batch` keeps the undo
bytes for those files in memory, and `write_batch` rolls back from an
`except` clause. Neither survives `kill -9`, a power loss or a closed laptop
lid.

So there is a window with the files already holding new ciphertext and the
config still saying no vault exists. Nothing can open those secrets afterwards:
the salt they were encrypted under was only ever in RAM. Ordinary exception
rollback works and always did - that is not what this is about.

The fix is a journal written *before* the first destructive write, holding the
salt and verifier but never the password and never a plaintext secret, plus
recovery on the next open. Recovery rolls **forward**: the files are already
encrypted under that salt, so committing the descriptor is what makes them
readable again. Rolling back would mean storing the plaintext in the journal,
which is a worse bug than the one being fixed.

These kill real child processes at the boundary, because the whole defect is
what `except` does not run.
"""

import json
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

pytest.importorskip("cryptography")

ENGINE = str(Path(__file__).resolve().parents[1])

#: Encrypt every secret, then die before the descriptor is committed - exactly
#: the window the journal exists to cover. The kill is `os._exit`, which runs
#: no `finally`, no `except` and no `atexit`: a real hard kill.
CRASH_ENABLE = """
import os, sys
sys.path.insert(0, {engine!r})
from numera import vault
from numera.store import ProfileStore

store = ProfileStore({root!r})
real = store._save_config_locked

def die(cfg):
    # The secret files are written by now; the descriptor is not.
    os._exit(9)

store._save_config_locked = die
try:
    store.enable_vault("master")
except BaseException:
    os._exit(9)
"""


def _make_store(tmp_path):
    from numera import generate
    from numera.models import Profile, Proxy
    from numera.repo import repo
    from numera.store import ProfileStore

    store = ProfileStore(tmp_path)
    repo(store).create(Profile(
        name="alpha", fingerprint=generate(seed=1),
        proxy=Proxy(server="http://h:1", username="u", password="secret-one")))
    return store


def _crash(tmp_path, script):
    out = subprocess.run(
        [sys.executable, "-c", textwrap.dedent(script).format(
            engine=ENGINE, root=str(tmp_path))],
        capture_output=True, text=True, timeout=120)
    assert out.returncode != 0, "the child was supposed to die mid-migration"
    return out


def test_a_journal_exists_before_the_first_destructive_write(tmp_path):
    """The ordering that makes recovery possible at all."""
    from numera import vault

    _make_store(tmp_path)
    _crash(tmp_path, CRASH_ENABLE)

    journal = vault.journal_path(tmp_path)
    assert journal.exists(), \
        "the migration wrote secret files with no durable record of the salt"


def test_the_journal_never_holds_the_password_or_a_plaintext_secret(tmp_path):
    """Recovery metadata is not a place to park the thing being protected."""
    from numera import vault

    _make_store(tmp_path)
    _crash(tmp_path, CRASH_ENABLE)

    blob = vault.journal_path(tmp_path).read_text(encoding="utf-8")

    assert "master" not in blob, "the master password was written to disk"
    assert "secret-one" not in blob, "a plaintext secret was written to disk"


def test_the_secret_is_recoverable_after_a_hard_kill(tmp_path):
    """The whole point: the user's credential still opens with their password.

    Before the journal, this was unrecoverable - ciphertext on disk, the salt
    gone with the process.
    """
    from numera.store import ProfileStore

    prof = _make_store(tmp_path).list()[0]
    _crash(tmp_path, CRASH_ENABLE)

    # Opening the store is what runs recovery.
    recovered = ProfileStore(tmp_path, password="master")

    assert recovered.get(prof.id).proxy.password == "secret-one", \
        "the credential did not survive a hard kill mid-migration"


def test_recovery_leaves_one_consistent_generation(tmp_path):
    """Every copy, not just the engine's: the dashboard twin and the pool are
    migrated by the same pass and have to land in the same state."""
    from numera.store import ProfileStore

    prof = _make_store(tmp_path).list()[0]
    _crash(tmp_path, CRASH_ENABLE)
    recovered = ProfileStore(tmp_path, password="master")

    assert recovered.vault_enabled(), "the vault stayed off over encrypted files"
    engine_raw = json.loads(
        (tmp_path / "profiles" / f"{prof.id}.json").read_text("utf-8"))
    dash_raw = json.loads(
        (tmp_path / "dashboard" / f"{prof.id}.json").read_text("utf-8"))
    from numera import crypto

    assert crypto.is_encrypted(engine_raw["proxy"]["password"])
    assert crypto.is_encrypted(dash_raw["proxy"]["pass"])


def test_the_journal_is_cleaned_up_after_recovery(tmp_path):
    """A journal left behind would re-run recovery on every open."""
    from numera import vault
    from numera.store import ProfileStore

    _make_store(tmp_path)
    _crash(tmp_path, CRASH_ENABLE)
    ProfileStore(tmp_path, password="master")

    assert not vault.journal_path(tmp_path).exists(), \
        "recovery did not clear its own journal"


def test_a_clean_enable_leaves_no_journal(tmp_path):
    """The control. A migration that finished has nothing to recover."""
    from numera import vault

    store = _make_store(tmp_path)
    store.enable_vault("master")

    assert not vault.journal_path(tmp_path).exists()
    assert store.vault_enabled()


def test_ordinary_failure_still_rolls_back_and_leaves_no_journal(tmp_path):
    """Exception rollback worked before and must keep working - the journal is
    for the path where no `except` runs, not a replacement for the one that
    does."""
    from numera import vault
    from numera.store import ProfileStore

    store = _make_store(tmp_path)
    prof = store.list()[0]

    def boom(cfg):
        raise OSError("the config volume went away")

    store._save_config_locked = boom
    with pytest.raises(OSError):
        store.enable_vault("master")

    assert not vault.journal_path(tmp_path).exists(), "a journal outlived a rollback"
    assert not ProfileStore(tmp_path).vault_enabled()
    assert ProfileStore(tmp_path).get(prof.id).proxy.password == "secret-one", \
        "rollback did not restore the plaintext"
