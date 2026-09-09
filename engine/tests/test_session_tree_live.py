"""A real browser, in a tree that holds it and nothing else.

The claim named `proctree.own_tree()`, which is the whole host process. Every
in-process session in one program pointed at the same one, so it could never
answer the only question that makes a profile safe to reuse - *is this
session's browser gone*. Release fell back to asking the session how its body
ended, which is a report, not proof.

A browser is started by a driver process and everything that driver starts
inherits its group or job, so the tree is decided at that spawn. These are the
tests that need a real one, because the seam is inside Playwright: nothing
about it can be proved with a stub.
"""

import subprocess
import sys
import time

import pytest

from _live import requires_browser

from numera import generate, proctree, sessions
from numera.models import Profile
from numera.paths import SessionBusy
from numera.repo import repo
from numera.store import ProfileStore

requires_browser()


@pytest.fixture
def store(tmp_path):
    return ProfileStore(tmp_path)


def made(store, name, seed=1):
    return repo(store).create(Profile(name=name, fingerprint=generate(seed=seed)))


def _tree_of(record) -> proctree.Tree:
    return sessions._tree_of(record)


INSTALLED = sorted(name for name, ok in __import__(
    "numera.drivers", fromlist=["available"]).available().items() if ok)


@pytest.mark.parametrize("engine", INSTALLED)
def test_every_installed_engine_gets_a_tree_of_its_own(store, engine):
    """Each backend, launched for real, in a tree containing only its browser.

    The wiring assertion in `test_session_tree_mandatory.py` checks that every
    backend *calls* `_own_driver_tree`; this checks that doing so works, which
    is a different claim and only a real browser can settle it. Parametrised
    over what is installed rather than a fixed list, so an engine that is
    present is exercised and one that is absent is visibly absent from the
    report instead of silently unclaimed.

    Camoufox is included through `drivers.session()`, which is the path that
    knows it is launch-and-wait only.
    """
    from numera.drivers import session as browser_session

    prof = made(store, engine, seed=abs(hash(engine)) % 1000)
    host = proctree.own_tree()

    with browser_session(store.get(prof.id), store, engine=engine, headless=True):
        tree = _tree_of(sessions.read(store.root, prof.id))
        assert tree is not None, f"{engine} opened without a usable tree"
        assert str(tree.group) != str(host.group), \
            f"{engine} named the whole host process"
        assert proctree.alive_tree(tree), f"{engine}'s own tree read as empty"

    assert sessions.read(store.root, prof.id) is None, \
        f"{engine} did not give its claim up"


def test_a_session_names_a_tree_that_is_not_the_hosts(store):
    """The claim used to name the host job or process group verbatim."""
    from numera.drivers import session as browser_session

    prof = made(store, "alpha")
    host = proctree.own_tree()

    with browser_session(store.get(prof.id), store, headless=True):
        record = sessions.read(store.root, prof.id)
        assert record is not None and record is not sessions.UNREADABLE
        tree = _tree_of(record)
        assert tree is not None, f"the claim names no usable tree: {record}"
        assert tree.kind == host.kind, "a session tree of a different kind"
        assert tree.group != host.group, \
            "the session claim still names the whole host process"
        assert proctree.alive_tree(tree), "the session's own tree reads empty"

    assert sessions.read(store.root, prof.id) is None, \
        "a closed session did not give its claim up"


def test_a_browser_left_running_keeps_the_profile_after_a_clean_exit(store):
    """The brief's sequence, with a real browser as its own descendant.

    `keep_open=True` hands the handles back and closes nothing, so the session
    body returns normally over a browser that is still up - the exact shape a
    leaked driver produces. Nothing this process *reported* can tell that from
    a tidy exit; the tree can.
    """
    from numera.drivers import get, open_session

    prof = made(store, "alpha")
    handles = None
    try:
        with open_session(store.get(prof.id), store) as current:
            handles = get("playwright")(current, store, headless=True,
                                        keep_open=True)
        record = sessions.read(store.root, prof.id)
        assert record not in (None, sessions.UNREADABLE), \
            "a clean exit released a claim over a browser that is still open"
        tree = _tree_of(record)
        assert proctree.alive_tree(tree)

        fresh = ProfileStore(store.root)
        with pytest.raises(SessionBusy):
            with open_session(fresh.get(prof.id), fresh):
                pass                                         # pragma: no cover
        assert sessions.settle(store.root, prof.id) is False, \
            "a live browser tree was recovered as stale"

        pw, context, _page = handles
        context.close()
        if pw:
            pw.stop()
        handles = None

        end = time.monotonic() + 30
        while proctree.alive_tree(tree) and time.monotonic() < end:
            time.sleep(0.1)
        assert not proctree.alive_tree(tree), "the browser tree never emptied"

        assert sessions.settle(store.root, prof.id) is True
        with open_session(fresh.get(prof.id), fresh):
            pass
    finally:
        if handles:                                          # pragma: no cover
            pw, context, _page = handles
            try:
                context.close()
            finally:
                if pw:
                    pw.stop()


def test_an_unrelated_host_child_is_in_no_session_tree(store):
    """A child of the embedding application belongs to the host, not to any
    session - which is the whole reason the host tree could not be the claim."""
    from numera.drivers import session as browser_session

    prof = made(store, "alpha")
    stranger = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    try:
        with browser_session(store.get(prof.id), store, headless=True):
            tree = _tree_of(sessions.read(store.root, prof.id))
        assert stranger.poll() is None, "the stranger died on its own"
        assert not proctree.alive_tree(tree), \
            "an unrelated host child was counted inside the session's tree"
        assert sessions.read(store.root, prof.id) is None
    finally:
        stranger.kill()
        stranger.wait(timeout=30)
