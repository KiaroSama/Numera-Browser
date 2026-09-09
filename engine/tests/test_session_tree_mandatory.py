"""A session's own tree is mandatory, and it is recorded before a browser starts.

0.29 gave a session a tree of its own when it could, and fell back to the whole
host process when it could not - a package that moved, a job object that would
not be created, a backend with a different launcher. A correctness primitive
that becomes optional under any of those is not one: the fallback is exactly
the state it was built to end, reached by a different door. So every one of
those is now a refusal, and anything already started is stopped.

And the tree was recorded *after* the launch call returned. Playwright starts
its browser well after `start()`, so the window was invisible there - but
Camoufox opens one inside `__enter__`, and a launcher hard-killed in that
window left a running browser whose only durable record still named the host
tree. Nothing could find the group it was actually in. The claim is narrowed
from inside the spawn now, while the driver is still only a driver.

`cloakbrowser` is not installed on this machine, so that engine cannot be
exercised for real. What can be proved is that the seam it shares is generic
rather than Playwright-shaped, which is what
`test_the_seam_takes_any_named_launcher` is for.
"""

import subprocess
import sys
import textwrap
from contextlib import contextmanager
from pathlib import Path

import pytest

from numera import generate, proctree, sessions
from numera.models import Profile
from numera.paths import SessionBusy
from numera.repo import repo
from numera.store import ProfileStore

ENGINE = str(Path(__file__).resolve().parents[1])


@pytest.fixture
def profile(tmp_path):
    return repo(ProfileStore(tmp_path)).create(
        Profile(name="owned", fingerprint=generate(seed=1)))


def _kill(pid) -> None:
    subprocess.run(["taskkill", "/F", "/PID", str(pid)] if proctree._WINDOWS
                   else ["kill", "-9", str(pid)], capture_output=True)


def _gone(pid, deadline=15.0) -> bool:
    import time

    end = time.monotonic() + deadline
    while proctree.alive(pid):
        if time.monotonic() >= end:
            return False
        time.sleep(0.05)
    return True


#: A stand-in for a browser: a child that stays up until the test ends it. The
#: duration is a ceiling, never a wait - every test that starts one kills it,
#: and `_gone` polls to a deadline rather than sleeping for an outcome.
IDLE = "import time; time.sleep(600)"


def _sleeper():
    return subprocess.Popen([sys.executable, "-c", IDLE])


def _end(proc) -> bool:
    """Kill a child this process owns, and reap it - a zombie still answers."""
    _kill(proc.pid)
    try:
        proc.wait(timeout=30)
    except Exception:                # noqa: BLE001         # pragma: no cover
        pass
    return _gone(proc.pid)


# The owner blocks *inside* the narrow seam: it prints only once the tree has
# been recorded, and then waits to be killed with the launch call still in
# flight. That print is the barrier - there is nothing here to time.
NARROWED = """
import subprocess, sys, time
from contextlib import contextmanager
sys.path.insert(0, {engine!r})
from numera import drivers, proctree
from numera.drivers import open_session
from numera.store import ProfileStore

kid, tree = proctree.spawn([sys.executable, "-c", "import time; time.sleep(300)"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

@contextmanager
def capture(executable, on_tree=None):
    holder = proctree.Adopted()
    holder.tree = tree
    on_tree(tree)
    print(kid.pid, tree.group, flush=True)
    time.sleep(300)
    yield holder

drivers.proctree.spawning_own_tree = capture
store = ProfileStore({root!r})
with open_session(store.get({pid!r}), store) as prof:
    with drivers._own_driver_tree("a driver"):
        pass
"""


def test_the_claim_names_the_new_tree_before_the_launch_returns(tmp_path, profile):
    """The Camoufox interleaving, made deterministic and hard-killed."""
    script = NARROWED.format(engine=ENGINE, root=str(tmp_path), pid=profile.id)
    owner = subprocess.Popen([sys.executable, "-c", textwrap.dedent(script)],
                             stdout=subprocess.PIPE, text=True,
                             **({} if proctree._WINDOWS else {"start_new_session": True}))
    kid = None
    try:
        kid, group = owner.stdout.readline().split()
        kid = int(kid)

        record = sessions.read(tmp_path, profile.id)
        # Compared as text: a job name is a string and a pgid is a number, so
        # the raw comparison passed on Windows and failed on POSIX for a claim
        # that was in fact narrowed correctly.
        assert str(record["tree"]["group"]) == group, \
            "the claim did not name the new tree until the launch returned"

        _kill(owner.pid)
        owner.wait(timeout=30)
        assert proctree.alive(kid), "the stand-in browser died with its launcher"
        assert sessions.busy(tmp_path, profile.id) is True

        store = ProfileStore(tmp_path)
        from numera.drivers import open_session

        with pytest.raises(SessionBusy):
            with open_session(store.get(profile.id), store):
                pass                                         # pragma: no cover

        _kill(kid)
        assert _gone(kid)
        assert sessions.busy(tmp_path, profile.id) is False, \
            "the claim was not recoverable once its tree was empty"
    finally:
        if kid and proctree.alive(kid):                      # pragma: no cover
            _kill(kid)
        if owner.poll() is None:                             # pragma: no cover
            _kill(owner.pid)


def test_a_session_may_not_open_a_browser_it_cannot_own(tmp_path, profile):
    """A driver this process cannot name is a refusal, not a downgrade."""
    from numera import drivers

    store = ProfileStore(tmp_path)
    with pytest.raises(proctree.TreeUnavailable, match="could not be identified"):
        with drivers.open_session(store.get(profile.id), store):
            with drivers._own_driver_tree(None):
                pass                                         # pragma: no cover
    assert sessions.read(tmp_path, profile.id) is None, \
        "a refused launch left a claim behind"


def test_a_driver_that_started_in_no_tree_is_stopped(tmp_path, profile, monkeypatch):
    """The window was entered, the driver never matched, and it is running."""
    from numera import drivers

    started = _sleeper()

    @contextmanager
    def missed(executable, on_tree=None):
        holder = proctree.Adopted()
        holder.proc = started           # a driver ran; it just got no tree
        yield holder

    monkeypatch.setattr(drivers.proctree, "spawning_own_tree", missed)
    store = ProfileStore(tmp_path)
    try:
        with pytest.raises(proctree.TreeUnavailable, match="did not start in a tree"):
            with drivers.open_session(store.get(profile.id), store):
                with drivers._own_driver_tree("a driver"):
                    pass
        assert _gone(started.pid), "the unowned driver was left running"
        assert sessions.read(tmp_path, profile.id) is None
    finally:
        if started.poll() is None:                           # pragma: no cover
            _end(started)


def test_a_claim_that_cannot_be_narrowed_stops_the_driver(tmp_path, profile, monkeypatch):
    """A tree nothing recorded is worth no more than no tree at all."""
    from numera import drivers

    monkeypatch.setattr(sessions, "narrow", lambda *a, **k: False)
    store = ProfileStore(tmp_path)
    with pytest.raises(proctree.TreeUnavailable, match="could not be pointed at"):
        with drivers.open_session(store.get(profile.id), store):
            with drivers._own_driver_tree(sys.executable):
                subprocess.Popen([sys.executable, "-c", "pass"]).wait(timeout=30)


def test_the_seam_takes_any_named_launcher(tmp_path):
    """Generic by construction, which is what Cloak needs it to be."""
    seen = []
    host = proctree.own_tree()
    with proctree.spawning_own_tree(sys.executable, on_tree=seen.append) as adopted:
        child = _sleeper()
        assert seen, "on_tree did not fire before the spawn call returned"
        assert adopted.tree is not None

    try:
        assert adopted.tree is seen[0]
        assert adopted.tree.group != host.group, \
            "the synthetic launcher landed in the host tree"
        assert proctree.alive_tree(adopted.tree), "its own tree read as empty"
    finally:
        assert _end(child)
    assert not proctree.alive_tree(adopted.tree), "the tree never emptied"


def test_every_engine_starts_its_browser_inside_the_seam():
    """`cloakbrowser` is not installed here, so this is the only honest check.

    The engine cannot be exercised - the package is absent on this machine and
    on CI - so what is asserted is the wiring: each backend that launches a
    browser does it inside `_own_driver_tree`, which is what makes the tree
    mandatory. A backend added later without that line fails here rather than
    silently owning nothing.
    """
    from numera import drivers

    source = Path(drivers.__file__).read_text(encoding="utf-8")
    for backend, call in (("_launch_playwright_like", "sync_playwright().start()"),
                          ("_driver_cloak", "cloakbrowser.launch_persistent_context("),
                          ("_camoufox_session", "cam.__enter__()")):
        body = source.split(f"def {backend}(")[1].split("\ndef ")[0]
        assert call in body, f"{backend} no longer starts a browser the way this expects"
        # `rindex`, because these functions also *name* their launch call in a
        # comment explaining an older bug; the call itself is the last one.
        assert body.index("with _own_driver_tree(") < body.rindex(call), \
            f"{backend} starts its browser outside the tree seam"


def test_an_unnamed_launcher_captures_nothing(tmp_path):
    """No executable means no match, which the caller turns into a refusal."""
    with proctree.spawning_own_tree(None) as adopted:
        child = subprocess.Popen([sys.executable, "-c", "pass"])
        child.wait(timeout=30)
    assert adopted.tree is None


@pytest.mark.skipif(not proctree._WINDOWS, reason="job objects are Windows-only")
def test_a_driver_that_cannot_be_put_in_a_job_is_stopped(monkeypatch):
    """It is running and it cannot be owned, so it is ended - not kept.

    Driven at `_spawn_leader` rather than through the interception: that
    interception *replaces* `subprocess.Popen`, so a test holding a reference
    to the real one walks straight past the thing it means to exercise.
    """
    child = _sleeper()
    monkeypatch.setattr(proctree, "_job_assign", lambda job, proc: False)

    with pytest.raises(proctree.TreeUnavailable, match="could not be put in its job"):
        proctree._spawn_leader(lambda *a, **k: child, [sys.executable], (), {})

    assert _gone(child.pid), "an unownable driver was left running"


@pytest.mark.skipif(not proctree._WINDOWS, reason="job objects are Windows-only")
def test_a_job_that_cannot_be_created_refuses_before_anything_starts(monkeypatch):
    """Nothing is spawned at all, so there is nothing left to clean up."""
    monkeypatch.setattr(proctree, "_job_open", lambda name: None)
    started = []

    with pytest.raises(proctree.TreeUnavailable, match="job object to own"):
        proctree._spawn_leader(lambda *a, **k: started.append(a),
                               [sys.executable], (), {})

    assert not started, "the driver was started before the job was refused"
