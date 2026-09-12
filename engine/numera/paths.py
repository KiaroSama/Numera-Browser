"""Where Numera keeps its data, and how it gets there.

One place decides this, because the answer differs by how Numera was started and
getting it wrong scatters a user's profiles across two directories.

Resolution order, first match wins:

1. ``--data-dir`` / an explicit argument. The caller always wins.
2. ``NUMERA_DATA_DIR``. How the desktop shell tells a spawned engine where to look.
3. A ``data/`` directory beside the application, when that location is writable.
   This is what makes the app *portable*: copy the folder to a flash drive and
   the profiles travel with it.
4. ``~/.numera`` -- a normal installed-per-user layout.
5. ``~/.persona`` -- only when it exists and none of the above do. The project
   was renamed from Persona; an existing install must not silently lose its
   profiles just because the product changed name.

The portable check is "is it writable", not "does it exist", because the common
failure is an app unpacked into Program Files, where a data directory beside the
binary would be read-only on Windows and every write would fail later, far from
the cause.
"""

from __future__ import annotations

import os
import hashlib
import sys
import tempfile
import time
import re
import threading
import json
from contextlib import contextmanager
from pathlib import Path
from typing import Optional

APP_DIR_NAME = "data"
LEGACY_HOME_DIR = ".persona"
HOME_DIR = ".numera"


# How often the parent watcher looks. A second is far below any human noticing
# a stale port, and costs nothing measurable.
_PARENT_POLL = 1.0


def _parent_alive(pid: int) -> bool:
    """Is `pid` still running?

    `os.kill(pid, 0)` is the portable liveness check on POSIX. Windows has no
    equivalent that is meaningful here, and does not need one: the desktop shell
    puts the engine in a job object there.
    """
    if os.name == "nt":                                  # pragma: no cover
        return True
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:                              # pragma: no cover
        return True          # it exists; we simply may not signal it
    return True


def watch_parent(pid=None, on_gone=None) -> "Optional[threading.Thread]":
    """Exit when the process that started this one disappears.

    The desktop shell owns the engine, and the two platforms that can enforce
    that do so themselves: Windows through a job object, Linux through
    `PR_SET_PDEATHSIG`. macOS has neither, so a force-killed shell left the
    engine alive holding its port, and the next launch started a second engine
    against the same portable data root - two writers, one directory.

    This is the portable half. It runs on every platform that can answer "is
    that pid alive", which makes one mechanism cover the gap rather than a
    different guess per platform.

    Returns the watcher thread, or None when there is nothing to watch.
    """
    raw = pid if pid is not None else os.environ.get("NUMERA_PARENT_PID")
    try:
        parent = int(raw)
    except (TypeError, ValueError):
        return None
    if parent <= 0 or not _parent_alive(parent):
        return None

    def leave():
        # `os._exit`, not `sys.exit`: this runs on a daemon thread, where
        # SystemExit would only end the thread and leave the server serving.
        os._exit(0)

    stop = on_gone or leave

    def watch():
        while True:
            time.sleep(_PARENT_POLL)
            if not _parent_alive(parent):
                stop()
                return

    thread = threading.Thread(target=watch, name="numera-parent-watch", daemon=True)
    thread.start()
    return thread


def app_dir() -> Path:
    """The directory the application itself lives in.

    Frozen (PyInstaller and friends) that is the executable's folder; from a
    source checkout it is the repository root, three levels up from this file
    (``engine/numera/paths.py``)."""
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parents[2]


def bundled_browsers() -> "Optional[Path]":
    """The Playwright browser bundle shipped beside the app, if there is one.

    Checked next to the frozen engine and one level up, because the portable
    layout puts the engine in `<app>/engine/` and the browsers in
    `<app>/browsers/` - a sibling, not a child, so a rebuild of the engine does
    not have to re-download 150MB.
    """
    for base in (app_dir(), app_dir().parent):
        candidate = base / "browsers"
        if candidate.is_dir() and any(candidate.iterdir()):
            return candidate
    return None


def use_bundled_browsers() -> "Optional[Path]":
    """Point Playwright at the bundled browsers, unless the caller chose one.

    `pip install playwright` installs the *package*; the browser is a separate
    download into a per-user cache. A portable build has no such cache on a
    machine that has never run Playwright, so the app started, served the
    dashboard, and failed at the first launch with "Executable doesn't exist".

    An explicit `PLAYWRIGHT_BROWSERS_PATH` always wins: someone who set it means
    it.
    """
    if os.environ.get("PLAYWRIGHT_BROWSERS_PATH"):
        return None
    found = bundled_browsers()
    if found is not None:
        os.environ["PLAYWRIGHT_BROWSERS_PATH"] = str(found)
    return found


def _is_writable(directory: Path) -> bool:
    """Can we actually create files here? Ask the filesystem, don't infer."""
    try:
        directory.mkdir(parents=True, exist_ok=True)
        with tempfile.NamedTemporaryFile(dir=directory, prefix=".wtest", delete=True):
            return True
    except OSError:
        return False


def resolve_data_dir(explicit: Optional[str | Path] = None) -> Path:
    """The data directory to use. See the module docstring for the order."""
    if explicit:
        return Path(explicit).expanduser()

    env = os.environ.get("NUMERA_DATA_DIR")
    if env:
        return Path(env).expanduser()

    portable = app_dir() / APP_DIR_NAME
    if _is_writable(portable):
        return portable

    home = Path.home() / HOME_DIR
    legacy = Path.home() / LEGACY_HOME_DIR
    if not home.exists() and legacy.exists():
        return legacy
    return home


def legacy_dirs() -> list[Path]:
    """Pre-portable locations worth importing from, newest naming first."""
    return [Path.home() / HOME_DIR, Path.home() / LEGACY_HOME_DIR]


_DRIVE_PREFIX = re.compile(r"^[A-Za-z]:")


def safe_join(base: Path, member: str) -> Path:
    """Resolve ``member`` under ``base``, or raise if it would escape.

    One helper for every place that writes a path taken from untrusted input:
    a backup archive, a `.crx`, an extension manifest. Each of those used to
    carry its own check and two of them were wrong in the same way -
    ``str(target).startswith(str(base))`` passes for a *sibling* whose name
    merely begins with the base, so `<ext>-evil/x` was accepted beside `<ext>/`.

    Comparing resolved paths as paths, not as strings, is the fix for `..` and
    absolute members. A drive letter is checked separately because the join only
    throws the base away on Windows - see below.
    """
    base = Path(base).resolve()
    # A member is always archive-relative. Normalising separators first means a
    # zip written on Windows is read the same way as one written anywhere else.
    rel = str(member).replace("\\", "/").strip("/")
    if not rel or rel in (".", ".."):
        raise ValueError(f"refusing empty or dot archive member: {member!r}")
    # `C:/x` is drive-absolute on Windows, so the join throws the base away and
    # the resolve check below catches it. On Linux it is an ordinary relative
    # name, so the same archive quietly created a directory called `C:` inside
    # the profile. An archive is portable; the refusal has to be too.
    if _DRIVE_PREFIX.match(rel):
        raise ValueError(
            f"{member!r} names a Windows drive and cannot be part of an archive")
    target = (base / rel).resolve()
    if target != base and base not in target.parents:
        raise ValueError(
            f"refusing archive member that escapes its folder: {member!r}")
    return target


# A profile id is an opaque token the app mints - `uuid4().hex[:12]` on the
# engine side, a base-36 counter plus randomness in the dashboard. It is never
# a path fragment, so nothing that looks like one is allowed near a filename.
_SAFE_ID = re.compile(r"^[A-Za-z0-9_-]{1,64}$")

# Reserved on Windows in every directory, case-insensitively, with or without an
# extension. Checked on every platform so a store written on Linux stays
# portable.
_WINDOWS_DEVICES = frozenset(
    ["CON", "PRN", "AUX", "NUL"]
    + [f"COM{i}" for i in range(1, 10)] + [f"LPT{i}" for i in range(1, 10)]
    # COM¹/COM²/COM³ superscripts are reserved too, and so are the 0 forms on
    # current Windows builds.
    + ["COM0", "LPT0"])


def safe_id(value, *, what: str = "id") -> str:
    """The id, or a ValueError explaining why it cannot be used as a filename.

    `POST /api/profiles` accepted a client-supplied `id` and both stores joined
    it straight onto their directory, so `"../../escaped"` wrote two levels up.
    Validating in the route alone would leave every other caller - import,
    restore, the CLI - relying on someone else having checked, so the check
    lives at the point the value becomes a path.
    """
    text = str(value or "")
    if not _SAFE_ID.match(text):
        raise ValueError(
            f"{what} {text!r} is not a valid profile id: expected 1-64 characters "
            "from A-Z, a-z, 0-9, '_' or '-'")
    # `CON`, `NUL`, `COM1` and friends are device names on Windows, not files -
    # with any extension and in any case, so `nul.json` is the device too. The
    # character class above lets them through because they contain nothing
    # unusual. Opening one succeeds and reads or writes the device instead of a
    # profile. Server-minted ids never look like this, but an import, a restore
    # or the CLI can carry one in.
    if text.split(".")[0].upper() in _WINDOWS_DEVICES:
        raise ValueError(
            f"{what} {text!r} is not a valid profile id: {text.split('.')[0].upper()} "
            "is a reserved device name on Windows")
    return text


def id_path(base: Path, value, suffix: str = "") -> Path:
    """`<base>/<id><suffix>`, refusing anything that would leave `base`."""
    return Path(base) / f"{safe_id(value)}{suffix}"


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
    try:
        return float(os.environ.get("NUMERA_LOCK_TIMEOUT") or _DEFAULT_LOCK_TIMEOUT)
    except ValueError:
        return _DEFAULT_LOCK_TIMEOUT


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

    The OS releases the lock when the file handle closes, including when the
    process dies, so a crash cannot leave the data root permanently locked. The
    lock *file* stays behind; it holds no state and is reused.
    """

    def __init__(self, path: Path):
        self._path = Path(path)
        self._rlock = threading.RLock()
        self._depth = 0
        self._fd = None

    def _open(self):
        self._path.parent.mkdir(parents=True, exist_ok=True)
        return os.open(self._path, os.O_RDWR | os.O_CREAT, 0o600)

    def __enter__(self):
        self._rlock.acquire()
        try:
            if self._depth == 0:
                self._acquire_os_lock()
            self._depth += 1
        except BaseException:
            self._rlock.release()
            raise
        return self

    def _acquire_os_lock(self) -> None:
        fd = self._open()
        limit = _lock_timeout()
        deadline = time.monotonic() + limit
        try:
            while True:
                if _take(fd):
                    self._fd = fd
                    return
                if time.monotonic() >= deadline:
                    raise LockTimeout(
                        f"another process has held {self._path} for more than "
                        f"{limit:g}s. If nothing else is running, delete "
                        f"that file; otherwise set NUMERA_LOCK_TIMEOUT higher.")
                time.sleep(_LOCK_POLL)
        except BaseException:
            os.close(fd)
            raise

    def __exit__(self, *exc) -> None:
        self._depth -= 1
        if self._depth == 0 and self._fd is not None:
            fd, self._fd = self._fd, None
            _drop(fd)
            os.close(fd)
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
    """Does this process already hold `root`'s transaction?

    So an API whose safety depends on the caller holding it can say so instead
    of documenting it and hoping. `_DataLock` counts its own re-entries, and
    the OS lock is per handle, so a non-zero depth means this process is
    inside the transaction - which is exactly the precondition being checked.
    """
    resolved = str(Path(root).resolve())
    with _LOCKS_GUARD:
        lock = _ROOT_LOCKS.get(resolved)
    return bool(lock is not None and lock._depth)


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


# Windows hands out ERROR_ACCESS_DENIED for a rename whose source or destination
# another process has open for a moment - an antivirus scanner or the search
# indexer looking at the file we just wrote. It clears on its own in
# milliseconds, and the write is already serialised by the caller's lock, so the
# only thing to do about it is wait and try again. Bounded: five attempts over
# about a quarter of a second, then the error is the caller's.
_REPLACE_ATTEMPTS = 5
_REPLACE_BACKOFF = 0.05


def _replace(tmp: Path, path: Path) -> None:
    """`os.replace`, retried past a transient Windows lock on the file."""
    for attempt in range(_REPLACE_ATTEMPTS):
        try:
            os.replace(tmp, path)              # atomic on POSIX and Windows
            return
        except PermissionError:
            if attempt == _REPLACE_ATTEMPTS - 1:
                raise
            time.sleep(_REPLACE_BACKOFF * (attempt + 1))


def child_env(password=None, base=None) -> dict:
    """The environment a child `numera` process needs to inherit the unlock.

    By environment, never argv. A password on a command line is readable by any
    process on the machine - `ps` on POSIX, the process table on Windows - so it
    would leak the vault to every local user the moment the server launched a
    browser.

    Exists because `--password` unlocked only the parent: the API spawns
    browsers, probes and cookie exports as child processes, and every one of
    them started locked. The profile then browsed with no proxy password, which
    looks exactly like success.
    """
    env = dict(os.environ if base is None else base)
    if password:
        env["NUMERA_PASSWORD"] = password
    return env


def mutate_json(path, change, default=None):
    """Read, change and write one JSON file inside a single lock.

    The read has to be inside the lock too. Three settings writers each did
    `get_config()` -> edit one key -> `save_config()`, with only the write
    locked, so two of them running at once both read the old config and the
    second one wrote its copy over the first. Changing the default engine and
    the headless flag together lost one of them.

    `change` is called with the loaded document and returns what to write.
    """
    path = Path(path)
    with _lock_for(path):
        current = default() if callable(default) else ({} if default is None else default)
        if path.exists():
            try:
                current = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                pass                # a corrupt file is replaced, not merged into
        updated = change(current)
        write_json(path, updated)
        return updated


def _restore_bytes(path: Path, blob: bytes) -> None:
    """Put `blob` back at `path` through the same temp-then-replace dance."""
    tmp = path.with_name(f"{path.name}.undo-{os.getpid()}-{threading.get_ident():x}")
    try:
        tmp.write_bytes(blob)
        _replace(tmp, path)
    finally:
        if tmp.exists():
            try:
                tmp.unlink()
            except OSError:                                     # pragma: no cover
                pass


class RollbackFailed(RuntimeError):
    """The batch failed *and* the undo failed: files are left half-written.

    `write_batch` promises all-or-nothing. When a restore itself raises, that
    promise is broken and the caller has to know - swallowing it handed back
    only the original error, so a partially written data directory looked
    exactly like a clean refusal and the obvious next move, retry, wrote on top
    of it. This carries the paths that could not be put back, and why, because
    that is what a repair needs.
    """

    def __init__(self, cause: BaseException, failures: list):
        self.cause = cause
        self.failures = list(failures)          # [(path, OSError), ...]
        detail = "; ".join(f"{path}: {err}" for path, err in self.failures)
        super().__init__(
            f"{type(cause).__name__}: {cause} - and the rollback could not "
            f"restore {len(self.failures)} file(s): {detail}. These files are "
            f"left as the failed write left them; restore them before retrying.")


class _Batch:
    """Files written as one change; `rollback()` puts every one back."""

    def __init__(self):
        self._undo: list = []

    def write(self, path, data) -> None:
        path = Path(path)
        self.remember(path)
        write_json(path, data)

    def remember(self, path) -> None:
        """Join this file to the batch without writing it through the batch.

        For a file some other code owns the writing of. The vault's descriptor
        is the case: it is persisted by a caller-supplied `save_config`, which
        takes its own lock and enforces its own transaction rule, so the batch
        must not write it - but it is half of one transaction with the secret
        files, and rolling only the other half back is what left a config
        naming a generation the disk was not in.
        """
        path = Path(path)
        self._undo.append((path, path.read_bytes() if path.exists() else None))

    def rollback(self) -> list:
        """Undo every write, and report the ones that could not be undone."""
        failures = []
        for path, original in reversed(self._undo):
            try:
                if original is None:
                    path.unlink(missing_ok=True)
                else:
                    # Atomically, exactly like the write it is undoing. A plain
                    # `write_bytes` truncates first, so a crash mid-rollback
                    # left the file empty - and an empty profiles.json reads as
                    # "no profiles", which is the failure the atomic writer
                    # exists to prevent.
                    _restore_bytes(path, original)
            except OSError as exc:
                # Keep going: one unrestorable file should not strand the
                # others. What changed is that the failure is now reported
                # instead of dropped.
                failures.append((path, exc))
        self._undo.clear()
        return failures


@contextmanager
def write_batch():
    """Write several files as one change, or leave them all as they were.

    Renaming a folder rewrites every profile in it. Failing half way through
    left some profiles moved and the rest behind, under a folder list that had
    already been updated - a state no screen shows correctly and no retry
    fixes.
    """
    batch = _Batch()
    try:
        yield batch
    except BaseException as exc:
        failures = batch.rollback()
        if failures:
            raise RollbackFailed(exc, failures) from exc
        raise


def write_json(path, data) -> None:
    """Serialise first, then replace in one step.

    Writing straight into the destination leaves a truncated file if anything
    fails halfway, and an unreadable proxies.json reads as an empty pool - which
    looks exactly like losing every proxy. Serialising before the file is opened
    means a failure loses the change rather than the data.

    ponytail: in-process locking only. The engine is one process; two
    `numera serve` instances over one data directory would need an OS-level
    lock, which is a bigger change than this bug calls for.
    """
    path = Path(path)
    blob = json.dumps(data, indent=2)          # may raise; nothing written yet
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{path.name}.tmp-{os.getpid()}-{threading.get_ident():x}")
    try:
        tmp.write_text(blob, encoding="utf-8")
        _replace(tmp, path)
    finally:
        if tmp.exists():
            try:
                tmp.unlink()
            except OSError:
                pass
