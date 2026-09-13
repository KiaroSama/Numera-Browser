"""What a helper owns, and whether its answer is whole.

`0.33.0` ended a helper's separately sessioned driver correctly - the OS agreed
the process was gone - and still got two things wrong that no amount of process
checking would catch.

**The answer was truncated in silence.** A helper that started a driver holding
both of its pipe writers, wrote `TRAILING-NO-NEWLINE` and exited 0 handed its
caller `returncode=0` with `stdout=''`. Three causes stacked: draining by line
never yields a trailing write with no newline; the reader threads were daemons
nobody joined; and the pipes were closed while a live process still held the
writers, so end-of-file was never reachable in the first place. Measured before
the fix: 6.89s for a 3s budget, `stdout=''`. After: 3.77s, output intact.

**Unknown ownership read as zero ownership.** A missing report, an unreadable
one, a truncated final record and a helper that genuinely started nothing all
returned the same empty list, so "the evidence is gone" cleared the registry
exactly like "there was nothing to clean up".

Every acceptance here uses an independent oracle - the OS for a process, the
collected bytes for the output, the registry contents for ownership - never the
same predicate the implementation uses to decide. Real children throughout,
because the whole subject is what survives a process boundary.
"""

import json
import subprocess
import sys
import time
from pathlib import Path

import pytest

from numera import proctree
from numera.server import helpers
from numera.server.helper_streams import Streams
from support_supervision import assert_bounded, supervise

ENGINE = str(Path(__file__).resolve().parents[1])

#: The declared deadlines, stated before any test runs them. Two, because the
#: two scenario classes are measuring different things and one number lied
#: about both: a helper that is *meant to finish* must be given long enough to
#: start Python and import `numera` on a loaded machine - at 3s it was killed
#: before it ever reached the driver spawn, and the test then "failed" on a
#: product that was working. A helper that is meant to *wedge* needs the
#: opposite: a budget short enough that the timeout path is what is observed.
#:
#: Each is an operation budget plus the one shared cleanup budget. The
#: scheduling allowance is `supervise`'s and stays separate on purpose, so a
#: slow machine cannot quietly widen the bound under test.
BUDGET_OK = 60.0
BUDGET_WEDGED = 3.0
TOTAL_OK = BUDGET_OK + helpers.CLEANUP_BUDGET
TOTAL_WEDGED = BUDGET_WEDGED + helpers.CLEANUP_BUDGET


def script(tmp_path_factory, name: str, body: str) -> Path:
    path = tmp_path_factory.mktemp(name) / f"{name}.py"
    path.write_text(body, encoding="utf-8")
    return path


@pytest.fixture(scope="module")
def scripts(tmp_path_factory):
    """Written once for the module; each test spawns, none re-creates."""
    driver_spawn = (
        "import subprocess, sys\n"
        f"sys.path.insert(0, {ENGINE!r})\n"
        "from numera import proctree\n"
        # The driver keeps BOTH of this process's pipe writers.
        # `close_fds=False` is what `_spawn_leader` uses on Windows to pass the
        # job handle, and the inherited std handles come with it - so this is
        # the production topology, not an invented one.
        "proc, tree = proctree._spawn_leader(\n"
        "    subprocess.Popen,\n"
        "    [sys.executable, '-c', 'import time; time.sleep(120)'],\n"
        "    (), {'stdout': sys.stdout, 'stderr': sys.stderr,\n"
        "         'close_fds': False})\n")
    return {
        # A separately sessioned driver holding both writers, then a clean exit
        # with trailing output and no newline.
        "outlives": script(tmp_path_factory, "outlives",
                           driver_spawn +
                           "sys.stdout.write('TRAILING-NO-NEWLINE')\n"
                           "sys.stdout.flush()\n"),
        # The same, but the helper never finishes: the timeout path.
        "wedged_driver": script(tmp_path_factory, "wedged_driver",
                                driver_spawn +
                                "import time; time.sleep(120)\n"),
        # A grandchild in the SAME group holding the pipes: the control that
        # separates "a driver left the group" from "a pipe is held open".
        "same_group": script(tmp_path_factory, "same_group",
                             "import subprocess, sys\n"
                             "subprocess.Popen([sys.executable, '-c',\n"
                             "                  'import time; time.sleep(120)'],\n"
                             "                 stdout=sys.stdout, stderr=sys.stderr,\n"
                             "                 close_fds=False)\n"
                             "sys.stdout.write('SAME-GROUP-TAIL')\n"
                             "sys.stdout.flush()\n"),
        "quick": script(tmp_path_factory, "quick", "print('ok')\n"),
        "unhealthy": script(tmp_path_factory, "unhealthy",
                            "import sys\n"
                            "sys.stdout.write('{\"ok\": false}')\n"
                            "sys.exit(3)\n"),
    }


@pytest.fixture
def manager():
    """One manager per test, and nothing of this test's left running."""
    made = helpers.HelperManager()
    yield made
    for h in made.live():
        made.settle(h, deadline=2.0)
        made._release(h)


def alive(pid: int) -> bool:
    """Ask the OS, not the implementation. The oracle for every process claim."""
    return proctree.alive(pid)


def units_of(helper) -> list:
    """The pids the helper reported owning, read before the record is cleared."""
    return [t.pid for t in helpers.read_units(helper.units_path).trees]


def run(manager, path, timeout=BUDGET_OK):
    return helpers.run_helper([sys.executable, str(path)], timeout,
                              manager=manager)


# ---------------------------------------------------------------------------
# N14.A - a separate session plus inherited pipes
# ---------------------------------------------------------------------------

def test_a_driver_holding_both_writers_does_not_truncate_the_answer(
        scripts, manager):
    """The regression, with its measured before: `rc=0` and `stdout=''`.

    The helper wrote its last bytes without a newline and exited. Nothing about
    that is exceptional - a probe printing compact JSON does it every time -
    and the caller was handed an empty string alongside a successful return
    code.
    """
    outcome = supervise(lambda: run(manager, scripts["outlives"]), budget=TOTAL_OK)
    assert_bounded(outcome, "run_helper with a driver holding both writers")

    assert outcome.error is None, f"unexpected failure: {outcome.error!r}"
    done = outcome.value
    assert done.returncode == 0
    assert done.stdout == "TRAILING-NO-NEWLINE", (
        "a trailing write with no newline was lost: draining by line never "
        f"yields it. Got {done.stdout!r}")
    assert manager.live() == [] and manager.stragglers() == []


def test_the_driver_is_gone_according_to_the_operating_system(
        scripts, manager):
    """Independent oracle. `settle()` returning True is not evidence about a
    process; only the OS is."""
    seen = {}
    real = helpers.read_units

    def watched(path):
        report = real(path)
        seen.setdefault("pids", [t.pid for t in report.trees])
        return report

    helpers.read_units = watched
    try:
        outcome = supervise(lambda: run(manager, scripts["outlives"]),
                            budget=TOTAL_OK)
    finally:
        helpers.read_units = real
    assert_bounded(outcome, "run_helper with a separately sessioned driver")

    assert seen.get("pids"), "the driver's own session was never reported"
    # Sampled before teardown, which is the only moment it proves anything.
    still = [p for p in seen["pids"] if alive(p)]
    assert still == [], f"a separately sessioned driver survived cleanup: {still}"


def test_a_grandchild_in_the_same_group_is_the_control(scripts, manager):
    """Holding the pipes and leaving the group are different failures.

    This one holds both writers but stays in the helper's own tree, so ending
    the outer tree alone is enough. If this passed while the separate-session
    case failed, the fault would be ownership; both passing means the fix is
    the ordering, not the tree walk.
    """
    outcome = supervise(lambda: run(manager, scripts["same_group"]),
                        budget=TOTAL_OK)
    assert_bounded(outcome, "run_helper with a same-group pipe holder")

    assert outcome.error is None, f"unexpected failure: {outcome.error!r}"
    assert outcome.value.stdout == "SAME-GROUP-TAIL"
    assert manager.live() == []


def test_a_wedged_helper_with_a_driver_times_out_within_the_declared_bound(
        scripts, manager):
    """The timeout half of the same topology. The brief's sequence was a 3s
    helper budget exceeding a 10s supervisor, which is what makes the bound the
    thing under test rather than the duration."""
    outcome = supervise(lambda: run(manager, scripts["wedged_driver"], BUDGET_WEDGED),
                        budget=TOTAL_WEDGED)
    assert_bounded(outcome, "run_helper timing out with a separate session")

    assert isinstance(outcome.error, helpers.HelperTimeout), \
        f"expected a timeout, got {outcome.error!r}"
    assert manager.live() == [], "a helper that was ended stayed registered"


@pytest.mark.parametrize("name,code,text", [
    ("quick", 0, "ok\n"),
    ("unhealthy", 3, '{"ok": false}'),
])
def test_a_plain_helper_returns_its_whole_output(scripts, manager, name,
                                                 code, text):
    """Normal and non-zero exits, and a non-zero one whose body has no newline -
    `allow_fail` callers parse exactly that."""
    outcome = supervise(lambda: run(manager, scripts[name]), budget=TOTAL_OK)
    assert_bounded(outcome, f"run_helper on {name}")

    assert outcome.error is None, f"unexpected failure: {outcome.error!r}"
    assert outcome.value.returncode == code
    assert outcome.value.stdout == text
    assert manager.live() == []


def test_a_reader_that_cannot_reach_the_end_says_so(tmp_path):
    """`complete` is the field the truncation fix rests on, at its own seam.

    A real OS pipe and a real reader thread, with the writer deliberately still
    open - which is precisely the state a live process holding the handle
    produces. No mock: the subsystem under test is doing its own work, and the
    only thing arranged is the condition.

    Why here and not through `run_helper`: on Windows a nested job object takes
    every descendant with the outer tree, so once the ownership units are ended
    there is no surviving handle holder left to keep a reader waiting - the
    ordering fix closes that path. On POSIX a process that escaped the group
    unreported could still do it, which is the residual case this branch exists
    for. Testing it at the seam covers both platforms with one assertion
    instead of a race that only one of them can lose.
    """
    import os

    read_fd, write_fd = os.pipe()
    reader_end = os.fdopen(read_fd, "rb")
    writer_end = os.fdopen(write_fd, "wb", buffering=0)

    class _Proc:
        stdout, stderr = reader_end, None

    streams = Streams(_Proc())
    writer_end.write(b"PARTIAL")
    writer_end.flush()
    try:
        out, _err, complete = streams.collect(0.4)   # the writer is still open

        assert complete is False, \
            "a reader still waiting on an open writer reported completion"
        assert out == "PARTIAL", "what did arrive should still be readable"
        assert streams.unfinished() == ["stdout"]
    finally:
        writer_end.close()
        streams.close()


def test_a_closed_writer_lets_the_reader_finish(tmp_path):
    """The control for the case above: once nothing holds the writer, the drain
    completes - which is exactly what ending the units buys."""
    import os

    read_fd, write_fd = os.pipe()
    reader_end = os.fdopen(read_fd, "rb")
    writer_end = os.fdopen(write_fd, "wb", buffering=0)

    class _Proc:
        stdout, stderr = reader_end, None

    streams = Streams(_Proc())
    writer_end.write(b"WHOLE")
    writer_end.close()
    try:
        out, _err, complete = streams.collect(2.0)

        assert complete is True and out == "WHOLE"
        assert streams.unfinished() == []
    finally:
        streams.close()


def test_no_reader_thread_outlives_the_call(scripts, manager):
    """A daemon reader still holding output is the truncation, one layer down."""
    import threading

    before = {t.ident for t in threading.enumerate()}
    outcome = supervise(lambda: run(manager, scripts["outlives"]), budget=TOTAL_OK)
    assert_bounded(outcome, "run_helper")

    left = [t.name for t in threading.enumerate()
            if t.ident not in before and t.name.startswith("helper-")]
    assert left == [], f"reader threads outlived the call: {left}"


# ---------------------------------------------------------------------------
# N14.C - unknown ownership is not zero ownership
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("body,why", [
    ('{"pid": 1,\n', "a truncated final record"),
    ("garbage\n", "a line that is not JSON"),
    ('{"tree": {"kind": "job", "group": "g"}}\n', "a record with no pid"),
])
def test_a_damaged_report_is_not_an_empty_one(tmp_path, body, why):
    """The regression. All of these used to answer `[]`, exactly like a helper
    that started nothing."""
    path = tmp_path / "units.jsonl"
    path.write_text(body, encoding="utf-8")

    report = helpers.read_units(path)

    assert report.complete is False, f"{why} read as a complete report"
    assert report.problem and "units.jsonl" in report.problem


def test_a_missing_report_is_not_an_empty_one(tmp_path):
    report = helpers.read_units(tmp_path / "never-written.jsonl")

    assert report.complete is False
    assert "missing" in report.problem


def test_no_report_path_at_all_is_not_an_empty_one():
    report = helpers.read_units(None)

    assert report.complete is False
    assert report.problem


def test_an_empty_report_is_a_real_answer(tmp_path):
    """The control, and the reason the distinction is not just strictness: a
    helper that starts no separately sessioned driver is the common case, and
    it must settle cleanly."""
    path = tmp_path / "units.jsonl"
    path.write_text("", encoding="utf-8")

    report = helpers.read_units(path)

    assert (report.trees, report.complete, report.problem) == ([], True, None)


def test_several_units_all_come_back(tmp_path):
    path = tmp_path / "units.jsonl"
    path.write_text("".join(
        json.dumps({"pid": pid, "pidToken": None,
                    "tree": {"kind": "job", "group": f"g{pid}"}}) + "\n"
        for pid in (11, 22, 33)), encoding="utf-8")

    report = helpers.read_units(path)

    assert [t.pid for t in report.trees] == [11, 22, 33]
    assert report.complete is True


def test_a_damaged_report_strands_the_helper_rather_than_clearing_it(
        scripts, manager):
    """The consequence, at the manager. Lost evidence must keep the record."""
    outcome = supervise(lambda: run(manager, scripts["quick"]), budget=TOTAL_OK)
    assert_bounded(outcome, "run_helper")
    helper = manager.admit([sys.executable, "-c", "pass"])
    manager.child_env(helper)
    helper.units_path.write_text('{"pid": 4242,\n', encoding="utf-8")

    assert manager.settle(helper, deadline=2.0) is False
    assert helper.state == helpers.STRANDED
    assert helper in manager.live(), "a helper with lost evidence was released"
    assert "unreadable record" in helper.straggler


def test_a_report_that_cannot_be_written_ends_the_launch(tmp_path, monkeypatch):
    """N14.C's other half, in the child.

    `_report_unit` swallowed `OSError`, so a driver whose ownership could never
    be communicated started anyway - and the cleanup that followed reported
    success without knowing it existed. The launch now fails closed.
    """
    monkeypatch.setenv(proctree.UNITS_ENV,
                       str(tmp_path / "no-such-dir" / "units.jsonl"))
    started = []
    real = subprocess.Popen

    def watched(*a, **kw):
        proc = real(*a, **kw)
        started.append(proc)
        return proc

    with pytest.raises(proctree.UnitReportFailed):
        proctree._spawn_leader(
            watched, [sys.executable, "-c", "import time; time.sleep(120)"],
            (), {"stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL})

    assert started, "the test did not reach the spawn it is about"
    deadline = time.monotonic() + 5.0
    while started[0].poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    assert started[0].poll() is not None, \
        "a driver whose ownership could not be recorded was left running"
