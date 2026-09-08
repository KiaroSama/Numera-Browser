"""A stand-in for a spawned session process that answers the startup handshake.

`spawn_owner` waits for the child to say a browser is open on the profile
before it registers it as the owner, so a test double that only pretends to be
a process now hangs the parent until its deadline. This one writes the same
file a real child writes - see `numera.handshake` - which keeps the wait
deterministic and lets a test choose what the child claims: ready, refused for
a named reason, or dead without a word.

What it cannot stand in for is the readiness point itself: it answers because
the test told it to, not because a browser opened. That is proved against real
children in `test_browser_readiness.py`.
"""

import json
from pathlib import Path

from numera.handshake import READY_ENV


class FakeChild:
    """A `Popen` stand-in whose startup result the test decides.

    `ready=True` answers "I own the session", which is what a healthy launch
    does. `ready=False` reports a refusal with `kind` ("busy", "locked", ...).
    `ready=None` says nothing at all, which is a child that died before it
    could - pair it with a non-None `code` so the parent notices instead of
    waiting out its deadline.
    """

    #: No process, so no process tree. `proctree` reads this to decide what it
    #: owns, and a fake that claimed a pid would have it looking up - and
    #: possibly signalling - whatever real process holds that number.
    pid = 0

    def __init__(self, cli=None, env=None, ready: bool = True, kind: str = "failed",
                 error: str = "", code=None, **_ignored):
        self.cli = list(cli or [])
        self.returncode = code
        self.terminated = False
        path = (env or {}).get(READY_ENV)
        if path and ready is not None:
            Path(path).write_text(
                json.dumps({"ok": bool(ready), "error": error, "kind": kind}),
                encoding="utf-8")

    def poll(self):
        return self.returncode

    def terminate(self):
        self.terminated = True
        self.returncode = -15

    def wait(self, timeout=None):
        return self.returncode

    def kill(self):
        self.returncode = -9


def spawning(record: list, **kw):
    """A `Popen` replacement that appends every child it makes to `record`."""
    def popen(cli, env=None, **_kw):
        child = FakeChild(cli, env, **kw)
        record.append(child)
        return child
    return popen


def spawning_tree(record: list, **kw):
    """A `proctree.spawn` replacement that appends every child it makes.

    The seam moved up a layer in 0.26. A session spawn now fails closed when it
    cannot put the child in an OS tree, and a stand-in process cannot be put in
    one - it has no pid and no existence. Faking `subprocess.Popen` underneath
    `proctree.spawn` therefore tests the failure path rather than the caller,
    so the fake stands in for the spawn itself and hands back a tree that owns
    nothing, which is exactly what a fake process owns.
    """
    from numera.proctree import Tree

    def spawn(argv, **_kw):
        child = FakeChild(argv, _kw.get("env"), **kw)
        record.append(child)
        return child, Tree(pid=0, kind="")
    return spawn
