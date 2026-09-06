"""A session process is not just a process: the registry has to say what it is.

`running` was `pid -> Popen`, and the scheduler's reaper walked all of it. Every
exited child was matched to schedules by `profileId` alone - but the same
dictionary holds ordinary launches, attach sessions and manual warm-ups, so:

* closing a normal browser rewrote a schedule's `lastOutcome`;
* one scheduled warm-up exiting wrote both rows of a profile with two
  schedules, one of which had not run at all;
* whichever of the reaper and `is_running()` saw the exit first took the record
  away from the other, so a reaped child never stamped `_last_active`.

A registration now carries what the child was started for and, for a scheduled
warm-up, the exact occurrence that spawned it; every exit leaves by one
finalisation path. And `POST /stop`, which can only terminate a child this
server started, no longer answers 200 for a browser it did not stop.
"""

import pytest
from fastapi.testclient import TestClient

from _child import FakeChild, spawning
from _rendezvous import Rendezvous, finish, start

from numera.server import KINDS, create_app

HOLDER = """
from numera.paths import session_lease
with session_lease(ROOT, {pid!r}):
    say("holding")
    wait()
say("released")
"""


@pytest.fixture()
def app_and_client(tmp_path):
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    with TestClient(app) as client:
        yield app, client


@pytest.fixture()
def children(monkeypatch):
    """Stand-in session processes that answer the startup handshake."""
    from numera import server as srv

    made: list = []
    monkeypatch.setattr(srv.subprocess, "Popen", spawning(made))
    return made


def a_profile(client, name="p") -> str:
    return client.post("/api/profiles", json={"name": name}).json()["id"]


def a_schedule(client, pid: str) -> str:
    return client.post("/api/schedules",
                       json={"profileId": pid, "everyHours": 1, "minutes": 1,
                             "preset": "general"}).json()["id"]


def rows_by_id(client) -> dict:
    return {s["id"]: s for s in client.get("/api/schedules").json()}


def exit_with(child: FakeChild, code: int) -> None:
    child.returncode = code


# --- only a scheduled child updates a schedule -----------------------------

def test_an_ordinary_launch_closing_is_not_a_warm_up_result(app_and_client, children):
    """A user opening a profile, browsing and closing the window says nothing
    about that profile's schedule. The reaper used to write `lastOutcome` for
    it anyway, so a launch closed at noon reported a warm-up nobody ran."""
    app, client = app_and_client
    pid = a_profile(client, "launched")
    sid = a_schedule(client, pid)

    assert client.post(f"/api/profiles/{pid}/launch").status_code == 200
    assert app.state.running[pid].kind == "launch"
    exit_with(children[0], 0)
    app.state.reap_children()

    # A row nothing ever touched has no outcome key at all, which is the honest
    # shape of "no result was recorded".
    row = rows_by_id(client)[sid]
    assert row.get("lastOutcome") is None, "a launch was recorded as a warm-up result"
    assert row.get("lastRun") is None
    assert app.state.running == {}, "the exited child stayed registered"


def test_a_manual_warm_up_is_not_attributed_to_a_schedule(app_and_client, children):
    """`POST /warmup` is somebody pressing a button, not the occurrence coming
    round. It shares the command and the registry with the scheduled kind,
    which is exactly why the registry has to tell them apart."""
    app, client = app_and_client
    pid = a_profile(client, "warmed")
    sid = a_schedule(client, pid)

    assert client.post(f"/api/profiles/{pid}/warmup",
                       json={"minutes": 1}).status_code == 200
    assert app.state.running[pid].kind == "manual_warmup"
    exit_with(children[0], 1)
    app.state.reap_children()

    row = rows_by_id(client)[sid]
    assert row.get("lastOutcome") is None, "a manual warm-up was written into a schedule"
    assert row.get("lastError") is None


def test_one_scheduled_child_updates_one_occurrence(app_and_client, children):
    """Two schedules on one profile are two rows with their own intervals and
    their own history. One warm-up exiting used to write both, so the row that
    had not run reported a result - and its next run moved on that strength."""
    app, client = app_and_client
    pid = a_profile(client, "twice")
    a_schedule(client, pid)
    a_schedule(client, pid)

    # Only one can start: the second row finds the profile busy and waits for
    # the next tick, which is the situation that produced the crossed write.
    app.state.scheduler_tick()
    started = [s for s in rows_by_id(client).values()
               if s.get("lastOutcome") == "started"]
    waiting = [s for s in rows_by_id(client).values()
               if s.get("lastOutcome") is None]
    assert len(started) == 1 and len(waiting) == 1, rows_by_id(client)
    assert app.state.running[pid].schedule_id == started[0]["id"]

    exit_with(children[0], 3)
    app.state.reap_children()

    after = rows_by_id(client)
    assert after[started[0]["id"]]["lastOutcome"] == "failed"
    assert "3" in (after[started[0]["id"]]["lastError"] or "")
    assert after[waiting[0]["id"]].get("lastOutcome") is None, \
        "a schedule that never ran was given the other one's result"
    assert after[waiting[0]["id"]].get("lastRun") is None


def test_a_scheduled_warm_up_is_still_recorded(app_and_client, children):
    """The narrowing must not cost the recording it narrows: the occurrence
    that actually spawned the child still gets its outcome."""
    app, client = app_and_client
    pid = a_profile(client, "scheduled")
    sid = a_schedule(client, pid)

    app.state.scheduler_tick()
    assert app.state.running[pid].kind == "scheduled_warmup"
    exit_with(children[0], 0)
    app.state.reap_children()

    assert rows_by_id(client)[sid]["lastOutcome"] == "ok"


def test_the_kind_of_a_child_is_not_guessable(app_and_client, children):
    """A typo would read as "not a scheduled warm-up" and silently drop a
    result, so the registry refuses a kind it does not know."""
    app, client = app_and_client
    pid = a_profile(client, "typo")
    with pytest.raises(ValueError):
        app.state.spawn_owner(pid, ["python"], kind="warmup")
    assert set(KINDS) == {"launch", "attach", "manual_warmup", "scheduled_warmup"}
    assert children == [], "a child was started for a kind nobody recognises"


# --- every exit leaves the same trail --------------------------------------

def test_a_reaped_child_stamps_last_active(app_and_client, children):
    """`_last_active` is what the dashboard shows for a stopped profile. The
    reaper removed the child before `is_running()` could stamp it, so a warm-up
    the scheduler noticed first left the profile reading "never"."""
    app, client = app_and_client
    pid = a_profile(client, "aged")
    assert app.state.dash.get(pid).get("_last_active") is None

    assert client.post(f"/api/profiles/{pid}/launch").status_code == 200
    exit_with(children[0], 0)
    app.state.reap_children()

    assert isinstance(app.state.dash.get(pid).get("_last_active"), float), \
        "a child reaped by the scheduler never stamped _last_active"


def test_stopping_a_child_stamps_last_active_too(app_and_client, children):
    app, client = app_and_client
    pid = a_profile(client, "stopped")
    assert client.post(f"/api/profiles/{pid}/launch").status_code == 200

    assert client.post(f"/api/profiles/{pid}/stop").status_code == 200
    assert children[0].terminated, "/stop did not terminate the registered owner"
    assert app.state.running == {}
    assert isinstance(app.state.dash.get(pid).get("_last_active"), float)


# --- /stop tells the truth about what it stopped ---------------------------

def test_stop_refuses_to_claim_it_stopped_a_foreign_owner(app_and_client, tmp_path,
                                                          monkeypatch):
    """`stop_session` terminates a local child and nothing else. With the lease
    held by another server, a desktop or a CLI, it does nothing at all - and
    the route answered 200, so the dashboard showed the profile as stopped
    while the browser was still open somewhere else."""
    app, client = app_and_client
    pid = a_profile(client, "foreign")

    bar = Rendezvous()
    holder = start(HOLDER.format(pid=pid), tmp_path, "holder", bar.port)
    try:
        bar.expect("holder", "holding")
        answer = client.post(f"/api/profiles/{pid}/stop")
        bar.release("holder")
        finish({"holder": holder})
    finally:
        bar.close()
        if holder.poll() is None:                            # pragma: no cover
            holder.kill()

    assert answer.status_code == 409, answer.text
    assert "did not start it" in answer.json()["detail"]


def test_stopping_a_profile_nobody_is_running_is_still_fine(app_and_client):
    """Nothing to stop is not a conflict: the browser is closed, which is what
    the caller asked for."""
    app, client = app_and_client
    pid = a_profile(client, "idle")
    answer = client.post(f"/api/profiles/{pid}/stop")
    assert answer.status_code == 200, answer.text
    assert answer.json()["status"] == "stopped"


def test_stopping_a_profile_that_does_not_exist_is_still_a_404(app_and_client):
    """The new conflict must not swallow the older answer: no such profile is
    not "someone else has it open"."""
    _app, client = app_and_client
    assert client.post("/api/profiles/nosuchprofile00/stop").status_code == 404
