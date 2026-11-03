"""One browser session per profile, across processes - and never a silent proxy.

`user-data/<id>` is a Chromium profile directory, and Chromium assumes it owns
one exclusively: two processes on the same directory corrupt its databases. The
storage locks never covered that. They guard a JSON write and are held for
milliseconds, while a session lasts as long as the window does, so nothing
stopped two servers, a desktop and a CLI from opening one profile at once - or
`backup()` from zipping a live one, or `restore()` from replacing it underneath.

The lease is the owner. It is deliberately unlike a data lock: it refuses
immediately instead of waiting, because "someone has this open" is an answer
rather than a delay, and the OS drops it when the holder dies.

The other half is the credential. A locked vault hands out `proxy.password =
None`, which reads exactly like "this proxy has no password" - so a launch
would start with no credential and browse from the host's own IP, the one
outcome an anti-detect profile exists to prevent.
"""
import time

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import generate
from numera.models import Profile, Proxy
from numera.paths import SessionBusy, session_lease
from numera.store import ProfileStore

HOLDER = """
from numera.paths import session_lease
with session_lease(ROOT, {pid!r}):
    say("holding")
    wait()
say("released")
"""


def seeded(root, name="p", **kw) -> Profile:
    prof = Profile(name=name, fingerprint=generate(seed=1), **kw)
    ProfileStore(root).save(prof)
    return prof


def test_a_second_process_cannot_take_a_held_lease(tmp_path):
    prof = seeded(tmp_path)
    bar = Rendezvous()
    procs = {"holder": start(HOLDER.format(pid=prof.id), tmp_path, "holder", bar.port)}
    try:
        bar.expect("holder", "holding")
        # Refuses at once. A data lock would have waited out NUMERA_LOCK_TIMEOUT
        # before saying the same thing.
        with pytest.raises(SessionBusy):
            with session_lease(tmp_path, prof.id):
                pass
        assert session_lease(tmp_path, prof.id).held_by_other_process()
        bar.release("holder")
        bar.expect("holder", "released")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    # Released with the holder, so the next open succeeds.
    with session_lease(tmp_path, prof.id):
        pass


@pytest.mark.parametrize("operation", ["mkdir", "open"])
def test_an_unavailable_lease_observation_is_not_reported_free(
        tmp_path, monkeypatch, operation):
    from pathlib import Path
    from numera import data_locks

    lease = session_lease(tmp_path, "unobserved")
    observed = []
    original_mkdir, original_open = Path.mkdir, data_locks.os.open

    def mkdir(path, *args, **kwargs):
        if path == tmp_path / ".locks" and operation == "mkdir":
            observed.append(operation)
            raise PermissionError("fixture lease directory is inaccessible")
        return original_mkdir(path, *args, **kwargs)

    def open_file(path, *args, **kwargs):
        if Path(path).parent == tmp_path / ".locks":
            assert operation == "open", "directory refusal reached file opening"
            observed.append(operation)
            raise PermissionError("fixture lease file is inaccessible")
        return original_open(path, *args, **kwargs)

    with monkeypatch.context() as patch:
        patch.setattr(Path, "mkdir", mkdir)
        patch.setattr(data_locks.os, "open", open_file)
        assert lease.held_by_other_process(), "unknown lease was reported free"
    assert observed == [operation]
    assert not lease.held_by_other_process(), "released lease stayed occupied"
    with lease:
        assert not lease.held_by_other_process(), "own lease was reported foreign"


def test_two_processes_racing_one_profile_produce_one_winner(tmp_path):
    prof = seeded(tmp_path)
    body = """
from numera.paths import SessionBusy, session_lease
wait()
try:
    with session_lease(ROOT, {pid!r}):
        print("WON")
        say("done")
except SessionBusy:
    print("REFUSED")
    say("done")
""".format(pid=prof.id)
    bar = Rendezvous()
    procs = {}
    try:
        # The first is parked *inside* the lease before the second is released,
        # so the outcome is stated rather than raced.
        procs["first"] = start(HOLDER.format(pid=prof.id), tmp_path, "first", bar.port)
        bar.expect("first", "holding")
        procs["second"] = start(body, tmp_path, "second", bar.port)
        bar.release("second")
        bar.expect("second", "done")
        bar.release("first")
        bar.expect("first", "released")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    assert results["second"]["out"] == "REFUSED", results["second"]


def test_a_killed_holder_releases_the_lease(tmp_path):
    """No sweeper, no stale-lock file to clean up: the OS drops the handle.

    Polled rather than asserted once, because the release is not synchronous
    with the exit on Windows - `Popen.wait()` returns while the kernel is still
    tearing the process down, and the handle goes a moment later. Measured here
    at one retry. This is bounded polling on a real state transition that has
    no event to subscribe to, not a sleep standing in for a signal.
    """
    prof = seeded(tmp_path)
    bar = Rendezvous()
    procs = {"holder": start(HOLDER.format(pid=prof.id), tmp_path, "holder", bar.port)}
    try:
        bar.expect("holder", "holding")
        procs["holder"].kill()
        procs["holder"].wait(30)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:                                # pragma: no cover
                pr.kill()

    deadline = time.monotonic() + 20
    while True:
        try:
            with session_lease(tmp_path, prof.id):
                return
        except SessionBusy:
            if time.monotonic() >= deadline:
                raise AssertionError(
                    "the lease was still held 20s after its holder was killed")
            time.sleep(0.1)


def test_delete_refuses_while_the_session_is_open(tmp_path):
    prof = seeded(tmp_path)
    store = ProfileStore(tmp_path)
    store.user_data_path(prof.id)                # the session directory exists

    bar = Rendezvous()
    procs = {"holder": start(HOLDER.format(pid=prof.id), tmp_path, "holder", bar.port)}
    try:
        bar.expect("holder", "holding")
        with pytest.raises(SessionBusy):
            store.delete(prof.id)
        # Nothing was touched: the refusal comes before the transaction.
        assert store.get(prof.id) is not None
        assert store.user_data_path(prof.id).exists()
        bar.release("holder")
        bar.expect("holder", "released")
        finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert store.delete(prof.id) is True


def test_backup_and_restore_refuse_while_the_session_is_open(tmp_path):
    prof = seeded(tmp_path)
    store = ProfileStore(tmp_path)
    (store.user_data_path(prof.id) / "Cookies").write_text("x", encoding="utf-8")
    archive = store.backup(prof.id, tmp_path / "before.numera")
    assert archive and archive.exists()

    bar = Rendezvous()
    procs = {"holder": start(HOLDER.format(pid=prof.id), tmp_path, "holder", bar.port)}
    try:
        bar.expect("holder", "holding")
        with pytest.raises(SessionBusy):
            store.backup(prof.id, tmp_path / "during.numera")
        with pytest.raises(SessionBusy):
            store.restore(archive)
        assert not (tmp_path / "during.numera").exists()
        bar.release("holder")
        bar.expect("holder", "released")
        finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert store.restore(archive).id == prof.id


# --- the credential half ----------------------------------------------------

def _credentialed(root, name="creds"):
    prof = Profile(name=name, fingerprint=generate(seed=2),
                   proxy=Proxy(server="http://1.2.3.4:8080", username="u",
                               password="SECRET"))
    ProfileStore(root).save(prof)
    return prof


def test_opening_a_session_is_refused_when_the_credential_cannot_be_read(tmp_path):
    """The profile has a password; this process cannot decrypt it. Starting the
    browser anyway would send it out with no proxy credential at all."""
    pytest.importorskip("cryptography")
    from numera import crypto
    from numera.drivers import open_session

    prof = _credentialed(tmp_path)
    ProfileStore(tmp_path).enable_vault("master")

    locked = ProfileStore(tmp_path)              # no password
    with pytest.raises(crypto.VaultLocked):
        with open_session(locked.resolve("creds"), locked):
            pass                                 # pragma: no cover

    unlocked = ProfileStore(tmp_path, password="master")
    with open_session(unlocked.resolve("creds"), unlocked):
        pass


def test_a_passwordless_proxy_still_opens_while_locked(tmp_path):
    """Refusing everything would be the wrong fix: only an *unreadable stored*
    secret is a reason to stop."""
    pytest.importorskip("cryptography")
    from numera.drivers import open_session

    prof = Profile(name="open", fingerprint=generate(seed=3),
                   proxy=Proxy(server="http://1.2.3.4:8080"))
    ProfileStore(tmp_path).save(prof)
    ProfileStore(tmp_path).enable_vault("master")

    locked = ProfileStore(tmp_path)
    with open_session(locked.resolve("open"), locked):
        pass


def test_a_profile_with_no_proxy_is_never_blocked(tmp_path):
    pytest.importorskip("cryptography")
    from numera.drivers import open_session

    prof = seeded(tmp_path, name="bare")
    ProfileStore(tmp_path).enable_vault("master")
    locked = ProfileStore(tmp_path)
    with open_session(locked.resolve("bare"), locked):
        pass
