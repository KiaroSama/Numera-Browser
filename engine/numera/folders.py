"""Folders, shared by the HTTP API and the CLI.

Folders were a dashboard-only idea: they live in the dashboard's profile shape,
and ``to_engine_profile()`` drops the field, so the engine store has nothing to
put one on. That made folder create/rename/delete the only capabilities in the
product with no headless path at all (bug B18).

The fix is not a second implementation. The rules here — a rename moves every
profile in the folder, a delete reassigns rather than destroys them, the list is
deduplicated and order-preserving — are subtle enough that two copies would
drift, and the drift would be silent: the GUI and the CLI would simply disagree
about where a profile lives. So the logic lives here once and both surfaces call
it.

Storage is the same file the dashboard already used, so this reads and writes
what the GUI wrote and nothing has to be migrated.
"""

from __future__ import annotations

import json
from pathlib import Path

from .paths import _lock_for, write_batch, write_json
from typing import Optional

UNFILED = "Unfiled"


def _meta_path(root: Path) -> Path:
    # Sibling of dashboard/, not inside it: the profile list globs dashboard/*.json
    # and would otherwise read this as a profile.
    return Path(root) / "dashboard-config.json"


def _profiles_dir(root: Path) -> Path:
    return Path(root) / "dashboard"


def _read_meta(root: Path) -> dict:
    f = _meta_path(root)
    if not f.exists():
        return {}
    try:
        return json.loads(f.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}


def _write_meta(root: Path, meta: dict) -> None:
    write_json(_meta_path(root), meta)


def _profiles(root: Path) -> list[tuple[Path, dict]]:
    out = []
    d = _profiles_dir(root)
    if not d.is_dir():
        return out
    for f in sorted(d.glob("*.json")):
        try:
            out.append((f, json.loads(f.read_text(encoding="utf-8"))))
        except json.JSONDecodeError:
            continue
    return out


def listing(root: Path) -> list[str]:
    """Configured folders plus any a profile actually references.

    A folder that only exists because a profile sits in it still has to appear,
    or the GUI would show a profile filed somewhere the folder list denies."""
    meta = _read_meta(root)
    used = [p.get("folder") for _, p in _profiles(root) if p.get("folder")]
    return list(dict.fromkeys([*meta.get("folders", []), *used]))


def _folder_lock(root: Path):
    """The lock every folder change takes, keyed on the file they all rewrite.

    `create`, `rename` and `delete` each read the folder list, change it, and
    write it back - and `rename`/`delete` rewrite every profile in the folder as
    well. None of that was locked, so two concurrent folder operations each read
    the same list and the second write silently dropped the first one's change.

    One lock for the whole read-modify-write, not just the final write.
    """
    return _lock_for(_meta_path(root))


def _persist(root: Path, names: list[str]) -> list[str]:
    meta = _read_meta(root)
    meta["folders"] = list(dict.fromkeys(n for n in names if n))
    _write_meta(root, meta)
    return meta["folders"]


def create(root: Path, name: str) -> list[str]:
    with _folder_lock(root):
        name = (name or "").strip()
        if not name:
            raise ValueError("folder name required")
        return _persist(root, [*listing(root), name])


def rename(root: Path, src: str, dst: str) -> list[str]:
    with _folder_lock(root):
        src, dst = (src or "").strip(), (dst or "").strip()
        if not src or not dst:
            raise ValueError("both a source and a destination name are required")
        # Every profile in the folder, or none of them. `write_text` straight into
        # each file meant a failure part-way left some profiles moved and the rest
        # behind, under a folder list that had already been updated - a state no
        # screen shows correctly and no retry fixes.
        with write_batch() as batch:
            for f, p in _profiles(root):
                if p.get("folder") == src:
                    p["folder"] = dst
                    batch.write(f, p)
            return _persist(root, [dst if x == src else x for x in listing(root)])


def delete(root: Path, name: str, reassign_to: Optional[str] = None) -> list[str]:
    """Remove a folder. Its profiles are moved, never deleted."""
    with _folder_lock(root):
        name = (name or "").strip()
        if not name:
            raise ValueError("folder name required")
        target = (reassign_to or UNFILED).strip() or UNFILED
        with write_batch() as batch:
            for f, p in _profiles(root):
                if p.get("folder") == name:
                    p["folder"] = target
                    batch.write(f, p)
            return _persist(root, [x for x in listing(root) if x != name])
