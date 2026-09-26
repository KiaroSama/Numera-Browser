"""A failed join must not poison later proved completion of the same output."""
import subprocess
import sys
import threading
from pathlib import Path

import pytest
from numera import proctree
from numera.server import helpers, helper_streams


@pytest.mark.parametrize('stream_name', ['stdout', 'stderr'])
def test_real_eof_and_late_thread_exit_can_finish_on_retry(monkeypatch, stream_name):
    arrived, release = threading.Event(), threading.Event()
    original = helper_streams._Reader._pump
    def pump(reader):
        original(reader)
        if reader.name == stream_name:
            # Real EOF has already been read. Only scheduling of thread exit
            # is delayed; no fake successful read or fake output is supplied.
            arrived.set()
            release.wait(5)
    monkeypatch.setattr(helper_streams._Reader, '_pump', pump)
    manager = helpers.HelperManager()
    helper = manager.admit(('fixture',))
    env = manager.child_env(helper)
    proc, tree = proctree.spawn(
        [sys.executable, '-c', 'import sys;sys.stdout.write("OUT");sys.stderr.write("ERR")'],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    manager.adopt(helper, proc, tree)
    helper.streams = helper_streams.Streams(proc)
    manager.attach(helper, proc, tree)
    try:
        proc.wait(timeout=5)
        assert arrived.wait(5)
        report = helper.units_path
        assert manager.settle(helper, deadline=.08) is False
        assert helper in manager.live() and helper.output_problem
        assert helper.output == ('OUT', 'ERR', True)
        assert report.exists()
        release.set()
        for reader in helper.streams.readers:
            reader.thread.join(2)
        assert not any(reader.thread.is_alive() for reader in helper.streams.readers)
        assert manager.settle(helper, deadline=1) is True
        assert helper.output == ('OUT', 'ERR', True)
        assert helper.output_problem is None
        assert helper.state == helpers.SETTLED and helper not in manager.live()
        assert helper.units_path is None and not report.exists()
        assert manager.settle(helper, deadline=.1) is True
    finally:
        release.set()
        if proc.poll() is None:
            proc.kill()
        proc.wait(timeout=3)
        helper.streams.close(2)
        if helper.units_path is not None:
            helper.units_path.unlink(missing_ok=True)


def test_genuine_read_error_cannot_be_cleared_by_repeated_settlement(monkeypatch):
    def fail(reader):
        reader.error = OSError('injected read failure')
    monkeypatch.setattr(helper_streams._Reader, '_pump', fail)
    manager = helpers.HelperManager()
    try:
        with pytest.raises(helpers.HelperFailed):
            helpers.run_helper([sys.executable, '-c', 'pass'], timeout=5,
                               manager=manager, cleanup=.3)
        helper, = manager.live()
        for _ in range(2):
            assert manager.settle(helper, deadline=.2) is False
            assert helper.output_problem and helper in manager.live()
            assert helper.output[2] is False
    finally:
        for helper in manager.live():
            if helper.streams:
                helper.streams.close(1)
            if helper.units_path:
                helper.units_path.unlink(missing_ok=True)
