"""A spawned session is nobody's until the child says it owns the profile.

Three defects lived in the gap between "a process started" and "a browser is
open on this profile":

* the gate checked the profile and *then* took the lease, so a delete
  committing in between let the opener launch the snapshot it had already read
  - recreating `user-data/<id>` for a profile that no longer existed;
* the API registered the child the instant `Popen` returned, so a launch about
  to exit with a locked vault or someone else's session was reported as
  running, and two simultaneous requests spawned two children with the loser
  overwriting the winner's registration - `/stop` then killed the wrong one;
* the routes materialised the engine copy from a dashboard record read before
  all of that, reverting whatever had been edited meanwhile.

The fix is one order and one signal: lease, re-read, announce - see
`numera.handshake`.
"""

import json
import os
import subprocess
import sys
import threading

import pytest
from fastapi.testclient import TestClient

from _child import FakeChild, spawning_tree
from _rendezvous import (ENGINE, RENDEZVOUS_TIMEOUT, Rendezvous, finish,
                         run_all, start)

from numera import generate
from numera.handshake import READY_ENV, StartupFailed, await_ready, ready_channel
from numera.models import Profile
from numera.repo import repo
from numera.server import create_app
from numera.store import ProfileStore

HOLDER = """
from numera.paths import session_lease
with session_lease(ROOT, {pid!r}):
    say("holding")
    wait()
say("released")
"""

# Switches the engine from another process, on the parent's word, and does not
# report back until the write has committed.
ENGINE_SWAP = """
from numera.repo import repo
from numera.store import ProfileStore
wait()
repo(ProfileStore(ROOT)).edit({pid!r}, lambda p: setattr(p, "engine", "camoufox") or p)
say("changed")
"""


@pytest.fixture()
def app_and_client(tmp_path):
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    with TestClient(app) as client:
        yield app, client


def a_profile(client, name="p") -> str:
    return client.post("/api/profiles", json={"name": name}).json()["id"]


def status_of(client, pid: str) -> str:
    """Live state as the dashboard sees it. The listing is where status is
    reported - there is no single-profile GET."""
    rows = {p["id"]: p for p in client.get("/api/profiles").json()}
    return rows[pid]["status"]


# --- the signal itself, with real processes --------------------------------

def _child(body: str, ready_path) -> subprocess.Popen:
    script = f"import sys; sys.path.insert(0, {ENGINE!r})\n" + body
    return subprocess.Popen([sys.executable, "-c", script],
                            env={**os.environ, READY_ENV: str(ready_path)})


def test_a_child_that_reports_ready_is_waited_for():
    with ready_channel() as ready:
        proc = _child("from numera.handshake import announce\nannounce(True)", ready)
        try:
            await_ready(proc, ready, timeout=RENDEZVOUS_TIMEOUT)   # returns, or fails
        finally:
            proc.wait(timeout=RENDEZVOUS_TIMEOUT)


def test_a_refusal_arrives_with_the_reason_that_caused_it():
    """The point of the file: "it exited 1" is not an answer a caller can map
    to a status code, and a locked vault is not the same problem as a busy
    session."""
    with ready_channel() as ready:
        proc = _child("from numera.handshake import announce\n"
                      "announce(False, 'the vault is locked', 'locked')\n"
                      "raise SystemExit(1)", ready)
        try:
            with pytest.raises(StartupFailed) as caught:
                await_ready(proc, ready, timeout=RENDEZVOUS_TIMEOUT)
        finally:
            proc.wait(timeout=RENDEZVOUS_TIMEOUT)
    assert caught.value.kind == "locked"
    assert "vault" in str(caught.value)


def test_a_child_that_dies_without_a_word_is_not_ready():
    with ready_channel() as ready:
        proc = _child("raise SystemExit(3)", ready)
        try:
            with pytest.raises(StartupFailed) as caught:
                await_ready(proc, ready, timeout=RENDEZVOUS_TIMEOUT)
        finally:
            proc.wait(timeout=RENDEZVOUS_TIMEOUT)
    assert "3" in str(caught.value)


def test_the_wait_is_bounded():
    """A child that neither answers nor dies must not hold the parent forever.
    It blocks on an event nothing ever sets rather than sleeping for a chosen
    number of seconds, so the parent's deadline is the only clock in the test
    and the child ends the moment it is terminated. (stdin is closed under
    pytest, so reading it returns at once and tests nothing.)"""
    with ready_channel() as ready:
        proc = _child("import threading\nthreading.Event().wait()", ready)
        try:
            with pytest.raises(StartupFailed) as caught:
                await_ready(proc, ready, timeout=0.5)
            assert caught.value.kind == "timeout"
        finally:
            proc.terminate()
            proc.wait(timeout=RENDEZVOUS_TIMEOUT)


# --- the gate takes the lease before it decides anything -------------------

def test_a_delete_that_lands_before_the_lease_cannot_launch(tmp_path, monkeypatch):
    """The TOCTOU the old order left open.

    The check came first and the lease second, so a delete could commit and
    release its own lease in between - and the opener then took the free lease
    and launched the profile it had read, recreating a session directory for
    something that no longer existed. The delete here fires at the exact
    instant the opener reaches for the lease.
    """
    from numera import drivers, paths

    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="doomed", fingerprint=generate(seed=1)))
    real = paths.session_lease
    fired = []

    def racing(root, profile_id):
        if not fired:                      # the delete needs the lease too
            fired.append(True)
            repo(ProfileStore(tmp_path)).delete(profile_id)
        return real(root, profile_id)

    monkeypatch.setattr(paths, "session_lease", racing)

    with pytest.raises(drivers.ProfileGone):
        with drivers.open_session(prof, store):
            pytest.fail("a deleted profile was handed to a browser")
    assert fired, "the delete never ran - the test proves nothing"
    assert not (store.userdata_dir / prof.id).exists(), \
        "a session directory was recreated for a deleted profile"


def test_the_gate_yields_the_profile_as_it_is_now(tmp_path, monkeypatch):
    """Callers launch what the gate returns, so a rename committed before the
    lease reaches the browser instead of the caller's stale object."""
    from numera import drivers, paths

    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="before", fingerprint=generate(seed=1)))
    real = paths.session_lease
    fired = []

    def racing(root, profile_id):
        if not fired:
            fired.append(True)
            repo(ProfileStore(tmp_path)).edit(
                profile_id, lambda p: setattr(p, "name", "after") or p)
        return real(root, profile_id)

    monkeypatch.setattr(paths, "session_lease", racing)
    with drivers.open_session(prof, store) as current:
        assert current.name == "after"


# --- and the caller has to launch what the gate yielded --------------------

def _engine_used(tmp_path, monkeypatch, override):
    """Run `numera launch` while another process switches the engine.

    The swap commits at the exact instant the command reaches for the lease -
    the window `open_session` exists to close - and the launcher records which
    backend it was asked for instead of opening a browser.
    """
    from types import SimpleNamespace

    from numera import launcher, paths
    from numera.cli.browser import cmd_launch

    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="switching", fingerprint=generate(seed=2)))
    assert prof.engine == "playwright", "the profile has to start somewhere else"

    used, fired = [], []
    monkeypatch.setattr(launcher, "launch",
                        lambda p, s, **kw: used.append(kw.get("engine")))
    real = paths.session_lease

    def drive(bar):
        def racing(root, profile_id):
            if not fired:
                fired.append(True)
                bar.release("editor")
                bar.expect("editor", "changed")   # blocks until it is committed
            return real(root, profile_id)

        monkeypatch.setattr(paths, "session_lease", racing)
        cmd_launch(SimpleNamespace(ref=prof.id, engine=override, headless=True), store)

    run_all({"editor": ENGINE_SWAP.format(pid=prof.id)}, tmp_path, drive)
    assert fired, "the swap never ran - the test proves nothing"
    assert ProfileStore(tmp_path).get(prof.id).engine == "camoufox", \
        "the other process's write did not land"
    return used


def test_the_engine_is_read_from_the_profile_the_lease_re_read(tmp_path, monkeypatch):
    """`cmd_launch` resolved the profile, picked the engine off it, and only
    then took the lease - so a switch landing in between sent the browser out
    on the old backend while every stored copy already said the new one."""
    assert _engine_used(tmp_path, monkeypatch, override=None) == ["camoufox"]


def test_an_explicit_engine_still_wins(tmp_path, monkeypatch):
    """--engine is a decision the user just made, and it is not a stale read."""
    assert _engine_used(tmp_path, monkeypatch, override="playwright") == ["playwright"]


# --- one owner per profile, and only once it owns it -----------------------

def test_two_simultaneous_launches_leave_exactly_one_owner(app_and_client, monkeypatch):
    """Both requests used to spawn, and the loser's registration overwrote the
    winner's - so `/stop` terminated a process that owned nothing while the
    real browser stayed open."""
    from numera import server as srv

    app, client = app_and_client
    pid = a_profile(client, "raced")
    children: list = []
    monkeypatch.setattr(srv.proctree, "spawn", spawning_tree(children))

    both, answers = threading.Barrier(2), {}

    def launch(who):
        both.wait(timeout=RENDEZVOUS_TIMEOUT)
        answers[who] = client.post(f"/api/profiles/{pid}/launch")

    threads = [threading.Thread(target=launch, args=(n,)) for n in ("a", "b")]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=RENDEZVOUS_TIMEOUT)
        assert not t.is_alive(), "a launch request never returned"

    assert len(children) == 1, f"{len(children)} children for one profile"
    assert [r.status_code for r in answers.values()] == [200, 200]
    client.post(f"/api/profiles/{pid}/stop")
    assert children[0].terminated, "/stop did not terminate the registered owner"


def test_a_locked_vault_launch_is_never_reported_as_running(app_and_client, monkeypatch):
    from numera import server as srv

    app, client = app_and_client
    pid = a_profile(client, "locked")
    children: list = []
    monkeypatch.setattr(srv.proctree, "spawn",
                        spawning_tree(children, ready=False, kind="locked",
                                 error="the vault is locked", code=1))

    answer = client.post(f"/api/profiles/{pid}/launch")
    assert answer.status_code == 423, answer.text
    assert "vault" in answer.json()["detail"]
    assert status_of(client, pid) == "stopped"


def test_a_child_that_exits_before_readiness_owns_nothing(app_and_client, monkeypatch):
    from numera import server as srv

    app, client = app_and_client
    pid = a_profile(client, "stillborn")
    children: list = []
    monkeypatch.setattr(srv.proctree, "spawn",
                        spawning_tree(children, ready=None, code=1))

    assert client.post(f"/api/profiles/{pid}/warmup",
                       json={"minutes": 1}).status_code == 500
    assert status_of(client, pid) == "stopped"


def test_a_foreign_lease_refuses_the_launch(app_and_client, tmp_path, monkeypatch):
    """`is_running()` can see another process's lease, but `stop_session()` can
    only terminate a local child - so answering "running" would offer a /stop
    that does nothing, and spawning anyway would put two browsers in one
    Chromium profile directory."""
    from numera import server as srv

    app, client = app_and_client
    pid = a_profile(client, "foreign")
    children: list = []

    bar = Rendezvous()
    # Started before the patch: `srv.subprocess` *is* the stdlib module, so
    # replacing `Popen` on it replaces the one the harness uses to start this
    # holder too, and the lease would never be taken by anybody.
    holder = start(HOLDER.format(pid=pid), tmp_path, "holder", bar.port)
    try:
        bar.expect("holder", "holding")
        monkeypatch.setattr(srv.proctree, "spawn", spawning_tree(children))
        answer = client.post(f"/api/profiles/{pid}/launch")
        attach = client.post(f"/api/profiles/{pid}/attach")
        bar.release("holder")
        finish({"holder": holder})
    finally:
        bar.close()
        if holder.poll() is None:
            holder.kill()

    assert answer.status_code == 409, answer.text
    assert attach.status_code == 409, attach.text
    assert children == [], "a child was spawned for a profile someone else owns"


# --- the engine copy is derived from the record that exists now ------------

def test_materialising_cannot_revert_a_concurrent_edit(app_and_client, monkeypatch):
    """A launch read the dashboard record, and wrote `to_engine_profile()` of
    that snapshot afterwards. An edit committed in between was undone on the
    engine side only, so the two copies disagreed and the browser opened the
    identity the dashboard no longer described."""
    from numera import server as srv

    app, client = app_and_client
    pid = a_profile(client, "before")
    monkeypatch.setattr(srv.proctree, "spawn", spawning_tree([]))

    original, once = app.state.dash.get, []

    def get_then_edit(wanted: str):
        record = original(wanted)
        if not once:
            once.append(True)
            client.put(f"/api/profiles/{pid}", json={**record, "name": "after"})
        return record

    monkeypatch.setattr(app.state.dash, "get", get_then_edit)
    assert client.post(f"/api/profiles/{pid}/launch").status_code == 200
    assert once, "the concurrent edit never ran - the test proves nothing"

    engine = ProfileStore(app.state.engine_store.root).get(pid)
    assert engine.name == "after", "the engine copy was reverted to the snapshot"
    assert app.state.dash.get(pid)["name"] == "after"


def _edit_once_on_read(app, client, pid, monkeypatch, name="after"):
    """Make an unrelated edit commit the first time the route reads the record.

    That is the window the bug lived in: the route held a snapshot while a
    browser ran, and wrote it back afterwards.
    """
    original, once = app.state.dash.get, []

    def get_then_edit(wanted: str):
        record = original(wanted)
        if not once:
            once.append(True)
            client.put(f"/api/profiles/{pid}", json={**record, "name": name})
        return record

    monkeypatch.setattr(app.state.dash, "get", get_then_edit)
    return once


def test_a_probe_does_not_materialise_a_stale_snapshot(app_and_client, monkeypatch):
    """Every probe route goes through `_prepare`, which used to write
    `to_engine_profile()` of the record it had just read - so an edit landing
    while a trust run held a browser open was reverted on the engine side."""
    import subprocess as sp

    app, client = app_and_client
    pid = a_profile(client, "probed")
    monkeypatch.setattr(sp, "run",
                        lambda *a, **k: sp.CompletedProcess(a[0], 0, '{"ok": true}', ""))
    once = _edit_once_on_read(app, client, pid, monkeypatch)

    assert client.post(f"/api/profiles/{pid}/proxy-test").status_code == 200
    assert once, "the concurrent edit never ran - the test proves nothing"
    assert ProfileStore(app.state.engine_store.root).get(pid).name == "after"


def test_the_scheduler_does_not_materialise_a_stale_snapshot(tmp_path, monkeypatch):
    from numera import server as srv

    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    with TestClient(app) as client:
        pid = a_profile(client, "scheduled")
        client.post("/api/schedules", json={"profileId": pid, "everyHours": 1,
                                            "minutes": 1, "preset": "general"})
        monkeypatch.setattr(srv.proctree, "spawn", spawning_tree([]))
        once = _edit_once_on_read(app, client, pid, monkeypatch)

        app.state.scheduler_tick()
        assert once, "the concurrent edit never ran - the test proves nothing"
        assert ProfileStore(tmp_path).get(pid).name == "after"


def test_materialising_a_profile_that_is_gone_says_so(tmp_path):
    store = ProfileStore(tmp_path)
    assert repo(store).materialize_current("nosuchprofile00") is None
