"""Ready means a browser is open, not that the lease was taken.

`drivers.open_session` used to announce success as soon as it held the session
lease and had re-read the profile - and only *then* yield to the caller that
starts the browser. Everything a launch can still fail on happened after the
parent had been told it was running:

* a missing browser executable, or a backend that will not import;
* an invalid launch option, or a headful launch with no display;
* a persistent context that will not open.

So the API returned 200, registered `running[pid]` and let the scheduler stamp
`lastRun` for sessions that were about to die. Readiness now lives on the far
side of the browser starting - every backend calls `drivers._opened()` the
instant it holds a live context and page - and the gate announces only failure,
and only a failure that got in before any browser opened.

The end-to-end cases here run a real `numera` child against an empty
`PLAYWRIGHT_BROWSERS_PATH`, which is the first failure in that list: the lease
is genuinely taken, the vault check genuinely passes, and Playwright then says
the executable does not exist.
"""

import os
import subprocess
import sys

import pytest
from fastapi.testclient import TestClient

from _rendezvous import ENGINE, RENDEZVOUS_TIMEOUT

from numera import drivers, generate
from numera.handshake import READY_ENV
from numera.models import Profile
from numera.repo import repo
from numera.server import create_app
from numera.store import ProfileStore


@pytest.fixture()
def ready_file(tmp_path, monkeypatch):
    """The channel a spawned child answers on, pointed somewhere we can read."""
    path = tmp_path / "channel" / "ready.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    monkeypatch.setenv(READY_ENV, str(path))
    return path


def a_profile(store, name="p") -> Profile:
    return repo(store).create(Profile(name=name, fingerprint=generate(seed=1)))


# --- the gate no longer speaks for the browser -----------------------------

def test_owning_the_lease_is_not_readiness(tmp_path, ready_file):
    """The defect itself, at its smallest.

    Inside the gate the lease is held and the profile has been re-read, which
    is everything the old code needed to call the session started. No browser
    has been asked for yet - the caller does that after the yield - so nothing
    may have been said to the parent at this point.
    """
    store = ProfileStore(tmp_path)
    with drivers.open_session(a_profile(store, "quiet"), store):
        assert not ready_file.exists(), \
            "the parent was told the session was ready before a browser opened"


def test_a_backend_announces_the_browser_it_opened(tmp_path, ready_file):
    """The other half: the signal has to actually be emitted at the new point.

    Driven against a fake Playwright rather than a real one, so this asserts
    where the announcement happens in `_launch_playwright_like` and not whether
    a Chromium is installed on the machine running the suite.
    """
    import json

    store = ProfileStore(tmp_path)
    prof = a_profile(store, "opens")
    seen = []

    class _Page:
        url = "about:blank"

        def goto(self, *a, **k):
            pass

    class _Context:
        pages: list = []

        def add_init_script(self, script):
            pass

        def on(self, event, handler):
            pass

        def new_cdp_session(self, page):
            class _Cdp:
                def send(self, method, params=None):
                    pass
            return _Cdp()

        def new_page(self):
            # The browser exists from here on, so this is the last moment at
            # which nothing may have been announced.
            seen.append(ready_file.exists())
            return _Page()

    class _Playwright:
        class chromium:
            @staticmethod
            def launch_persistent_context(**kwargs):
                return _Context()

        def start(self):
            return self

        def stop(self):
            pass

    drivers._launch_playwright_like(_Playwright, prof, store, headless=True,
                                    keep_open=True)
    assert seen == [False], "readiness was announced before the page existed"
    assert ready_file.exists(), "an open browser never reported itself ready"
    assert json.loads(ready_file.read_text(encoding="utf-8"))["ok"] is True


def test_a_failure_after_the_lease_is_reported_as_a_failure(tmp_path, ready_file):
    """A backend that dies where the browser should have opened is the case the
    parent must hear about, with the reason attached rather than an exit code."""
    import json

    store = ProfileStore(tmp_path)
    prof = a_profile(store, "doomed")
    with pytest.raises(RuntimeError):
        with drivers.open_session(prof, store):
            raise RuntimeError("Executable doesn't exist at /nowhere/chrome")

    answer = json.loads(ready_file.read_text(encoding="utf-8"))
    assert answer["ok"] is False
    assert "Executable" in answer["error"]


# --- end to end, with a real child that cannot open a browser --------------

@pytest.fixture()
def no_browsers(tmp_path, monkeypatch):
    """A child that will reach Playwright and find no browser to launch.

    `use_bundled_browsers()` leaves an explicit `PLAYWRIGHT_BROWSERS_PATH`
    alone, so pointing it at an empty directory is a deterministic version of
    the machine that has never downloaded Chromium - and it fails *after* the
    lease and the vault check, which is the whole point.

    `PYTHONPATH` because the engine is importable from its own directory
    rather than installed into the interpreter running the suite.
    """
    empty = tmp_path / "no-browsers"
    empty.mkdir()
    monkeypatch.setenv("PLAYWRIGHT_BROWSERS_PATH", str(empty))
    monkeypatch.setenv("PYTHONPATH",
                       os.pathsep.join([ENGINE, os.environ.get("PYTHONPATH", "")]))
    return empty


@pytest.fixture()
def spawned(monkeypatch):
    """Every real child the server starts, so the test can see it was reaped."""
    from numera import server as srv

    made: list = []
    real = srv.proctree.spawn

    def watched(cli, **kw):
        proc, tree = real(cli, **kw)
        made.append(proc)
        return proc, tree

    monkeypatch.setattr(srv.proctree, "spawn", watched)
    return made


@pytest.fixture()
def app_and_client(tmp_path):
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    with TestClient(app) as client:
        yield app, client


def _reap(procs) -> None:
    for proc in procs:
        if proc.poll() is None:                              # pragma: no cover
            proc.kill()
        proc.wait(timeout=RENDEZVOUS_TIMEOUT)


@pytest.mark.usefixtures("no_browsers")
def test_a_launch_whose_browser_never_opens_is_not_a_success(app_and_client, spawned):
    """The API case: the child takes the lease, passes the vault check, then
    fails to start a browser. It used to answer 200 and register the child.

    `engine` is named because `no_browsers` empties `PLAYWRIGHT_BROWSERS_PATH`,
    which is a Playwright-family lever and nothing else's. This used to rely on
    a create that named no engine producing `playwright` - which it did only
    because the route's own "use the saved default" fallback was unreachable
    (it ran *after* normalisation had already filled the converter's default
    in). With that fixed the profile gets whatever default the machine has, and
    a Camoufox one would sail past the fixture and open a real browser.
    """
    app, client = app_and_client
    pid = client.post("/api/profiles",
                      json={"name": "no-chromium", "engine": "playwright"}).json()["id"]

    answer = client.post(f"/api/profiles/{pid}/launch")
    try:
        assert answer.status_code == 500, answer.text
        # The child's own reason, not the route's placeholder - that is what
        # makes a startup failure actionable, and `"command failed"` is what
        # the route falls back to when the child said nothing at all.
        #
        # Which reason arrives is a property of the machine, not of the
        # behaviour under test: with `playwright` importable but its browser
        # binaries absent it is "no executable"; on a runner that never
        # installed the package it is "the package isn't installed". Both are
        # the child diagnosing itself, so pinning either one made this test
        # pass on two of three CI matrix legs and fail on the third.
        detail = answer.json()["detail"].lower()
        assert detail != "command failed", answer.text
        assert "playwright" in detail, answer.text
        assert app.state.running == {}, "a session that never opened stayed registered"
        assert spawned, "no child was started - the test proves nothing"
        assert spawned[0].poll() is not None, "the failed child was left running"
        rows = {p["id"]: p for p in client.get("/api/profiles").json()}
        assert rows[pid]["status"] == "stopped"
    finally:
        _reap(spawned)


@pytest.mark.usefixtures("no_browsers")
def test_a_scheduled_warm_up_that_never_opens_does_not_advance_the_occurrence(
        app_and_client, spawned):
    """And the scheduler case, through a second command: `numera warmup` opens
    its browser through `drivers.session()`, so the same failure arrives by a
    different route. `lastRun` moving here would silence the schedule for a
    full interval on the strength of a browser that never started.

    Explicit `engine` for the reason the test above gives: `no_browsers` only
    takes Chromium away from the Playwright family.
    """
    app, client = app_and_client
    pid = client.post("/api/profiles",
                      json={"name": "cold", "engine": "playwright"}).json()["id"]
    client.post("/api/schedules", json={"profileId": pid, "everyHours": 1,
                                        "minutes": 1, "preset": "general"})

    app.state.scheduler_tick()
    try:
        row = client.get("/api/schedules").json()[0]
        assert row["lastRun"] is None, "an occurrence with no browser was marked as run"
        assert row["lastOutcome"] == "failed"
        assert row["claimedBy"] is None, "the occurrence stayed claimed for the TTL"
        assert app.state.running == {}, "a session that never opened stayed registered"
        assert spawned, "no child was started - the test proves nothing"
        assert spawned[0].poll() is not None, "the failed child was left running"
    finally:
        _reap(spawned)
