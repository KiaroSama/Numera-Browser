"""Data locks: one owner at a time for a file, a root transaction, a session.

Moved out of `paths` whole, so that module stays under the size ceiling; every
name is imported back there and `from .paths import X` keeps working.
Monkeypatch these internals here (`data_locks._take`), not on `paths` - the
classes look them up in this module.
"""
from __future__ import annotations

import hashlib
import math
import os
import re
import threading
import time
from pathlib import Path


# One lock per file, shared by every store instance that points at it.
#
# `/api/proxies/test` runs eight health checks in a thread pool and each did a
# read-modify-write of the same JSON file, so a check finishing second wrote
# back a snapshot taken before the first one landed and silently dropped it.
# Keyed by resolved path, because the stores are constructed per request - a
# lock on the instance would protect nothing.
_FILE_LOCKS: dict = {}
_LOCKS_GUARD = threading.Lock()

# How long to wait for a lock another process holds before giving up. Long
# enough to outlast any legitimate write (the slowest here is the vault
# migration), short enough that a stuck holder is reported rather than waited on
# forever. `NUMERA_LOCK_TIMEOUT` overrides it for a slow disk or a big vault.
#
# Read per acquisition, not once at import. As a module constant, the only way
# to change it was to reload the module - which the tests did, and reloading
# this module replaces `_FILE_LOCKS`, `_ROOT_LOCKS` and every class defined
# here. Two `_DataLock` objects for one path means the in-process RLock layer
# stops serialising anything, and `except LockTimeout` stops matching the class
# that is now raised. One `os.environ` read per lock is not worth that.
_DEFAULT_LOCK_TIMEOUT = 30.0
_LOCK_POLL = 0.05


def _lock_timeout() -> float:
    """A finite supported wait; invalid settings keep the existing default."""
    try:
        limit = float(os.environ.get("NUMERA_LOCK_TIMEOUT") or _DEFAULT_LOCK_TIMEOUT)
    except (ValueError, OverflowError):
        return _DEFAULT_LOCK_TIMEOUT
    if not math.isfinite(limit) or not 0 <= limit <= threading.TIMEOUT_MAX:
        return _DEFAULT_LOCK_TIMEOUT
    return limit


class LockTimeout(RuntimeError):
    """Another process held a data lock for too long."""


def _lock_dir(path: Path) -> Path:
    """Where the lock file for `path` lives.

    Beside the data root rather than beside the file itself: a `.lock` next to
    every profile would show up in `profiles/*.json` globs, and those globs are
    how the vault finds secrets and how folders enumerate profiles.
    """
    return Path(path).parent / ".locks"


def _lock_path(key: str, name: str) -> Path:
    digest = hashlib.sha256(key.encode("utf-8")).hexdigest()[:16]
    # The readable part is for a human staring at the directory; the digest is
    # what makes it unique.
    safe = re.sub(r"[^A-Za-z0-9._-]", "_", name)[:40] or "lock"
    return Path(f"{safe}.{digest}.lock")


if os.name == "nt":
    import msvcrt

    def _take(fd) -> bool:
        try:
            msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
            return True
        except OSError:
            return False

    def _drop(fd) -> None:
        try:
            os.lseek(fd, 0, os.SEEK_SET)
            msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
        except OSError:                                          # pragma: no cover
            pass
else:
    import fcntl

    def _take(fd) -> bool:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return True
        except OSError:
            return False

    def _drop(fd) -> None:
        try:
            fcntl.flock(fd, fcntl.LOCK_UN)
        except OSError:                                          # pragma: no cover
            pass


class _DataLock:
    """Mutual exclusion across threads *and* processes for one piece of state.

    Two layers, because they answer different questions:

    * a `threading.RLock`, so the owning thread can re-enter (a save inside a
      folder transaction inside a batch) and other threads in this process wait;
    * an OS lock on a file, so *other processes* wait too.

    The second layer is the point. Numera runs several processes against one
    data root - `numera serve`, CLI commands, detached jobs, the scheduler, a
    second desktop window - and the in-process `RLock` these locks used to be
    was invisible to every one of them. Two processes could read, modify and
    atomically replace the same JSON and the later write simply erased the
    earlier one.

    Every other contender shares one finite deadline across both layers. The
    owner is recorded as a thread, not only as a depth: a depth is
    process-wide, so another thread could pass `holding_transaction()` while
    only the first one held the root transaction.

    The OS releases the lock when the handle closes, including when the process
    dies, so a crash cannot leave the data root permanently locked. The lock
    *file* stays behind; it holds no state and is reused - and must not be
    unlinked, which could start a second lock domain beside a live holder.
    """

    def __init__(self, path: Path):
        self._path = Path(path)
        self._rlock = threading.RLock()
        self._depth = 0
        self._fd = None
        self._owner = None
        self._owner_pid = None

    def owned_by_current_thread(self) -> bool:
        """A nonblocking ownership check, not merely a process-wide depth."""
        return (self._depth > 0 and self._owner is threading.current_thread()
                and self._owner_pid == os.getpid())

    def _open(self):
        self._path.parent.mkdir(parents=True, exist_ok=True)
        return os.open(self._path, os.O_RDWR | os.O_CREAT, 0o600)

    def __enter__(self):
        limit = _lock_timeout()
        deadline = time.monotonic() + limit
        # Contention on either layer spends the same acquisition budget.
        if not self._rlock.acquire(timeout=max(0.0, deadline - time.monotonic())):
            raise self._timed_out(limit)
        try:
            if self._depth == 0:
                self._acquire_os_lock(deadline=deadline, limit=limit)
                self._owner = threading.current_thread()
                self._owner_pid = os.getpid()
            elif not self.owned_by_current_thread():
                raise RuntimeError("cannot reuse a data lock inherited while held")
            self._depth += 1
        except BaseException:
            self._rlock.release()
            raise
        return self

    def _timed_out(self, limit: float) -> LockTimeout:
        return LockTimeout(
            f"another thread or process has held {self._path} for more than "
            f"{limit:g}s. Wait for the owner to finish or investigate the "
            f"blocked operation; NUMERA_LOCK_TIMEOUT controls this wait. "
            f"Keep the lock file in place while any instance may use it.")

    def _acquire_os_lock(self, *, deadline=None, limit=None) -> None:
        limit = _lock_timeout() if limit is None else limit
        deadline = time.monotonic() + limit if deadline is None else deadline
        fd = self._open()
        try:
            while True:
                if _take(fd):
                    self._fd = fd
                    return
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise self._timed_out(limit)
                time.sleep(min(_LOCK_POLL, remaining))
        except BaseException:
            os.close(fd)
            raise

    def __exit__(self, *exc) -> None:
        # Refuse before touching depth or fd: RLock.release rejects too late.
        if not self.owned_by_current_thread():
            raise RuntimeError("data lock must be released by its owning thread")
        self._depth -= 1
        try:
            if self._depth == 0:
                fd, self._fd = self._fd, None
                self._owner = self._owner_pid = None
                if fd is not None:
                    try:
                        _drop(fd)
                    finally:
                        os.close(fd)
        finally:
            self._rlock.release()


class SessionBusy(RuntimeError):
    """Another process already has this profile's browser session open."""


class _SessionLease:
    """One profile's browser session, owned across processes.

    Deliberately *not* a `_DataLock`. The two answer different questions and
    behave differently on both axes that matter:

    * **Duration.** A data lock is held for one atomic write. A lease is held
      for as long as a browser is open - minutes to hours - so it must never be
      taken while a data transaction is, or every write in the data root would
      wait on a human closing a window.
    * **Contention.** A data lock waits, because the holder is about to finish.
      A lease refuses immediately with `SessionBusy`, because "someone has this
      profile open" is an answer, not a delay. Waiting 30 seconds to say it is
      the worst of both.

    Not reentrant across the *same* process either: two `launch()` calls in one
    server are as wrong as two across two servers, and an RLock would let the
    second through. `held_by_other_process()` is what status checks use, and it
    is honest about its one blind spot - it cannot see a lease this process
    holds, which is exactly what the in-process registry already knows.

    The OS drops the lock when the handle closes, so a killed browser or a
    crashed engine releases it without a sweeper.
    """

    def __init__(self, path: Path):
        self._path = Path(path)
        self._guard = threading.Lock()
        self._fd = None

    def __enter__(self):
        if not self._guard.acquire(blocking=False):
            raise SessionBusy(f"{self._path.stem} is already open in this process")
        try:
            self._path.parent.mkdir(parents=True, exist_ok=True)
            fd = os.open(self._path, os.O_RDWR | os.O_CREAT, 0o600)
            if not _take(fd):
                os.close(fd)
                raise SessionBusy(
                    "another process has this profile's browser session open. "
                    "Close it there, or wait for it to finish.")
            self._fd = fd
        except BaseException:
            self._guard.release()
            raise
        return self

    def __exit__(self, *exc) -> None:
        fd, self._fd = self._fd, None
        if fd is not None:
            _drop(fd)
            os.close(fd)
        self._guard.release()

    def held_by_other_process(self) -> bool:
        """True when some *other* process holds this lease.

        Probes by taking the OS lock and dropping it again. A lease this
        process holds is invisible here - the OS lock is per handle and this
        one is ours - so callers pair it with their own session registry.
        """
        if self._fd is not None:
            return False
        try:
            self._path.parent.mkdir(parents=True, exist_ok=True)
            fd = os.open(self._path, os.O_RDWR | os.O_CREAT, 0o600)
        except OSError:                                          # pragma: no cover
            return False
        try:
            if _take(fd):
                _drop(fd)
                return False
            return True
        finally:
            os.close(fd)


_SESSION_LEASES: dict = {}


def session_lease(root, profile_id: str) -> "_SessionLease":
    """The lease for one profile's browser session under `root`.

    Held for the browser's whole lifetime by every path that opens the
    persistent `user-data/<id>` directory, and by everything that moves or
    deletes it. Without it `user-data` had no owner at all: two servers, a
    desktop and a CLI could open one Chromium profile directory at the same
    time, and delete/backup/restore could run against a live one.
    """
    resolved = Path(root).resolve()
    key = f"{resolved}\x00{profile_id}"
    with _LOCKS_GUARD:
        lease = _SESSION_LEASES.get(key)
        if lease is None:
            lease = _SESSION_LEASES[key] = _SessionLease(
                resolved / ".locks" / _lock_path(key, f"session-{profile_id}"))
        return lease


def _lock_for(path):
    """The lock guarding one file, held against other processes too."""
    resolved = Path(path).resolve()
    key = str(resolved)
    with _LOCKS_GUARD:
        lock = _FILE_LOCKS.get(key)
        if lock is None:
            lock = _FILE_LOCKS[key] = _DataLock(
                _lock_dir(resolved) / _lock_path(key, resolved.name))
        return lock


_ROOT_LOCKS: dict = {}


def holding_transaction(root) -> bool:
    """Does the calling thread own this root's transaction right now?

    A transaction held by another thread does not authorize a whole-config
    replacement. The replacement must belong to the same read/write unit.
    """
    resolved = str(Path(root).resolve())
    with _LOCKS_GUARD:
        lock = _ROOT_LOCKS.get(resolved)
    return lock is not None and lock.owned_by_current_thread()


def transaction_lock(root):
    """The one lock every multi-file profile mutation under `root` takes first.

    A batch snapshots each file, writes them all, and on failure puts the old
    bytes back. Per-file locks are not enough for that: a single `save()` can
    land in the window between the snapshot and the rollback, and the rollback
    then overwrites a change that had already succeeded. There is no per-file
    ordering that fixes it, because the batch does not know its full file set
    until it has finished writing.

    So multi-file mutations serialise against each other and against single
    saves, and the order is always this lock first, per-file lock second - one
    lock, taken outermost, cannot deadlock against itself.

    ponytail: one lock per root, not per file. Profile writes are user-paced
    (a save, a rename, a folder move); if a workload ever makes this the
    bottleneck, the upgrade is a reader/writer lock, not finer keys.
    """
    resolved = Path(root).resolve()
    key = str(resolved)
    with _LOCKS_GUARD:
        lock = _ROOT_LOCKS.get(key)
        if lock is None:
            lock = _ROOT_LOCKS[key] = _DataLock(
                resolved / ".locks" / _lock_path(key, "transaction"))
        return lock
