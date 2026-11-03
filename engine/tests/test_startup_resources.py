"""Nothing survives a startup that failed, and nothing is released early.

0.22 moved readiness past real browser creation, which was right and opened
three windows behind it:

* **Between the context and READY.** `launch_persistent_context()` succeeds,
  and then the init script, the client-hint overrides or the first page can
  raise. The parent is correctly told the session failed - and a live Chromium,
  a running Playwright driver and a relay port were left behind for it. Even
  the launch-failure path only closed the relay; it never stopped the driver.

* **After the kill.** `terminate()` escalated to `kill()` and returned without
  waiting, so `stop_session` read `poll()` on a child that had not been reaped:
  the exit code recorded was `None`, the registry entry was dropped while the
  child still held the session lease, and `/stop` reported success on a profile
  that was still busy.

* **After READY, before the occurrence is durable.** The scheduler waits for a
  real browser, then writes `lastRun`. If that write fails, the old code
  released the claim and left the warm-up running - so the row was immediately
  due again and the next tick started a second browser beside the first.

The browser tests use a fake driver rather than a real Chromium: the defect is
in Numera's resource handling, and injecting a failure at an exact point of it
is the only way to reach the window at all.
"""

import importlib.machinery
import subprocess
import sys
import types

import pytest

from _child import spawning_tree

from numera import drivers, generate
from numera.handshake import end_child
from numera.models import Profile
from numera.repo import repo
from numera.store import ProfileStore


# --- fakes: a browser stack that records what was released -----------------

class FakePage:
    def wait_for_event(self, *a, **kw):
        return None


class FakeContext:
    def __init__(self):
        self.closed = False

    def on(self, event, fn):
        pass

    def add_init_script(self, script):
        pass

    @property
    def pages(self):
        return []

    def new_page(self):
        return FakePage()

    def close(self):
        self.closed = True


class FakeChromium:
    def __init__(self, context):
        self._context = context

    def launch_persistent_context(self, **kwargs):
        return self._context


class FakeDriver:
    def __init__(self, context):
        self.chromium = FakeChromium(context)
        self.stopped = False

    def stop(self):
        self.stopped = True


class FakeRelay:
    def __init__(self):
        self.closed = False

    def close(self):
        self.closed = True


def fake_playwright(driver):
    class Starter:
        def start(self):
            return driver
    return lambda: Starter()


def a_profile(tmp_path, name="p") -> Profile:
    store = ProfileStore(tmp_path)
    return repo(store).create(Profile(name=name, fingerprint=generate(seed=1)))


def fail_between_context_and_ready(monkeypatch, opened: list):
    """Break the first step that runs after the context exists.

    `build_init_script` is called immediately after `launch_persistent_context`
    returns and the relay is tied to it - the exact window, and one no
    missing-executable test can reach, because that fails before a context
    exists at all.
    """
    monkeypatch.setattr(drivers, "_opened", lambda: opened.append(True))
    monkeypatch.setattr(drivers, "build_init_script",
                        lambda fp: (_ for _ in ()).throw(
                            RuntimeError("the init script would not compile")))


# --- the window between a live context and READY ---------------------------

def test_playwright_releases_everything_when_startup_fails_after_the_context(
        tmp_path, monkeypatch):
    prof = a_profile(tmp_path)
    store = ProfileStore(tmp_path)
    context, relay = FakeContext(), FakeRelay()
    driver = FakeDriver(context)
    opened: list = []

    monkeypatch.setattr(drivers, "_browser_proxy",
                        lambda profile: ({"server": "http://127.0.0.1:1"}, relay))
    fail_between_context_and_ready(monkeypatch, opened)

    with pytest.raises(RuntimeError, match="init script"):
        drivers._launch_playwright_like(fake_playwright(driver), prof, store,
                                        headless=True, keep_open=True)

    assert not opened, "READY was announced for a session that never opened"
    assert context.closed, "a live browser was left behind"
    assert driver.stopped, "the Playwright driver was left running"
    assert relay.closed, "the relay port was stranded"


def test_playwright_stops_the_driver_when_the_context_itself_fails(
        tmp_path, monkeypatch):
    """The pre-existing path closed the relay and returned - leaving the driver
    it had just started running."""
    prof = a_profile(tmp_path)
    store = ProfileStore(tmp_path)
    relay = FakeRelay()
    driver = FakeDriver(FakeContext())
    monkeypatch.setattr(driver.chromium, "launch_persistent_context",
                        lambda **kw: (_ for _ in ()).throw(
                            RuntimeError("no browser executable")))
    monkeypatch.setattr(drivers, "_browser_proxy",
                        lambda profile: ({"server": "http://127.0.0.1:1"}, relay))

    with pytest.raises(RuntimeError, match="no browser executable"):
        drivers._launch_playwright_like(fake_playwright(driver), prof, store,
                                        headless=True, keep_open=True)

    assert driver.stopped, "the driver survived a failed launch"
    assert relay.closed


def test_cloak_releases_everything_when_startup_fails_after_the_context(
        tmp_path, monkeypatch):
    prof = a_profile(tmp_path)
    store = ProfileStore(tmp_path)
    context, relay = FakeContext(), FakeRelay()
    opened: list = []

    # A real module object with a spec: `drivers.get` asks `find_spec` whether
    # the backend is installed, and that reads `__spec__` off what is in
    # `sys.modules`.
    fake = types.ModuleType("cloakbrowser")
    fake.__spec__ = importlib.machinery.ModuleSpec("cloakbrowser", None)
    fake.launch_persistent_context = lambda **kwargs: context
    monkeypatch.setitem(sys.modules, "cloakbrowser", fake)
    monkeypatch.setattr('numera.cloak_acquisition.launch_options', lambda: {})
    monkeypatch.setattr(drivers, "_browser_proxy",
                        lambda profile: ({"server": "http://127.0.0.1:1"}, relay))
    monkeypatch.setattr(drivers, "_opened", lambda: opened.append(True))
    # Cloak has no init script; its first step after the context is the page.
    monkeypatch.setattr(FakeContext, "new_page",
                        lambda self: (_ for _ in ()).throw(
                            RuntimeError("the first page would not open")))

    with pytest.raises(RuntimeError, match="first page"):
        drivers.get("cloak")(prof, store, headless=True, keep_open=True)

    assert not opened, "READY was announced for a session that never opened"
    assert context.closed, "a live browser was left behind"
    assert relay.closed, "the relay port was stranded"


def test_the_lease_is_free_again_after_a_failed_open(tmp_path, monkeypatch):
    """The point of releasing everything: the next launch must not be refused
    for a browser that does not exist."""
    from numera.paths import session_lease

    prof = a_profile(tmp_path)
    store = ProfileStore(tmp_path)
    opened: list = []
    monkeypatch.setattr(drivers, "_opened", lambda: opened.append(True))
    monkeypatch.setitem(
        drivers._DRIVERS, "explodes",
        {"fn": lambda *a, **k: (_ for _ in ()).throw(RuntimeError("boom")),
         "requires": None})

    with pytest.raises(RuntimeError, match="boom"):
        with drivers.session(prof, store, engine="explodes"):
            pytest.fail("a failed backend yielded a session")

    assert not opened
    with session_lease(store.root, prof.id):
        pass                       # acquired at once, or this raises SessionBusy


# --- a signal is a request; wait() is the answer ---------------------------

def sleeper() -> subprocess.Popen:
    """A child that stays up until it is stopped, without a timed sleep."""
    return subprocess.Popen([sys.executable, "-c",
                             "import threading; threading.Event().wait()"])


def test_end_child_returns_a_real_code_and_only_once_the_child_is_gone():
    proc = sleeper()
    try:
        code = end_child(proc)
        assert code is not None, "a stopped child reported no exit code"
        assert proc.poll() is not None, "end_child returned before the child died"
    finally:
        if proc.poll() is None:                              # pragma: no cover
            proc.kill()


class Lingering:
    """A child that reports nothing until it is waited on.

    That is the state a real process is in between the signal and the reap, and
    it is precisely the state `poll()` cannot see through. A real child is the
    wrong instrument for this: on Windows `kill()` is `TerminateProcess` and
    the OS usually reaps fast enough that a bare `poll()` happens to return a
    code - so the missing wait passes by luck, and a test that depends on that
    luck proves nothing about the bug.
    """

    def __init__(self):
        self.returncode = None
        self.terminated = self.killed = False

    def poll(self):
        return self.returncode

    def terminate(self):
        self.terminated = True          # ignored, like a child with a handler

    def kill(self):
        self.killed = True              # signalled, but not yet reaped

    def wait(self, timeout=None):
        if self.returncode is not None:
            return self.returncode      # it went on the polite ask
        if not self.killed:
            raise subprocess.TimeoutExpired("child", timeout)
        self.returncode = -9            # reaping is what produces the code
        return self.returncode


def test_end_child_waits_after_the_kill_escalation():
    """The whole of the fix: `kill()` is a request, and only `wait()` answers.

    The old version escalated and returned `proc.poll()` immediately, which is
    `None` for a child that has not been reaped - and that `None` was written
    into a schedule outcome and used to drop a registry entry whose child still
    held the session lease.
    """
    proc = Lingering()
    code = end_child(proc)

    assert proc.terminated and proc.killed, "the escalation did not happen"
    assert code == -9, f"the exit code was read before the reap: {code!r}"
    assert proc.poll() is not None, "end_child returned before the child was gone"


def test_end_child_escalates_only_when_the_polite_ask_is_ignored():
    """A child that goes on `terminate()` must not be killed as well."""
    proc = Lingering()
    proc.terminate = lambda: setattr(proc, "returncode", 0)

    assert end_child(proc) == 0
    assert not proc.killed, "a child that stopped politely was killed anyway"


def test_end_child_on_a_dead_child_reports_its_real_code():
    proc = subprocess.Popen([sys.executable, "-c", "raise SystemExit(7)"])
    proc.wait(timeout=30)
    assert end_child(proc) == 7


# --- the occurrence and the browser it opened stay together ----------------

@pytest.fixture()
def app_and_client(tmp_path):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    with TestClient(app) as client:
        yield app, client


def schedule_for(client, name="warmed") -> str:
    pid = client.post("/api/profiles", json={"name": name}).json()["id"]
    client.post("/api/schedules", json={"profileId": pid, "everyHours": 1,
                                        "minutes": 1, "preset": "general"})
    return pid


def test_a_bookkeeping_failure_after_ready_stops_the_browser_it_opened(
        app_and_client, monkeypatch):
    """The forbidden state is a live scheduled warm-up against a row that is
    due and unclaimed: the next tick starts a second one beside it."""
    from numera import server as srv
    from numera.scheduler import ScheduleStore

    app, client = app_and_client
    schedule_for(client)
    children: list = []
    monkeypatch.setattr(srv.proctree, "spawn", spawning_tree(children))
    monkeypatch.setattr(ScheduleStore, "mark_run",
                        lambda self, sid, when: (_ for _ in ()).throw(
                            OSError("no space left on device")))

    app.state.scheduler_tick()

    assert children, "the tick never got as far as opening a browser"
    row = client.get("/api/schedules").json()[0]
    assert row["lastRun"] is None and row["lastOutcome"] == "failed"
    live = children[0].poll() is None
    assert not (live and row.get("claimedBy") is None), \
        "a live warm-up was left against an unclaimed, immediately-due row"
    assert children[0].terminated, "the browser it had opened was left running"


def test_a_confirmed_start_still_records_normally(app_and_client, monkeypatch):
    """The guard must not turn a working occurrence into a failure."""
    from numera import server as srv

    app, client = app_and_client
    schedule_for(client, "healthy")
    children: list = []
    monkeypatch.setattr(srv.proctree, "spawn", spawning_tree(children))

    app.state.scheduler_tick()

    row = client.get("/api/schedules").json()[0]
    assert row["lastOutcome"] == "started" and row["lastRun"] is not None
    assert not children[0].terminated, "a healthy warm-up was stopped"


def test_the_relay_is_closed_when_the_driver_will_not_even_start(tmp_path, monkeypatch):
    """The guard used to begin *after* `sync_playwright().start()`.

    An authenticated proxy opens a local listener before that line, so a driver
    that would not start leaked the port with nothing left holding a reference
    to close it - and the next launch of the same profile opened another.
    """
    prof = a_profile(tmp_path)
    store = ProfileStore(tmp_path)
    relay = FakeRelay()
    monkeypatch.setattr(drivers, "_browser_proxy",
                        lambda profile: ({"server": "http://127.0.0.1:1"}, relay))

    def will_not_start():
        raise RuntimeError("the driver would not start")

    class Starter:
        start = staticmethod(will_not_start)

    with pytest.raises(RuntimeError, match="would not start"):
        drivers._launch_playwright_like(lambda: Starter(), prof, store,
                                        headless=True, keep_open=True)
    assert relay.closed, "the relay port was stranded"


def test_a_cleanup_that_fails_is_surfaced_not_swallowed(tmp_path, monkeypatch):
    """"No browser survives a failed startup" is a promise. Discarding the
    evidence that `close()` failed turns it into a guess, and the caller was
    told only the original error while a browser might still be running."""
    prof = a_profile(tmp_path)
    store = ProfileStore(tmp_path)
    context, relay = FakeContext(), FakeRelay()
    driver = FakeDriver(context)
    monkeypatch.setattr(context, "close",
                        lambda: (_ for _ in ()).throw(RuntimeError("close refused")))
    monkeypatch.setattr(drivers, "_browser_proxy",
                        lambda profile: ({"server": "http://127.0.0.1:1"}, relay))
    fail_between_context_and_ready(monkeypatch, [])

    with pytest.raises(drivers.StartupRecoveryFailed) as caught:
        drivers._launch_playwright_like(fake_playwright(driver), prof, store,
                                        headless=True, keep_open=True)

    assert "init script" in str(caught.value.cause), "the original failure was lost"
    assert any("browser" in what for what, _err in caught.value.failures)
    assert driver.stopped and relay.closed, \
        "one failing step stopped the others from being attempted"
