"""One bounded lifetime for a helper's processes, reports and readers.

Six ways the bound leaked, each measured rather than reasoned about. What they
have in common is that the helper machinery was correct *while nothing went
wrong*: cleanup succeeded, so closing the pipes was safe; attach ran, so the
record knew its child; one shutdown happened, so the label still meant what it
said. Every one of them stopped holding at the first failure, which is the only
time any of it matters.

These run against the real `HelperManager`, `run_helper`, `Streams` and
`unit_report.read`. Fault injection is used to *hold a door open* - block a
lock, fail a termination, raise where a raise is possible - and never to
manufacture a successful cleanup, a decryption or a validation.

Every test here declares its own bound and asserts against it, because the
defect under test is in every case "this took longer than the caller allowed".
"""

import json
import os
import subprocess
import sys
import threading
import time

import pytest

from numera import proctree, unit_report
from numera.server import helpers
from numera.server.helper_streams import Streams

#: Operation-plus-cleanup bound for the cases that drive a real child. Well
#: above the ~1s they need and well below the outer supervisor, so "it finished"
#: and "something rescued it" cannot be confused.
BOUND = 20.0

#: What a bounded call may take when the thing it waits for never arrives. The
#: budget under test is 0.05-0.5s; this is the margin that makes a *blocked*
#: call unambiguous rather than a slow one.
SLACK = 3.0


@pytest.fixture
def manager():
    """A manager of our own, swept whatever the test did to it."""
    made = helpers.HelperManager()
    yield made
    for helper in made.live():
        try:
            if helper.proc is not None:
                proctree.end_session(helper.proc, helper.tree, grace=2.0)
        except Exception:                                # noqa: BLE001
            pass


def _pipe_pair():
    """A readable pipe whose writer this test keeps open.

    The writer is the whole fixture: it is what makes the reader thread block
    in `read1`, which is the state every close has to survive.

    **Buffered on purpose**, matching `subprocess.PIPE`. Opened with
    `buffering=0` this is a `FileIO`, which has no `read1` at all - the reader
    thread dies on `AttributeError` in its first iteration, never blocks, and
    the test passes against any implementation. The lock a blocking close waits
    for belongs to `BufferedReader`, so the fixture has to produce one.
    """
    read_fd, write_fd = os.pipe()
    return open(read_fd, "rb"), os.fdopen(write_fd, "wb")


class _FakeProc:
    def __init__(self, stdout=None, stderr=None):
        self.stdout, self.stderr = stdout, stderr


# ---------------------------------------------------------------------------
# A. A failed cleanup must not turn into a blocking close
# ---------------------------------------------------------------------------

def test_closing_does_not_block_on_a_reader_holding_a_live_writer():
    """The regression, at the seam where it blocked.

    `close()` called `BufferedReader.close()` on every pipe unconditionally.
    A reader blocked in `read1` holds that object's own lock and `close`
    flushes under it, so the close waited for the writer to go - and the writer
    belongs to the unit that cleanup had just failed to end. The six-second
    outer supervisor was what ended it.
    """
    pipe, writer = _pipe_pair()
    streams = Streams(_FakeProc(stdout=pipe))
    try:
        started = time.monotonic()
        alive = streams.close(budget=0.2)
        took = time.monotonic() - started

        assert took < SLACK, (
            f"close() blocked for {took:.1f}s on a reader whose writer is "
            f"still open; it is supposed to be bounded")
        # The outcome changed with the cancellable pump, and the new one is
        # stronger. A blocking `read1` could not be interrupted, so the only
        # safe thing left was to RETAIN the reader and report it; this test
        # used to assert exactly that. A cancellable reader is told to stop,
        # stops, and its pipe is genuinely closed - so there is nothing to
        # retain even though the writer is still open. Retention is for what
        # cannot be closed, which is what the very next test pins.
        #
        # What the writer being alive still means is that the OUTPUT is not
        # complete, and that is reported on the output, not as a leaked pipe.
        assert alive == [], (
            "a cancellable reader must be disposed, not retained: "
            f"{alive} was left holding a pipe")
        assert streams.retained() == [], \
            "nothing failed to close, so nothing should be reported retained"
    finally:
        writer.close()
        streams.close(budget=SLACK)


def test_a_reader_that_ended_is_still_closed():
    """The control: retention is for what cannot be closed, not a new default."""
    pipe, writer = _pipe_pair()
    writer.write(b"done")
    writer.close()                       # EOF: the reader will end on its own
    streams = Streams(_FakeProc(stdout=pipe))

    assert streams.close(budget=SLACK) == [], "a finished reader was not closed"
    assert streams.retained() == []
    assert pipe.closed, "the pipe of an ended reader was left open"


def test_an_unsettled_helper_records_the_readers_it_retained(manager, tmp_path):
    """The production boundary: `run_helper` must stay bounded and say why.

    Termination is made to fail for every tree, which is what a separately
    sessioned child retaining both writers looks like from here.
    """
    script = tmp_path / "chatty.py"
    script.write_text("import sys, time\n"
                      "sys.stdout.write('PARTIAL')\n"
                      "sys.stdout.flush()\n"
                      "time.sleep(600)\n", encoding="utf-8")

    seen = {}
    real_spawn = proctree.spawn
    real_end_tree, real_end_session = proctree.end_tree, proctree.end_session

    def watched(argv, **kw):
        proc, tree = real_spawn(argv, **kw)
        seen["proc"] = proc
        return proc, tree

    proctree.spawn = watched
    # Both, or the fault is not the one being modelled: patching `end_tree`
    # alone leaves `end_session` free to kill the child, after which the
    # writers are gone, the readers reach EOF and nothing is ever retained -
    # a test that asserts retention while quietly arranging a clean exit.
    proctree.end_tree = lambda tree, **kw: False          # nothing can be ended
    proctree.end_session = lambda proc, tree, **kw: None  # ... and nothing dies
    started = time.monotonic()
    try:
        with pytest.raises(helpers.HelperFailed):
            helpers.run_helper([sys.executable, str(script)], timeout=1.0,
                               manager=manager, cleanup=3.0)
        took = time.monotonic() - started

        assert took < BOUND, f"run_helper took {took:.1f}s with cleanup failing"
        stuck = [h for h in manager.live() if h.straggler]
        assert stuck, "a helper nothing could end was not retained as a straggler"
        assert stuck[0].state == helpers.STRANDED
        assert "still holding a pipe" in stuck[0].straggler, \
            f"the retained reader is not named on the record: {stuck[0].straggler}"
    finally:
        # Retention is the product behaving correctly; leaving it retained is
        # this test leaking. The child is ours and known by identity, so end it
        # and let the reader reach EOF before the pipes are closed.
        proctree.spawn = real_spawn
        proctree.end_tree, proctree.end_session = real_end_tree, real_end_session
        proc = seen.get("proc")
        if proc is not None:
            proc.kill()
            proc.wait(timeout=SLACK)
            for pipe in (proc.stdout, proc.stderr):
                if pipe is not None and not pipe.closed:
                    pipe.close()


# ---------------------------------------------------------------------------
# B. A failure before attach must not lose the child
# ---------------------------------------------------------------------------

def test_a_failure_before_attach_still_owns_the_spawned_child(manager, tmp_path):
    """`Streams(proc)` raising left `proc` and `tree` in locals only.

    Cleanup then looked at a record with no process, found nothing to end and
    released it - while the child it had just written off was running with two
    open pipes. The existing test raised only *after* `real_attach`, which is
    the case that already worked.
    """
    script = tmp_path / "sleeper.py"
    script.write_text("import time; time.sleep(600)\n", encoding="utf-8")

    seen = {}
    real_spawn = proctree.spawn

    def watched(argv, **kw):
        proc, tree = real_spawn(argv, **kw)
        seen["proc"], seen["tree"] = proc, tree
        return proc, tree

    class Boom(RuntimeError):
        pass

    real_streams = helpers.Streams
    proctree_spawn, helpers.Streams = proctree.spawn, None
    proctree.spawn = watched

    def exploding(proc, **kwargs):
        raise Boom("constructing the readers failed")

    helpers.Streams = exploding
    try:
        with pytest.raises(Boom):
            helpers.run_helper([sys.executable, str(script)], timeout=5.0,
                               manager=manager, cleanup=5.0)
    finally:
        proctree.spawn, helpers.Streams = proctree_spawn, real_streams

    assert seen, "the fixture never spawned; the test proves nothing"
    # The record must name the exact child, and cleanup must have ended it.
    assert seen["proc"].poll() is not None, \
        "the child outlived a failure between its spawn and its attach"
    for pipe in (seen["proc"].stdout, seen["proc"].stderr):
        assert pipe is None or pipe.closed, \
            "a pipe of the abandoned child was left open"


# ---------------------------------------------------------------------------
# C. Repeated shutdown must not release an unresolved reservation
# ---------------------------------------------------------------------------

#: The brief asks for the startup/shutdown race repeated, because a race that
#: is only run once is a coin toss reported as a result. Twenty is enough to
#: make a one-in-four flake a one-in-a-million pass, and the case costs
#: milliseconds - no process, no browser, just the registry and two events.
RACE_REPEATS = 20


@pytest.mark.parametrize("attempt", range(RACE_REPEATS))
def test_the_startup_shutdown_race_holds_every_time(manager, attempt):
    """The same race as below, repeated, with every attempt kept.

    No rerun-on-failure and no flake masking: a single red attempt fails the
    parametrization, which is the point. The reservation resolves *while* the
    shutdown is cancelling it, which is the interleave that decides whether a
    spawn can complete behind a finished sweep.
    """
    helper = manager.admit(("python", "-c", "pass"))
    manager.child_env(helper, {})

    ready = threading.Barrier(2)

    def resolve_mid_shutdown():
        ready.wait(timeout=SLACK)
        helper.resolved.set()          # the spawner finishing, concurrently

    racer = threading.Thread(target=resolve_mid_shutdown, name="late-spawn")
    racer.start()
    try:
        ready.wait(timeout=SLACK)
        left = manager.end_all(deadline=1.0)
    finally:
        racer.join(timeout=SLACK)
        assert not racer.is_alive(), "the racing thread never returned"

    # Whichever side won, the record must be accounted for: either settled
    # cleanly or reported. What it may never be is silently released while
    # still able to produce a process.
    if helper in manager.live():
        assert helper in left, (
            f"attempt {attempt}: a live record was not reported by the sweep")
    else:
        assert helper.state == helpers.SETTLED, (
            f"attempt {attempt}: released in state {helper.state!r}")


def test_a_second_shutdown_does_not_release_a_still_unresolved_reservation(manager):
    """The first shutdown wrote STRANDED; the second asked "is it STARTING?".

    It is not - the first shutdown changed that - so the second fell through to
    `settle`, found a record with no process, and released a reservation that
    can still spawn. The question has to be whether it resolved, which is a
    fact about the spawner, not the label the last writer happened to leave.
    """
    helper = manager.admit(("python", "-c", "pass"))     # never resolved
    # `run_helper` creates the report file before it spawns, so a reservation
    # that has not spawned still has an *empty, readable* one. That detail is
    # the whole defect: without it `settle` reads "no report" and strands the
    # record anyway, so a first version of this test passed against the unfixed
    # code. With it, the report is complete and empty, settlement succeeds, and
    # the reservation is released - which is what actually happened in
    # production.
    manager.child_env(helper, {})

    first = manager.end_all(deadline=0.2)
    assert helper in first, "an unresolved reservation was not reported"
    assert helper in manager.live(), "it was released on the first shutdown"

    second = manager.end_all(deadline=0.2)
    assert helper in second, \
        "the second shutdown treated an unresolved reservation as finished"
    assert helper in manager.live(), \
        "a reservation still able to spawn was released by a repeat shutdown"
    assert helper.state == helpers.STRANDED


def test_a_resolved_reservation_is_still_settled_normally(manager):
    """The control: only the unresolved ones are held back."""
    helper = manager.admit(("python", "-c", "pass"))
    manager.child_env(helper, {})
    helper.resolved.set()                                # resolved, no process

    assert manager.end_all(deadline=1.0) == []
    assert helper.state == helpers.SETTLED
    assert helper not in manager.live()


# ---------------------------------------------------------------------------
# D. The report format is validated, and reading never raises
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("line,why", [
    ("17", "a bare number"),
    ("[1]", "a list"),
    ('"text"', "a bare string"),
    ('{"pid": "abc"}', "a non-numeric pid"),
    ('{"pid": -1}', "a negative pid"),
    ('{"pid": 0}', "a zero pid"),
    ('{"pid": true}', "a boolean pid"),
    ('{"pid": 5, "tree": [1]}', "a list where the tree belongs"),
    ('{"pid": 5, "tree": {"kind": "weird"}, "pidToken": 1}', "an unsupported kind"),
    ('{"pid": 5, "tree": {"kind": 7}, "pidToken": 1}', "a non-string kind"),
    ('{"pid": 5}', "no durable identity at all"),
    ('{"pid": 5, "tree": {"kind": "job"}}', "a job with no name"),
])
def test_an_untrustworthy_record_is_incomplete_evidence(tmp_path, line, why):
    """Each of these used to raise out of `read`, or be accepted as complete.

    Raising is the worse half: it escapes settlement entirely, so the non-daemon
    reader threads are left alive outside any cleanup.
    """
    path = tmp_path / "units.jsonl"
    path.write_text(line + "\n", encoding="utf-8")

    report = unit_report.read(path)

    assert report.complete is False, f"{why} was accepted as a whole report"
    assert report.problem and "unreadable record" in report.problem
    assert report.trees == [], f"{why} produced a tree to act on"


def test_an_undecodable_report_is_incomplete_evidence(tmp_path):
    """`read_text` raised `UnicodeDecodeError` - a `ValueError`, so the OSError
    handler never saw it and it left `read` entirely."""
    path = tmp_path / "units.jsonl"
    path.write_bytes(b'{"pid": 5, "tree": {"kind": "pgid", "group": 5}}\n\xff\xfe')

    report = unit_report.read(path)

    assert report.complete is False
    # The reason is now reported as one category covering both ways a line can
    # fail to decode into a record; either wording satisfies what this pins.
    assert ("not valid UTF-8" in report.problem
            or "invalid JSON/UTF-8" in report.problem)


def test_a_partial_report_still_yields_the_records_it_did_have(tmp_path):
    """Incomplete is not empty: whatever was readable still gets ended."""
    good = json.dumps({"pid": 4321, "pidToken": 99,
                       "tree": {"kind": "pgid", "group": 4321}})
    path = tmp_path / "units.jsonl"
    path.write_text(good + "\n" + '{"pid": "abc"}\n', encoding="utf-8")

    report = unit_report.read(path)

    assert report.complete is False
    assert [t.pid for t in report.trees] == [4321], \
        "a usable record was discarded along with the damaged one"


@pytest.mark.parametrize("record,kind", [
    ({"pid": 4321, "pidToken": 99, "tree": {"kind": "pgid", "group": 4321}}, "pgid"),
    ({"pid": 4321, "tree": {"kind": "job", "group": "numera-abc"}}, "job"),
])
def test_a_valid_record_is_complete(tmp_path, record, kind):
    """The control, both native primitives. A validator that rejects everything
    is not a validator."""
    path = tmp_path / "units.jsonl"
    path.write_text(json.dumps(record) + "\n", encoding="utf-8")

    report = unit_report.read(path)

    assert report.complete is True, report.problem
    assert [t.kind for t in report.trees] == [kind]
    assert report.trees[0].verified, "an accepted record cannot even be signalled"


def test_an_initialized_zero_unit_report_is_a_real_answer(tmp_path):
    """Zero units, trustworthy because the receipt was INITIALIZED.

    This used to write an empty file and call that trustworthy evidence of
    zero units. It is not: an erased, truncated or never-written report is
    byte-for-byte identical to one from a helper that spawned nothing, so
    "the file exists and is empty" proved only that nothing could be read.
    The receipt now carries a versioned header written before any child can
    exist, which is what makes zero a positive answer rather than an absence.
    """
    path = tmp_path / "units.jsonl"
    path.write_text("", encoding="utf-8")
    unit_report.initialize(path)

    report = unit_report.read(path)

    assert report.complete is True and report.trees == []


def test_an_erased_report_is_not_zero_units(tmp_path):
    """The other half, kept separate on purpose: an empty file is refused."""
    path = tmp_path / "units.jsonl"
    path.write_text("", encoding="utf-8")

    report = unit_report.read(path)

    assert report.complete is False
    assert "zero-unit receipt" in report.problem


# ---------------------------------------------------------------------------
# F. The serialization lock is inside the deadline
# ---------------------------------------------------------------------------

def test_settle_does_not_wait_past_its_deadline_for_the_finish_lock(manager):
    """`settle(deadline=.05)` sat on `helper.finish` past `.3s`.

    The budget only started once the lock was held, so the blocking part of the
    call was outside the bound the caller declared - which is the one part a
    caller cannot plan around.
    """
    helper = manager.admit(("python", "-c", "pass"))
    manager.child_env(helper, {})
    helper.resolved.set()

    release = threading.Event()
    holding = threading.Event()

    def hold():
        with helper.finish:
            holding.set()
            release.wait(timeout=SLACK * 2)

    keeper = threading.Thread(target=hold, name="lock-holder")
    keeper.start()
    try:
        assert holding.wait(timeout=SLACK), "the fixture never took the lock"
        started = time.monotonic()
        proved = manager.settle(helper, deadline=0.05)
        took = time.monotonic() - started

        assert took < SLACK, (
            f"settle(deadline=.05) waited {took:.1f}s for the lock; the "
            f"deadline has to include acquiring it")
        assert proved is False, \
            "a settle that never ran reported the helper as proved clean"
        assert helper.straggler, "a bounded failure left no reason on the record"
    finally:
        release.set()
        keeper.join(timeout=SLACK)


def test_the_finish_lock_is_released_for_the_next_caller(manager):
    """The control: bounding the wait must not leak the lock."""
    helper = manager.admit(("python", "-c", "pass"))
    manager.child_env(helper, {})
    helper.resolved.set()

    assert manager.settle(helper, deadline=1.0) is True
    assert helper.finish.acquire(timeout=SLACK), "the finish lock was not released"
    helper.finish.release()
