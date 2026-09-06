"""When a delete fails, the rollback is the outcome - so it has to be honest.

Delete detaches the session before it touches metadata, which is the right
order: a held directory raises before anything is destroyed. But the rollback
itself could fail quietly. `restore_session()` swallowed a failed rename-back,
so if metadata removal failed *and* the session could not be put back, the
caller saw only the first error while the profile record was restored and its
real session was left stranded under a `.deleting-*` name nothing points at.

A profile that exists with an orphaned session is the one state no screen shows
correctly and no retry repairs.
"""
import json

import pytest

from numera import generate
from numera.models import Profile
from numera.paths import RollbackFailed
from numera.store import ProfileStore, SessionStranded

pytest.importorskip("fastapi")

from numera.server.stores import DashboardStore     # noqa: E402


def make(tmp_path):
    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    prof = dash.save({"name": "victim"})
    pid = prof["id"]
    # A session directory, so there is something to detach and put back.
    session = engine.userdata_dir / pid
    session.mkdir(parents=True, exist_ok=True)
    (session / "Cookies").write_text("real-session", encoding="utf-8")
    return engine, dash, pid, session


def test_a_failed_delete_puts_the_session_back(tmp_path, monkeypatch):
    """The ordinary rollback: metadata removal fails, everything returns."""
    engine, dash, pid, session = make(tmp_path)

    real_unlink = type(session).unlink

    def refuse(self, *a, **k):
        if self.suffix == ".json":
            raise OSError("metadata is read-only")
        return real_unlink(self, *a, **k)

    monkeypatch.setattr("pathlib.Path.unlink", refuse)
    with pytest.raises(OSError):
        dash.delete(pid)
    monkeypatch.undo()

    assert dash.get(pid) is not None, "the profile did not come back"
    assert (session / "Cookies").read_text(encoding="utf-8") == "real-session", \
        "the session was not restored"
    assert not list(engine.userdata_dir.glob(".deleting-*")), \
        "a tombstone was left behind by a rolled-back delete"


def test_a_rollback_that_cannot_finish_names_the_tombstone(tmp_path, monkeypatch):
    """Both halves fail, and the caller gets every casualty, not just the first.

    This used to raise the `SessionStranded` alone, chained onto the delete
    failure. That was right about which message matters - the one saying where
    the session went - and wrong about how many there can be: the metadata
    restore can fail too, and a rollback that stops at its first problem never
    attempts the rest. `RollbackFailed` carries the original failure and every
    path that could not be put back, which is what a repair actually needs.
    """
    engine, dash, pid, session = make(tmp_path)

    real_unlink = type(session).unlink

    def refuse_unlink(self, *a, **k):
        if self.suffix == ".json":
            raise OSError("metadata is read-only")
        return real_unlink(self, *a, **k)

    real_rename = type(session).rename

    def refuse_rename(self, *a, **k):
        # Only the restore direction. `detach_session` renames *to* the
        # tombstone and must still work, or the delete fails before it ever
        # reaches the metadata this test is about.
        if ".deleting-" in self.name:
            raise OSError("the session directory is held open")
        return real_rename(self, *a, **k)

    monkeypatch.setattr("pathlib.Path.unlink", refuse_unlink)
    monkeypatch.setattr("pathlib.Path.rename", refuse_rename)
    with pytest.raises(RollbackFailed) as caught:
        dash.delete(pid)
    monkeypatch.undo()

    message = str(caught.value)
    assert ".deleting-" in message or "deleting" in message.lower(), \
        f"the error does not name the tombstone: {message}"
    assert pid in message
    # The original failure is preserved both ways, so nothing is hidden.
    assert caught.value.__cause__ is not None, "the first failure was dropped"
    assert isinstance(caught.value.cause, OSError)
    assert any(isinstance(err, SessionStranded) for _path, err in caught.value.failures), \
        "the stranded session is not among the casualties"


def test_the_metadata_comes_back_through_the_atomic_writer(tmp_path, monkeypatch):
    """Rollback restored with a plain `write_bytes`, which truncates first - a
    crash mid-rollback left an empty profile file, which reads as corruption
    rather than as the delete that did not happen."""
    engine, dash, pid, session = make(tmp_path)
    path = tmp_path / "dashboard" / f"{pid}.json"
    before = path.read_bytes()

    real_unlink = type(session).unlink
    seen = {"engine_gone": False}

    def refuse(self, *a, **k):
        # Let the dashboard copy go, then refuse the engine one, so the
        # rollback has real work to do.
        if self.parent.name == "profiles":
            seen["engine_gone"] = True
            raise OSError("engine store is read-only")
        return real_unlink(self, *a, **k)

    monkeypatch.setattr("pathlib.Path.unlink", refuse)
    with pytest.raises(OSError):
        dash.delete(pid)
    monkeypatch.undo()

    assert seen["engine_gone"], "the test never reached the engine record"
    assert path.read_bytes() == before, "the restored metadata is not what was there"
    json.loads(path.read_text(encoding="utf-8"))      # and it is still valid JSON


def test_a_successful_delete_still_removes_everything(tmp_path):
    """The guard must not turn a working delete into a failure."""
    engine, dash, pid, session = make(tmp_path)

    assert dash.delete(pid) is True
    assert dash.get(pid) is None
    assert not (tmp_path / "dashboard" / f"{pid}.json").exists()
    assert not session.exists()
