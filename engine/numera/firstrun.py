"""Carry an existing installation's data into the portable root, once.

Numera used to keep everything in ``~/.persona`` (and briefly ``~/.numera``).
The portable build keeps it beside the application instead. Someone upgrading
has profiles, a proxy pool and a config in the old place, and losing them
because the product changed shape would be indefensible.

Design decisions worth keeping:

* **Copy, never move.** If the import goes wrong, or the user goes back to an
  older build, their data is still where it was. Disk is cheap; a lost profile
  is not.
* **Never overwrite.** Anything already in the destination wins, so running
  this twice cannot clobber work done after the first import.
* **Mark it done with a file, not a flag in config.** The marker lives in the
  destination, so a fresh copy of the portable folder imports again -- which is
  the behaviour you want when you move the app to a new machine.
* **Browser session data is included.** ``user-data/`` holds cookies and
  logins; importing profiles without it would hand the user profiles that are
  all logged out, which looks exactly like data loss.
"""

from __future__ import annotations

import shutil
from pathlib import Path
from typing import Optional

from .paths import legacy_dirs

MARKER = ".imported"

# Everything a profile needs to keep working. `dashboard` is the dashboard's own
# saved state, `proxies.json.bak` is the backup made when the pool was converted
# from SOCKS to HTTP - both cheap to carry and annoying to lose.
# Everything that is the user's, rather than something the app rebuilds.
# `dashboard-config.json` (folders), `tokens.json` and `schedules.json` were
# added to the product after this list was written and were being left behind -
# an import that silently drops the folder layout and every API token is not an
# import. `extensions/` travels too: the paths in config point into it.
ITEMS = ("profiles", "user-data", "dashboard", "extensions",
         "config.json", "dashboard-config.json", "proxies.json",
         "proxies.json.bak", "tokens.json", "schedules.json")


def _merge_dir(src: Path, dst: Path) -> tuple:
    """Copy every entry under `src` that `dst` does not already have.

    Returns (added, kept). The destination always wins per entry, so running
    this twice is safe and a partially-imported directory finishes on the next
    attempt instead of being skipped forever.
    """
    added = kept = 0
    dst.mkdir(parents=True, exist_ok=True)
    for item in src.iterdir():
        if item.name == ".locks":
            # Lock files belong to the install that made them. Copying another
            # root's is harmless but pointless - they hold no state.
            continue
        target = dst / item.name
        if target.exists():
            if item.is_dir():
                a, k = _merge_dir(item, target)
                added += a
                kept += k
            else:
                kept += 1
            continue
        if item.is_dir():
            shutil.copytree(item, target)
            added += 1
        else:
            shutil.copy2(item, target)
            added += 1
    return added, kept


def find_legacy(destination: Path) -> Optional[Path]:
    """An older data directory worth importing, or None."""
    for candidate in legacy_dirs():
        if candidate.resolve() == destination.resolve():
            continue                      # already using it; nothing to import
        if (candidate / "profiles").is_dir() or (candidate / "proxies.json").is_file():
            return candidate
    return None


def already_imported(destination: Path) -> bool:
    return (destination / MARKER).exists()


def import_from(source: Path, destination: Path) -> dict:
    """Copy `source` into `destination` without overwriting anything.

    Returns a summary the caller can log or show. Individual failures are
    collected rather than raised: one unreadable file must not abort the whole
    import and leave the user with half their profiles."""
    copied: list[str] = []
    skipped: list[str] = []
    failed: list[str] = []

    destination.mkdir(parents=True, exist_ok=True)
    for name in ITEMS:
        src = source / name
        dst = destination / name
        if not src.exists():
            continue
        try:
            if src.is_dir():
                # Merge per entry. Skipping a whole directory because it exists
                # meant one profile in the destination hid twenty in the source -
                # the user saw an import that "worked" and lost almost everything.
                added, kept = _merge_dir(src, dst)
                if added:
                    copied.append(f"{name} ({added})")
                if kept:
                    skipped.append(f"{name} ({kept} already here)")
            elif dst.exists():
                skipped.append(name)      # a single file: the destination wins
            else:
                shutil.copy2(src, dst)
                copied.append(name)
        except OSError as e:
            failed.append(f"{name}: {e}")

    profiles = destination / "profiles"
    return {
        "source": str(source),
        "destination": str(destination),
        "copied": copied,
        "skipped": skipped,
        "failed": failed,
        "profiles": len(list(profiles.glob("*.json"))) if profiles.is_dir() else 0,
    }


def run_if_needed(destination: Path) -> Optional[dict]:
    """Import once, the first time this destination is used. None if not needed.

    The marker is written even when nothing was copied, so a user who genuinely
    has no old data is not rescanned on every start."""
    destination = Path(destination)
    if already_imported(destination):
        return None

    source = find_legacy(destination)
    result = import_from(source, destination) if source else None

    destination.mkdir(parents=True, exist_ok=True)
    # Do not close the door on an import that partly failed. The marker used to
    # be written unconditionally, so one unreadable file meant the rest was
    # never retried - and the user was never asked to do anything about it.
    if result and result.get("failed"):
        return result
    (destination / MARKER).write_text(
        f"imported from {source}\n" if source else "nothing to import\n",
        encoding="utf-8")
    return result
