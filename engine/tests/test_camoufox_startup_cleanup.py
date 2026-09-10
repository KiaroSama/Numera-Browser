"""A successful backend enter must be unwound even if ownership is refused."""

import subprocess
import sys

import pytest

from numera import drivers, generate, proctree, sessions
from numera.models import Profile
from numera.paths import SessionBusy, session_lease
from numera.repo import repo
from numera.store import ProfileStore


@pytest.fixture
def startup(monkeypatch, tmp_path):
    store = ProfileStore(tmp_path)
    profile = repo(store).create(Profile(name="startup", fingerprint=generate(seed=1)))
    monkeypatch.setattr(drivers, "_playwright_program",
                        lambda: str(tmp_path / "unmatched-driver"))

    class Camoufox:
        child = None
        exits = []
        failure = None
        suppress = False
        state = None

        def __enter__(self):
            self.child = subprocess.Popen(
                [sys.executable, "-c", "import sys; sys.stdin.read(1)"],
                stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL)
            self.state = drivers._live()
            self.claim = sessions.read(tmp_path, profile.id)
            return self

        def __exit__(self, *exc):
            self.exits.append(exc)
            assert self.child.poll() is None
            assert sessions.read(tmp_path, profile.id) == self.claim
            with pytest.raises(SessionBusy):
                with session_lease(tmp_path, profile.id):
                    pytest.fail("the lease became reusable before cleanup")
            with pytest.raises(SessionBusy):
                sessions.take(tmp_path, profile.id, proctree.own_tree())
            if self.failure is not None:
                raise self.failure
            self.stop()
            return self.suppress

        def stop(self):
            if self.child is not None:
                self.child.stdin.close()
                try:
                    self.child.wait(timeout=5)
                finally:
                    if self.child.poll() is None:
                        self.child.kill()
                        self.child.wait(timeout=5)

    cam = Camoufox()
    monkeypatch.setattr(drivers, "_camoufox", lambda kwargs: cam)
    try:
        yield store, profile, cam
    finally:
        cam.stop()


@pytest.mark.parametrize("suppress", [False, True])
def test_ownership_refusal_exits_entered_backend_before_releasing(startup, suppress):
    store, profile, cam = startup
    cam.suppress = suppress
    with pytest.raises(proctree.TreeUnavailable, match="did not start in a tree") as caught:
        with drivers.open_session(profile, store):
            with drivers._camoufox_session({}):
                pytest.fail("a browser without an owned tree was yielded")

    assert len(cam.exits) == 1, "ownership refusal skipped the backend exit"
    assert cam.exits[0][1] is caught.value
    assert cam.child.poll() is not None, "refused startup left its child running"
    assert cam.state["closed"] is True
    assert sessions.read(store.root, profile.id) is None
    with session_lease(store.root, profile.id):
        claim = sessions.take(store.root, profile.id, proctree.own_tree())
        assert sessions.release(store.root, profile.id, claim["session"])


def test_ownership_refusal_and_failed_exit_preserve_both_failures_and_claim(startup):
    store, profile, cam = startup
    cam.failure = RuntimeError("backend cleanup refused")
    with pytest.raises(drivers.StartupRecoveryFailed) as caught:
        with drivers.open_session(profile, store):
            with drivers._camoufox_session({}):
                pytest.fail("a browser without an owned tree was yielded")

    assert len(cam.exits) == 1
    assert isinstance(caught.value.cause, proctree.TreeUnavailable)
    assert cam.exits[0][1] is caught.value.cause
    assert caught.value.failures == [("the browser", cam.failure)]
    assert cam.child.poll() is None
    assert cam.state["closed"] is False
    assert sessions.read(store.root, profile.id) == cam.claim
    with pytest.raises(SessionBusy):
        with drivers.open_session(profile, store):
            pytest.fail("failed cleanup made the profile reusable")
