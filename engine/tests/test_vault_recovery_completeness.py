"""A journal is not recovery. Finishing, or refusing, is.

`0.31.0` added a journal so a killed `vault enable` could be rolled forward,
and closed the one crash boundary it tested: a kill *after every secret file
was encrypted* and before the descriptor committed. Recovery commits the
descriptor and the store opens.

That is one point on a line. The migration writes files one at a time, so a
kill lands anywhere:

* **before the first secret write** - journal on disk, three plaintext
  credentials, no vault in the config;
* **after the first secret file** - one file ciphertext, two plaintext.

In both, `recover()` sees "journal present, no vault in config", commits the
descriptor, and the product reports an enabled vault over readable passwords.
That is worse than the bug it replaced: the old failure was unreadable data,
this one is a false claim of protection.

Rotation is not covered at all. `change_vault_password` never calls
`begin_journal`, and even with one `recover()` would delete it on sight,
because the config already names a vault - the *old* one. A kill mid-rotation
leaves some secrets under the new key and some under the old, and the only
record of the new salt is the journal being thrown away.

Three rules are asserted here, and they are the whole finding:

1. Protection is never marked complete over plaintext.
2. An unfinished migration is finished, or refused - never discarded, and
   never papered over by a descriptor that does not match the disk.
3. The last recovery evidence outlives a failed rollback.

Every crash in this file is a real `os._exit` in a real child process, because
the entire defect is about what `except` does not run.
"""

import json
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

pytest.importorskip("cryptography")

ENGINE = str(Path(__file__).resolve().parents[1])

#: Die before a single secret file is written - the earliest possible crash
#: after the journal exists. Every credential is still readable.
CRASH_BEFORE_ANY_WRITE = """
import os, sys
sys.path.insert(0, {engine!r})
from numera import vault
from numera.store import ProfileStore

real = vault._Migration.encrypt_all

def die(self, password, salt):
    os._exit(9)

vault._Migration.encrypt_all = die
try:
    ProfileStore({root!r}).enable_vault("master")
except BaseException:
    os._exit(9)
os._exit(9)
"""

#: Die after exactly one secret-bearing file has been written. One file holds
#: ciphertext, the rest are plaintext - the state no reader can describe.
CRASH_AFTER_FIRST_FILE = """
import os, sys
sys.path.insert(0, {engine!r})
from numera import vault
from numera.paths import _Batch
from numera.store import ProfileStore

real = _Batch.write
seen = []

def counted(self, path, data):
    out = real(self, path, data)
    seen.append(path)
    if len(seen) == 1:
        os._exit(9)
    return out

_Batch.write = counted
try:
    ProfileStore({root!r}).enable_vault("master")
except BaseException:
    os._exit(9)
os._exit(9)
"""

#: Rotation, killed after one file moved to the new key. The remaining
#: secrets still open with the old password; that one does not.
CRASH_MID_ROTATION = """
import os, sys
sys.path.insert(0, {engine!r})
from numera.paths import _Batch
from numera.store import ProfileStore

real = _Batch.write
seen = []

def counted(self, path, data):
    out = real(self, path, data)
    seen.append(path)
    if len(seen) == 1:
        os._exit(9)
    return out

store = ProfileStore({root!r}, password="master")
_Batch.write = counted
try:
    store.change_vault_password("master", "second")
except BaseException:
    os._exit(9)
os._exit(9)
"""


def _seeded(tmp_path):
    """A root with a secret in all three places the vault covers.

    An engine profile, the dashboard's copy of it, and the shared proxy pool -
    `vault.SITES`. A migration that only reaches one of them is the state these
    tests are about, so all three have to exist.
    """
    from numera import generate
    from numera.models import Profile, Proxy
    from numera.repo import repo
    from numera.store import ProfileStore

    store = ProfileStore(tmp_path)
    repo(store).create(Profile(
        name="alpha", fingerprint=generate(seed=1),
        proxy=Proxy(server="http://h:1", username="u", password="secret-one")))

    dash = tmp_path / "dashboard"
    dash.mkdir(exist_ok=True)
    (dash / "alpha.json").write_text(json.dumps(
        {"id": "alpha", "name": "alpha",
         "proxy": {"type": "http", "host": "h", "port": 1,
                   "user": "u", "pass": "secret-two"}}), encoding="utf-8")
    (tmp_path / "proxies.json").write_text(json.dumps(
        [{"id": "p1", "type": "http", "host": "h", "port": 1,
          "user": "u", "pass": "secret-three"}]), encoding="utf-8")
    return store


def _crash(tmp_path, script):
    out = subprocess.run(
        [sys.executable, "-c", textwrap.dedent(script).format(
            engine=ENGINE, root=str(tmp_path))],
        capture_output=True, text=True, timeout=180)
    assert out.returncode != 0, "the child was supposed to die mid-migration"
    return out


def _enabled_with_vault(tmp_path):
    from numera import vault
    return bool(json.loads((tmp_path / "config.json").read_text("utf-8")).get("vault")) \
        if (tmp_path / "config.json").exists() else False


# ---------------------------------------------------------------------------
# 1. Protection is never marked complete over plaintext
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("script,label", [
    (CRASH_BEFORE_ANY_WRITE, "before any secret was written"),
    (CRASH_AFTER_FIRST_FILE, "after one secret file of three"),
])
def test_an_unfinished_enable_never_reports_an_enabled_vault(tmp_path, script, label):
    """The regression, at both kill points the earlier fix did not cover."""
    from numera import vault
    from numera.store import ProfileStore

    _seeded(tmp_path)
    _crash(tmp_path, script)

    leftover = vault.plaintext_secrets(tmp_path)
    assert leftover, f"the fixture did not crash {label}; nothing to recover"

    try:
        store = ProfileStore(tmp_path)
    except Exception:
        return          # failing closed is the other acceptable answer

    assert not store.vault_enabled(), (
        f"the vault reported itself enabled with {len(leftover)} credential(s) "
        f"still readable on disk (crashed {label})")


@pytest.mark.parametrize("script", [CRASH_BEFORE_ANY_WRITE, CRASH_AFTER_FIRST_FILE])
def test_an_unfinished_enable_is_refused_rather_than_served(tmp_path, script):
    """A store in a mixed state cannot answer honestly, so it does not answer.

    Opening without the password must say so - and say what to do - rather than
    hand back a store whose config and disk disagree.
    """
    from numera.store import ProfileStore

    _seeded(tmp_path)
    _crash(tmp_path, script)

    with pytest.raises(Exception) as caught:
        ProfileStore(tmp_path).list()

    assert "password" in str(caught.value).lower(), \
        f"the refusal does not tell the user what it needs: {caught.value!r}"


# ---------------------------------------------------------------------------
# 2. An unfinished migration is finished, or refused - never discarded
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("script", [CRASH_BEFORE_ANY_WRITE, CRASH_AFTER_FIRST_FILE])
def test_the_password_finishes_an_interrupted_enable(tmp_path, script):
    """The repair: the same password that started it completes it."""
    from numera import vault
    from numera.store import ProfileStore

    _seeded(tmp_path)
    _crash(tmp_path, script)

    store = ProfileStore(tmp_path, password="master")

    assert store.vault_enabled(), "the vault did not come back on"
    assert vault.plaintext_secrets(tmp_path) == [], \
        "the resume left a readable credential under an enabled vault"
    assert not vault.journal_path(tmp_path).exists(), \
        "a finished migration left its journal behind"


def test_finishing_an_interrupted_enable_is_idempotent(tmp_path):
    """Reopen twice: the second open is an ordinary open, not a second repair."""
    from numera import vault
    from numera.store import ProfileStore

    prof = _seeded(tmp_path).list()[0]
    _crash(tmp_path, CRASH_AFTER_FIRST_FILE)

    first = ProfileStore(tmp_path, password="master")
    second = ProfileStore(tmp_path, password="master")

    assert first.get(prof.id).proxy.password == "secret-one"
    assert second.get(prof.id).proxy.password == "secret-one"
    assert vault.plaintext_secrets(tmp_path) == []
    assert not vault.journal_path(tmp_path).exists()


def test_a_rotation_records_a_journal_at_all(tmp_path):
    """The precondition for every rotation assertion below.

    `change_vault_password` rewrites every secret under a new salt and wrote
    nothing down first, so a kill left the new salt nowhere on disk.
    """
    from numera import vault
    from numera.store import ProfileStore

    _seeded(tmp_path)
    ProfileStore(tmp_path).enable_vault("master")

    seen = {}
    real = vault.begin_journal

    def watched(root, op, *args, **kwargs):
        seen["op"] = op
        return real(root, op, *args, **kwargs)

    vault.begin_journal = watched
    try:
        ProfileStore(tmp_path, password="master").change_vault_password("master", "second")
    finally:
        vault.begin_journal = real

    assert seen.get("op") == "rotate", \
        "a rotation rewrote every secret with no durable record of the new salt"


def test_an_interrupted_rotation_is_not_discarded(tmp_path):
    """`recover` used to delete any journal whenever the config named a vault.

    During a rotation it always does - the *old* descriptor - so the one record
    of the new salt was thrown away on the next open.
    """
    from numera import vault
    from numera.store import ProfileStore

    _seeded(tmp_path)
    ProfileStore(tmp_path).enable_vault("master")
    _crash(tmp_path, CRASH_MID_ROTATION)

    assert vault.journal_path(tmp_path).exists(), \
        "the rotation left no journal to recover from"

    try:
        ProfileStore(tmp_path)
    except Exception:
        pass

    assert vault.journal_path(tmp_path).exists(), \
        "opening the store discarded an unfinished rotation"


def test_an_interrupted_rotation_can_be_finished(tmp_path):
    """Both passwords, because both keys are on disk - and afterwards only one
    of them opens anything."""
    from numera import crypto, vault
    from numera.store import ProfileStore

    prof = _seeded(tmp_path).list()[0]
    ProfileStore(tmp_path).enable_vault("master")
    _crash(tmp_path, CRASH_MID_ROTATION)

    ProfileStore.resume_vault(tmp_path, old_password="master", new_password="second")

    reopened = ProfileStore(tmp_path, password="second")
    assert reopened.get(prof.id).proxy.password == "secret-one", \
        "the rotation did not finish: the new password does not open this secret"
    assert not vault.journal_path(tmp_path).exists()

    assert not ProfileStore(tmp_path, password="master").unlocked(), \
        "the old master password still opens the vault after a finished rotation"


#: Crash *during the recovery itself*. The resume re-encrypts what the first
#: run did not reach, and dies partway through doing it - so the store is in a
#: third state: partly migrated by one dead process, partly by another.
CRASH_DURING_RECOVERY = """
import os, sys
sys.path.insert(0, {engine!r})
from numera.paths import _Batch
from numera.store import ProfileStore

real = _Batch.write
seen = []

def counted(self, path, data):
    out = real(self, path, data)
    seen.append(path)
    if len(seen) == 1:
        os._exit(9)
    return out

_Batch.write = counted
try:
    ProfileStore({root!r}, password="master")
except BaseException:
    os._exit(9)
os._exit(9)
"""


def test_a_crash_during_recovery_is_itself_recoverable(tmp_path):
    """The barrier the brief asks for that the others do not cover.

    Recovery is a migration too, so it can be killed like one. It has to be
    idempotent rather than one-shot: the second resume must pick up whatever
    the first left, not refuse because the state no longer matches what the
    journal originally described.
    """
    from numera import vault
    from numera.store import ProfileStore

    prof = _seeded(tmp_path).list()[0]
    _crash(tmp_path, CRASH_AFTER_FIRST_FILE)
    _crash(tmp_path, CRASH_DURING_RECOVERY)

    finished = ProfileStore(tmp_path, password="master")

    assert finished.vault_enabled()
    assert vault.plaintext_secrets(tmp_path) == [], \
        "a crash during recovery left a credential readable"
    assert finished.get(prof.id).proxy.password == "secret-one"
    assert not vault.journal_path(tmp_path).exists()


def test_a_crash_after_the_commit_only_clears_the_journal(tmp_path):
    """The other side of the descriptor commit.

    Everything is encrypted and the config already names the vault; the process
    died before `end_journal`. There is nothing to repair, and the repair must
    not run - re-committing a descriptor over a finished store is how a
    concurrent reader used to corrupt one.

    **The key is part of the precondition, and that is not a concession.** This
    on-disk shape - a config and a journal naming one identical descriptor - is
    byte-for-byte what `test_an_already_committed_descriptor_is_not_a_receipt_
    without_a_key` and both `test_same_salt_foreign_ciphertext_is_refused_*` in
    `test_vault_completion_everywhere.py` construct over *foreign* ciphertext.
    Nothing on disk separates the two, which is exactly the N02 finding: a
    prior config write is a receipt for nothing. Opening without the password
    was tried and it re-opened that hole, so the crash-after-commit case pays
    the same price as every other completion - the key - and what this test
    still pins is the part that is its own: the descriptor is not rewritten and
    the journal goes.
    """
    from numera import vault
    from numera.store import ProfileStore

    prof = _seeded(tmp_path).list()[0]
    ProfileStore(tmp_path).enable_vault("master")
    before = json.loads((tmp_path / "config.json").read_text("utf-8"))["vault"]

    # Exactly the state a kill between the commit and the cleanup leaves.
    vault.begin_journal(tmp_path, "enable", new=before)

    reopened = ProfileStore(tmp_path, password="master")

    assert not vault.journal_path(tmp_path).exists(), "the stale journal was kept"
    assert json.loads((tmp_path / "config.json").read_text("utf-8"))["vault"] == before, \
        "recovery rewrote a descriptor that had already committed"
    assert reopened.vault_enabled()


# ---------------------------------------------------------------------------
# 3. Recovery evidence outlives a failed rollback
# ---------------------------------------------------------------------------

def test_a_failed_rollback_keeps_its_journal(tmp_path):
    """`migration()` cleared the journal on every exit, including the one exit
    where it is the only thing left pointing at the new salt."""
    from numera import vault
    from numera.paths import RollbackFailed

    _seeded(tmp_path)
    vault.begin_journal(tmp_path, "enable",
                        new={"salt": "c2FsdA==", "verifier": "v"})

    with pytest.raises(RollbackFailed):
        with vault.migration(tmp_path) as step:
            step.rollback = lambda: [(tmp_path / "proxies.json", "device busy")]
            raise OSError("the volume went away")

    assert vault.journal_path(tmp_path).exists(), \
        "a failed rollback deleted the last record of the new salt"


def test_a_failed_resume_keeps_its_journal_even_when_rollback_works(tmp_path):
    """The subtler half, and one the first fix got wrong.

    Rolling back an ordinary migration restores a clean store, so clearing the
    journal afterwards is right. Rolling back a *resume* restores the
    half-migrated state the crash left - whose ciphertext opens only with the
    journal's salt. Clearing it there destroys the last copy of the key
    material for a store that still needs it.
    """
    from numera import vault
    from numera.store import ProfileStore

    _seeded(tmp_path)
    _crash(tmp_path, CRASH_AFTER_FIRST_FILE)
    assert vault.journal_path(tmp_path).exists()

    # A resume that fails for an ordinary reason, with rollback succeeding.
    broken = tmp_path / "proxies.json"
    broken.write_text("{not json", encoding="utf-8")

    with pytest.raises(Exception):
        ProfileStore(tmp_path, password="master")

    assert vault.journal_path(tmp_path).exists(), \
        "a failed resume cleared the journal the encrypted files still depend on"


# ---------------------------------------------------------------------------
# The invariant that must survive every change above
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("script,setup_vault", [
    (CRASH_BEFORE_ANY_WRITE, False),
    (CRASH_AFTER_FIRST_FILE, False),
    (CRASH_MID_ROTATION, True),
])
def test_no_journal_holds_a_password_or_a_plaintext_secret(tmp_path, script, setup_vault):
    """Recovery metadata is not a place to park the thing being protected."""
    from numera import vault
    from numera.store import ProfileStore

    _seeded(tmp_path)
    if setup_vault:
        ProfileStore(tmp_path).enable_vault("master")
    _crash(tmp_path, script)

    blob = vault.journal_path(tmp_path).read_text(encoding="utf-8")

    for forbidden in ("master", "second", "secret-one", "secret-two", "secret-three"):
        assert forbidden not in blob, f"{forbidden!r} was written to the journal"


def test_a_clean_enable_and_rotation_still_leave_nothing_behind(tmp_path):
    """The control. Nothing above may turn an ordinary success into a pending
    repair on the next open."""
    from numera import vault
    from numera.store import ProfileStore

    prof = _seeded(tmp_path).list()[0]
    ProfileStore(tmp_path).enable_vault("master")
    assert not vault.journal_path(tmp_path).exists()

    ProfileStore(tmp_path, password="master").change_vault_password("master", "second")
    assert not vault.journal_path(tmp_path).exists()

    assert ProfileStore(tmp_path, password="second").get(prof.id).proxy.password == "secret-one"
    assert vault.plaintext_secrets(tmp_path) == []
