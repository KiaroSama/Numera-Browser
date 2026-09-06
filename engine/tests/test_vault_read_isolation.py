"""A read that decrypts must not observe half a vault migration.

`ProfileStore.get/list` joined the root transaction in 0.20.0 for exactly this
reason: what a stored secret *means* depends on the record and `config.json`
agreeing, and `vault enable`/`vault change` rewrite every secret-bearing file
before committing the descriptor that names the key.

`DashboardStore.get/list/_raw` and the proxy pool's reads were left outside it,
and they read the same secrets from `dashboard/*.json` and `proxies.json` -
both named in `vault.SITES`, both rewritten by the same migration. A read that
crossed the boundary found a sealed record under a config that still said "no
vault", and `vault.decrypt_for_read` answers `None` for that: the API returned
a profile whose proxy password had apparently been removed, and
`secret_withheld()` - the check several operations use to refuse rather than
produce a confidently wrong artifact - answered about a key state that never
existed.

**How the interleaving is stated without a race.** The migration is frozen the
instant it has sealed the file under test: the transaction lock is held and the
descriptor has not committed. The reader is then released, and its very next
message decides the test. A read that joins the boundary says so *before* it
waits; a read that does not has already finished and says what it saw. Waiting
for that one word is what makes this a barrier rather than a coin flip - an
earlier draft signalled before the read instead, and then the migration usually
committed first and the test passed against code with no boundary at all.

Removing `transaction_lock` from the reads under test makes each of these fail
on that message, and the credential read back as `None` behind it.
"""
import pytest

from _rendezvous import Rendezvous, finish, start

from numera import generate
from numera.models import Profile, Proxy
from numera.repo import repo
from numera.store import ProfileStore

pytest.importorskip("fastapi")

from numera.server.stores import DashboardStore, ProxyPool     # noqa: E402


# Freezes the migration the instant the file named by TARGET is on disk. That
# file is sealed, the transaction lock is held, and `config.json` still
# describes a store with no vault at all.
MIGRATOR = """
from numera import paths
from numera.store import ProfileStore

_write = paths._Batch.write
_stopped = []

def hooked(self, path, data):
    _write(self, path, data)
    if TARGET in str(path) and not _stopped:
        _stopped.append(path)
        say("inside " + str(path))
        wait()

paths._Batch.write = hooked
ProfileStore(ROOT).enable_vault("master")
say("done")
"""

# Announces the moment the read enters the snapshot boundary - which is the
# behaviour under test, so it is reported rather than inferred. A build without
# the boundary never calls this and the parent hears the read's *result*
# instead, which is the failure.
ANNOUNCE = """
from numera.server import stores as stores_mod

_tx = stores_mod.transaction_lock

def announced(root):
    say("at the boundary")
    return _tx(root)

stores_mod.transaction_lock = announced
"""


def frozen_migration(root, target: str, reader: str, announce: str = None):
    """Freeze `enable_vault` with `target` sealed, then read into that window.

    `announce` says which module the reader announces the boundary from: the
    module that owns the read is the one that owns the name it looks the lock
    up by, so patching the wrong one produces a test that proves nothing.
    """
    pytest.importorskip("cryptography")
    bar = Rendezvous()
    procs = {}
    try:
        procs["reader"] = start((announce or ANNOUNCE) + reader,
                                root, "reader", bar.port)
        procs["migrator"] = start(f"TARGET = {target!r}\n" + MIGRATOR,
                                  root, "migrator", bar.port)
        bar.expect("migrator", "inside ")     # the file is sealed, the lock is held
        bar.release("reader")
        # The whole proof. A read that waits for the migration says this and
        # then blocks; a read that walked into the half-committed store has
        # already printed what it found and says "done" instead.
        bar.expect("reader", "at the boundary")
        bar.release("migrator")               # the migration may commit
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:             # a deadlock, not a slow machine
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    return results


def seeded_profile(root) -> Profile:
    store = ProfileStore(root)
    return repo(store).create(Profile(
        name="p", fingerprint=generate(seed=1),
        proxy=Proxy(server="http://1.2.3.4:8080", username="u", password="SECRET")))


DASHBOARD_READER = """
from numera.server.stores import DashboardStore
from numera.store import ProfileStore

wait()
dash = DashboardStore(ROOT, ProfileStore(ROOT, password="master"))
print("READ", (dash.get(PID).get("proxy") or {}).get("pass"))
say("done")
"""


def test_a_dashboard_read_cannot_see_a_record_sealed_before_the_descriptor(tmp_path):
    """The dashboard's own copy of the profile is one of the migration's sites.

    Read across the boundary it comes back with `pass=None` - which the API
    hands to the UI, and which no caller can tell apart from "this profile has
    no proxy password".
    """
    prof = seeded_profile(tmp_path)

    results = frozen_migration(tmp_path, "dashboard",
                               f"PID = {prof.id!r}\n" + DASHBOARD_READER)

    assert results["reader"]["out"] == "READ SECRET", (
        "the dashboard read observed an intermediate key state: "
        f"{results['reader']}")


DASHBOARD_LIST_READER = """
from numera.server.stores import DashboardStore
from numera.store import ProfileStore

wait()
dash = DashboardStore(ROOT, ProfileStore(ROOT, password="master"))
print("READ", sorted((p.get("proxy") or {}).get("pass") for p in dash.list()))
say("done")
"""


def test_a_dashboard_listing_cannot_see_a_half_committed_migration(tmp_path):
    """`GET /api/profiles` is the polled endpoint, so it is the one most likely
    to land inside a migration - and it decrypts every record it returns."""
    seeded_profile(tmp_path)

    results = frozen_migration(tmp_path, "dashboard", DASHBOARD_LIST_READER)

    assert results["reader"]["out"] == "READ ['SECRET']", (
        "the listing decrypted against a key state that had not committed: "
        f"{results['reader']}")


POOL_READER = """
from numera.server.stores import ProxyPool
from numera.store import ProfileStore

wait()
pool = ProxyPool(ROOT, ProfileStore(ROOT, password="master"))
entry = pool.list()[0]
print("READ", entry.get("pass"), pool.secret_withheld(entry["id"]))
say("done")
"""


def test_a_pool_read_cannot_see_a_record_sealed_before_the_descriptor(tmp_path):
    """`proxies.json` is rewritten whole by the migration, and the pool's
    readers took no transaction at all - so every credential in the pool read
    back as `None`, and `secret_withheld()` said the store was hiding secrets
    from a vault that had not been created yet."""
    engine = ProfileStore(tmp_path)
    ProxyPool(tmp_path, engine).add_many(
        [{"host": "1.1.1.1", "port": 8080, "protocol": "http",
          "user": "u", "pass": "SECRET"}])

    results = frozen_migration(tmp_path, "proxies.json", POOL_READER)

    assert results["reader"]["out"] == "READ SECRET False", (
        "the pool read observed an intermediate key state: "
        f"{results['reader']}")


def test_the_reads_still_work_with_no_migration_in_flight(tmp_path):
    """The boundary is reentrant and only ever waits on a real migration; the
    ordinary read must not have become conditional on anything."""
    prof = seeded_profile(tmp_path)
    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine)
    pool = ProxyPool(tmp_path, engine)
    pool.add_many([{"host": "1.1.1.1", "port": 8080, "protocol": "http",
                    "user": "u", "pass": "SECRET"}])

    assert dash.get(prof.id)["proxy"]["pass"] == "SECRET"
    assert [p["proxy"]["pass"] for p in dash.list()] == ["SECRET"]
    assert dash._raw(prof.id)["proxy"]["pass"] == "SECRET"
    assert pool.list()[0]["pass"] == "SECRET"
    assert pool.secret_withheld(pool.list()[0]["id"]) is False


# The engine store looks the lock up in its own module globals, so the reader
# that announces the boundary is patched there rather than in `stores`.
ANNOUNCE_ENGINE = """
from numera import store as store_mod

_tx = store_mod.transaction_lock

def announced(root):
    say("at the boundary")
    return _tx(root)

store_mod.transaction_lock = announced
"""

ENGINE_WITHHELD_READER = """
from numera.store import ProfileStore

wait()
store = ProfileStore(ROOT, password="master")
print("READ", store.secret_withheld(PID))
say("done")
"""


def test_the_engine_side_withheld_check_cannot_straddle_the_boundary(tmp_path):
    """`ProfileStore.get/list` joined the snapshot in 0.20.0; `secret_withheld`
    did not.

    It read the record's bytes with no lock and then asked `vault.is_withheld`
    about them, comparing a stored value to a key state that never coexisted
    with it. This is the check several operations use to *refuse* rather than
    produce a confidently wrong artifact - a backup or an export that silently
    drops a proxy password - so a wrong answer here is not cosmetic.

    Unlocked with the right password, the honest answer is False: nothing is
    being withheld from this reader.
    """
    prof = seeded_profile(tmp_path)
    reader = "PID = " + repr(prof.id) + "\n" + ENGINE_WITHHELD_READER

    results = frozen_migration(tmp_path, "profiles", reader,
                               announce=ANNOUNCE_ENGINE)

    assert results["reader"]["out"] == "READ False", (
        "the engine-side withheld check judged a sealed record against a "
        "descriptor that had not committed: " + str(results["reader"]))
