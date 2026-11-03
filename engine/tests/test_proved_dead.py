"""Stopping something means proving it stopped.

Two terminators, one defect between them. Each sent a signal and reported
success for having sent it:

* `handshake.end_child` returned `None` when a signal was refused or the
  process was still unreaped after the kill - and every caller read `None` as
  an ordinary exit. `/stop` answered 200 for a browser that was still running,
  a schedule recorded "warm-up exited None", and the registry entry was dropped
  while the child still held the profile's session lease.

* `jobs._terminate_tree` returned True for *delivering* `SIGTERM`, with no
  wait, no escalation and no check - and `stop()` ignored even that. So
  `numera jobs stop`, `numera stop` and the MCP job-stop wrote `state=stopped`
  over a detached browser that was still open.

A signal is a request. Only observing the process answer it is proof.
"""

import subprocess
import sys

import pytest

from numera import jobs
from numera.handshake import ChildTerminationFailed, end_child


class Unkillable:
    """A process that never dies, however it is asked.

    `poll()` stays None throughout, which is exactly the state a real child is
    in while it ignores a signal - and the state the old code reported as an
    ordinary exit.
    """

    def __init__(self, refuse_signals: bool = False):
        self.returncode = None
        self.refuse = refuse_signals
        self.terminated = self.killed = False
        self.pid = 4242

    def poll(self):
        return self.returncode

    def terminate(self):
        if self.refuse:
            raise PermissionError("not permitted")
        self.terminated = True

    def kill(self):
        if self.refuse:
            raise PermissionError("not permitted")
        self.killed = True

    def wait(self, timeout=None):
        raise subprocess.TimeoutExpired("child", timeout)


# --- the in-process child --------------------------------------------------

def test_a_child_that_survives_the_kill_is_not_reported_as_stopped():
    proc = Unkillable()
    with pytest.raises(ChildTerminationFailed) as caught:
        end_child(proc)
    assert proc.terminated and proc.killed, "it did not even escalate"
    assert "still" in str(caught.value).lower()


def test_a_signal_that_is_refused_is_not_reported_as_stopped():
    proc = Unkillable(refuse_signals=True)
    with pytest.raises(ChildTerminationFailed):
        end_child(proc)


def test_a_child_that_dies_between_the_poll_and_the_signal_is_fine():
    """The one case that looks like a refusal and is not: the signal fails
    because there is nothing left to signal."""
    class Raced(Unkillable):
        def terminate(self):
            self.returncode = 0         # it exited just now
            raise ProcessLookupError("no such process")

    assert end_child(Raced()) == 0


def test_stop_session_keeps_a_child_it_could_not_kill(tmp_path, monkeypatch):
    """Dropping the registry entry would tell the next launch the profile was
    free while a browser still had it open - which is the one state nothing can
    recover from on its own."""
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera import server as srv
    from numera.server import create_app

    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    with TestClient(app) as client:
        pid = client.post("/api/profiles", json={"name": "stuck"}).json()["id"]

        def spawn_unkillable(cli, env=None, **kw):
            from _child import FakeChild
            from numera.proctree import Tree

            child = FakeChild(cli, env)          # answers the handshake
            child.wait = lambda timeout=None: (_ for _ in ()).throw(
                subprocess.TimeoutExpired("child", timeout))
            child.poll = lambda: None            # never reaped
            return child, Tree(pid=0, kind="")

        monkeypatch.setattr(srv.proctree, "spawn", spawn_unkillable)
        assert client.post(f"/api/profiles/{pid}/launch").status_code == 200

        answer = client.post(f"/api/profiles/{pid}/stop")
        assert answer.status_code != 200, "a stop nobody proved answered success"
        rows = {p["id"]: p for p in client.get("/api/profiles").json()}
        assert rows[pid]["status"] == "running", \
            "ownership was dropped for a child that is still running"


# --- the detached job ------------------------------------------------------

def test_a_job_that_ignores_the_first_signal_is_escalated(monkeypatch):
    """And it is the escalation that empties the tree, not the wait.

    The tree stays occupied until the forced signal lands, so a version that
    asked once and waited it out cannot reach the assertion.
    """
    from numera import proctree

    calls = []
    monkeypatch.setattr(proctree, "signal_tree",
                        lambda tree, force=False: calls.append(force) or True)
    monkeypatch.setattr(proctree, "alive_tree", lambda tree: True not in calls)

    tree = proctree.Tree(pid=1234, token=1)
    assert proctree.end_tree(tree, grace=0.05, kill_grace=0.05) is True
    assert calls == [False, True], f"escalation did not happen: {calls}"


def test_a_job_that_cannot_be_killed_is_not_recorded_as_stopped(tmp_path, monkeypatch):
    """Writing the outcome anyway is what made `jobs list` say "stopped" for a
    browser that was still open and still holding its profile."""
    rec = jobs.spawn(tmp_path, [sys.executable, "-c", "import sys; sys.stdin.read()"],
                     command="launch", profile=None)
    try:
        from numera import proctree

        monkeypatch.setattr(proctree, "end_tree", lambda *a, **kw: False)
        with pytest.raises(RuntimeError, match="did not stop"):
            jobs.stop(tmp_path, rec["id"])
        monkeypatch.undo()

        after = jobs.get(tmp_path, rec["id"])
        assert after["state"] != "stopped", \
            "a job that could not be killed was recorded as stopped"
    finally:
        proctree.signal_tree(proctree.Tree.from_record(rec), force=True)


HOLDER = """
from numera.paths import session_lease
with session_lease(ROOT, {pid!r}):
    say("holding")
    wait()
say("released")
"""


def test_stopping_a_profile_nobody_here_owns_says_so(tmp_path, capsys):
    """`numera stop` knows only what this machine's CLI detached. Reporting
    "nothing running" for a profile another process is holding was simply
    false - and it is the answer a user acts on."""
    from _rendezvous import Rendezvous, finish, start

    from numera import cli, generate
    from numera.models import Profile
    from numera.repo import repo
    from numera.store import ProfileStore

    prof = repo(ProfileStore(tmp_path)).create(
        Profile(name="held", fingerprint=generate(seed=1)))

    bar = Rendezvous()
    holder = start(HOLDER.format(pid=prof.id), tmp_path, "holder", bar.port)
    try:
        bar.expect("holder", "holding")
        code = cli.main(["--data-dir", str(tmp_path), "stop", "held", "--json"])
        out = capsys.readouterr().out
        bar.release("holder")
        finish({"holder": holder})
    finally:
        bar.close()
        if holder.poll() is None:
            holder.kill()

    assert code == 1, "a stop that stopped nothing reported success"
    assert "another process" in out, out


def _deny_claim_observation(monkeypatch, claim, error=PermissionError, *, operation="both"):
    import os
    from pathlib import Path

    observed = []
    original_stat, original_path_stat, original_open = os.stat, Path.stat, Path.open

    def stat(path, *args, **kwargs):
        if (operation != "read" and path in (claim, str(claim))
                and kwargs.get("follow_symlinks", True)):
            observed.append("stat")
            raise error("fixture claim observation denied")
        return original_stat(path, *args, **kwargs)

    def path_stat(path, *args, **kwargs):
        if operation != "read" and path == claim and kwargs.get("follow_symlinks", True):
            observed.append("stat")
            raise error("fixture claim observation denied")
        return original_path_stat(path, *args, **kwargs)

    def open_file(path, *args, **kwargs):
        if path == claim:
            observed.append("read")
            raise error("fixture claim reading denied")
        return original_open(path, *args, **kwargs)

    # Python 3.10 caches the OS functions in pathlib's accessor at import time.
    monkeypatch.setattr(os, "stat", stat)
    monkeypatch.setattr(Path, "stat", path_stat)
    monkeypatch.setattr(Path, "open", open_file)
    return observed


@pytest.mark.parametrize("error", [PermissionError, OSError])
@pytest.mark.parametrize("operation", ["stat", "read"])
def test_an_unobservable_claim_is_not_reported_free(tmp_path, monkeypatch, error, operation):
    from numera import proctree, sessions
    from numera.paths import SessionBusy

    claim = tmp_path / "sessions/unobserved.json"
    claim.parent.mkdir()
    payload = b'{"session":"preserved","what":"fixture session"}'
    claim.write_bytes(payload)
    with monkeypatch.context() as patch:
        observed = _deny_claim_observation(patch, claim, error, operation=operation)
        assert sessions.read(tmp_path, "unobserved") is sessions.UNREADABLE, \
            "an inaccessible claim was reported absent"
        assert sessions.busy(tmp_path, "unobserved")
        assert sessions.held(tmp_path, "unobserved")
        with pytest.raises(SessionBusy):
            sessions.take(tmp_path, "unobserved", proctree.Tree(pid=0, kind=""))
    assert operation in observed, "claim observation was never attempted"
    assert claim.read_bytes() == payload
    assert sessions.read(tmp_path, "unobserved")["session"] == "preserved"
    claim.unlink()
    assert sessions.read(tmp_path, "unobserved") is None
    assert not sessions.busy(tmp_path, "unobserved")
    assert sessions.read(tmp_path, "../invalid") is None


def test_missing_directory_observation_is_not_a_missing_claim(tmp_path, monkeypatch):
    from pathlib import Path
    from numera import sessions

    original = Path.mkdir
    observed = []

    def mkdir(path, *args, **kwargs):
        if path == tmp_path / "sessions":
            observed.append(path)
            raise FileNotFoundError("fixture claim directory is unavailable")
        return original(path, *args, **kwargs)

    with monkeypatch.context() as patch:
        patch.setattr(Path, "mkdir", mkdir)
        assert sessions.read(tmp_path, "unobserved") is sessions.UNREADABLE
    assert observed
    assert sessions.read(tmp_path, "unobserved") is None


@pytest.mark.parametrize("interface", ["cli", "http"])
@pytest.mark.parametrize("observation", ["lease", "claim"])
def test_unreadable_ownership_cannot_report_a_profile_stopped(
        tmp_path, monkeypatch, capsys, interface, observation):
    import json
    from pathlib import Path
    from numera import cli, data_locks, generate
    from numera.models import Profile
    from numera.repo import repo
    from numera.store import ProfileStore

    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="unobserved", fingerprint=generate(seed=1)))
    original_open = data_locks.os.open
    observed = []
    claim = tmp_path / "sessions" / f"{prof.id}.json"
    payload = b'{"session":"preserved","what":"fixture session"}'
    if observation == "claim":
        claim.parent.mkdir(exist_ok=True)
        claim.write_bytes(payload)

    def open_file(path, *args, **kwargs):
        path = Path(path)
        if path.parent == tmp_path / ".locks" and path.name.startswith("session-"):
            observed.append(path)
            raise PermissionError("fixture lease observation denied")
        return original_open(path, *args, **kwargs)

    def deny(patch):
        if observation == "lease":
            patch.setattr(data_locks.os, "open", open_file)
            return observed
        return _deny_claim_observation(patch, claim)

    def clear_claim():
        if observation == "claim":
            assert claim.read_bytes() == payload
            claim.unlink()

    if interface == "cli":
        with monkeypatch.context() as patch:
            observed = deny(patch)
            code = cli.main(["--data-dir", str(tmp_path), "stop", prof.id, "--json"])
            answer = json.loads(capsys.readouterr().out)
        assert code == 1, "an unobserved session was reported stopped"
        assert answer["ok"] is False and answer["stopped"] == []
        assert "another process" in answer["error"]
        clear_claim()
        assert cli.main(["--data-dir", str(tmp_path), "stop", prof.id, "--json"]) == 0
        assert json.loads(capsys.readouterr().out)["ok"] is True
    else:
        pytest.importorskip("fastapi")
        from fastapi.testclient import TestClient
        from numera.server import create_app

        app = create_app(data_dir=str(tmp_path), start_scheduler=False)
        with TestClient(app) as client:
            with monkeypatch.context() as patch:
                observed = deny(patch)
                rows = {p["id"]: p for p in client.get("/api/profiles").json()}
                assert rows[prof.id]["status"] == "running", "unknown ownership was reported free"
                assert client.post(f"/api/profiles/{prof.id}/stop").status_code == 409
            clear_claim()
            rows = {p["id"]: p for p in client.get("/api/profiles").json()}
            assert rows[prof.id]["status"] == "stopped"
            assert client.post(f"/api/profiles/{prof.id}/stop").status_code == 200
    assert observed, "the OS ownership observation was never attempted"
    assert store.get(prof.id).name == "unobserved"
