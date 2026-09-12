"""A helper is finished when its tree is provably empty, not when it returns.

`0.32.0` registered helpers, bounded the wait and closed the pipes. It still
took the wrong things as an ending:

* the `finally` released the record whatever happened, so a tree that refused
  to die was dropped by the only thing tracking it;
* a helper that exited normally was accepted without asking whether the browser
  it started had exited too - `communicate()` returning says nothing about a
  grandchild, and the outer group is not automatically the driver's own
  ownership unit;
* `end_helpers` returned its stragglers and the shutdown dispatcher discarded
  the list, so a cleanup that failed and one that worked reached the server
  identically.

Real children here, and deliberately: the whole finding is about what survives
a process boundary, and no in-process fake can be wrong in the way a detached
grandchild is. They are kept as cheap as that allows - one module-scoped script
file instead of one per test, short deadlines, and no browser backend, which
these assertions do not need.
"""

import subprocess
import sys

import pytest

from numera import proctree
from numera.server import helpers


@pytest.fixture(scope="module")
def scripts(tmp_path_factory):
    """Written once for the module. Each test spawns, none re-creates."""
    d = tmp_path_factory.mktemp("helper-scripts")
    wedged = d / "wedged.py"
    wedged.write_text(
        "import subprocess, sys, time\n"
        "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(600)'])\n"
        "time.sleep(600)\n", encoding="utf-8")
    outlives = d / "outlives.py"
    # Exits at once, leaving a grandchild behind - the browser that outlives the
    # probe that opened it.
    outlives.write_text(
        "import subprocess, sys\n"
        "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(600)'])\n"
        "print('done', flush=True)\n", encoding="utf-8")
    quick = d / "quick.py"
    quick.write_text("print('ok')", encoding="utf-8")
    return {"wedged": wedged, "outlives": outlives, "quick": quick}


@pytest.fixture(autouse=True)
def _open_for_business():
    """`end_helpers` latches the door shut; every test starts with it open."""
    helpers.reopen()
    yield
    helpers.reopen()
    for h in helpers.live_helpers():
        helpers.settle(h, deadline=2.0)
        helpers._release(h)


def run(script, timeout=30.0):
    return helpers.run_helper([sys.executable, str(script)], timeout)


# ---------------------------------------------------------------------------
# 1. A clean return is not proof that the tree ended
# ---------------------------------------------------------------------------

def test_a_helper_that_exits_leaving_a_child_still_has_its_tree_ended(scripts):
    """The regression. The command returned 0; the browser it started did not."""
    seen = {}
    real = proctree.spawn

    def watched(argv, **kw):
        proc, tree = real(argv, **kw)
        seen["tree"] = tree
        return proc, tree

    proctree.spawn, kept = watched, proctree.spawn
    try:
        out = run(scripts["outlives"])
    finally:
        proctree.spawn = kept

    assert out.returncode == 0 and "done" in out.stdout
    assert proctree.prove_empty(seen["tree"], deadline=5.0), \
        "the helper returned cleanly and left its grandchild running"
    assert helpers.live_helpers() == [], "a settled helper stayed in the registry"


def test_a_plain_helper_leaves_nothing_behind(scripts):
    """The control, and the common path."""
    out = run(scripts["quick"])

    assert out.returncode == 0
    assert helpers.live_helpers() == []
    assert helpers.stragglers() == []


# ---------------------------------------------------------------------------
# 2. An unproved end keeps its record
# ---------------------------------------------------------------------------

def test_a_tree_that_cannot_be_proved_empty_stays_in_the_registry(scripts, monkeypatch):
    """The `finally` used to release the record regardless, so the one thing
    tracking a browser that would not die forgot about it."""
    monkeypatch.setattr(proctree, "prove_empty", lambda tree, **kw: False)

    with pytest.raises(helpers.HelperTimeout) as caught:
        run(scripts["wedged"], timeout=3.0)

    left = helpers.stragglers()
    assert len(left) == 1, "an unprovable helper was dropped"
    assert left[0].straggler, "the record does not say what was left"
    assert "straggler" in str(caught.value).lower(), \
        f"the caller is not told cleanup failed: {caught.value}"


def test_end_helpers_reports_what_it_could_not_end(scripts, monkeypatch):
    """And keeps it: a failed cleanup is not completed work."""
    monkeypatch.setattr(proctree, "prove_empty", lambda tree, **kw: False)
    with pytest.raises(helpers.HelperTimeout):
        run(scripts["wedged"], timeout=3.0)

    left = helpers.end_helpers(deadline=2.0)

    assert len(left) == 1
    assert helpers.live_helpers(), "a straggler was cleared by the shutdown sweep"


def test_a_successful_shutdown_reports_nothing(scripts):
    run(scripts["quick"])

    assert helpers.end_helpers(deadline=5.0) == []


# ---------------------------------------------------------------------------
# 3. Nothing escapes the shutdown snapshot
# ---------------------------------------------------------------------------

def test_no_helper_may_start_during_the_shutdown_sweep(scripts):
    """A request arriving mid-shutdown could otherwise open a browser behind
    the snapshot `end_helpers` just took - the exact process nothing tracks."""
    with helpers.closed_for_shutdown():
        with pytest.raises(helpers.HelperShuttingDown):
            run(scripts["quick"])

    assert helpers.live_helpers() == [], "the refused helper was left registered"


def test_the_door_reopens_once_the_sweep_is_over(scripts):
    """The window to protect is the sweep, not the rest of the process.

    A permanent latch meant the first shutdown anywhere - another app in the
    same process, an earlier test - refused every probe afterwards in a server
    that was still running. That is how it was found: four unrelated suites
    began failing with `HelperShuttingDown`.
    """
    helpers.end_helpers(deadline=2.0)

    out = run(scripts["quick"])          # must not raise

    assert out.returncode == 0


# ---------------------------------------------------------------------------
# 4. The timeout contract the routes depend on
# ---------------------------------------------------------------------------

def test_a_wedged_helper_times_out_and_is_ended(scripts):
    with pytest.raises(helpers.HelperTimeout):
        run(scripts["wedged"], timeout=3.0)

    assert helpers.live_helpers() == [], \
        "a helper that was successfully ended stayed registered"


def test_a_failing_helper_still_returns_its_output(tmp_path):
    """`allow_fail` callers read stdout from a non-zero exit."""
    script = tmp_path / "unhealthy.py"
    script.write_text('import sys; print(\'{"ok": false}\'); sys.exit(3)',
                      encoding="utf-8")

    out = run(script)

    assert out.returncode == 3 and '"ok": false' in out.stdout


def test_pipes_are_closed_on_every_path(scripts):
    """One pair of file objects per wedged helper, otherwise."""
    seen = {}
    real = proctree.spawn

    def watched(argv, **kw):
        proc, tree = real(argv, **kw)
        seen["proc"] = proc
        return proc, tree

    proctree.spawn, kept = watched, proctree.spawn
    try:
        with pytest.raises(helpers.HelperTimeout):
            run(scripts["wedged"], timeout=3.0)
    finally:
        proctree.spawn = kept

    proc = seen["proc"]
    assert [n for n in ("stdout", "stderr")
            if getattr(proc, n) is not None and not getattr(proc, n).closed] == []


def test_the_shutdown_dispatcher_surfaces_the_list(tmp_path, monkeypatch, capsys):
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
