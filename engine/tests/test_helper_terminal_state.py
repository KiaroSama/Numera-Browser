"""A helper is finished when everything it owns is provably empty.

`0.32.0` registered helpers, bounded the wait and closed the pipes, and
refused to release a tree it could not prove empty. What it still got wrong is
what "everything it owns" and "finished" mean:

* only the timeout path raised. A helper that exited 0 while its cleanup failed
  returned a `CompletedProcess`, so a caller could not tell a probe that worked
  from one that left a browser running;
* the spawn happened before the shutdown check, so a refused request could
  already have created a child - and one that was never registered is in no
  diagnostic;
* only the outer tree was tracked. `proctree._spawn_leader` gives a browser
  driver its **own** session, which on POSIX is not in the helper's process
  group at all, so an empty outer tree said nothing about it;
* the registry and the shutdown flag were module globals, so one app's sweep
  reached another app's helpers.

Real children here, and deliberately: the whole finding is about what survives
a process boundary, and no in-process fake can be wrong in the way a detached
grandchild is. They are kept as cheap as that allows - one module-scoped script
file instead of one per test, short deadlines, and no browser backend, which
these assertions do not need.

**The `outlives` fixture used to be the bug rather than the control.** Its
grandchild inherited the helper's `PIPE` writers, so `communicate()` waited for
EOF on a pipe the grandchild was holding open and timed out even though the
parent had exited - in a test whose whole point was a clean parent exit. The
grandchild's streams now go to `DEVNULL`, and the pipe-holding topology gets
its own test, where a bounded timeout is the expected answer rather than a
confusing one.
"""

import subprocess
import sys
import time
from pathlib import Path

import pytest

from numera import proctree
from numera.server import helpers

ENGINE = str(Path(__file__).resolve().parents[1])


@pytest.fixture(scope="module")
def scripts(tmp_path_factory):
    """Written once for the module. Each test spawns, none re-creates."""
    d = tmp_path_factory.mktemp("helper-scripts")
    wedged = d / "wedged.py"
    wedged.write_text(
        "import subprocess, sys, time\n"
        "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)'],\n"
        "                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)\n"
        # The sleep is the subject, not a wait: this script exists to be a
        # process that never finishes on its own, so the test's 3s helper
        # deadline is what bounds the run. Two minutes rather than ten so that
        # a process which somehow escapes the sweep costs less.
        "time.sleep(120)\n", encoding="utf-8")
    outlives = d / "outlives.py"
    # Exits at once, leaving a grandchild behind - the browser that outlives the
    # probe that opened it. Its streams go to DEVNULL so the grandchild does not
    # hold this helper's PIPE writers open: that is a different failure, and it
    # has its own test below.
    outlives.write_text(
        "import subprocess, sys\n"
        "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)'],\n"
        "                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)\n"
        "print('done', flush=True)\n", encoding="utf-8")
    holds = d / "holds_pipes.py"
    # The other topology: the grandchild is handed this process's own stdout
    # and stderr, so the writers stay open after the parent exits and a reader
    # waiting for EOF has nothing to reach. `close_fds=False` is what
    # `_spawn_leader` uses on Windows for the job handle, and it brings the
    # inherited std handles with it - so this is the production shape, not an
    # invented one.
    holds.write_text(
        "import subprocess, sys\n"
        "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)'],\n"
        "                 stdout=sys.stdout, stderr=sys.stderr, close_fds=False)\n"
        "print('parent done', flush=True)\n", encoding="utf-8")
    sessioned = d / "sessioned.py"
    # A driver in its own session, made the way a real one is: `_spawn_leader`
    # is what `driver_capture` calls for every browser driver, and it records
    # the new tree in the units file this helper was given.
    sessioned.write_text(
        "import subprocess, sys\n"
        f"sys.path.insert(0, {ENGINE!r})\n"
        "from numera import proctree\n"
        "proctree._spawn_leader(subprocess.Popen,\n"
        "                       [sys.executable, '-c', 'import time; time.sleep(120)'],\n"
        "                       (), {'stdout': subprocess.DEVNULL,\n"
        "                            'stderr': subprocess.DEVNULL})\n"
        "print('spawned', flush=True)\n", encoding="utf-8")
    quick = d / "quick.py"
    quick.write_text("print('ok')", encoding="utf-8")
    return {"wedged": wedged, "outlives": outlives, "holds": holds,
            "sessioned": sessioned, "quick": quick}


@pytest.fixture
def manager():
    """A fresh manager per test, and nothing of this test's left running."""
    made = helpers.reopen()
    yield made
    for h in made.live():
        made.settle(h, deadline=2.0)
        made._release(h)


def run(script, timeout=30.0, manager=None):
    return helpers.run_helper([sys.executable, str(script)], timeout,
                              manager=manager)


# ---------------------------------------------------------------------------
# 1. A clean return is not proof that the tree ended
# ---------------------------------------------------------------------------

def test_a_helper_that_exits_leaving_a_child_still_has_its_tree_ended(
        scripts, manager):
    """The regression. The command returned 0; the browser it started did not."""
    seen = {}
    real = proctree.spawn

    def watched(argv, **kw):
        proc, tree = real(argv, **kw)
        seen["tree"] = tree
        return proc, tree

    proctree.spawn, kept = watched, proctree.spawn
    try:
        out = run(scripts["outlives"], manager=manager)
    finally:
        proctree.spawn = kept

    assert out.returncode == 0 and "done" in out.stdout
    assert proctree.prove_empty(seen["tree"], deadline=5.0), \
        "the helper returned cleanly and left its grandchild running"
    assert manager.live() == [], "a settled helper stayed in the registry"


def test_a_plain_helper_leaves_nothing_behind(scripts, manager):
    """The control, and the common path."""
    out = run(scripts["quick"], manager=manager)

    assert out.returncode == 0
    assert manager.live() == []
    assert manager.stragglers() == []


def test_a_clean_exit_with_failed_cleanup_is_not_a_result(
        scripts, manager, monkeypatch):
    """The first of the four remaining defects.

    Only the timeout path used to raise, so a probe that answered 0 while its
    browser refused to die handed that answer back as ordinary work - and the
    route turned it into a 200. An unproved cleanup is a failure whatever the
    exit code was.
    """
    monkeypatch.setattr(proctree, "end_tree", lambda tree, **kw: False)

    with pytest.raises(helpers.HelperUnsettled) as caught:
        run(scripts["quick"], manager=manager)

    assert "could not be fully stopped" in str(caught.value)
    left = manager.stragglers()
    assert len(left) == 1 and left[0].state == helpers.STRANDED


# ---------------------------------------------------------------------------
# 2. Everything it owns, not only the outer tree
# ---------------------------------------------------------------------------

def test_a_driver_in_its_own_session_is_ended_too(scripts, manager):
    """The third defect, with a real separately sessioned descendant.

    `_spawn_leader` puts a browser driver in its own job or session. On POSIX
    that takes it out of the helper's process group entirely, so proving the
    outer tree empty proved nothing about it, and no descendant walk can find
    it either - it is not a descendant any more. The record it writes is what
    makes it reachable.
    """
    seen = {}
    real = helpers.read_units

    def watched(path):
        found = real(path)
        seen.setdefault("units", found)
        return found

    monkey = helpers.read_units
    helpers.read_units = watched
    try:
        out = run(scripts["sessioned"], manager=manager)
    finally:
        helpers.read_units = monkey

    assert out.returncode == 0 and "spawned" in out.stdout
    assert seen["units"], "the driver's own session was never reported"
    for tree in seen["units"]:
        assert proctree.prove_empty(tree, deadline=5.0), \
            f"a separately sessioned driver survived helper cleanup: {tree}"
    assert manager.live() == []


def test_an_unreported_session_does_not_block_an_ordinary_helper(manager):
    """The control for the reporting channel: no units file, no units."""
    assert helpers.read_units(None) == []
    assert helpers.read_units(Path("does-not-exist.jsonl")) == []


# ---------------------------------------------------------------------------
# 3. An unproved end keeps its record
# ---------------------------------------------------------------------------

def test_a_tree_that_cannot_be_proved_empty_stays_in_the_registry(
        scripts, manager, monkeypatch):
    """The `finally` used to release the record regardless, so the one thing
    tracking a browser that would not die forgot about it."""
    monkeypatch.setattr(proctree, "end_tree", lambda tree, **kw: False)

    with pytest.raises(helpers.HelperTimeout) as caught:
        run(scripts["wedged"], timeout=3.0, manager=manager)

    left = manager.stragglers()
    assert len(left) == 1, "an unprovable helper was dropped"
    assert left[0].straggler, "the record does not say what was left"
    assert "straggler" in str(caught.value).lower(), \
        f"the caller is not told cleanup failed: {caught.value}"


def test_end_all_reports_what_it_could_not_end(scripts, manager, monkeypatch):
    """And keeps it: a failed cleanup is not completed work."""
    monkeypatch.setattr(proctree, "end_tree", lambda tree, **kw: False)
    with pytest.raises(helpers.HelperTimeout):
        run(scripts["wedged"], timeout=3.0, manager=manager)

    left = manager.end_all(deadline=2.0)

    assert len(left) == 1
    assert manager.live(), "a straggler was cleared by the shutdown sweep"


def test_a_successful_shutdown_reports_nothing(scripts, manager):
    run(scripts["quick"], manager=manager)

    assert manager.end_all(deadline=5.0) == []


# ---------------------------------------------------------------------------
# 4. Admission, and whose shutdown it is
# ---------------------------------------------------------------------------

def test_no_helper_may_start_once_the_sweep_has_begun(scripts, manager):
    """A request arriving mid-shutdown could otherwise open a browser behind
    the snapshot `end_all` just took - the exact process nothing tracks."""
    manager.end_all(deadline=2.0)

    with pytest.raises(helpers.HelperShuttingDown):
        run(scripts["quick"], manager=manager)

    assert manager.live() == [], "the refused helper was left registered"


def test_a_refused_helper_never_gets_as_far_as_a_process(scripts, manager):
    """The second defect. The check used to run *after* the spawn, so a
    rejected request could already have started a browser - and the record that
    would have named it was never created."""
    manager.end_all(deadline=2.0)
    started = []

    real = proctree.spawn
    proctree.spawn = lambda argv, **kw: started.append(argv) or real(argv, **kw)
    try:
        with pytest.raises(helpers.HelperShuttingDown):
            run(scripts["quick"], manager=manager)
    finally:
        proctree.spawn = real

    assert started == [], "a refused request spawned a child anyway"


def test_a_new_app_is_not_closed_by_another_app_s_shutdown(scripts, manager):
    """The fourth defect, and the regression that found it.

    A process builds more than one app - every test that constructs one does,
    and so does a server restarting in place. With a module-global flag, the
    first shutdown anywhere refused every probe afterwards in a server that was
    still running, and four unrelated suites began failing. A manager per app
    has no flag to share: a swept one stays closed because its app is over, and
    the next app gets its own.
    """
    manager.end_all(deadline=2.0)
    assert manager.closed is True

    restarted = helpers.HelperManager()

    assert restarted.closed is False
    assert run(scripts["quick"], manager=restarted).returncode == 0
    assert restarted.end_all(deadline=5.0) == []


def test_one_app_s_sweep_does_not_reach_another_app_s_helpers(
        scripts, manager, monkeypatch):
    """Two servers in one process own their own work and nobody else's."""
    other = helpers.HelperManager()
    monkeypatch.setattr(proctree, "end_tree", lambda tree, **kw: False)
    with pytest.raises(helpers.HelperTimeout):
        run(scripts["wedged"], timeout=3.0, manager=other)
    monkeypatch.undo()

    assert manager.end_all(deadline=2.0) == [], \
        "one app's sweep settled another app's helper"
    assert len(other.live()) == 1

    assert other.end_all(deadline=5.0) == []


# ---------------------------------------------------------------------------
# 5. The timeout contract the routes depend on
# ---------------------------------------------------------------------------

def test_a_wedged_helper_times_out_and_is_ended(scripts, manager):
    """Two properties of one ending, from one wedged helper.

    They used to be two tests spawning two real process trees and paying the
    deadline twice for the same scenario; the scenario is what is expensive
    here, not the assertions.
    """
    seen = {}
    real = proctree.spawn

    def watched(argv, **kw):
        proc, tree = real(argv, **kw)
        seen["proc"] = proc
        return proc, tree

    proctree.spawn, kept = watched, proctree.spawn
    try:
        with pytest.raises(helpers.HelperTimeout):
            run(scripts["wedged"], timeout=3.0, manager=manager)
    finally:
        proctree.spawn = kept

    assert manager.live() == [], \
        "a helper that was successfully ended stayed registered"
    proc = seen["proc"]
    assert [n for n in ("stdout", "stderr")
            if getattr(proc, n) is not None and not getattr(proc, n).closed] == [], \
        "one pair of file objects per wedged helper, otherwise"


def test_a_grandchild_holding_the_pipes_does_not_hold_the_result(
        scripts, manager):
    """The topology the old `outlives` fixture had by accident.

    `communicate` waits for EOF, and a grandchild that inherited the writers
    keeps the pipe open long after the parent exits - so the deadline of a
    finite probe was really the browser's lifetime. The wait that matters is
    the process's; draining is bounded separately, and `close_pipes` ends it.
    """
    started = time.monotonic()

    out = run(scripts["holds"], timeout=20.0, manager=manager)
    took = time.monotonic() - started

    assert out.returncode == 0
    assert took < 15.0, (
        f"the helper exited immediately and the call still took {took:.1f}s - "
        f"the reader is waiting on a pipe the grandchild holds")
    assert manager.live() == [], "the pipe-holding tree was not ended"
    assert manager.stragglers() == []


def test_a_failing_helper_still_returns_its_output(tmp_path, manager):
    """`allow_fail` callers read stdout from a non-zero exit."""
    script = tmp_path / "unhealthy.py"
    script.write_text('import sys; print(\'{"ok": false}\'); sys.exit(3)',
                      encoding="utf-8")

    out = run(script, manager=manager)

    assert out.returncode == 3 and '"ok": false' in out.stdout


def test_the_shutdown_dispatcher_surfaces_the_list(tmp_path, capsys):
    """`run_shutdown` discarded every hook's return value, so 'could not clean
    up' and 'cleaned up' arrived identically."""
    pytest.importorskip("fastapi")
    from numera.server import create_app

    app = create_app(data_dir=str(tmp_path), start_scheduler=False)

    class Left:
        def describe(self):
            return "pid 4242 (job:x)"

    app.state.add_shutdown(lambda: [Left()])
    app.state.run_shutdown()

    assert "4242" in capsys.readouterr().err, \
        "a shutdown that left a process behind said nothing"


def test_an_app_owns_its_own_manager(tmp_path):
    """The wiring: the app holds the registry, and its shutdown sweeps it."""
    pytest.importorskip("fastapi")
    from numera.server import create_app

    app = create_app(data_dir=str(tmp_path), start_scheduler=False)

    assert isinstance(app.state.helpers, helpers.HelperManager)
    assert app.state.helpers is not helpers.default_manager()
    app.state.run_shutdown()
    assert app.state.helpers.closed is True
