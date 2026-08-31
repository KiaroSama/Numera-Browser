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
import sys
import tempfile
import re
import threading
import json
from pathlib import Path
from typing import Optional

APP_DIR_NAME = "data"
LEGACY_HOME_DIR = ".persona"
HOME_DIR = ".numera"


def app_dir() -> Path:
    """The directory the application itself lives in.

    Frozen (PyInstaller and friends) that is the executable's folder; from a
    source checkout it is the repository root, three levels up from this file
    (``engine/numera/paths.py``)."""
    if getattr(sys, "frozen", False):
        return Path(sys.executable).resolve().parent
    return Path(__file__).resolve().parents[2]


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


def safe_join(base: Path, member: str) -> Path:
    """Resolve ``member`` under ``base``, or raise if it would escape.

    One helper for every place that writes a path taken from untrusted input:
    a backup archive, a `.crx`, an extension manifest. Each of those used to
    carry its own check and two of them were wrong in the same way -
    ``str(target).startswith(str(base))`` passes for a *sibling* whose name
    merely begins with the base, so `<ext>-evil/x` was accepted beside `<ext>/`.

    Comparing resolved paths as paths, not as strings, is the fix. `..`,
    absolute members and Windows drive letters are all rejected by the same
    test, because `Path("a") / "C:/x"` is `C:/x` - the join throws the base away.
    """
    base = Path(base).resolve()
    # A member is always archive-relative. Normalising separators first means a
    # zip written on Windows is read the same way as one written anywhere else.
    rel = str(member).replace("\\", "/").strip("/")
    if not rel or rel in (".", ".."):
        raise ValueError(f"refusing empty or dot archive member: {member!r}")
    target = (base / rel).resolve()
    if target != base and base not in target.parents:
        raise ValueError(
            f"refusing archive member that escapes its folder: {member!r}")
    return target


# A profile id is an opaque token the app mints - `uuid4().hex[:12]` on the
# engine side, a base-36 counter plus randomness in the dashboard. It is never
# a path fragment, so nothing that looks like one is allowed near a filename.
_SAFE_ID = re.compile(r"^[A-Za-z0-9_-]{1,64}$")


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


def _lock_for(path) -> threading.RLock:
    key = str(Path(path).resolve())
    with _LOCKS_GUARD:
        lock = _FILE_LOCKS.get(key)
        if lock is None:
            lock = _FILE_LOCKS[key] = threading.RLock()
        return lock


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
        os.replace(tmp, path)                  # atomic on POSIX and Windows
    finally:
        if tmp.exists():
            try:
                tmp.unlink()
            except OSError:
                pass
