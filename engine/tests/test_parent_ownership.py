"""The engine must not outlive the shell that started it.

Windows enforces this with a job object and Linux with `PR_SET_PDEATHSIG`, both
in the Tauri shell. macOS has neither, so a force-killed shell left the engine
alive holding its port - and the next launch started a *second* engine against
the same portable data root. Two writers, one directory.

`watch_parent()` is the portable half: it runs wherever "is that pid alive" can
be answered, so one mechanism covers the gap instead of a different guess per
platform.
"""
import os
import subprocess
import sys
import textwrap
import time
from pathlib import Path

import pytest

from numera.paths import _parent_alive, watch_parent

ENGINE = str(Path(__file__).resolve().parents[1])
POSIX = os.name != "nt"


def test_nothing_to_watch_starts_no_thread(monkeypatch):
    monkeypatch.delenv("NUMERA_PARENT_PID", raising=False)
    assert watch_parent() is None
    assert watch_parent("") is None
    assert watch_parent("not-a-pid") is None


def test_it_reads_the_environment_the_shell_sets(monkeypatch):
    monkeypatch.setenv("NUMERA_PARENT_PID", str(os.getpid()))
    thread = watch_parent(on_gone=lambda: None)
    assert thread is not None and thread.daemon, \
        "the watcher must not keep the process alive by itself"


@pytest.mark.skipif(not POSIX, reason="liveness is only answerable on POSIX")
def test_a_dead_parent_is_not_watched(monkeypatch):
    """Nothing to wait for - and starting a watcher would fire immediately."""
    monkeypatch.delenv("NUMERA_PARENT_PID", raising=False)
    assert watch_parent(9_999_999) is None


@pytest.mark.skipif(not POSIX, reason="liveness is only answerable on POSIX")
def test_liveness_says_what_it_means():
    assert _parent_alive(os.getpid()) is True
    assert _parent_alive(9_999_999) is False


@pytest.mark.skipif(not POSIX, reason="the orphan case only exists on POSIX")
def test_the_child_exits_when_its_named_parent_dies(tmp_path):
    """The behaviour itself, with two real processes.

    A middle process starts the watched child and is then killed outright, so
    the child is reparented to init exactly as a force-killed shell would leave
    the engine.
    """
    child_script = tmp_path / "child.py"
    child_script.write_text(textwrap.dedent(f"""
        import os, sys, time
        sys.path.insert(0, {ENGINE!r})
        from numera.paths import watch_parent
        watch_parent(int(sys.argv[1]))
        print("WATCHING", flush=True)
        # Park on stdin. The watcher is what ends this process, so a timer
        # would only be a guess at how long the test takes - and a wrong guess
        # ends it for the wrong reason while still passing.
        sys.stdin.read()
    """), encoding="utf-8")

    # The middle process only has to exist until it is killed, so it parks on
    # stdin too rather than on a clock.
    middle = subprocess.Popen([sys.executable, "-c", "import sys; sys.stdin.read()"],
                              stdin=subprocess.PIPE)
    try:
        child = subprocess.Popen([sys.executable, str(child_script), str(middle.pid)],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        assert child.stdout.readline().strip() == "WATCHING"

        middle.kill()
        middle.wait(30)

        # The watcher polls once a second; give it several before calling it.
        child.wait(timeout=30)
        assert child.returncode == 0, \
            f"the child did not exit cleanly when its parent died: {child.returncode}"
    finally:
        for proc in (middle, child):
            if proc.poll() is None:                      # pragma: no cover
                proc.kill()


@pytest.mark.skipif(not POSIX, reason="the orphan case only exists on POSIX")
def test_the_child_stays_up_while_its_parent_lives(tmp_path):
    """The watcher must not shut a healthy engine down."""
    child_script = tmp_path / "child.py"
    child_script.write_text(textwrap.dedent(f"""
        import os, sys, time
        sys.path.insert(0, {ENGINE!r})
        from numera.paths import watch_parent
        watch_parent(os.getppid())
        print("WATCHING", flush=True)
        sys.stdin.read()         # the parent decides when this ends
    """), encoding="utf-8")

    child = subprocess.Popen([sys.executable, str(child_script)],
                             stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        assert child.stdout.readline().strip() == "WATCHING"
        with pytest.raises(subprocess.TimeoutExpired):
            child.wait(timeout=4)          # several poll intervals
        assert child.poll() is None, "the watcher killed a child whose parent is alive"
    finally:
        child.kill()
        child.wait(10)
