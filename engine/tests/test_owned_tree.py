"""Stopping a session means the browser, not the process Numera can see.

`numera launch` is a Python process that starts Playwright, which starts a Node
driver, which starts Chromium. Numera holds a handle to the first one only, and
every "is it stopped?" answer was about that handle:

* the server dropped its registry entry when the launcher exited;
* the session lease, held by that launcher, was released by the OS with it;
* `/stop`, `numera stop` and `jobs stop` wrote "stopped";
* and a Chromium still holding `user-data/<id>` was invisible to all of it - so
  the next launch took the free lease and opened a second browser on the same
  profile directory, which is the corruption the lease exists to prevent.

The tests here use a real helper that behaves the way that Chromium does: a
leader that starts something long-lived and then leaves. `test_the_control`
below is the one that keeps the rest honest - it does the same thing *without*
Numera's ownership and shows the descendant surviving, so a version that proved
nothing could not pass this file by accident.
"""

import subprocess
import sys
import time

import pytest

from numera import jobs, proctree

# A leader that starts something long-lived and then exits, printing the pid it
# started so the test can watch that process rather than infer anything.
LEADER = (
    "import subprocess, sys, time;"
    "p = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)']);"
    "print(p.pid, flush=True);"
    "time.sleep({linger})"
)


def _gone(pid, deadline=15.0) -> bool:
    end = time.monotonic() + deadline
    while proctree.alive(pid):
        if time.monotonic() >= end:
            return False
        time.sleep(0.05)
    return True


def _kill(pid) -> None:
    subprocess.run(["taskkill", "/F", "/PID", str(pid)] if proctree._WINDOWS
                   else ["kill", "-9", str(pid)], capture_output=True)


@pytest.fixture
def leader():
    """A spawned tree, and the pid of the descendant it started."""
    made = []

    def spawn(linger=0.4):
        proc, tree = proctree.spawn(
            [sys.executable, "-c", LEADER.format(linger=linger)],
            stdout=subprocess.PIPE, text=True)
        kid = int(proc.stdout.readline().strip())
        made.append((proc, tree, kid))
        return proc, tree, kid

    yield spawn
    for proc, tree, kid in made:
        try:
            proctree.end_tree(tree, grace=1.0, kill_grace=2.0)
        except proctree.Unverified:                          # pragma: no cover
            pass
        _kill(kid)
        if proc.poll() is None:                              # pragma: no cover
            proc.kill()


# --- what the tree is -------------------------------------------------------

def test_a_spawned_session_owns_what_it_starts(leader):
    """The descendant is inside the tree, not merely a child of the leader."""
    proc, tree, kid = leader(linger=60)

    assert tree.verified, "the spawned tree has no identity"
    assert tree.kind in ("job", "pgid"), f"no tree primitive: {tree}"
    assert proctree.alive_tree(tree) is True
    if proctree._WINDOWS:
        known, held = proctree._job_active(str(tree.group))
        assert known and held >= 2, \
            f"the job holds {held}: the descendant is not in it"
    else:
        import os

        assert os.getpgid(kid) == tree.group, "the descendant is in another group"


def test_the_control(leader):
    """Without the ownership, the descendant does survive its parent.

    Every other test in this file asserts that something is gone. This one
    proves the scenario can actually leave a process behind, so "it was gone"
    means the ownership worked rather than that the helper was harmless.
    """
    plain = subprocess.Popen([sys.executable, "-c", LEADER.format(linger=0.4)],
                             stdout=subprocess.PIPE, text=True)
    orphan = int(plain.stdout.readline().strip())
    try:
        plain.wait(timeout=30)
        # It outlived its parent by long enough for any of the checks below.
        assert not _gone(orphan, deadline=1.0), \
            "the helper did not reproduce the orphan, so this file proves nothing"
    finally:
        _kill(orphan)


def test_the_tree_survives_its_leader(leader):
    """The two platforms answer this differently, and both answers are the
    point.

    Windows: the job object is closed by the leader's own exit, and a job set
    to kill on close takes the rest of the tree with it. A descendant *cannot*
    outlive the session - there is nothing to detect afterwards.

    POSIX: the descendant is reparented and lives on, so what matters is that
    the group id outlives the leader and the group still reads as occupied.
    That is what makes the survivor findable at all; before this, "the leader
    is gone" was the whole of the answer.
    """
    proc, tree, kid = leader(linger=0.4)
    proc.wait(timeout=30)

    if proctree._WINDOWS:
        assert _gone(kid), "the browser outlived the launcher that started it"
        assert proctree.prove_empty(tree, deadline=5.0) is True
        return

    assert proctree.alive(kid) is True, "the fixture did not outlive its leader"
    assert proctree.alive_tree(tree) is True, \
        "the tree read as empty while a process it started was running"
    assert proctree.owned(tree) is True, "the group handle was lost with the leader"
    assert proctree.end_tree(tree, grace=2.0, kill_grace=3.0) is True
    assert _gone(kid), "the survivor was not killed"


def test_a_group_of_corpses_is_not_a_running_browser(leader):
    """POSIX only, and it is why `killpg(pgid, 0)` is not the whole answer: a
    process that exited but has not been reaped stays in its group and goes on
    answering that probe. A tree of zombies read as a live browser, and no stop
    could ever prove otherwise however many signals it sent."""
    if proctree._WINDOWS:
        pytest.skip("POSIX process groups")

    proc, tree = proctree.spawn([sys.executable, "-c", "pass"])
    while proc.poll() is None:                               # pragma: no cover
        time.sleep(0.02)
    # Deliberately not reaped: `proc` is still ours and never waited on.
    assert proctree.alive_tree(tree) is False, "a zombie read as a live tree"
    assert proctree.end_tree(tree, grace=1.0, kill_grace=1.0) is True


def test_ending_a_session_ends_the_whole_tree(leader):
    proc, tree, kid = leader(linger=60)

    code = proctree.end_session(proc, tree, grace=2.0, kill_grace=3.0)

    assert code is not None, "end_session did not return the leader's exit code"
    assert _gone(kid), "the descendant survived the session being ended"
    assert proctree.alive_tree(tree) is False


def test_the_tree_is_not_the_engines_own_process_group():
    """On POSIX a plain `Popen` leaves the child in *this* process's group, so
    a group signal aimed at the browser would have taken the engine with it."""
    if proctree._WINDOWS:
        pytest.skip("POSIX process groups")
    import os

    proc, tree = proctree.spawn([sys.executable, "-c", "import sys; sys.stdin.read()"],
                                stdin=subprocess.PIPE)
    try:
        assert tree.group != os.getpgrp(), "the session shares the engine's group"
        assert os.getpgid(proc.pid) == tree.group
    finally:
        proc.stdin.close()
        proc.wait(timeout=30)


# --- what may be signalled --------------------------------------------------

def test_an_unverified_tree_is_never_signalled():
    with pytest.raises(proctree.Unverified):
        proctree.signal_tree(proctree.Tree(pid=999_999))


def test_a_job_with_no_verified_identity_refuses_to_stop(tmp_path, monkeypatch):
    """A record from before this release has a pid and no tree, and its token
    may be missing too. The pid alone is not an identity - the OS reuses them -
    so `jobs stop` must refuse rather than signal whatever holds that number
    now. A missing identity is the reason to refuse, not permission to guess.
    """
    import os

    signalled = []
    monkeypatch.setattr(proctree, "signal_tree",
                        lambda *a, **kw: signalled.append(a) or True)

    # The shape 0.24 wrote, pointed at a pid that is definitely alive: this one.
    d = tmp_path / "jobs"
    d.mkdir(parents=True, exist_ok=True)
    (d / "old.json").write_text(
        '{"id": "old", "command": "launch", "pid": %d, "pidToken": null,'
        ' "started": 0, "log": ""}' % os.getpid(), encoding="utf-8")

    rec = jobs.get(tmp_path, "old")
    assert rec["running"] is True, "the fixture is not a running job"
    with pytest.raises(RuntimeError, match="verified identity"):
        jobs.stop(tmp_path, "old")
    assert signalled == [], "a job with no verified identity was signalled"
    assert jobs.get(tmp_path, "old")["state"] != "stopped"


def test_a_job_records_the_tree_beside_the_pid(tmp_path):
    rec = jobs.spawn(tmp_path, [sys.executable, "-c", "import sys; sys.stdin.read()"],
                     command="launch", profile=None)
    try:
        assert rec["verified"] is True, "a spawned job was left unidentified"
        assert rec["tree"]["kind"] in ("job", "pgid"), rec["tree"]
        assert rec["tree"]["group"], "no group handle was recorded"
        assert proctree.Tree.from_record(rec).verified is True
    finally:
        jobs.stop(tmp_path, rec["id"])


def test_a_child_that_exits_before_it_can_be_named_records_its_outcome(tmp_path,
                                                                       monkeypatch):
    """A child that is already gone when `spawn` returns has an outcome that is
    knowable right now and may never be again - it can exit before its own exit
    hook runs - so it is recorded there and then. Nothing later has any reason
    to reach for a pid that may since have been handed to somebody else.
    """
    def gone_before_we_looked(proc, deadline=2.0):
        proc.wait(timeout=30)
        return None

    monkeypatch.setattr(proctree, "identify", gone_before_we_looked)

    rec = jobs.spawn(tmp_path, [sys.executable, "-c", "raise SystemExit(3)"],
                     command="warmup", profile=None)

    after = jobs.get(tmp_path, rec["id"])
    assert after["running"] is False, "a job that had already exited looked alive"
    assert after["state"] == "failed", after["state"]
    assert after["exit"] is not None, "the outcome was knowable and was not recorded"


def test_a_job_that_could_not_be_identified_refuses_to_stop(tmp_path, monkeypatch):
    """The other half: it is still running, and unnamed. Nothing may be sent to
    it, because the pid is all there is and pids get reused.

    POSIX only, and that is the point of the asymmetry: a Windows session is in
    a job object, which is an identity nothing else can hold, so this state
    cannot be reached there at all - `spawn` refuses to start a session it
    cannot put in one. A process group is only as good as the token that proves
    the leader was ours.
    """
    if proctree._WINDOWS:
        pytest.skip("a Windows session always has a job object to identify it")
    monkeypatch.setattr(proctree, "identify", lambda proc, deadline=2.0: None)
    signalled = []
    monkeypatch.setattr(proctree, "signal_tree",
                        lambda *a, **kw: signalled.append(a) or True)

    rec = jobs.spawn(tmp_path, [sys.executable, "-c", "import sys; sys.stdin.read()"],
                     command="launch", profile=None)
    try:
        assert rec["verified"] is False
        with pytest.raises(RuntimeError, match="verified identity"):
            jobs.stop(tmp_path, rec["id"])
        assert signalled == [], "an unidentified job was signalled"
    finally:
        monkeypatch.undo()
        _kill(rec["pid"])


def test_a_finished_job_that_left_something_running_is_not_called_finished(tmp_path,
                                                                          monkeypatch):
    """The leader's own exit hook says "done". It is not done while the browser
    it started is still up, holding the profile directory the next launch wants.
    """
    d = tmp_path / "jobs"
    d.mkdir(parents=True, exist_ok=True)
    # Written rather than spawned: the leader has to be *gone*, and a real one
    # that this process spawned and never reaped is a zombie on POSIX, which is
    # a different state with its own test above.
    (d / "j1.json").write_text(
        '{"id": "j1", "command": "launch", "profile": "p1", "pid": 999999,'
        ' "pidToken": 1, "started": 0, "log": "",'
        ' "tree": {"kind": "pgid", "group": 999999}}', encoding="utf-8")
    (d / "j1.exit").write_text('{"state": "done", "exit": 0}', encoding="utf-8")

    monkeypatch.setattr(proctree, "alive_tree", lambda tree: True)
    monkeypatch.setattr(proctree, "owned", lambda tree: True)

    after = jobs.get(tmp_path, "j1")
    assert after["orphaned"] is True, "a surviving browser was not reported"
    assert after["running"] is True, "the record would have been pruned away"


# --- ctypes signatures are part of the correctness, not decoration ----------

def test_every_kernel32_call_is_bound_and_the_library_is_one_object():
    """An unbound ctypes call truncates a 64-bit Windows handle to `c_int`.

    `OpenProcess` returns a pointer-sized HANDLE; unbound, ctypes takes it as a
    32-bit signed int and sign-extends it, and the `CloseHandle` or
    `WaitForSingleObject` that follows then acts on whatever that value names.
    It looked like it worked because handle values are small - luck, not a
    guarantee - and the two call sites that reached for the *unbound*
    `ctypes.windll.kernel32` were `alive()` and `started_at()`, which is the
    liveness check the entire durable claim rests on.

    The second half matters as much: `WinDLL` returns a NEW object every call
    and ctypes caches signatures per object, so binding a throwaway instance
    leaves every other call site unbound. One cached library, or none of this
    holds.
    """
    from pathlib import Path

    from numera import proctree

    source = Path(proctree.__file__).read_text(encoding="utf-8")
    assert "ctypes.windll" not in source.replace("`ctypes.windll.kernel32`", ""), \
        "a call site is back on the unbound library"

    if not proctree._WINDOWS:
        pytest.skip("kernel32 exists only on Windows")

    import ctypes.wintypes as wt

    k32 = proctree._kernel32()
    assert proctree._kernel32() is k32, "a fresh WinDLL per call binds nothing"
    for name in ("CreateJobObjectW", "OpenJobObjectW", "OpenProcess",
                 "CloseHandle", "WaitForSingleObject", "GetProcessTimes",
                 "AssignProcessToJobObject", "TerminateJobObject",
                 "SetInformationJobObject", "QueryInformationJobObject"):
        call = getattr(k32, name)
        # `argtypes is None` is ctypes' "never bound", and every handle this
        # module passes in goes through one of these.
        assert call.argtypes is not None, f"{name} takes ctypes' default types"
    # The three that hand a handle *back* are where truncation actually bites;
    # a `BOOL` return is `c_long` either way, so only these can be asserted on.
    for name in ("CreateJobObjectW", "OpenJobObjectW", "OpenProcess"):
        assert getattr(k32, name).restype is wt.HANDLE,             f"{name} returns ctypes' 32-bit default, so a handle is truncated"
