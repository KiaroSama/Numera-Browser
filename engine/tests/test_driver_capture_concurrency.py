"""Preparation may overlap; driver creation and durable recording serialize."""

from concurrent.futures import ThreadPoolExecutor
import subprocess
import sys
import threading

import pytest

from numera import driver_capture, drivers, proctree, sessions
from numera.fingerprint import generate
from numera.models import Profile
from numera.repo import repo
from numera.store import ProfileStore


def spawn_and_close(holder):
    child = None
    try:
        child = subprocess.Popen([sys.executable, "-c", "import sys; sys.stdin.read(1)"],
                                 stdin=subprocess.PIPE, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL)
        assert holder.tree is not None
        assert holder.tree.pid == child.pid
        assert proctree.alive_tree(holder.tree)
        return holder.tree
    finally:
        # The capture callback can fail after Popen created a pipe but before
        # it returned. Its holder retains that process for failure cleanup.
        child = child or holder.proc
        if child is not None:
            child.stdin.close()
            try:
                child.wait(timeout=5)
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)


def test_paused_preflight_does_not_block_another_profiles_driver():
    preparing, resume, second_done = (threading.Event() for _ in range(3))
    original = subprocess.Popen

    def cloak_style():
        with proctree.spawning_own_tree(sys.executable) as holder:
            preparing.set()
            assert resume.wait(10), "preflight was never resumed"
            return spawn_and_close(holder)

    def playwright_style():
        with proctree.spawning_own_tree(sys.executable) as holder:
            tree = spawn_and_close(holder)
            second_done.set()
            return tree

    with ThreadPoolExecutor(max_workers=2) as workers:
        first = workers.submit(cloak_style)
        try:
            assert preparing.wait(5)
            second = workers.submit(playwright_style)
            assert second_done.wait(3), "preflight blocked an unrelated driver"
        finally:
            resume.set()
        trees = [first.result(timeout=8), second.result(timeout=8)]
    assert trees[0].group != trees[1].group
    assert all(not proctree.alive_tree(tree) for tree in trees)
    assert subprocess.Popen is original


def test_actual_spawns_and_their_record_callbacks_remain_serialized(monkeypatch):
    ready = threading.Barrier(3, timeout=8)
    first_recording, release_record, second_matching = (
        threading.Event() for _ in range(3))
    records = []
    original_match = driver_capture._is_target

    def match(args, targets):
        if threading.current_thread().name == "second-driver":
            second_matching.set()
        return original_match(args, targets)

    def record(tree):
        assert driver_capture._SPAWNING.locked(), "recording escaped spawn serialization"
        records.append(tree)
        if len(records) == 1:
            first_recording.set()
            assert release_record.wait(8), "durable recording was not released"

    def launch(name):
        threading.current_thread().name = name
        with proctree.spawning_own_tree(sys.executable, on_tree=record) as holder:
            ready.wait()
            if name == "second-driver":
                assert first_recording.wait(5)
            return spawn_and_close(holder)

    monkeypatch.setattr(driver_capture, "_is_target", match)
    with ThreadPoolExecutor(max_workers=2) as workers:
        futures = [workers.submit(launch, name)
                   for name in ("first-driver", "second-driver")]
        try:
            ready.wait()
            assert first_recording.wait(5)
            assert second_matching.wait(5), "second driver never reached capture"
            assert len(records) == 1, "captures overlapped durable recording"
        finally:
            release_record.set()
        trees = [future.result(timeout=8) for future in futures]
    assert len(records) == 2
    assert {t.group for t in records} == {t.group for t in trees}
    assert len({t.group for t in trees}) == 2


def test_matching_spawn_on_an_unregistered_thread_is_not_captured():
    with proctree.spawning_own_tree(sys.executable) as holder:
        with ThreadPoolExecutor(max_workers=1) as workers:
            child = workers.submit(subprocess.Popen,
                                   [sys.executable, "-c", "pass"]).result(timeout=5)
            try:
                child.wait(timeout=5)
                assert holder.tree is None, "a sibling's process was adopted"
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait(timeout=5)
        spawn_and_close(holder)


def test_failed_preflight_leaves_no_claim_or_interception(tmp_path):
    store = ProfileStore(tmp_path)
    profile = repo(store).create(Profile(name="preflight", fingerprint=generate(seed=1)))
    original = subprocess.Popen
    with pytest.raises(RuntimeError, match="preflight failed"):
        with drivers.open_session(profile, store):
            with drivers._own_driver_tree(sys.executable):
                raise RuntimeError("preflight failed")
    assert not sessions.held(store.root, profile.id)
    assert subprocess.Popen is original
