"""Shutdown and a helper that has not spawned yet are one state machine.

`0.33.0` reserved the record before the spawn, which was the right order and
not enough. `end_all` then *settled* that reservation: it found no process on
it, concluded there was nothing to end, and released it from the registry.
Measured before the fix, with nothing patched:

    after admit      : state=starting live=1
    end_all returned : []
    after end_all    : live=0 closed=True
    reservation      : state=settled

The caller still held that reservation and went on to spawn. So a child was
created after shutdown had reported success, registered nowhere, tracked by
nothing - the exact process the reservation existed to name.

A reservation is now cancelled and waited for, never released on the strength
of having no process yet, and the wait happens with the manager's lock free
because resolving it needs that same lock. If it does not resolve inside the
cleanup budget it stays registered and stranded, because "it might still create
work" is the honest answer.

The scheduling barriers here inject *when* things happen, never *what* they do:
the manager, `proctree.spawn` and the real children are all the production
ones. Each race runs `REPEATS` times because a race that passes once has not
been tested; no rerun plugin is involved and every failure is kept.
"""

import subprocess
import sys
import threading

import pytest

from numera import proctree
from numera.server import helpers
from support_supervision import assert_bounded, supervise

#: Deterministic races are cheap - no browser, a child that exits at once - so
#: the count is set by what makes a race credible rather than by the clock.
REPEATS = 20

#: The declared bound for one admission race: the manager's cleanup budget plus
#: the time a trivial child needs to start and exit.
TOTAL = helpers.CLEANUP_BUDGET + 30.0

TRIVIAL = [sys.executable, "-c", "pass"]


@pytest.fixture
def manager():
    made = helpers.HelperManager()
    yield made
    for h in made.live():
        made.settle(h, deadline=2.0)
        made._release(h)


# ---------------------------------------------------------------------------
# 1. The reproduction, without any patching at all
# ---------------------------------------------------------------------------

def test_a_shutdown_does_not_release_a_reservation_that_has_not_spawned(
        manager):
    """The regression, exactly as it was measured.

    No barrier and no injection: `admit` then `end_all` is the whole sequence.
    The reservation must still be accounted for afterwards, because the caller
    holding it can still create a process.
    """
    helper = manager.admit(TRIVIAL)
    assert helper.state == helpers.STARTING

    left = manager.end_all(deadline=1.0)

    assert manager.closed is True
    assert helper in manager.live(), \
        "a reservation that could still spawn was dropped from the registry"
    assert helper in left, "shutdown reported success over an unresolved start"
    assert helper.state == helpers.STRANDED
    assert helper.cancelled, "the reservation was not told to stop"


def test_a_cancelled_reservation_refuses_before_it_spawns(manager):
    """The other half: the spawner must see the cancellation and not create a
    child at all. Nothing is patched here either."""
    started = []
    real = proctree.spawn
    proctree.spawn = lambda argv, **kw: started.append(argv) or real(argv, **kw)
    try:
        manager.end_all(deadline=1.0)
        with pytest.raises(helpers.HelperShuttingDown):
            helpers.run_helper(TRIVIAL, 30.0, manager=manager)
    finally:
        proctree.spawn = real

    assert started == [], "a refused request spawned a child anyway"


# ---------------------------------------------------------------------------
# 2. The three pause points, each raced with shutdown
# ---------------------------------------------------------------------------

def _race(manager, barrier_at, *, repeats=REPEATS):
    """Run `run_helper` with a barrier at one point, and shut down inside it.

    Returns the list of per-attempt outcomes. The barrier is a scheduling
    injection - it delays a real call, it does not replace one - so every
    attempt exercises the production admit/spawn/attach path.
    """
    outcomes = []
    for _ in range(repeats):
        mgr = helpers.HelperManager()
        reached, release = threading.Event(), threading.Event()
        original = getattr(mgr, barrier_at)

        def barrier(*a, _original=original, **kw):
            reached.set()
            release.wait(15.0)
            return _original(*a, **kw)

        setattr(mgr, barrier_at, barrier)

        result = {}

        def attempt():
            try:
                result["value"] = helpers.run_helper(TRIVIAL, 30.0, manager=mgr)
            except BaseException as e:                       # noqa: BLE001
                result["error"] = e

        worker = threading.Thread(target=attempt, daemon=True)
        worker.start()
        assert reached.wait(20.0), f"the barrier at {barrier_at} was never hit"

        # Shutdown, from another thread, while the start is held at the barrier.
        swept = {}

        def shutdown():
            swept["left"] = mgr.end_all(deadline=3.0)

        sweeper = threading.Thread(target=shutdown, daemon=True)
        sweeper.start()
        release.set()
        sweeper.join(25.0)
        worker.join(25.0)
        outcomes.append((mgr, result, swept))
        for h in mgr.live():
            mgr.settle(h, deadline=2.0)
            mgr._release(h)
    return outcomes


@pytest.mark.parametrize("barrier_at,where", [
    ("child_env", "between admission and the spawn"),
    ("check_open", "immediately before the spawn"),
    ("attach", "after the spawn, before the record is bound to it"),
])
def test_a_shutdown_racing_a_start_never_loses_the_outcome(
        barrier_at, where, manager):
    """Whatever the interleaving, the start is either refused before a child
    exists or its child ends up registered and accounted for.

    Never: shutdown reports success and a process appears afterwards.
    """
    outcome = supervise(lambda: _race(manager, barrier_at), budget=TOTAL * 2)
    assert_bounded(outcome, f"{REPEATS} shutdown races {where}")
    assert outcome.error is None, f"the race harness failed: {outcome.error!r}"

    for index, (mgr, result, swept) in enumerate(outcome.value):
        refused = isinstance(result.get("error"), helpers.HelperShuttingDown)
        completed = "value" in result
        assert refused or completed, (
            f"attempt {index} {where}: neither refused nor completed - "
            f"{result.get('error')!r}")
        # Whatever happened, nothing may be left running and unrecorded.
        unresolved = [h for h in mgr.live() if not h.resolved.is_set()]
        assert unresolved == [], (
            f"attempt {index} {where}: a reservation was left unresolved "
            f"after both the start and the shutdown finished")
        for helper in mgr.live():
            assert helper.straggler, (
                f"attempt {index} {where}: a helper is still registered with "
                f"nothing recorded about why")


# ---------------------------------------------------------------------------
# 3. Failures on the way up
# ---------------------------------------------------------------------------

def test_a_report_file_that_cannot_be_created_starts_nothing(
        manager, monkeypatch):
    """A temp-file failure is a start failure, and it must not leave a
    reservation behind either."""
    def refuse(*a, **kw):
        raise OSError("no space left on device")

    monkeypatch.setattr(helpers.tempfile, "mkstemp", refuse)
    started = []
    real = proctree.spawn
    proctree.spawn = lambda argv, **kw: started.append(argv) or real(argv, **kw)
    try:
        with pytest.raises(OSError):
            helpers.run_helper(TRIVIAL, 30.0, manager=manager)
    finally:
        proctree.spawn = real

    assert started == [], "a helper spawned despite having no way to report"
    assert manager.live() == [], "a reservation survived a start that failed"


def test_a_spawn_failure_leaves_no_reservation(manager, monkeypatch):
    """`proctree.spawn` abandons its own child on every failure path, so a
    raise from it means nothing is running - and the record goes with it."""
    def refuse(argv, **kw):
        raise proctree.TreeUnavailable("no job object available")

    monkeypatch.setattr(proctree, "spawn", refuse)

    with pytest.raises(proctree.TreeUnavailable):
        helpers.run_helper(TRIVIAL, 30.0, manager=manager)

    assert manager.live() == []


def test_an_attach_failure_does_not_lose_the_process_it_created(manager,
                                                                monkeypatch):
    """The one failure that must not simply discard: the child already exists.

    "Accounted for" is not "still listed". A process that was created, ended
    and *proved* gone is finished, and holding a record of it would be the
    opposite mistake. What must never happen is the exception propagating while
    the child runs on with nothing tracking it - so the oracle is the OS, and
    the registry assertion is that nothing was left stranded.
    """
    real_attach = manager.attach
    seen = {}

    def explode(helper, proc, tree):
        seen["proc"] = proc
        real_attach(helper, proc, tree)
        raise MemoryError("something else went wrong")

    monkeypatch.setattr(manager, "attach", explode)

    with pytest.raises(MemoryError):
        helpers.run_helper(TRIVIAL, 30.0, manager=manager)

    proc = seen.get("proc")
    assert proc is not None, "the test did not reach the spawn it is about"
    assert proc.poll() is not None, \
        "an attach failure left its child running and unreaped"
    assert not proctree.alive(proc.pid), \
        "the child survived an attach failure according to the OS"
    assert manager.stragglers() == [], \
        "a child that was proved gone was recorded as stranded"
    for pipe in (proc.stdout, proc.stderr):
        assert pipe is None or pipe.closed, \
            "an attach failure left a pipe open on a dead child"


# ---------------------------------------------------------------------------
# 4. Repeated and concurrent shutdown, and two apps
# ---------------------------------------------------------------------------

def test_shutting_down_twice_is_not_a_different_answer(manager):
    helpers.run_helper(TRIVIAL, 30.0, manager=manager)

    first = manager.end_all(deadline=3.0)
    second = manager.end_all(deadline=3.0)

    assert first == [] and second == []
    assert manager.closed is True


def test_concurrent_shutdowns_do_not_race_each_other(manager):
    helpers.run_helper(TRIVIAL, 30.0, manager=manager)
    results = []

    def sweep():
        results.append(manager.end_all(deadline=3.0))

    threads = [threading.Thread(target=sweep, daemon=True) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(20.0)

    assert all(r == [] for r in results), f"a concurrent sweep disagreed: {results}"
    assert manager.live() == []


def test_one_app_s_shutdown_leaves_another_app_s_helper_alone(manager):
    """Two apps with active helpers. The other app's sentinel must survive -
    the brief's explicit requirement, and the shape of the bug that made every
    manager process-global in the first place."""
    other = helpers.HelperManager()
    sentinel = subprocess.Popen(
        [sys.executable, "-c", "import time; time.sleep(30)"],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        helpers.run_helper(TRIVIAL, 30.0, manager=other)

        assert manager.end_all(deadline=3.0) == []

        assert other.closed is False, "one app's shutdown closed another's"
        assert sentinel.poll() is None, \
            "another app's process was terminated by this app's shutdown"
        assert helpers.run_helper(TRIVIAL, 30.0, manager=other).returncode == 0
        assert other.end_all(deadline=5.0) == []
    finally:
        sentinel.kill()
        sentinel.wait(timeout=10)
