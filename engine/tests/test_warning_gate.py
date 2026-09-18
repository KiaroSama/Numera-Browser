"""Does the leak gate actually fail a run? Planted leaks, and the answer.

`filterwarnings = ["error::ResourceWarning"]` had been in `pyproject.toml` for
releases, and the CI log of a **passing** Python 3.13 job carried unclosed
`BufferedReader`s and still-running subprocesses anyway. The configuration read
correctly and caught nothing.

The reason is that the warnings it was meant to catch never arrive as
`ResourceWarning`. One raised during garbage collection, or inside a thread,
has no test frame left to propagate into, so pytest re-emits it as
`PytestUnraisableExceptionWarning` or `PytestUnhandledThreadExceptionWarning` -
a different class, which the filter did not name.

So this plants each kind of leak in a subprocess pytest run and asserts the
verdict. A gate nobody has watched fail is a line in a config file, not a gate,
and these are the only tests here that would notice it being deleted.

They spawn their own `pytest`, because a gate cannot be tested from inside the
run it governs: the leak has to reach *that* run's teardown to be judged.
"""

import subprocess
import sys
import textwrap

import pytest

#: Each run is one tiny test; this is the ceiling for the whole subprocess,
#: interpreter start included.
BUDGET = 120.0

PLANTED = {
    # Collected after the test returns, so the warning surfaces with no frame
    # to raise into - the exact shape the bare filter missed.
    "unclosed file": """
        def test_leaks_a_file(tmp_path):
            target = tmp_path / "left-open.txt"
            target.write_text("x", encoding="utf-8")
            handle = open(target, "rb")      # deliberately never closed
            assert handle.read() == b"x"
    """,
    # A thread that raises: pytest wraps it as an unhandled thread exception,
    # which is its own class again.
    "thread failure": """
        import threading

        def test_leaks_a_failing_thread():
            def boom():
                raise RuntimeError("a planted thread failure")
            worker = threading.Thread(target=boom, name="planted")
            worker.start()
            worker.join(timeout=10)
    """,
    # A child nobody reaps. `subprocess` warns on collection that it is still
    # running, which is the second kind the CI logs were full of.
    #
    # It lives about two seconds, not thirty. The warning fires when the
    # `Popen` is collected - milliseconds after the test returns - so a short
    # life is just as real a leak at the only moment that decides the verdict,
    # and it leaves nothing for the outer run's survivor sweep to find.
    # Planting a leak is not a reason to leak one.
    "running subprocess": """
        import subprocess, sys

        def test_leaks_a_subprocess():
            proc = subprocess.Popen(
                [sys.executable, "-c",
                 "import time; deadline = time.monotonic() + 2\\n"
                 "while time.monotonic() < deadline: time.sleep(0.05)"])
            assert proc.poll() is None        # deliberately never ended
    """,
}


def _run(tmp_path, body: str, *, gated: bool) -> subprocess.CompletedProcess:
    """One planted test, run by its own pytest, with the gate on or off."""
    (tmp_path / "test_planted.py").write_text(textwrap.dedent(body),
                                              encoding="utf-8")
    gate = [
        "-W", "error::ResourceWarning",
        "-W", "error::pytest.PytestUnraisableExceptionWarning",
        "-W", "error::pytest.PytestUnhandledThreadExceptionWarning",
    ] if gated else ["-W", "always"]
    return subprocess.run(
        [sys.executable, "-m", "pytest", "-q", "--no-header", "-p", "no:cacheprovider",
         *gate, str(tmp_path / "test_planted.py")],
        capture_output=True, text=True, timeout=BUDGET, cwd=str(tmp_path))


@pytest.mark.parametrize("what", sorted(PLANTED))
def test_the_gate_fails_a_planted_leak(tmp_path, what):
    """With the gate configured, each planted leak must fail its run."""
    done = _run(tmp_path, PLANTED[what], gated=True)

    assert done.returncode != 0, (
        f"a planted {what} passed under the gate, so the gate does not gate:\n"
        f"{done.stdout[-1500:]}")


@pytest.mark.parametrize("what", sorted(PLANTED))
def test_the_planted_leak_is_otherwise_a_passing_test(tmp_path, what):
    """The control, and the half that makes the other half mean something.

    Without the gate every one of these passes - they assert nothing false and
    fail for no reason of their own. So the failure above is the gate's doing,
    not a broken fixture.
    """
    done = _run(tmp_path, PLANTED[what], gated=False)

    assert done.returncode == 0, (
        f"the planted {what} fails on its own merits, so it proves nothing "
        f"about the gate:\n{done.stdout[-1500:]}")


def test_a_clean_test_still_passes_under_the_gate(tmp_path):
    """The gate must not fail a run that leaks nothing."""
    done = _run(tmp_path, """
        def test_clean(tmp_path):
            target = tmp_path / "tidy.txt"
            target.write_text("x", encoding="utf-8")
            with open(target, "rb") as handle:
                assert handle.read() == b"x"
    """, gated=True)

    assert done.returncode == 0, \
        f"the gate failed a clean test:\n{done.stdout[-1500:]}"
