"""Two processes, one data root, and no lost writes.

Numera runs several processes against the same directory - `numera serve`, CLI
commands, detached jobs, the scheduler, a second desktop window. The storage
locks were `threading.RLock`s in module dictionaries, which serialise threads
inside one interpreter and are invisible to every other process. Two of them
could read, modify and atomically replace the same JSON, and the later write
erased the earlier one with nothing to show for it.

These tests use real child processes on purpose. A thread-only test passes on
the old `RLock` and proves nothing about the case that was broken.
"""
import json
import os
import subprocess
import sys
import textwrap
import time
from pathlib import Path

import pytest

from numera import generate
from numera.models import Profile
from numera.paths import LockTimeout, _lock_for, transaction_lock, write_json
from numera.store import ProfileStore

ENGINE = str(Path(__file__).resolve().parents[1])


def run_children(body: str, root, count: int, extra=()) -> list:
    """Run `body` in `count` real subprocesses and return their results.

    Started together and joined together, so their critical sections actually
    overlap; a sequential run would pass with no locking at all.
    """
    script = textwrap.dedent(f"""
        import json, os, sys, time
        sys.path.insert(0, {ENGINE!r})
        ROOT = sys.argv[1]
        WHO = sys.argv[2]
        {textwrap.indent(textwrap.dedent(body), "        ").lstrip()}
    """)
    procs = [
        subprocess.Popen([sys.executable, "-c", script, str(root), f"w{i}", *extra],
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        for i in range(count)
    ]
    out = []
    for pr in procs:
        stdout, stderr = pr.communicate(timeout=120)
        out.append({"code": pr.returncode, "out": stdout.strip(), "err": stderr.strip()})
    return out


# --- the lock itself --------------------------------------------------------

def test_a_second_process_waits_for_the_file_lock(tmp_path):
    """The child must not get the lock while this process holds it."""
    target = tmp_path / "guarded.json"
    write_json(target, {"held": True})

    body = """
        from numera.paths import _lock_for, LockTimeout
        os.environ["NUMERA_LOCK_TIMEOUT"] = "1"
        import importlib, numera.paths as P
        importlib.reload(P)
        try:
            with P._lock_for(os.path.join(ROOT, "guarded.json")):
                print("GOT-IT")
        except P.LockTimeout:
            print("BLOCKED")
    """
    with _lock_for(target):
        results = run_children(body, tmp_path, 1)

    assert results[0]["code"] == 0, results[0]["err"]
    assert results[0]["out"] == "BLOCKED", \
        f"a second process took a lock this one holds: {results[0]}"


def test_the_lock_is_released_when_the_holder_exits(tmp_path):
    """Crash release: the OS drops the lock with the process, so a killed
    writer cannot wedge the data root permanently."""
    target = tmp_path / "guarded.json"
    write_json(target, {})

    holder = subprocess.Popen(
        [sys.executable, "-c", textwrap.dedent(f"""
            import sys, time
            sys.path.insert(0, {ENGINE!r})
            from numera.paths import _lock_for
            with _lock_for(sys.argv[1]):
                print("HELD", flush=True)
                # Park on stdin, not on a clock: the parent kills this process
                # as soon as it reads HELD, so a timer would only be a guess at
                # how long that takes.
                sys.stdin.read()
        """), str(target)],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    try:
        assert holder.stdout.readline().strip() == "HELD"
        holder.kill()                     # no chance to release it politely
        holder.wait(30)
    finally:
        if holder.poll() is None:         # pragma: no cover
            holder.kill()

    # The lock must be free now. A short timeout keeps a regression from
    # hanging the suite for the full default instead of failing.
    os.environ["NUMERA_LOCK_TIMEOUT"] = "5"
    try:
        import importlib

        from numera import paths as P
        importlib.reload(P)
        with P._lock_for(target):
            pass
    finally:
        os.environ.pop("NUMERA_LOCK_TIMEOUT", None)
        importlib.reload(P)


# --- what the lock is for: writes that survive each other -------------------

def test_two_processes_editing_different_profiles_both_survive(tmp_path):
    """Each writes its own profile under the transaction lock. Before the
    cross-process lock the two `write_batch` snapshots overlapped and one
    rollback could take the other's file with it."""
    engine = ProfileStore(tmp_path)
    for i in range(2):
        engine.save(Profile(name=f"p{i}", fingerprint=generate(seed=i)))

    body = """
        from numera.store import ProfileStore
        store = ProfileStore(ROOT)
        prof = store.resolve("p0" if WHO == "w0" else "p1")
        prof.notes = WHO
        store.save(prof)
        print("OK")
    """
    results = run_children(body, tmp_path, 2)
    assert all(r["code"] == 0 for r in results), results

    after = ProfileStore(tmp_path)
    assert after.resolve("p0").notes == "w0"
    assert after.resolve("p1").notes == "w1", \
        "one process's write was erased by the other"


def test_concurrent_proxy_pool_adds_do_not_lose_entries(tmp_path):
    """`proxies.json` is one document holding every proxy, so two adders are a
    read-modify-write on the same file - the exact shape that loses data."""
    pytest.importorskip("fastapi")

    body = """
        from pathlib import Path
        from numera.store import ProfileStore
        from numera.server.stores import ProxyPool
        pool = ProxyPool(Path(ROOT), ProfileStore(ROOT))
        pool.add_many([{"type": "HTTP", "host": "10.0.0." + WHO[-1],
                        "port": "8080", "user": "u", "pass": "p"}])
        print("OK")
    """
    results = run_children(body, tmp_path, 4)
    assert all(r["code"] == 0 for r in results), results

    entries = json.loads((tmp_path / "proxies.json").read_text(encoding="utf-8"))
    hosts = sorted(e["host"] for e in entries)
    assert len(hosts) == 4, f"entries were lost: {hosts}"


def test_concurrent_schedule_writes_all_survive(tmp_path):
    """`schedules.json` has the same single-document shape."""
    pytest.importorskip("fastapi")

    body = """
        from numera.scheduler import ScheduleStore
        s = ScheduleStore(ROOT)
        s.add(profile_id=WHO, every_hours=6, minutes=30)
        print("OK")
    """
    results = run_children(body, tmp_path, 4)
    assert all(r["code"] == 0 for r in results), results

    kept = json.loads((tmp_path / "schedules.json").read_text(encoding="utf-8"))
    rows = kept if isinstance(kept, list) else kept.get("schedules", [])
    assert len(rows) == 4, f"schedule rows were lost: {rows}"


def test_a_vault_migration_is_not_interleaved_with_a_profile_write(tmp_path):
    """`vault enable` rewrites every secret-bearing file. A profile write
    landing inside that migration is how a store ends up half-encrypted, which
    no reader can recover from."""
    pytest.importorskip("cryptography")

    engine = ProfileStore(tmp_path)
    from numera.models import Proxy
    for i in range(6):
        engine.save(Profile(name=f"p{i}", fingerprint=generate(seed=i),
                            proxy=Proxy(server="http://1.2.3.4:8080",
                                        username="u", password="SECRET")))

    body = """
        from numera.store import ProfileStore
        store = ProfileStore(ROOT)
        if WHO == "w0":
            store.enable_vault("master")
        else:
            time.sleep(0.05)
            try:
                prof = store.resolve("p3")
                prof.notes = "edited"
                store.save(prof)
            except Exception as e:
                print("REFUSED", type(e).__name__)
                raise SystemExit(0)
        print("OK")
    """
    results = run_children(body, tmp_path, 2)
    assert all(r["code"] == 0 for r in results), results

    # Whatever the interleaving, every stored secret must be in one state.
    from numera import vault
    plaintext = vault.plaintext_secrets(tmp_path)
    assert not plaintext, f"the migration left readable secrets behind: {plaintext}"
    unlocked = ProfileStore(tmp_path, password="master")
    assert unlocked.unlocked()
    for i in range(6):
        assert unlocked.resolve(f"p{i}").proxy.password == "SECRET", \
            f"p{i} cannot be read back - the store is half-migrated"
