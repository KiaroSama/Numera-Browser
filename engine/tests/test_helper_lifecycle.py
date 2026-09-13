"""A deadline ends the wait. Something still has to end the helper.

`0.31.0` gave the finite browser-backed helpers - cookies export, proxy test,
trust, tls, dns, align - a budget, spawned each into a tree it owns, and ends
that tree when the budget runs out. The wait is bounded and the browser is
killed, and that was the finding.

What it does not do is finish:

* on timeout it terminates and raises with `proc.stdout` and `proc.stderr`
  still open - the pipes leak one pair per wedged helper, and on Windows the
  handles keep the child's file objects alive;
* every exit that is neither "finished" nor "timed out" - a cancelled request,
  a `KeyboardInterrupt`, a bug in the timeout branch itself - runs no cleanup
  at all, because there is no `finally`;
* nothing anywhere knows the helper exists. The shutdown hook walks `running`,
  which holds *sessions*; a helper that is mid-probe when the server stops is
  not in it, so its browser outlives the process that started it.

The last one is the one that matters most, and it is why this file asserts
ownership rather than just cleanup: a browser nothing is tracking is exactly
the condition `proctree` exists to make impossible.

One thing is asserted in the negative as well: the finite-helper deadline must
not reach a persistent browsing session. A session is meant to outlive the
request that started it.
"""

import subprocess
import sys
import time

import pytest

from numera import proctree
from numera.server import routes_browser


@pytest.fixture
def wedged(tmp_path):
    """A helper that starts a grandchild and then never finishes.

    The grandchild is the stand-in for the browser: it is what survives killing
    the direct child, and the reason a plain `subprocess.run(timeout=...)` is
    not enough here.
    """
    script = tmp_path / "wedged.py"
    script.write_text(
        "import subprocess, sys, time\n"
        "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(600)'])\n"
        "time.sleep(600)\n",
        encoding="utf-8")
    return [sys.executable, str(script)]


@pytest.fixture
def watch(monkeypatch):
    """Capture the `(proc, tree)` `run_helper` spawns, so it can be inspected
    after the call has raised."""
    seen = {}
    real = proctree.spawn

    def watched(argv, **kwargs):
        proc, tree = real(argv, **kwargs)
        seen["proc"], seen["tree"] = proc, tree
        return proc, tree

    monkeypatch.setattr(routes_browser.proctree, "spawn", watched)
    return seen


def _pipes_open(proc) -> list:
    return [name for name in ("stdout", "stderr")
            if getattr(proc, name) is not None and not getattr(proc, name).closed]


# ---------------------------------------------------------------------------
# Cleanup on every exit, not just the happy one
# ---------------------------------------------------------------------------

def test_a_timed_out_helper_closes_its_pipes(wedged, watch):
    """The regression. `communicate` closes them; the timeout path never did."""
    with pytest.raises(routes_browser.HelperTimeout):
        routes_browser.run_helper(wedged, timeout=3.0)

    assert _pipes_open(watch["proc"]) == [], \
        f"a wedged helper left {_pipes_open(watch['proc'])} open"


def test_a_timed_out_helper_is_reaped(wedged, watch):
    """Killed is not the same as reaped. An unwaited child stays a zombie and
    still answers `os.kill(pid, 0)`, so nothing downstream can tell it is
    gone."""
    with pytest.raises(routes_browser.HelperTimeout):
        routes_browser.run_helper(wedged, timeout=3.0)

    assert watch["proc"].poll() is not None, "the helper was killed but never waited on"


def test_a_helper_that_finishes_closes_its_pipes(tmp_path, watch):
    """The control - this path already worked, and must keep working."""
    script = tmp_path / "quick.py"
    script.write_text("print('done')", encoding="utf-8")

    out = routes_browser.run_helper([sys.executable, str(script)], timeout=60.0)

    assert out.stdout.strip() == "done"
    assert _pipes_open(watch["proc"]) == []


def test_an_unexpected_exception_still_ends_the_helper(wedged, watch, monkeypatch):
    """Neither finished nor timed out: the exit with no handler at all.

    A cancelled request raises here. Without a `finally` the browser the helper
    started is simply left running.
    """
    def explode(*a, **k):
        raise MemoryError("something else went wrong")

    with pytest.raises(MemoryError):
        real_spawn = routes_browser.proctree.spawn

        def spawn_then_break(argv, **kwargs):
            proc, tree = real_spawn(argv, **kwargs)
            watch["proc"], watch["tree"] = proc, tree
            # `wait`, not `communicate`: the wait that bounds a helper is the
            # process's now, and draining its pipes is a separate bounded step
            # - a grandchild holding the writers must not extend the deadline.
            monkeypatch.setattr(proc, "wait", explode)
            return proc, tree

        monkeypatch.setattr(routes_browser.proctree, "spawn", spawn_then_break)
        routes_browser.run_helper(wedged, timeout=30.0)

    # The strong oracle first, and it is deterministic: the tree primitive is
    # what "provably gone" means, and the OS answers it from the job's live
    # process count.
    assert not proctree.alive_tree(watch["tree"]), \
        "an unexpected failure left the helper's process tree running"

    # `poll()` is the weaker signal and it lags. On Windows `GetExitCodeProcess`
    # can still report STILL_ACTIVE for a moment after `TerminateJobObject`
    # returns, because the process object transitions asynchronously - measured
    # here as about 0.1s, on one run in five. Sampling it with no tolerance made
    # this test fail three times in five for a helper that was already dead, so
    # it is read with a bounded wait rather than instantly. The assertion is
    # unchanged: the process must be gone.
    deadline = time.monotonic() + 5.0
    while watch["proc"].poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    assert watch["proc"].poll() is not None, \
        "an unexpected failure left the helper process running"
    assert _pipes_open(watch["proc"]) == [], \
        "an unexpected failure left the helper's pipes open"


# ---------------------------------------------------------------------------
# Ownership: something knows the helper exists while it runs
# ---------------------------------------------------------------------------

def test_a_running_helper_is_tracked_and_released(tmp_path):
    """A helper nobody is tracking cannot be cleaned up by anything but luck."""
    script = tmp_path / "brief.py"
    script.write_text("import time; time.sleep(0.4)", encoding="utf-8")

    seen = []
    real = routes_browser.run_helper

    def spy():
        seen.append(len(routes_browser.live_helpers()))

    before = len(routes_browser.live_helpers())
    import threading
    t = threading.Thread(target=lambda: real([sys.executable, str(script)], timeout=30.0))
    t.start()
    time.sleep(0.15)
    during = len(routes_browser.live_helpers())
    t.join(timeout=30)

    assert during > before, "a running helper was not registered anywhere"
    assert len(routes_browser.live_helpers()) == before, \
        "a finished helper stayed in the registry"


def test_a_timed_out_helper_leaves_the_registry(wedged):
    before = len(routes_browser.live_helpers())

    with pytest.raises(routes_browser.HelperTimeout):
        routes_browser.run_helper(wedged, timeout=3.0)

    assert len(routes_browser.live_helpers()) == before, \
        "a timed-out helper was left in the registry forever"


def test_shutdown_ends_a_helper_that_is_still_running(wedged, watch):
    """The whole reason ownership is tracked: the server stopping must take the
    browser with it, exactly as it does for a session."""
    import threading

    def run_and_swallow():
        try:
            routes_browser.run_helper(wedged, timeout=120.0)
        except Exception:           # noqa: BLE001 - shutdown kills it; that is the point
            pass

    t = threading.Thread(target=run_and_swallow)
    t.daemon = True
    t.start()
    time.sleep(1.0)
    assert routes_browser.live_helpers(), "the fixture helper never registered"

    stuck = routes_browser.end_helpers()

    assert stuck == [], f"shutdown could not end {stuck}"
    assert watch["proc"].poll() is not None, "the helper survived shutdown"
    t.join(timeout=30)


def test_shutdown_is_wired_into_the_app(tmp_path):
    """A cleanup function nothing calls is not cleanup."""
    pytest.importorskip("fastapi")
    from numera.server import create_app

    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    app.state.run_shutdown()

    # Reaching here without an exception is the assertion: `end_helpers` is
    # registered and safe to call with nothing running.
    assert routes_browser.live_helpers() == []


# ---------------------------------------------------------------------------
# The deadline belongs to finite helpers only
# ---------------------------------------------------------------------------

def test_the_finite_deadline_does_not_reach_a_persistent_session():
    """A browsing session is meant to outlive the request that started it, so
    it must not go through the helper path at all."""
    import ast
    import inspect

    tree = ast.parse(inspect.getsource(routes_browser))
    calls = {}

    class Visitor(ast.NodeVisitor):
        def __init__(self):
            self.fn = None

        def visit_FunctionDef(self, node):
            outer, self.fn = self.fn, node.name
            self.generic_visit(node)
            self.fn = outer

        visit_AsyncFunctionDef = visit_FunctionDef

        def visit_Call(self, node):
            name = getattr(node.func, "id", None) or getattr(node.func, "attr", None)
            calls.setdefault(self.fn, set()).add(name)
            self.generic_visit(node)

    Visitor().visit(tree)

    # The AST, not the source text: a comment or docstring that *mentions*
    # `run_helper` - and several here do, explaining that a session must not use
    # it - would make a substring search fail for the opposite of the reason
    # this test exists.
    session_starters = [fn for fn, seen in calls.items() if "spawn_owner" in seen]
    assert session_starters, "the launch path moved; this guard needs updating"

    for fn in session_starters:
        assert "run_helper" not in calls[fn], \
            f"{fn} starts a persistent session through the finite-helper deadline"


def test_a_second_settle_arriving_mid_settle_does_not_strand_the_helper():
    """The overlap `end_all` and `run_helper` have by construction.

    Both callers are ordinary: `run_helper` settles the helper it started and a
    shutdown settles every live one, so the two meet whenever a shutdown lands
    while a helper is finishing - the common case, not a rare one.

    A state check alone did not close it. The first caller sets `STOPPING` and
    releases the manager lock *before* the slow part, so a second arriving
    inside that window saw "not settled yet" and ran the whole of `settle`
    again. One of them then reached `read_units` after the other had succeeded
    and cleared `units_path`, read the absent report as lost evidence, and
    reported **stranded** a helper whose process had demonstrably exited.

    The window is microseconds wide with a trivial child - a first version of
    this test synchronised only the two starts and passed against the unfixed
    code, proving nothing. So the window is held open here: the first settle
    blocks inside `end_session` until the second has reached the point that
    used to misread the cleared path. On the fixed code the second never gets
    there, and the rendezvous is bounded so that is not a hang.
    """
    import threading

    from numera.server import helpers as helpers_mod

    manager = helpers_mod.HelperManager()
    helper = manager.admit((sys.executable, "-c", "pass"))
    manager.child_env(helper, {})                 # gives it a units report path

    proc, tree = proctree.spawn([sys.executable, "-c", "pass"],
                                stdin=subprocess.DEVNULL,
                                stdout=subprocess.DEVNULL,
                                stderr=subprocess.DEVNULL)
    manager.attach(helper, proc, tree)
    proc.wait(timeout=30)            # settling is about cleanup, not about exit

    inside = threading.Event()       # the first settle is in the slow part
    proceed = threading.Event()      # ... and may now leave it
    real_end, real_read = helpers_mod.proctree.end_session, helpers_mod.read_units
    calls = []

    def held_end(*a, **kw):
        calls.append(1)
        if len(calls) == 1:
            inside.set()
            # Bounded: on the fixed code the second settle blocks on the
            # helper's lock and never signals, so this is a deadline, not a
            # wait for something that is coming.
            proceed.wait(timeout=2.0)
        return real_end(*a, **kw)

    def watched_read(path):
        if threading.current_thread().name == "second":
            proceed.set()            # only the broken interleave reaches here
        return real_read(path)

    helpers_mod.proctree.end_session = held_end
    helpers_mod.read_units = watched_read
    verdicts = {}
    try:
        first = threading.Thread(name="first",
                                 target=lambda: verdicts.__setitem__(
                                     "first", manager.settle(helper)))
        first.start()
        assert inside.wait(timeout=30), "the first settle never reached cleanup"

        second = threading.Thread(name="second",
                                  target=lambda: verdicts.__setitem__(
                                      "second", manager.settle(helper)))
        second.start()
        for t in (first, second):
            t.join(timeout=30)
            assert not t.is_alive(), f"the {t.name} settle never returned"
    finally:
        helpers_mod.proctree.end_session = real_end
        helpers_mod.read_units = real_read

    assert verdicts == {"first": True, "second": True}, (
        f"concurrent settles disagreed: {verdicts} - state={helper.state!r} "
        f"straggler={helper.straggler!r} units_problem={helper.units_problem!r}")
    assert helper.state == helpers_mod.SETTLED, helper.straggler
