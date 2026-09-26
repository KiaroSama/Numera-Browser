"""Thread ownership and one total budget for actual data/config locks."""
from contextlib import contextmanager
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import threading

import pytest
from numera import data_locks, paths
from numera.config_store import ConfigStore


@contextmanager
def held_elsewhere(lock):
    ready, release = threading.Event(), threading.Event()
    errors = []
    def hold():
        try:
            with lock:
                ready.set()
                if not release.wait(5):
                    raise AssertionError("fixture owner was not released")
        except BaseException as error:
            errors.append(error)
    owner = threading.Thread(target=hold)
    owner.start()
    try:
        assert ready.wait(3), errors
        yield
    finally:
        release.set()
        owner.join(3)
        assert not owner.is_alive(), "fixture leaked its lock owner"
        assert not errors, errors


def config(root):
    store = ConfigStore()
    store.root, store.config_path = root, root / "config.json"
    return store


def test_another_threads_transaction_does_not_authorize_a_config_replacement(tmp_path):
    store = config(tmp_path)
    paths.write_json(store.config_path, {"keep": "original"})
    original = store.config_path.read_bytes()
    with held_elsewhere(paths.transaction_lock(tmp_path)):
        with pytest.raises(RuntimeError, match="transaction"):
            store._save_config_locked({"foreign": "replacement"})
        assert store.config_path.read_bytes() == original


def test_transaction_predicate_is_about_the_calling_thread(tmp_path):
    lock = paths.transaction_lock(tmp_path)
    assert not paths.holding_transaction(tmp_path)
    with held_elsewhere(lock):
        assert not paths.holding_transaction(tmp_path)
    with lock:
        assert paths.holding_transaction(tmp_path)
        with lock:
            assert paths.holding_transaction(tmp_path)
        assert paths.holding_transaction(tmp_path)
    assert not paths.holding_transaction(tmp_path)


@pytest.mark.parametrize("kind", ["root", "file"])
def test_thread_contention_obeys_the_configured_deadline(tmp_path, monkeypatch, kind):
    monkeypatch.setenv("NUMERA_LOCK_TIMEOUT", "0.05")
    lock = paths.transaction_lock(tmp_path) if kind == "root" else paths._lock_for(tmp_path / "x.json")
    done, began = threading.Event(), threading.Event()
    outcomes = []
    def contender():
        began.set()
        try:
            with lock:
                outcomes.append("entered")
        except paths.LockTimeout:
            outcomes.append("timeout")
        except BaseException as error:
            outcomes.append(type(error).__name__)
        finally:
            done.set()
    thread = threading.Thread(target=contender)
    with held_elsewhere(lock):
        thread.start()
        assert began.wait(2)
        finished_before_owner_release = done.wait(.4)
    thread.join(3)
    assert not thread.is_alive()
    assert finished_before_owner_release, "thread wait ignored NUMERA_LOCK_TIMEOUT"
    assert outcomes == ["timeout"]
    with lock:
        pass


@pytest.mark.parametrize("raw", ["nan", "NaN", "inf", "-inf", "-1", "-0.1", "1e309", "1e100"])
def test_invalid_timeout_cannot_select_an_infinite_or_invalid_wait(monkeypatch, raw):
    monkeypatch.setenv("NUMERA_LOCK_TIMEOUT", raw)
    value = paths._lock_timeout()
    assert math.isfinite(value) and 0 <= value <= threading.TIMEOUT_MAX
    assert value == paths._DEFAULT_LOCK_TIMEOUT


@pytest.mark.parametrize("raw,expected", [("0", 0), (".02", .02), ("3", 3), ("", 30), ("invalid", 30)])
def test_valid_timeout_and_existing_invalid_text_fallback(monkeypatch, raw, expected):
    monkeypatch.setenv("NUMERA_LOCK_TIMEOUT", raw)
    assert paths._lock_timeout() == expected


def test_thread_and_os_wait_share_one_budget(tmp_path, monkeypatch):
    class Clock:
        now = 10.0
        def monotonic(self):
            return self.now
        def sleep(self, duration):
            self.now += duration
    clock = Clock()
    class DelayedRLock:
        released = False
        def acquire(self, *args, **kwargs):
            clock.now += .06
            return True
        def release(self):
            self.released = True
    lock = paths._DataLock(tmp_path / "lock")
    lock._rlock = DelayedRLock()
    monkeypatch.setattr(paths, "time", clock)
    monkeypatch.setattr(data_locks, "_take", lambda fd: False)
    monkeypatch.setenv("NUMERA_LOCK_TIMEOUT", ".1")
    with pytest.raises(paths.LockTimeout):
        with lock:
            pytest.fail("contended OS lock was acquired")
    assert clock.now <= 10.100001, "OS stage restarted the full acquisition budget"
    assert lock._rlock.released and lock._fd is None


def test_wrong_thread_exit_cannot_change_depth_or_release_os_lock(tmp_path):
    lock = paths._DataLock(tmp_path / "lock")
    errors = []
    with lock:
        original_fd = lock._fd
        def foreign_exit():
            try:
                lock.__exit__(None, None, None)
            except RuntimeError:
                errors.append("refused")
        thread = threading.Thread(target=foreign_exit)
        thread.start()
        thread.join(3)
        assert not thread.is_alive()
        assert errors == ["refused"]
        assert lock._depth == 1 and lock._fd == original_fd
    assert lock._depth == 0 and lock._fd is None


def test_reentrant_config_replacement_keeps_root_file_lock_order(tmp_path):
    store = config(tmp_path)
    with paths.transaction_lock(tmp_path):
        store._save_config_locked({"a": 1})
        with paths.transaction_lock(tmp_path):
            store._save_config_locked({"a": 1, "b": 2})
    assert store.get_config() == {"a": 1, "b": 2}


def test_acquisition_error_releases_the_thread_lock(tmp_path, monkeypatch):
    lock = paths._DataLock(tmp_path / "lock")
    def refuse(fd):
        raise PermissionError("injected OS acquisition error")
    with monkeypatch.context() as patch:
        patch.setattr(data_locks, "_take", refuse)
        with pytest.raises(PermissionError):
            with lock:
                pass
    assert lock._fd is None and lock._depth == 0
    with held_elsewhere(lock):
        pass


def test_concurrent_real_config_updates_preserve_all_keys(tmp_path):
    barrier = threading.Barrier(5)
    errors = []
    def update(index):
        try:
            store = config(tmp_path)
            barrier.wait(timeout=3)
            for n in range(8):
                store.set_config_many({f"key-{index}-{n}": n})
        except BaseException as error:
            errors.append(error)
    threads = [threading.Thread(target=update, args=(index,)) for index in range(4)]
    for thread in threads:
        thread.start()
    barrier.wait(timeout=3)
    for thread in threads:
        thread.join(10)
    assert not any(thread.is_alive() for thread in threads)
    assert not errors
    assert len(config(tmp_path).get_config()) == 32


def test_process_contention_times_out_then_succeeds_without_deleting_lock_file(tmp_path):
    root = str(tmp_path)
    program = '''
import sys
from pathlib import Path
from numera import data_locks, paths
root = Path(sys.argv[1])
try:
    with paths.transaction_lock(root):
        print("entered")
except paths.LockTimeout:
    print("timeout")
'''
    env = dict(os.environ, NUMERA_LOCK_TIMEOUT=".05")
    env["PYTHONPATH"] = str(Path(paths.__file__).resolve().parents[1])
    lock = paths.transaction_lock(tmp_path)
    with lock:
        before = lock._path.stat().st_ino
        run = subprocess.run([sys.executable, "-c", program, root], env=env,
                             capture_output=True, text=True, timeout=5)
        assert run.returncode == 0 and run.stdout.strip() == "timeout", run.stderr
    run = subprocess.run([sys.executable, "-c", program, root], env=env,
                         capture_output=True, text=True, timeout=5)
    assert run.returncode == 0 and run.stdout.strip() == "entered", run.stderr
    assert lock._path.stat().st_ino == before


def test_timeout_message_never_recommends_removing_the_lock_file(tmp_path, monkeypatch):
    lock = paths._DataLock(tmp_path / "lock")
    monkeypatch.setenv("NUMERA_LOCK_TIMEOUT", "0")
    monkeypatch.setattr(data_locks, "_take", lambda fd: False)
    with pytest.raises(paths.LockTimeout) as caught:
        with lock:
            pass
    assert "delete" not in str(caught.value).lower()
