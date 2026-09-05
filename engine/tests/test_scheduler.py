"""Warm-up schedule bookkeeping: the store round-trips, and the due-rule fires
at the right time. No browser, no clock — a fixed ``now`` is passed in."""

import pytest

from numera.scheduler import (MIN_INTERVAL_HOURS, ScheduleStore, due_schedules,
                               next_run_at)

HOUR = 3600


def test_add_and_list_round_trip(tmp_path):
    st = ScheduleStore(tmp_path)
    entry = st.add(profile_id="p1", every_hours=12, minutes=7, preset="ads", now=1000)
    assert entry["profileId"] == "p1" and entry["preset"] == "ads"
    assert st.list()[0]["id"] == entry["id"]
    assert st.get(entry["id"])["minutes"] == 7


def test_interval_is_floored(tmp_path):
    """A small positive interval is clamped; zero is refused.

    `add()` used to clamp anything, including 0, while `update()` and the API
    create route ran `validate_schedule` and refused a non-positive interval.
    The two surfaces disagreed about the same store. `add()` uses the validator
    now, so 0 is refused everywhere rather than becoming a schedule nobody asked
    for.
    """
    st = ScheduleStore(tmp_path)
    entry = st.add(profile_id="p1", every_hours=0.0001, now=0)
    assert entry["everyHours"] == MIN_INTERVAL_HOURS
    with pytest.raises(ValueError):
        st.add(profile_id="p1", every_hours=0, now=0)


def test_delete(tmp_path):
    st = ScheduleStore(tmp_path)
    e = st.add(profile_id="p1", every_hours=1, now=0)
    assert st.delete(e["id"]) is True
    assert st.list() == []
    assert st.delete("nope") is False


def test_never_run_is_due_at_creation():
    s = {"id": "x", "profileId": "p", "everyHours": 12, "enabled": True,
         "lastRun": None, "createdAt": 5000}
    assert next_run_at(s) == 5000
    assert due_schedules([s], now=5000) == [s]
    assert due_schedules([s], now=4999) == []


def test_next_run_is_last_plus_interval():
    s = {"id": "x", "profileId": "p", "everyHours": 12, "enabled": True,
         "lastRun": 10_000, "createdAt": 0}
    assert next_run_at(s) == 10_000 + 12 * HOUR
    assert due_schedules([s], now=10_000 + 12 * HOUR) == [s]
    assert due_schedules([s], now=10_000 + 11 * HOUR) == []


def test_disabled_never_due():
    s = {"id": "x", "profileId": "p", "everyHours": 1, "enabled": False,
         "lastRun": None, "createdAt": 0}
    assert due_schedules([s], now=10 ** 9) == []


def test_mark_run_moves_next_due(tmp_path):
    st = ScheduleStore(tmp_path)
    e = st.add(profile_id="p1", every_hours=6, now=0)
    assert due_schedules(st.list(), now=1) == st.list()   # due (never run)
    st.mark_run(e["id"], 1000)
    assert due_schedules(st.list(), now=1000) == []        # not due again yet
    assert due_schedules(st.list(), now=1000 + 6 * HOUR) == st.list()

# --- audit findings --------------------------------------------------------

import pytest as _pytest

_pytest.importorskip("fastapi")


@_pytest.fixture
def app_and_client(tmp_path, monkeypatch):
    from fastapi.testclient import TestClient

    from numera.server import create_app

    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    return app, TestClient(app)


class FakeProc:
    """A warm-up process that is still running unless told otherwise."""

    def __init__(self, code=None):
        self.returncode = code
        self.terminated = False

    def poll(self):
        return self.returncode

    def terminate(self):
        self.terminated = True
        self.returncode = -15

    def wait(self, timeout=None):
        return self.returncode

    def kill(self):
        self.returncode = -9


def schedule_for(c, name):
    pid = c.post("/api/profiles", json={"name": name}).json()["id"]
    c.post("/api/schedules", json={"profileId": pid, "everyHours": 1,
                                   "minutes": 1, "preset": "general"})
    return pid


def test_one_unusable_schedule_does_not_stop_the_others(app_and_client, monkeypatch):
    """The try/except wrapped the whole loop, so an exception on the first row
    silently skipped every row after it - and the only symptom was warm-ups
    that quietly stopped happening."""
    from numera.server import routes_schedules as rs

    app, c = app_and_client
    bad = schedule_for(c, "bad")
    good = schedule_for(c, "good")

    real = rs.to_engine_profile
    monkeypatch.setattr(rs, "to_engine_profile",
                        lambda d: (_ for _ in ()).throw(ValueError("unconvertible"))
                        if d.get("id") == bad else real(d))
    started = []
    monkeypatch.setattr(rs.subprocess, "Popen",
                        lambda *a, **k: started.append(a[0][6]) or FakeProc())

    app.state.scheduler_tick()

    assert good in started, "the row after the bad one never ran"
    rows = {s["profileId"]: s for s in c.get("/api/schedules").json()}
    assert rows[bad]["lastOutcome"] == "failed"
    assert rows[bad]["lastError"]


def test_a_warm_up_that_dies_immediately_is_not_recorded_as_run(app_and_client, monkeypatch):
    """`Popen` returning means a process started, not that it worked. Marking
    the schedule as run here hid the failure until the profile went cold."""
    from numera.server import routes_schedules as rs

    app, c = app_and_client
    pid = schedule_for(c, "dies")
    monkeypatch.setattr(rs.subprocess, "Popen", lambda *a, **k: FakeProc(code=1))

    app.state.scheduler_tick()
    row = c.get("/api/schedules").json()[0]
    assert row["lastOutcome"] == "failed"
    assert row["lastRun"] is None, "a warm-up that never ran was marked as run"


def test_the_exit_code_of_a_finished_warm_up_is_recorded(app_and_client, monkeypatch):
    from numera.server import routes_schedules as rs

    app, c = app_and_client
    pid = schedule_for(c, "finishes")
    proc = FakeProc()
    monkeypatch.setattr(rs.subprocess, "Popen", lambda *a, **k: proc)

    app.state.scheduler_tick()
    assert c.get("/api/schedules").json()[0]["lastOutcome"] == "started"

    proc.returncode = 3                      # it crashed after a while
    app.state.reap_warmups()
    row = c.get("/api/schedules").json()[0]
    assert row["lastOutcome"] == "failed"
    assert "3" in (row["lastError"] or "")


def test_shutdown_terminates_the_children_it_started(app_and_client, monkeypatch):
    """Each one holds a lock on a profile's user-data directory. Left behind,
    the next launch of that profile fails with a directory-in-use error that
    names nothing the user can act on."""
    from fastapi.testclient import TestClient

    from numera.server import create_app, routes_schedules as rs

    app, c = app_and_client
    proc = FakeProc()
    monkeypatch.setattr(rs.subprocess, "Popen", lambda *a, **k: proc)
    schedule_for(c, "child")
    app.state.scheduler_tick()

    with TestClient(app):
        pass                                  # entering and leaving fires shutdown
    assert proc.terminated or proc.returncode is not None, \
        "a live child survived shutdown"


# --- one schema for create and update --------------------------------------

@_pytest.mark.parametrize("bad", [
    {"everyHours": "soon"}, {"everyHours": -4}, {"everyHours": 0},
    {"everyHours": 1e9}, {"minutes": -10}, {"minutes": 0},
    {"minutes": "lots"}, {"enabled": "yes"}, {"preset": 5},
])
def test_a_malformed_schedule_is_refused_not_stored(app_and_client, bad):
    """`update()` applied raw patches, so a negative or non-numeric value was
    stored happily and surfaced later as a broken next-run time or a warm-up
    asked to browse for minus four minutes."""
    app, c = app_and_client
    pid = c.post("/api/profiles", json={"name": "s"}).json()["id"]
    made = c.post("/api/schedules", json={"profileId": pid, "everyHours": 12,
                                          "minutes": 5, "preset": "general"})
    assert made.status_code == 200, made.text
    sid = made.json()["id"]

    before = c.get("/api/schedules").json()[0]
    r = c.put(f"/api/schedules/{sid}", json=bad)
    assert r.status_code == 400, (bad, r.status_code)
    assert c.get("/api/schedules").json()[0] == before, "a bad patch was stored"


@_pytest.mark.parametrize("bad", [
    {"everyHours": "soon"}, {"minutes": -1}, {"everyHours": 100000},
])
def test_a_malformed_create_is_a_4xx_not_a_500(app_and_client, bad):
    app, c = app_and_client
    pid = c.post("/api/profiles", json={"name": "s"}).json()["id"]
    r = c.post("/api/schedules", json={"profileId": pid, **bad})
    assert r.status_code == 400, (bad, r.status_code)


def test_scheduler_owned_fields_cannot_be_patched(app_and_client):
    """Writing `lastRun` moves the next run; writing `lastOutcome` forges a
    result nobody produced."""
    app, c = app_and_client
    pid = c.post("/api/profiles", json={"name": "s"}).json()["id"]
    sid = c.post("/api/schedules", json={"profileId": pid}).json()["id"]
    for key in ("lastRun", "lastOutcome", "profileId"):
        assert c.put(f"/api/schedules/{sid}", json={key: 1}).status_code == 400, key


def test_a_valid_update_still_works(app_and_client):
    app, c = app_and_client
    pid = c.post("/api/profiles", json={"name": "s"}).json()["id"]
    sid = c.post("/api/schedules", json={"profileId": pid}).json()["id"]
    r = c.put(f"/api/schedules/{sid}", json={"everyHours": 6, "enabled": False})
    assert r.status_code == 200, r.text
    assert r.json()["everyHours"] == 6
    assert r.json()["enabled"] is False


def test_a_non_finite_interval_is_refused():
    """`inf` cannot travel through JSON, so this is asserted against the
    validator directly - a client library that allows it must still be caught."""
    import pytest as _pt

    from numera.scheduler import validate_schedule

    for bad in (float("inf"), float("-inf"), float("nan")):
        with _pt.raises(ValueError):
            validate_schedule({"everyHours": bad}, partial=True)


def test_a_tiny_interval_is_clamped_not_refused():
    """`add()` has always clamped to the minimum; refusing instead would be a
    behaviour change for anyone already sending a small number."""
    from numera.scheduler import MIN_INTERVAL_HOURS, validate_schedule

    assert validate_schedule({"everyHours": 0.0001},
                             partial=True)["everyHours"] == MIN_INTERVAL_HOURS
