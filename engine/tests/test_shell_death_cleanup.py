"""A force-killed shell takes the browsers with it, not just the engine.

`watch_parent` leaves through `os._exit(0)`, which runs no atexit handler and
no lifespan teardown. The engine died, which is what the ownership tests
checked - and every browser it had launched stayed up: orphaned, holding its
profile directory open, invisible to the next launch and to the dashboard,
which reads the engine's own registry. The next launch of that profile then met
a directory another Chromium still owned.

The shutdown hooks that close those browsers now run on that path too. The test
proves it with a real grandchild process: the engine's child holds an OS lock,
and the parent can only take that lock once the process holding it is gone.
Nothing here waits on a clock - the lock is released by the kernel when the
process dies, so acquiring it *is* the proof.
"""
import os
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

from numera.paths import LockTimeout, _lock_for

pytest.importorskip("fastapi")

ENGINE = str(Path(__file__).resolve().parents[1])


@pytest.mark.skipif(os.name == "nt", reason="Windows owns the engine through a "
                                              "job object, so `_parent_alive` "
                                              "there is a constant True and the "
                                              "watcher never runs")
def test_a_killed_shell_closes_the_browsers_the_engine_launched(tmp_path):
    held = tmp_path / "browser.lock"
    # Its own file rather than a nested `-c`: a Windows path inside a string
    # inside a string is an escape sequence away from a SyntaxError that looks
    # nothing like the thing under test.
    browser_script = tmp_path / "browser.py"
    browser_script.write_text(textwrap.dedent(f"""
        import sys
        sys.path.insert(0, {ENGINE!r})
        from numera.paths import _lock_for
        lock = _lock_for(sys.argv[1])
        lock.__enter__()
        print("HOLDING", flush=True)
        sys.stdin.read()
    """), encoding="utf-8")
    engine_script = tmp_path / "engine.py"
    engine_script.write_text(textwrap.dedent(f"""
        import os, subprocess, sys
        sys.path.insert(0, {ENGINE!r})
        from numera.paths import watch_parent
        from numera.server import create_app

        app = create_app({str(tmp_path)!r}, start_scheduler=False)

        # Stands in for a launched browser: a real process holding a real OS
        # lock, so "did it survive" is answered by the kernel and not by us.
        browser = subprocess.Popen(
            [sys.executable, {str(browser_script)!r}, {str(held)!r}],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        assert browser.stdout.readline().strip() == "HOLDING"

        def close_browser():
            browser.terminate()
            try:
                browser.wait(timeout=10)
            except subprocess.TimeoutExpired:
                browser.kill()

        app.state.add_shutdown(close_browser)

        # Exactly what `serve()` arms once the app exists.
        def shell_is_gone():
            try:
                app.state.run_shutdown()
            finally:
                os._exit(0)

        watch_parent(int(sys.argv[1]), on_gone=shell_is_gone)
        print("WATCHING", flush=True)
        sys.stdin.read()
    """), encoding="utf-8")

    # The stand-in shell: it only has to exist until it is killed.
    shell = subprocess.Popen([sys.executable, "-c", "import sys; sys.stdin.read()"],
                             stdin=subprocess.PIPE)
    engine = None
    try:
        engine = subprocess.Popen([sys.executable, str(engine_script), str(shell.pid)],
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        assert engine.stdout.readline().strip() == "WATCHING"

        shell.kill()                       # a force-kill, not a graceful close
        shell.wait(30)
        engine.wait(timeout=60)
        assert engine.returncode == 0, \
            f"the engine did not exit when the shell died: {engine.returncode}"

        # The lock is free only if the process that held it is gone.
        os.environ["NUMERA_LOCK_TIMEOUT"] = "10"
        try:
            with _lock_for(held):
                pass
        except LockTimeout:
            pytest.fail("the browser outlived the shell: its lock is still held")
        finally:
            os.environ.pop("NUMERA_LOCK_TIMEOUT", None)
    finally:
        for proc in (shell, engine):
            if proc is not None and proc.poll() is None:    # pragma: no cover
                proc.kill()
