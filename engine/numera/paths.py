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
    if portable.exists() or _is_writable(portable):
        return portable

    home = Path.home() / HOME_DIR
    legacy = Path.home() / LEGACY_HOME_DIR
    if not home.exists() and legacy.exists():
        return legacy
    return home


def legacy_dirs() -> list[Path]:
    """Pre-portable locations worth importing from, newest naming first."""
    return [Path.home() / HOME_DIR, Path.home() / LEGACY_HOME_DIR]
