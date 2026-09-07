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

* `save_config()` took the config file's lock and not the root transaction, so
  it was a lock domain of its own. A vault migration holds the transaction,
  reads the config, does minutes of work and saves it back; an outside settings
  save never waited for that, so it could write into the middle of it, be told
  it had succeeded, and then be overwritten by the read-modify-write it had
  interrupted.

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
# root transaction, read the config, do work, and save it back. The other saves
# a setting through the public entry point while that is open. If the two are
# in different lock domains the second one writes into the middle of the first,
# is told it succeeded, and is then overwritten by the read-modify-write it
# interrupted.
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
    store.save_config(cfg)            # ... and the write, much later
say("committed")
"""

SAVER = """
from numera.store import ProfileStore

store = ProfileStore(ROOT)
say("ready")
wait()
pending = {**store.get_config(), "default_engine": "patchright"}
say("saving")
store.save_config(pending)
say("saved")
"""


def test_a_public_config_save_waits_for_the_transaction_it_would_undercut(tmp_path):
    """The saver must not be able to write inside another process's
    transaction, be told it worked, and then be silently overwritten.

    Both halves are asserted, and each fails on its own against the
    file-lock-only version: the saver is shown to be blocked while the
    transaction is open - a bounded silence, the only way to state that
    something is not happening - and the value it saved is still there at the
    end rather than lost under the transaction's own read-modify-write.
    """
    ProfileStore(tmp_path).save_config({"default_engine": "playwright"})

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
        bar.release("writer")                 # it now writes what it read
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
        "an acknowledged config save was overwritten by the transaction it raced")


def test_a_public_config_save_takes_the_root_transaction(tmp_path):
    """Stated directly as well, because the race above can pass by luck of
    timing on a fast machine and this cannot: while another process holds the
    root transaction, the save has to wait rather than write through it."""
    store = ProfileStore(tmp_path)
    store.save_config({"default_engine": "playwright"})

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
            target=lambda: (store.save_config({"default_engine": "camoufox"}),
                            done.set()), daemon=True)
        worker.start()
        assert not done.wait(timeout=2.0), \
            "save_config wrote while another process held the root transaction"

        bar.release("holder")
        bar.expect("holder", "released")
        assert done.wait(timeout=30), "save_config never completed once the lock was free"
        worker.join(timeout=30)
        assert finish({"holder": holder})["holder"]["code"] == 0
    finally:
        bar.close()
        if holder.poll() is None:                            # pragma: no cover
            holder.kill()

    assert ProfileStore(tmp_path).get_default_engine() == "camoufox"
