"""A browser-backed helper command may not hold a request open forever.

`routes_browser._run_cli` is the boundary six probes go through - cookies
export, proxy test, trust, tls, dns and align. Each one starts a *second*
process that opens a real browser, and the API thread then blocks on it.

It blocked with no deadline at all. A helper that wedges - a browser that never
finishes starting, a proxy that accepts the connection and never answers - held
the request thread until something outside killed it, and the browser it had
already started was not on the tracked session path, so nothing was accounting
for it either.

These are the two halves: the wait is bounded, and what the wait gives up on is
actually ended - not just let go of. `subprocess.run(timeout=...)` alone would
do the first and not the second: it kills the direct child, and the browser
three processes further down survives it. That is the same distinction the
whole `proctree` module exists for.
"""

import subprocess
import sys
import time

import pytest

from numera import proctree
from numera.server import routes_browser


def test_the_module_states_a_finite_deadline():
    """A named, inspectable budget rather than a number buried in a call."""
    assert isinstance(routes_browser.HELPER_TIMEOUT, (int, float))
    assert 0 < routes_browser.HELPER_TIMEOUT <= 1800, \
        "a helper budget outside this range is not a deadline, it is a hang"


def test_a_wedged_helper_is_ended_rather_than_abandoned(tmp_path):
    """The regression, with a real process tree.

    A stand-in helper starts a grandchild that outlives it, exactly as a probe
    that opened a browser would. `run_helper` must give up on the wait *and*
    leave nothing running.
    """
    script = tmp_path / "wedged.py"
    script.write_text(
        "import subprocess, sys, time\n"
        "subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(600)'])\n"
        "time.sleep(600)\n",
        encoding="utf-8")

    started = time.monotonic()
    with pytest.raises(routes_browser.HelperTimeout):
        routes_browser.run_helper([sys.executable, str(script)], timeout=5.0)
    waited = time.monotonic() - started

    assert waited < 60, f"the wait was not bounded: {waited:.1f}s"


def test_a_helper_that_finishes_is_untouched(tmp_path):
    """The control: the deadline must not cost a normal completion."""
    script = tmp_path / "quick.py"
    script.write_text("print('done')", encoding="utf-8")

    out = routes_browser.run_helper([sys.executable, str(script)], timeout=60.0)

    assert out.returncode == 0
    assert "done" in out.stdout


def test_a_failing_helper_still_returns_its_output(tmp_path):
    """`allow_fail` callers read stdout from a non-zero exit - a failing proxy
    test is a result, not a broken command. The deadline must not swallow it."""
    script = tmp_path / "unhealthy.py"
    script.write_text("import sys; print('{\"ok\": false}'); sys.exit(3)",
                      encoding="utf-8")

    out = routes_browser.run_helper([sys.executable, str(script)], timeout=60.0)

    assert out.returncode == 3
    assert '"ok": false' in out.stdout


def test_the_helper_runs_in_a_tree_that_can_be_ended(tmp_path, monkeypatch):
    """Ownership is the point: a helper spawned outside a tree cannot have its
    browser cleaned up, whatever the timeout says."""
    seen = {}
    real = proctree.spawn

    def watched(argv, **kwargs):
        proc, tree = real(argv, **kwargs)
        seen["tree"] = tree
        return proc, tree

    monkeypatch.setattr(routes_browser.proctree, "spawn", watched)
    script = tmp_path / "quick.py"
    script.write_text("print('ok')", encoding="utf-8")

    routes_browser.run_helper([sys.executable, str(script)], timeout=60.0)

    assert seen.get("tree") is not None, "the helper was not spawned into a tree"
    assert seen["tree"].kind in ("job", "pgid")
