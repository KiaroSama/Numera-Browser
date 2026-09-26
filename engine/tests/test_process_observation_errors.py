"""A failed group probe is not proof that the group is gone.

`alive_tree` asks `killpg(pgid, 0)` whether a process group still exists. Only
ESRCH (`ProcessLookupError`) says it does not. Every other error - EINVAL, EIO
and the like - was reported as "gone", so a stop could claim success for a
browser that never received a signal. Unknown is owned.

The probe is replaced for every case, so no real signal is ever sent, and
`_WINDOWS` is cleared so the POSIX branch runs on the Windows machine too.
"""
import errno

import pytest
from numera import proctree

PGID = 424242


@pytest.fixture
def probe(monkeypatch):
    calls = []
    outcome = {}

    def killpg(pgid, sig):
        calls.append((pgid, sig))
        if "error" in outcome:
            raise outcome["error"]

    monkeypatch.setattr(proctree, "_WINDOWS", False)
    monkeypatch.setattr(proctree.os, "killpg", killpg, raising=False)
    monkeypatch.setattr(proctree.os, "getpgrp", lambda: 1, raising=False)
    monkeypatch.setattr(proctree, "_group_is_running", lambda pgid: True)
    yield outcome
    assert calls == [(PGID, 0)]


@pytest.mark.parametrize("error, expected", [
    (OSError(errno.EINVAL, "injected"), True),
    (OSError(errno.EIO, "injected"), True),
    (ProcessLookupError(errno.ESRCH, "injected"), False),
    (PermissionError(errno.EPERM, "injected"), True),
    (None, True),
], ids=["EINVAL", "EIO", "ESRCH", "EPERM", "probe-ok"])
def test_only_esrch_proves_the_group_gone(probe, error, expected):
    if error is not None:
        probe["error"] = error
    tree = proctree.Tree(pid=PGID, token=1, group=PGID, kind="pgid")
    assert proctree.alive_tree(tree) is expected
