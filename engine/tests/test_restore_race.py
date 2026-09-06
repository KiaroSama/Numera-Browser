"""A failing restore must undo its own work and nothing else.

`archive.restore()` used to snapshot both metadata files, stage the session,
swap it in and commit through the repository - and on *any* exception write
those snapshot bytes back unconditionally. The snapshot was taken outside any
data transaction, and metadata edits need no session lease, so the window
between it and the commit was wide open: a large archive can take minutes to
unpack, and an API or CLI edit committed in that window was acknowledged, then
silently overwritten by a restore that failed afterwards.

The repository's own `write_batch` already restores both records when its
commit fails. The second, older rollback stacked on top of it was the defect.

So restore now: stages outside every lock, takes the session lease, takes a
short root transaction for the swap and the commit, lets the batch own the
metadata rollback, and puts back only the session directory it moved.

The interleaving below is stated with the socket barrier, never with sleeps.
"""
import json
import shutil
from pathlib import Path

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import generate
from numera.models import Profile
from numera.repo import repo
from numera.store import ProfileStore, SessionStranded


def seeded(root, name="alpha", note="") -> Profile:
    store = ProfileStore(root)
    prof = repo(store).create(Profile(name=name, fingerprint=generate(seed=1),
                                      notes=note))
    ud = store.user_data_path(prof.id)
    (ud / "Cookies").write_text("the live session", encoding="utf-8")
    return prof


def backup(root, prof, dest) -> Path:
    return ProfileStore(root).backup(prof.id, dest)


def dash_raw(root, pid) -> dict:
    return json.loads((Path(root) / "dashboard" / f"{pid}.json").read_text(
        encoding="utf-8"))


# --- the barrier regression -------------------------------------------------

# Parks with staging fully prepared and no lock held - `session_lease` is the
# next thing restore does - then fails the swap once it is released. That is
# exactly the shape of the window the old outer rollback reached back across.
RESTORER = """
import shutil
from numera import archive
from numera.store import ProfileStore

_lease = archive.session_lease

def parked(root, pid):
    say("staged")                 # the archive is unpacked; nothing live touched
    wait()                        # ...and the editor commits here
    return _lease(root, pid)

archive.session_lease = parked
archive.shutil.move = lambda *a, **k: (_ for _ in ()).throw(OSError("in use"))

try:
    ProfileStore(ROOT).restore(ARCHIVE)
    print("RESTORED")
except OSError as e:
    print("FAILED", e)
say("done")
"""

EDITOR = """
from numera.repo import repo
from numera.store import ProfileStore

wait()
repo(ProfileStore(ROOT)).edit(PID, lambda p: setattr(p, "notes", "kept") or p)
print("EDITED")
say("edited")
"""


def test_an_edit_that_succeeded_survives_a_failing_restore(tmp_path):
    """The edit lands while the restore holds nothing but its staging
    directory, and the restore then fails. Its rollback must not reach the
    metadata at all: the repository put those records back itself, and the only
    bytes this restore is entitled to undo are the session directory's.
    """
    prof = seeded(tmp_path, note="original")
    archive = backup(tmp_path, prof, tmp_path / "b.zip")

    bar = Rendezvous()
    procs = {}
    try:
        procs["editor"] = start(
            f"PID = {prof.id!r}\n" + EDITOR, tmp_path, "editor", bar.port)
        procs["restorer"] = start(
            f"ARCHIVE = {str(archive)!r}\n" + RESTORER, tmp_path, "restorer", bar.port)
        bar.expect("restorer", "staged")     # staging ready, no lock held
        bar.release("editor")
        bar.expect("editor", "edited")       # the edit is committed, and known to be
        bar.release("restorer")              # only now may the restore fail
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()

    assert all(r["code"] == 0 for r in results.values()), results
    assert results["restorer"]["out"].startswith("FAILED"), results["restorer"]

    store = ProfileStore(tmp_path)
    assert store.get(prof.id).notes == "kept", \
        "the failing restore wrote a stale snapshot over an acknowledged edit"
    assert dash_raw(tmp_path, prof.id)["notes"] == "kept", \
        "the dashboard copy was rolled back to a state nobody asked for"
    # And the session it did not manage to replace is still the live one.
    assert (store.user_data_path(prof.id) / "Cookies").read_text(
        encoding="utf-8") == "the live session"
    assert not [d for d in store.userdata_dir.iterdir() if ".replaced-" in d.name], \
        "the set-aside session was left behind"


def test_a_write_cannot_land_inside_the_restore_transaction(tmp_path):
    """The other half of the fix: while the swap and the commit run, the root
    transaction excludes every other writer, so there is no longer any window
    for an edit to be acknowledged and then undone. Released together, the two
    serialise - and whichever order they take, both results are intact.
    """
    prof = seeded(tmp_path, note="original")
    archive = backup(tmp_path, prof, tmp_path / "b.zip")

    swapping = """
from numera import archive
from numera.store import ProfileStore

_move = archive.shutil.move

def hooked(src, dst):
    say("swapping")               # inside the lease AND the transaction
    wait()
    return _move(src, dst)

archive.shutil.move = hooked
ProfileStore(ROOT).restore(ARCHIVE)
print("RESTORED")
say("done")
"""
    bar = Rendezvous()
    procs = {}
    try:
        procs["editor"] = start(
            f"PID = {prof.id!r}\n" + EDITOR, tmp_path, "editor", bar.port)
        procs["restorer"] = start(
            f"ARCHIVE = {str(archive)!r}\n" + swapping, tmp_path, "restorer", bar.port)
        bar.expect("restorer", "swapping")
        bar.release("editor")        # blocks on the transaction the restore holds
        bar.release("restorer")      # which is released when the restore commits
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()

    assert all(r["code"] == 0 for r in results.values()), results
    assert results["restorer"]["out"].startswith("RESTORED"), results["restorer"]
    assert results["editor"]["out"].startswith("EDITED"), results["editor"]

    store = ProfileStore(tmp_path)
    # The edit was serialised after the restore, so it is the state that stands
    # - and both copies say the same thing, which is the invariant that matters.
    assert store.get(prof.id).notes == "kept"
    assert dash_raw(tmp_path, prof.id)["notes"] == "kept"
    assert (store.user_data_path(prof.id) / "Cookies").read_text(
        encoding="utf-8") == "the live session"


# --- when the rollback itself cannot finish --------------------------------

def test_a_session_that_cannot_be_put_back_is_named_in_the_error(tmp_path,
                                                                 monkeypatch):
    """A restore that fails *and* cannot restore the session leaves the profile
    intact with its browser session under a name nothing points at. The
    original `OSError` describes none of that, so the recovery is a hunt unless
    the error carries both paths.
    """
    prof = seeded(tmp_path)
    archive = backup(tmp_path, prof, tmp_path / "b.zip")
    store = ProfileStore(tmp_path)
    ud = store.user_data_path(prof.id)

    monkeypatch.setattr(shutil, "move",
                        lambda *a, **k: (_ for _ in ()).throw(OSError("in use")))
    real_rename = Path.rename

    def refuse_the_undo(self, target):
        # Only the rollback's rename; the swap-aside that precedes it must run.
        if Path(target) == ud:
            raise OSError("the session directory is held open")
        return real_rename(self, target)

    monkeypatch.setattr(Path, "rename", refuse_the_undo)

    with pytest.raises(SessionStranded) as caught:
        store.restore(archive)

    monkeypatch.undo()
    message = str(caught.value)
    assert ".replaced-" in message, f"the stranded session is not named: {message}"
    assert str(ud) in message, f"the destination is not named: {message}"
    assert "in use" in message, "the failure that started it was dropped"
    # The metadata never moved: the repository's batch never ran.
    assert ProfileStore(tmp_path).get(prof.id).name == "alpha"


def test_a_restore_that_fails_before_the_lease_touches_nothing(tmp_path):
    """Staging happens before any lock now, so a hostile archive is refused
    without the profile's session or records being reached at all."""
    import zipfile

    prof = seeded(tmp_path)
    bad = tmp_path / "evil.zip"
    with zipfile.ZipFile(bad, "w") as z:
        z.writestr("profile.json", json.dumps(prof.to_dict()))
        z.writestr("user-data/../../../pwned.txt", b"x")

    with pytest.raises(ValueError):
        ProfileStore(tmp_path).restore(bad)

    store = ProfileStore(tmp_path)
    assert (store.user_data_path(prof.id) / "Cookies").read_text(
        encoding="utf-8") == "the live session"
    assert not [d for d in store.root.iterdir()
                if d.is_dir() and d.name.startswith("numera-restore-")], \
        "a staging directory was left in the data root"
