"""A profile stays owned until its browser is gone, not until its launcher is.

The session lease is an OS file lock, and the kernel releases it when the
holder dies. 0.25 taught the *local* owner that a browser can outlive its
launcher, but nothing a different process consults before opening the same
profile - so the sequence that corrupts a Chromium directory was still
reachable, just by outliving the lock rather than by racing it:

1. the launcher dies (crash, `kill -9`, a lid closing);
2. its Chromium survives in the process group;
3. the lease is released by the OS;
4. another Numera process takes it and opens the same `user-data/<id>`.

So ownership is written down as well - `numera/sessions.py` - naming the tree,
not just the process. Every browser open goes through `drivers.open_session`,
which is where the claim is taken, so the CLI, `--detach`, the API, the
scheduler and any Python caller get the same answer.

The two platforms answer differently once the leader is dead, and both answers
are correct:

* POSIX has no way to make descendants die with their parent, so the group
  outlives the leader and the claim is what keeps the profile owned;
* Windows sessions are in a job object set to kill on close, so the descendants
  are already gone - the state cannot arise, and the claim is recovered.

`test_the_control` keeps the POSIX half honest.
"""

import json
import os
import subprocess
import sys
import textwrap
import time
from pathlib import Path

import pytest

from _rendezvous import Rendezvous, finish

from numera import archive, generate, proctree, sessions
from numera.models import Profile
from numera.paths import SessionBusy
from numera.repo import repo
from numera.store import ProfileStore

ENGINE = str(Path(__file__).resolve().parents[1])

# A session owner: takes the profile through the real gate, starts something
# that outlives it the way a browser does, and then waits to be killed.
OWNER = """
import subprocess, sys, time
sys.path.insert(0, {engine!r})
from numera.drivers import open_session
from numera.store import ProfileStore

store = ProfileStore({root!r})
prof = store.get({pid!r})
with open_session(prof, store) as prof:
    kid = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(120)"])
    print(kid.pid, flush=True)
    time.sleep(300)
"""


@pytest.fixture
def profile(tmp_path):
    return repo(ProfileStore(tmp_path)).create(
        Profile(name="owned", fingerprint=generate(seed=1)))


def _owner(tmp_path, pid):
    """Start a session owner in its own process group, as a shell would."""
    script = OWNER.format(engine=ENGINE, root=str(tmp_path), pid=pid)
    proc = subprocess.Popen([sys.executable, "-c", textwrap.dedent(script)],
                            stdout=subprocess.PIPE, text=True,
                            **({} if proctree._WINDOWS else {"start_new_session": True}))
    kid = int(proc.stdout.readline().strip())
    return proc, kid


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


# --- the claim outlives the process that made it ---------------------------

def test_a_killed_launcher_does_not_release_the_profile(tmp_path, profile):
    """The whole point, and it is stated with a hard kill because a graceful
    exit was never the problem: nothing runs on the way out of a `kill -9`."""
    owner, kid = _owner(tmp_path, profile.id)
    try:
        assert sessions.busy(tmp_path, profile.id), "the claim was never written"
        _kill(owner.pid)
        owner.wait(timeout=30)

        if proctree._WINDOWS:
            # The job object was set to kill on close and the leader held the
            # only handle, so the descendant is already gone and the claim is
            # recoverable. There is nothing left to refuse.
            assert _gone(kid), "the descendant outlived the job that owned it"
            assert sessions.busy(tmp_path, profile.id) is False
            return

        assert proctree.alive(kid) is True, "the descendant did not outlive its leader"
        assert sessions.busy(tmp_path, profile.id) is True, \
            "the profile read as free while the browser it owns is running"

        store = ProfileStore(tmp_path)
        from numera.drivers import open_session

        with pytest.raises(SessionBusy):
            with open_session(store.get(profile.id), store):
                pass                                         # pragma: no cover

        # Only once the survivor is gone may the profile be taken again.
        _kill(kid)
        assert _gone(kid)
        assert sessions.busy(tmp_path, profile.id) is False, \
            "the claim was not recovered once its tree was empty"
        with open_session(store.get(profile.id), store):
            pass
    finally:
        _kill(kid)
        if owner.poll() is None:                             # pragma: no cover
            _kill(owner.pid)


def test_the_control(tmp_path, profile):
    """Without the claim, a killed launcher leaves the lease free.

    The assertion above is "refused"; this shows the OS lock alone would not
    have refused, so the refusal comes from the claim and not from something
    that was already true.
    """
    owner, kid = _owner(tmp_path, profile.id)
    try:
        _kill(owner.pid)
        owner.wait(timeout=30)
        from numera.paths import session_lease

        # Bounded, not instantaneous: `wait()` reaps the process, and the OS
        # releasing its file handles is a moment later. Asserting on the
        # instant made this fail under load for a reason that has nothing to
        # do with what it is testing.
        end = time.monotonic() + 15.0
        while session_lease(tmp_path, profile.id).held_by_other_process():
            assert time.monotonic() < end, \
                "the lease outlived its holder, so this file proves nothing"
            time.sleep(0.05)
    finally:
        _kill(kid)


def test_a_claim_this_machine_cannot_check_is_not_free(tmp_path, profile):
    """A data root shared with another machine, or a record with no identity in
    it. Unknown is the one answer that must never read as available - it is the
    only case where guessing costs a corrupted profile."""
    tree = proctree.Tree(pid=os.getpid(), token=proctree.started_at(os.getpid()),
                         group=os.getpid(), kind="pgid")
    sessions.take(tmp_path, profile.id, tree, "a browser session")
    path = tmp_path / "sessions" / f"{profile.id}.json"
    record = json.loads(path.read_text(encoding="utf-8"))
    record["host"] = "a-different-machine"
    path.write_text(json.dumps(record), encoding="utf-8")

    assert sessions.state(record) == "unknown"
    assert sessions.busy(tmp_path, profile.id) is True
    assert sessions.clear(tmp_path, profile.id) is False, \
        "a claim that could not be checked was cleared"

    store = ProfileStore(tmp_path)
    from numera.drivers import open_session

    with pytest.raises(SessionBusy):
        with open_session(store.get(profile.id), store):
            pass                                             # pragma: no cover


def test_a_deleted_profile_cannot_be_deleted_while_its_browser_runs(tmp_path, profile):
    """`user-data/<id>` is renamed by a delete, and the lease that used to
    guard that is free the moment the launcher dies."""
    tree = proctree.Tree(pid=os.getpid(), token=proctree.started_at(os.getpid()),
                         group=os.getpid(), kind="pgid")
    sessions.take(tmp_path, profile.id, tree, "a browser session")
    path = tmp_path / "sessions" / f"{profile.id}.json"
    record = json.loads(path.read_text(encoding="utf-8"))
    record["pid"], record["host"] = 424242, "a-different-machine"
    path.write_text(json.dumps(record), encoding="utf-8")

    with pytest.raises(SessionBusy):
        repo(ProfileStore(tmp_path)).delete(profile.id)
    assert ProfileStore(tmp_path).get(profile.id) is not None, "it deleted anyway"


def test_stop_reports_a_session_whose_launcher_is_gone(tmp_path, profile, capsys):
    """`numera stop` said "nothing running" for exactly the state that most
    needs reporting: no lease, no job record, and a browser still open."""
    if proctree._WINDOWS:
        pytest.skip("a Windows session cannot outlive its job")
    owner, kid = _owner(tmp_path, profile.id)
    try:
        _kill(owner.pid)
        owner.wait(timeout=30)
        assert proctree.alive(kid) is True

        from numera import cli

        code = cli.main(["--data-dir", str(tmp_path), "stop", "owned", "--json"])
        out = capsys.readouterr().out
        assert code == 1, "a profile with a live browser reported nothing running"
        assert "another process" in out, out
    finally:
        _kill(kid)


# --- ownership that cannot be established is not started -------------------

def test_a_session_that_cannot_be_owned_is_not_spawned(tmp_path, monkeypatch):
    """Tree ownership is a correctness primitive, not an optimisation. The
    first version degraded to leader-only when the job object could not be
    made, which is the original defect reached by another route."""
    if not proctree._WINDOWS:
        pytest.skip("only Windows can fail to create the tree primitive")
    monkeypatch.setattr(proctree, "_job_open", lambda name: None)

    with pytest.raises(proctree.TreeUnavailable):
        proctree.spawn([sys.executable, "-c", "import time; time.sleep(60)"])


def test_a_child_that_cannot_be_put_in_its_job_is_stopped(tmp_path, monkeypatch):
    """And it is not left running: a process outside the tree that started a
    browser is exactly what nothing could account for afterwards."""
    if not proctree._WINDOWS:
        pytest.skip("only Windows assigns a job object")
    started = []
    real = proctree._job_assign
    monkeypatch.setattr(proctree, "_job_assign",
                        lambda job, proc: started.append(proc) and False)

    with pytest.raises(proctree.TreeUnavailable):
        proctree.spawn([sys.executable, "-c", "import time; time.sleep(60)"])
    assert started, "the fixture never reached the assignment"
    assert _gone(started[0].pid), "the child was left running"
    assert real is not None


def test_a_job_that_cannot_be_queried_is_not_empty(monkeypatch):
    """`_job_active` reported "no such job" and "the query failed" as the same
    answer, and `alive_tree` read both as empty - so one failed query declared
    a live browser tree gone."""
    tree = proctree.Tree(pid=1, token=1, group="numera-tree-whatever", kind="job")
    monkeypatch.setattr(proctree, "_job_active", lambda name: (False, 0))
    assert proctree.alive_tree(tree) is True, "unknown was read as empty"
    monkeypatch.setattr(proctree, "_job_active", lambda name: (True, 0))
    assert proctree.alive_tree(tree) is False
    monkeypatch.setattr(proctree, "_job_active", lambda name: (False, 0))
    assert proctree.others_alive(tree, besides=os.getpid()) is True


def test_a_spawned_session_tells_its_child_which_tree_it_is_in():
    """A Windows child cannot ask which job it is in, and it needs the name to
    write its own claim. POSIX children derive it from their process group."""
    proc, tree = proctree.spawn(
        [sys.executable, "-c",
         "import json, os; print(json.dumps(os.environ.get('NUMERA_TREE')))"],
        stdout=subprocess.PIPE, text=True)
    try:
        passed = json.loads(proc.stdout.readline().strip())
        if proctree._WINDOWS:
            assert json.loads(passed)["group"] == tree.group
        # POSIX needs nothing in the environment: the child is a session leader.
    finally:
        proc.wait(timeout=30)


def test_the_rendezvous_helper_is_untouched():
    """A guard, not a test of the feature: the barrier these tests share is the
    one thing whose failure would make every result here meaningless."""
    assert Rendezvous and finish


# --- one profile, one session-specific owner -------------------------------

BARE_OWNER = """
import subprocess, sys, time
sys.path.insert(0, {engine!r})
from numera.drivers import open_session
from numera.store import ProfileStore

store = ProfileStore({root!r})
prof = store.get({pid!r})
with open_session(prof, store) as prof:
    kid = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(120)"])
    print(kid.pid, flush=True)
    time.sleep(300)
"""


def test_a_launcher_that_never_led_a_group_still_owns_its_browser(tmp_path, profile):
    """The 0.26 compromise, and the window it left open.

    POSIX only used the process group when the process *already* led one, which
    an interactive shell provides and a script with job control off, cron,
    systemd or an application importing the Python API does not. Those got a
    claim naming the leader alone - so killing the launcher left its Chromium
    running with the claim reading free, which is the corruption the claim
    exists to prevent.

    The owner here is started deliberately *without* `start_new_session`, so it
    is not a group leader when it starts.
    """
    if proctree._WINDOWS:
        pytest.skip("a Windows session is in a job object however it was started")

    script = BARE_OWNER.format(engine=ENGINE, root=str(tmp_path), pid=profile.id)
    owner = subprocess.Popen([sys.executable, "-c", textwrap.dedent(script)],
                             stdout=subprocess.PIPE, text=True)
    kid = int(owner.stdout.readline().strip())
    try:
        record = sessions.read(tmp_path, profile.id)
        assert record["tree"]["kind"] == "pgid", \
            f"a browser session was given a leader-only claim: {record['tree']}"
        assert record["tree"]["group"] not in (None, os.getpgrp()), \
            "the claim names the group this test already shares"

        _kill(owner.pid)
        owner.wait(timeout=30)
        assert proctree.alive(kid) is True, "the descendant did not outlive its leader"
        assert sessions.busy(tmp_path, profile.id) is True, \
            "a killed non-leader launcher released the profile"

        store = ProfileStore(tmp_path)
        from numera.drivers import open_session

        with pytest.raises(SessionBusy):
            with open_session(store.get(profile.id), store):
                pass                                         # pragma: no cover

        _kill(kid)
        assert _gone(kid)
        assert sessions.busy(tmp_path, profile.id) is False
        with open_session(store.get(profile.id), store):
            pass
    finally:
        _kill(kid)
        if owner.poll() is None:                             # pragma: no cover
            _kill(owner.pid)


def test_two_profiles_in_one_process_do_not_hold_each_other(tmp_path):
    """The claim belongs to the session, not to the process.

    Both sessions name the same process tree, so closing the first used to find
    the second's browser in it and keep the first profile owned - sometimes
    until the host exited. An unrelated child of the host did the same.
    """
    from numera.drivers import open_session

    store = ProfileStore(tmp_path)
    a = repo(store).create(Profile(name="a", fingerprint=generate(seed=1)))
    b = repo(store).create(Profile(name="b", fingerprint=generate(seed=2)))
    stranger = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    try:
        with open_session(store.get(a.id), store):
            with open_session(store.get(b.id), store):
                assert sessions.held(tmp_path, a.id) and sessions.held(tmp_path, b.id)
            # B closed first this time round; A is still open and still owned.
            assert sessions.held(tmp_path, b.id) is False, "B's claim outlived B"
            assert sessions.held(tmp_path, a.id) is True

        assert sessions.held(tmp_path, a.id) is False, \
            "A's claim was kept by something that was never A's browser"
        assert sessions.read(tmp_path, a.id) is None
        assert sessions.read(tmp_path, b.id) is None
    finally:
        stranger.kill()
        stranger.wait(timeout=30)


def test_each_session_writes_its_own_identity(tmp_path, profile):
    """Two claims from one process must not be interchangeable."""
    tree = proctree.own_tree()
    first = sessions.take(tmp_path, profile.id, tree, "a browser session")
    assert first["session"], "a claim with no session identity"
    assert sessions.release(tmp_path, profile.id, "some-other-session") is False, \
        "a claim was removed by a session that did not own it"
    assert sessions.release(tmp_path, profile.id, first["session"]) is True


# --- our own claim is still a claim ----------------------------------------

def test_a_destructive_operation_refuses_our_own_standing_claim(tmp_path, profile):
    """`busy()` answers "is somebody *else* using it", which is right for a
    listing and wrong for a delete: a session of ours whose browser would not
    close leaves the claim standing on purpose, and that is exactly when the
    directory must not be renamed away."""
    tree = proctree.own_tree()
    sessions.take(tmp_path, profile.id, tree, "a browser session")

    assert sessions.busy(tmp_path, profile.id) is False, "the status answer changed"
    assert sessions.held(tmp_path, profile.id) is True

    store = ProfileStore(tmp_path)
    for attempt in (lambda: repo(store).delete(profile.id),
                    lambda: store.delete_session(profile.id),
                    lambda: archive.backup(store, profile.id, tmp_path / "b.zip")):
        with pytest.raises(SessionBusy):
            attempt()
    assert store.get(profile.id) is not None, "it deleted anyway"


def test_the_public_session_delete_honours_the_durable_claim(tmp_path, profile):
    """0.25 gave this entry point the lease; the lease is exactly what a killed
    launcher does not leave behind."""
    store = ProfileStore(tmp_path)
    live = store.user_data_path(profile.id)
    (live / "Cookies").write_text("live", encoding="utf-8")
    record = sessions.take(tmp_path, profile.id, proctree.own_tree(), "a browser session")

    with pytest.raises(SessionBusy):
        store.delete_session(profile.id)
    assert (live / "Cookies").read_text(encoding="utf-8") == "live"
    assert not list(live.parent.glob("*.deleting-*")), "a tombstone was left behind"

    sessions.release(tmp_path, profile.id, record["session"])
    store.delete_session(profile.id)
    assert not live.exists(), "the session survived a delete nothing was holding"


# --- what a failed session leaves behind -----------------------------------
#
# The claim used to be kept for every exception that reached `open_session`,
# because "the yield happened" was read as "a browser is running". It is true
# of a body that raises before opening anything, and of one that raises after
# its own `finally` closed the browser again - and both left a profile owned by
# nothing, until a person went and deleted the claim by hand. The backends say
# what actually happened: `_opened` when a browser is up, `_closed` when it is
# down again and its driver is stopped.

def test_a_failure_before_the_browser_leaves_nothing_owned(tmp_path, profile):
    """Nothing was opened, so there is nothing to keep the profile for."""
    from numera.drivers import open_session

    store = ProfileStore(tmp_path)
    with pytest.raises(RuntimeError, match="failed before launch"):
        with open_session(store.get(profile.id), store):
            raise RuntimeError("failed before launch")

    assert sessions.read(tmp_path, profile.id) is None, \
        "a claim was left for a browser that was never started"
    with open_session(store.get(profile.id), store):
        pass                                    # and the profile is usable again


def test_a_browser_that_was_closed_again_does_not_keep_the_profile(tmp_path, profile):
    """The `session()` shape: user code raises, the `finally` closes the browser
    successfully, and the exception carries on out. Nothing is left running."""
    from numera.drivers import _closed, _opened, open_session

    store = ProfileStore(tmp_path)
    with pytest.raises(ValueError, match="the caller's own bug"):
        with open_session(store.get(profile.id), store):
            _opened()
            try:
                raise ValueError("the caller's own bug")
            finally:
                _closed()                       # the browser and its driver, down

    assert sessions.read(tmp_path, profile.id) is None, \
        "a closed browser kept the profile owned"


def test_a_browser_that_would_not_close_keeps_the_profile(tmp_path, profile):
    """The state the claim exists for. `_closed` is reached only *after* a close
    returns, so a close that raised never says the browser is gone."""
    from numera.drivers import _opened, open_session

    store = ProfileStore(tmp_path)
    with pytest.raises(RuntimeError, match="close failed"):
        with open_session(store.get(profile.id), store):
            _opened()
            raise RuntimeError("close failed")

    assert sessions.held(tmp_path, profile.id) is True, \
        "a browser that may still be running lost its claim"
    with pytest.raises(SessionBusy):
        with open_session(store.get(profile.id), store):
            pass                                             # pragma: no cover


def test_an_inner_session_does_not_answer_for_the_one_around_it(tmp_path):
    """One thread, two sessions: B's failure must not speak for A's browser.

    A single per-thread flag would have been overwritten by B - and A, whose
    browser is open, would have released its claim on the way out."""
    from numera.drivers import _opened, open_session

    store = ProfileStore(tmp_path)
    a = repo(store).create(Profile(name="a", fingerprint=generate(seed=1)))
    b = repo(store).create(Profile(name="b", fingerprint=generate(seed=2)))

    with pytest.raises(RuntimeError, match="A's browser is stuck"):
        with open_session(store.get(a.id), store):
            _opened()                                   # A has a browser
            with pytest.raises(RuntimeError, match="B never started"):
                with open_session(store.get(b.id), store):
                    raise RuntimeError("B never started")
            assert sessions.read(tmp_path, b.id) is None, "B kept a claim"
            raise RuntimeError("A's browser is stuck")

    assert sessions.held(tmp_path, a.id) is True, \
        "A's claim was released by what happened in B"
    sessions.release(tmp_path, a.id, sessions.read(tmp_path, a.id)["session"])


# --- a claim that cannot be read is not an absent one ----------------------
#
# `read()` answered `None` for both "no claim file" and "a claim file I cannot
# parse", and `take()` reads `None` as an empty slot - so a truncated or
# hand-edited claim, which may be the only surviving record of a browser whose
# launcher died, was simply overwritten by the next opener. Three states now,
# and the third fails closed.

BAD_CLAIMS = {
    "truncated json": '{"session": "abc", "pid": 1',
    "a list, not a record": '[]',
    "a pid that is not a number": '{"session": "a", "pid": "many", "tree": {"kind": "pgid", "group": 3}}',
    "a tree that is not an object": '{"session": "a", "pid": 3, "tree": "nonsense"}',
    "a kind that is not a string": '{"session": "a", "pid": 3, "tree": {"kind": 42}}',
    "a pgid that is not a number": '{"session": "a", "pid": 3, "tree": {"kind": "pgid", "group": "x"}}',
    "a job whose name is a number": '{"session": "a", "pid": 3, "tree": {"kind": "job", "group": 7}}',
    "a session id that is not a string": '{"session": 17, "pid": 3, "tree": {"kind": "pgid", "group": 3}}',
    "no identity at all": '{"what": "a browser session"}',
}


def _write_claim(root, pid, text):
    path = sessions._path(root, pid)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


@pytest.mark.parametrize("shape", sorted(BAD_CLAIMS))
def test_a_claim_that_cannot_be_read_refuses_a_second_open(tmp_path, profile, shape):
    """The lease is free - its holder is gone - and the claim is all that is
    left. Every one of these must be a controlled refusal, not a traceback and
    not a free profile."""
    from numera.drivers import open_session

    _write_claim(tmp_path, profile.id, BAD_CLAIMS[shape])
    store = ProfileStore(tmp_path)

    assert sessions.held(tmp_path, profile.id) is True
    assert sessions.busy(tmp_path, profile.id) is True
    with pytest.raises(SessionBusy):
        with open_session(store.get(profile.id), store):
            pass                                             # pragma: no cover


def test_a_claim_that_cannot_be_read_refuses_every_destructive_operation(tmp_path, profile):
    """Delete, session delete, backup and restore all move `user-data/<id>` out
    from under whatever is using it. An unreadable claim is the state where
    nothing can say what that is."""
    store = ProfileStore(tmp_path)
    (store.user_data_path(profile.id) / "Cookies").write_text("live", encoding="utf-8")
    archive.backup(store, profile.id, tmp_path / "before.zip")
    _write_claim(tmp_path, profile.id, BAD_CLAIMS["truncated json"])

    for attempt in (lambda: repo(store).delete(profile.id),
                    lambda: store.delete_session(profile.id),
                    lambda: archive.backup(store, profile.id, tmp_path / "b.zip"),
                    lambda: archive.restore(store, tmp_path / "before.zip")):
        with pytest.raises(SessionBusy):
            attempt()

    assert store.get(profile.id) is not None, "it went ahead anyway"
    assert (store.user_data_path(profile.id) / "Cookies").read_text("utf-8") == "live"


def test_an_unreadable_claim_says_what_a_person_has_to_do(tmp_path, profile):
    """"Busy" with nothing to stop is a dead end; the message names the file."""
    _write_claim(tmp_path, profile.id, BAD_CLAIMS["truncated json"])

    with pytest.raises(SessionBusy) as refused:
        sessions.refuse_if_busy(tmp_path, profile.id)
    assert "sessions/<id>.json" in str(refused.value)


def test_a_claim_that_is_simply_absent_still_opens(tmp_path, profile):
    """The control: fail-closed on unreadable must not mean fail-closed on
    nothing at all."""
    from numera.drivers import open_session

    store = ProfileStore(tmp_path)
    assert sessions.read(tmp_path, profile.id) is None
    with open_session(store.get(profile.id), store):
        assert sessions.held(tmp_path, profile.id) is True
    assert sessions.read(tmp_path, profile.id) is None


# --- adopting a tree moves the host, so it is all or nothing ---------------

# A worker thread asking for a tree, with a terminal it cannot take. The
# descriptor and the foreground group are stand-ins - what matters is that
# `signal.signal` genuinely refuses off the main thread, which is where the
# host used to be left in a group it never asked for.
THREADED_ADOPTION = """
import os, sys, threading
sys.path.insert(0, {engine!r})
from numera import proctree

proctree._terminal = lambda: 0
os.tcgetpgrp = lambda fd: 4242          # a foreground group that is not ours

before = os.getpgrp()
outcome = []


def ask():
    try:
        proctree.own_tree()
        outcome.append("adopted")
    except proctree.TreeUnavailable:
        outcome.append("refused")
    except BaseException as err:
        outcome.append(type(err).__name__)


worker = threading.Thread(target=ask)
worker.start()
worker.join(30)
print(outcome[0] if outcome else "hung", before == os.getpgrp(), flush=True)
"""


def test_a_tree_that_cannot_be_taken_leaves_the_host_where_it_was(tmp_path):
    """`setpgrp()` moves the embedding process, and `signal.signal` - the next
    step - raises `ValueError` off the main thread. The group change used to
    stand: an application whose first browser open happened on a worker thread
    was left out of the terminal's foreground group, deaf to Ctrl+C, over an
    exception `own_tree` did not even recognise as a refusal."""
    if proctree._WINDOWS:
        pytest.skip("Windows adopts a job object and never touches signals")

    child = subprocess.run(
        [sys.executable, "-c", textwrap.dedent(
            THREADED_ADOPTION.format(engine=ENGINE))],
        capture_output=True, text=True, timeout=60)
    assert child.returncode == 0, child.stderr
    outcome, unmoved = child.stdout.split()

    assert outcome == "refused", \
        f"a tree that could not be taken was reported as {outcome}"
    assert unmoved == "True", "the host was left in a group it never asked for"
