"""A vault migration is one cross-process transaction, proved without sleeping.

`vault enable` and `vault change` rewrite every secret-bearing file in the data
root. They held no cross-process lock, so another process could write a
plaintext secret *after* the migration had converted that file and *before* the
vault descriptor committed - leaving a store the config calls encrypted with one
readable secret in it, which no reader can detect and no retry repairs.

The tests force exactly that interleaving. The migration is stopped immediately
after it has written its first file - the transaction lock is held at that
moment, and the file it names is already sealed on disk - and the other process
is released to write over that very file, carrying the plaintext it read before
the migration began. Every wait is a blocking socket read, so the ordering is a
fact rather than a hope: there is no `sleep` anywhere.

Removing the transaction lock from `enable_vault` makes all of these fail with
a plaintext secret on disk, which is what a regression test is for.
"""
import json

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import generate
from numera.models import Profile, Proxy
from numera.store import ProfileStore

# Stops the migration the instant its first file is on disk. That file is
# sealed, the transaction lock is held, and the descriptor has not committed -
# the exact window a concurrent write used to fall into.
MIGRATOR = """
from numera import paths, vault
from numera.store import ProfileStore
_write = paths._Batch.write
_stopped = []

def hooked(self, path, data):
    _write(self, path, data)
    if not _stopped:
        _stopped.append(path)
        say("inside " + str(path))
        wait()

paths._Batch.write = hooked
{action}
say("done")
"""

ENABLE = 'ProfileStore(ROOT).enable_vault("master")'
ROTATE = ('ProfileStore(ROOT, password="master")'
          '.change_vault_password("master", "second")')


def seeded(root, count=6) -> ProfileStore:
    store = ProfileStore(root)
    for i in range(count):
        store.save(Profile(name=f"p{i}", fingerprint=generate(seed=i),
                           proxy=Proxy(server="http://1.2.3.4:8080",
                                       username="u", password="SECRET")))
    return store


def assert_sealed(root, password: str, names=()) -> None:
    from numera import vault

    leaked = vault.plaintext_secrets(root)
    assert not leaked, f"a secret was left readable on disk: {leaked}"
    unlocked = ProfileStore(root, password=password)
    assert unlocked.unlocked(), "the vault does not open with the winning password"
    for name in names:
        assert unlocked.resolve(name).proxy.password == "SECRET", \
            f"{name} cannot be read back - the store is half-migrated"


def race(action: str, writer: str, root, password="master"):
    """Freeze a migration mid-write, force a write into it, assert nothing leaked."""
    pytest.importorskip("cryptography")
    bar = Rendezvous()
    procs = {}
    try:
        procs["migrator"] = start(MIGRATOR.format(action=action), root, "migrator", bar.port)
        procs["writer"] = start(writer, root, "writer", bar.port)
        frozen = bar.expect("migrator", "inside ")   # one file sealed, lock held
        bar.release("writer", frozen)                # write over that same file
        bar.expect("writer", "trying")
        bar.release("migrator")                      # the migration may finish
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:                    # a deadlock, not a slow machine
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    return results


# The writers all read their secret *before* the migration starts, which is the
# realistic case: a request already had the record in hand.
PROFILE_WRITER = """
from numera import crypto
from numera.store import ProfileStore
store = ProfileStore(ROOT{password})
cached = {{p.id: p for p in store.list()}}     # plaintext, taken before the migration
target = pathlib.Path(wait().split(" ", 1)[1]).stem
prof = cached[target]
prof.notes = "edited"
say("trying")
try:
    store.save(prof)                           # blocks on the transaction lock
    print("WROTE")
except crypto.VaultLocked as e:
    print("REFUSED", e)
say("done")
"""


def test_a_profile_write_cannot_land_inside_enable(tmp_path):
    seeded(tmp_path)
    results = race(ENABLE, PROFILE_WRITER.format(password=""), tmp_path)
    # Refused is the right answer: the writer never had the master password, so
    # once the vault is on it cannot seal the plaintext it is holding. What it
    # must never do is write that plaintext over the sealed record.
    assert results["writer"]["out"].startswith("REFUSED"), results["writer"]
    assert_sealed(tmp_path, "master", [f"p{i}" for i in range(6)])


def test_a_secret_write_cannot_land_inside_a_password_rotation(tmp_path):
    pytest.importorskip("cryptography")
    seeded(tmp_path).enable_vault("master")
    results = race(ROTATE, PROFILE_WRITER.format(password=', password="master"'),
                   tmp_path, password="second")
    # The writer unlocked with "master" and the rotation happened under it, so
    # its cached vault identity is stale. Sealing with the old salt over a
    # record the rotation had just rekeyed is what left a secret nothing could
    # open; refusing is the correct answer.
    assert results["writer"]["out"].startswith("REFUSED"), results["writer"]
    assert_sealed(tmp_path, "second", [f"p{i}" for i in range(6)])
    assert not ProfileStore(tmp_path, password="master").unlocked(), \
        "the old password still opens the store after a rotation"


def test_a_proxy_credential_write_cannot_land_inside_a_migration(tmp_path):
    """`proxies.json` is the pool's file and one of the migration's sites.

    Seeded with no profiles, so the frozen first write *is* `proxies.json` and
    the pool write that follows lands squarely on top of it, with the writer
    holding what it believes is the current master password.

    Unlike the profile cases, this one survives even with the transaction lock
    removed, and the docstring says so rather than implying a proof it does not
    give: the pool rewrites `proxies.json` whole, so a rekeyed entry it can no
    longer read makes it refuse before it writes anything. That is a second,
    independent guard. The test pins the invariant - no credential is ever left
    under a salt the current password cannot open - against a regression in
    either of them.
    """
    pytest.importorskip("cryptography")
    from numera.server.stores import ProxyPool

    engine = ProfileStore(tmp_path)
    ProxyPool(tmp_path, engine).add_many(
        [{"host": "1.1.1.1", "port": 8080, "protocol": "http",
          "user": "u", "pass": "SECRET"}])
    engine.enable_vault("master")
    writer = """
from numera import crypto
from numera.store import ProfileStore
from numera.server.stores import ProxyPool
pool = ProxyPool(ROOT, ProfileStore(ROOT, password="master"))
wait()
say("trying")
try:
    pool.add_many([{"host": "9.9.9.9", "port": 3128, "protocol": "http",
                    "user": "u", "pass": "SECRET"}])
    print("WROTE")
except crypto.VaultLocked as e:
    print("REFUSED", e)
say("done")
"""
    results = race(ROTATE, writer, tmp_path, password="second")
    assert results["writer"]["out"].startswith("REFUSED"), results["writer"]
    assert_sealed(tmp_path, "second")
    # Every credential in the pool opens with the new password - none was left
    # sealed under the old salt by a write that slipped inside the rotation.
    pool = ProxyPool(tmp_path, ProfileStore(tmp_path, password="second"))
    for entry in pool.list():
        assert entry.get("pass") == "SECRET", \
            f"a pool credential cannot be read after the rotation: {entry}"
    stored = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    assert not [e for e in stored if e.get("host") == "9.9.9.9"], \
        "a refused pool write still landed on disk"


def test_two_simultaneous_enables_cannot_race_two_salts(tmp_path):
    """Both used to see "disabled" and commit different salts.

    Whichever verifier landed last became the config's, and every secret the
    other had encrypted was then unreadable for good.

    The interleaving is stated, not raced. `first` is frozen mid-migration and
    holds the lock; `second` is then started and parked at a checkpoint one
    statement before `enable_vault`, so when it is released it has nothing left
    to do but take that lock. With the lock it blocks and can only refuse once
    `first` has committed; without it, it passes the same "is a vault set up?"
    check `first` already passed and freezes in its own migration - and this
    fails on the message, one statement in, rather than on a coin flip.
    """
    pytest.importorskip("cryptography")
    seeded(tmp_path)
    body = """
from numera import paths
from numera.store import ProfileStore
_write = paths._Batch.write
_stopped = []

def hooked(self, path, data):
    _write(self, path, data)
    if not _stopped:
        _stopped.append(path)
        say("inside " + str(path))
        wait()

paths._Batch.write = hooked
if WHO == "second":
    say("ready")            # imported and warm; the next statement takes the lock
    wait()
try:
    ProfileStore(ROOT).enable_vault(WHO)
    print("ENABLED")
except RuntimeError as e:
    print("REFUSED", e)
say("done")
"""
    bar = Rendezvous()
    procs = {}
    try:
        procs["first"] = start(body, tmp_path, "first", bar.port)
        bar.expect("first", "inside ")          # mid-migration, holding the lock
        procs["second"] = start(body, tmp_path, "second", bar.port)
        bar.expect("second", "ready")
        bar.release("second")                   # straight into `enable_vault`
        bar.release("first")                    # which commits and lets go
        bar.expect("second", "done")            # second never wrote anything
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    assert results["first"]["out"].startswith("ENABLED"), results["first"]
    assert results["second"]["out"].startswith("REFUSED"), results["second"]
    assert_sealed(tmp_path, "first", [f"p{i}" for i in range(6)])
