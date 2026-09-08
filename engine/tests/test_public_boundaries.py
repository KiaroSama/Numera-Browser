"""The public entry points obey the same rules the private ones do.

Two invariants had been made true everywhere the audits looked - the API, the
CLI, the coordinator - and left false on the plain Python surface `ProfileStore`
exposes. A store object is public: the MCP server, a script and a future caller
all hold one, and an invariant that only holds on the paths that happen to be
tested is not an invariant.

* `delete_session()` moved a profile's `user-data/<id>` aside without taking
  the session lease. `detach_session` is a rename, and on POSIX renaming a
  directory a process has open succeeds, so this could pull the directory out
  from under a browser another Numera process was running in it - the exact
  collision the lease exists to prevent.

* `save_config()` replaced the whole config from a snapshot the caller built
  *before* any lock. A vault migration holds the transaction, reads the config,
  does minutes of work and writes back what it read; anything committed in
  between is deleted by that write, and both callers are told they succeeded.
  Adding the transaction to the write does not fix it, because the stale read
  is the problem. Full replacement is private and requires the transaction that
  produced the snapshot; settings go through `set_config_many`, which reads
  under the lock it writes under.

Both are stated across real processes with the socket barrier, because both are
about two processes and neither can be shown with threads in one.
"""
import json

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import generate
from numera.models import Profile
from numera.paths import SessionBusy
from numera.repo import repo
from numera.store import ProfileStore


def seeded(root, name="alpha") -> Profile:
    store = ProfileStore(root)
    prof = repo(store).create(Profile(name=name, fingerprint=generate(seed=1)))
    (store.user_data_path(prof.id) / "Cookies").write_text("live", encoding="utf-8")
    return prof


# --- the session lease ------------------------------------------------------

HOLDER = """
from numera.paths import session_lease
with session_lease(ROOT, {pid!r}):
    say("holding")
    wait()
say("released")
"""


def test_deleting_a_session_waits_for_the_browser_that_has_it(tmp_path):
    """Another process has the lease, so the public delete must refuse - and
    must leave the session exactly where it is, not half-detached."""
    prof = seeded(tmp_path)
    store = ProfileStore(tmp_path)
    live = store.user_data_path(prof.id)

    bar = Rendezvous()
    holder = start(HOLDER.format(pid=prof.id), tmp_path, "holder", bar.port)
    try:
        bar.expect("holder", "holding")

        with pytest.raises(SessionBusy):
            store.delete_session(prof.id)

        assert live.exists(), "a live session was detached out from under its owner"
        assert (live / "Cookies").read_text(encoding="utf-8") == "live"
        assert not list(live.parent.glob("*.deleting-*")), "a tombstone was left behind"

        bar.release("holder")
        bar.expect("holder", "released")
        assert finish({"holder": holder})["holder"]["code"] == 0
    finally:
        bar.close()
        if holder.poll() is None:                            # pragma: no cover
            holder.kill()

    # And once it is free, the same call goes through.
    store.delete_session(prof.id)
    assert not live.exists(), "the session survived a delete nothing was holding"


def test_deleting_a_session_lets_go_before_it_reclaims(tmp_path):
    """The rename commits the delete; the recursive removal after it is a whole
    Chromium directory, and holding the lease across it makes the next launch
    of that profile wait on bytes nothing can reference any more.

    The lease is what this checks, because the lease is what the delete takes.
    An unrelated profile is asserted as well, so that a future version which
    reaches for the root transaction here is caught by the same test.
    """
    import threading

    from numera.paths import session_lease

    prof = seeded(tmp_path)
    other = repo(ProfileStore(tmp_path)).create(
        Profile(name="other", fingerprint=generate(seed=2)))
    store = ProfileStore(tmp_path)

    reached, release = threading.Event(), threading.Event()
    real = ProfileStore.discard_tombstone

    def frozen(tomb):
        reached.set()
        assert release.wait(timeout=30), "the test never released cleanup"
        return real(tomb)

    ProfileStore.discard_tombstone = staticmethod(frozen)
    failed = []
    worker = threading.Thread(target=lambda: store.delete_session(prof.id), daemon=True)
    worker.start()
    try:
        assert reached.wait(timeout=30), "the delete never reached cleanup"
        with session_lease(tmp_path, prof.id):
            pass                        # free already, or this raises SessionBusy
        repo(ProfileStore(tmp_path)).edit(
            other.id, lambda p: setattr(p, "notes", "went through") or p)
    except BaseException as exc:                             # noqa: BLE001
        failed.append(exc)
    finally:
        release.set()
        worker.join(timeout=30)
        ProfileStore.discard_tombstone = staticmethod(real)
    assert not failed, f"an unrelated write was blocked by cleanup: {failed}"
    assert ProfileStore(tmp_path).get(other.id).notes == "went through"


# --- the config lock domain -------------------------------------------------

# One child does what `enable_vault` and `change_vault_password` do: hold the
# root transaction, read the config, do work, and write it back. The other
# changes a setting through the public mutation API while that is open. Both
# calls report success, so both changes have to be on disk at the end - and the
# second one must not have been able to write into the middle of the first.
WRITER = """
from numera.paths import transaction_lock
from numera.store import ProfileStore

store = ProfileStore(ROOT)
say("ready")
wait()
with transaction_lock(ROOT):
    cfg = store.get_config()          # the read the vault migrations do
    say("read")
    wait()
    cfg["headless_launch"] = True
    store._save_config_locked(cfg)    # ... and the write, much later
say("committed")
"""

SAVER = """
from numera.store import ProfileStore

store = ProfileStore(ROOT)
say("ready")
wait()
say("saving")
store.set_config_many({"default_engine": "patchright"})
say("saved")
"""


def test_two_acknowledged_config_changes_both_survive(tmp_path):
    """The lost update a lock order alone does not fix.

    Taking the root transaction inside a full-replacement call closes the
    separate-lock-domain hole, but the snapshot is still built *before* the
    lock: the writer reads, another process commits a key, the writer writes
    what it read, and that key is gone. Both callers were told they succeeded.

    So full replacement is private and requires the transaction that read it,
    and ordinary settings go through `set_config_many`, which reads under the
    lock it writes under. Then the interleaving below has one outcome: the
    saver waits, and both changes are on disk.
    """
    ProfileStore(tmp_path).set_config_many({"default_engine": "playwright"})

    bar = Rendezvous()
    procs = {}
    try:
        procs["writer"] = start(WRITER, tmp_path, "writer", bar.port)
        procs["saver"] = start(SAVER, tmp_path, "saver", bar.port)
        bar.expect("writer", "ready")
        bar.expect("saver", "ready")
        bar.release("writer")
        bar.expect("writer", "read")          # transaction open, config read
        bar.release("saver")
        bar.expect("saver", "saving")
        assert bar.silent("saver", 3.0), (
            "the save went through inside another process's transaction")
        bar.release("writer")
        bar.expect("writer", "committed")
        bar.expect("saver", "saved")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:                            # pragma: no cover
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results

    on_disk = json.loads(ProfileStore(tmp_path).config_path.read_text("utf-8"))
    assert on_disk["default_engine"] == "patchright", (
        "the setting the saver was told it had written is gone")
    assert on_disk["headless_launch"] is True, (
        "the transaction's own acknowledged change was overwritten")


def test_replacing_the_whole_config_needs_the_transaction_that_read_it(tmp_path):
    """Structural, not documented: a caller that never took the transaction
    cannot make a whole-document write at all. It is the only way to state
    "your snapshot must come from inside this lock" in a signature."""
    store = ProfileStore(tmp_path)
    store.set_config_many({"default_engine": "playwright"})

    with pytest.raises(RuntimeError, match="root transaction"):
        store._save_config_locked({"default_engine": "camoufox"})

    assert store.get_default_engine() == "playwright", "it wrote anyway"

    from numera.paths import transaction_lock

    with transaction_lock(tmp_path):
        store._save_config_locked({**store.get_config(),
                                   "default_engine": "camoufox"})
    assert ProfileStore(tmp_path).get_default_engine() == "camoufox"


def test_a_settings_change_waits_for_the_transaction(tmp_path):
    """Stated directly as well, because the race above can pass by luck of
    timing on a fast machine and this cannot: while another process holds the
    root transaction, a settings change has to wait rather than write through
    it."""
    store = ProfileStore(tmp_path)
    store.set_config_many({"default_engine": "playwright"})

    body = """
from numera.paths import transaction_lock
with transaction_lock(ROOT):
    say("holding")
    wait()
say("released")
"""
    bar = Rendezvous()
    holder = start(body, tmp_path, "holder", bar.port)
    try:
        bar.expect("holder", "holding")

        import threading

        done = threading.Event()
        worker = threading.Thread(
            target=lambda: (store.set_config_many({"default_engine": "camoufox"}),
                            done.set()), daemon=True)
        worker.start()
        assert not done.wait(timeout=2.0), (
            "a settings change wrote while another process held the transaction")

        bar.release("holder")
        bar.expect("holder", "released")
        assert done.wait(timeout=30), "it never completed once the lock was free"
        worker.join(timeout=30)
        assert finish({"holder": holder})["holder"]["code"] == 0
    finally:
        bar.close()
        if holder.poll() is None:                            # pragma: no cover
            holder.kill()

    assert ProfileStore(tmp_path).get_default_engine() == "camoufox"
