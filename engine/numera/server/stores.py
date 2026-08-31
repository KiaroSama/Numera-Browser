"""The two JSON-file stores the API owns.

`DashboardStore` holds the richer shape the GUI edits and mirrors every
write into the engine store, so a profile created here is visible to the
CLI and the other way round. `ProxyPool` is the shared pool both surfaces
read."""

from __future__ import annotations

import json
import time
import uuid
from pathlib import Path
from typing import Optional
from .convert import to_engine_profile


class DashboardStore:
    """Persists dashboard-shaped profile dicts, one JSON file per profile.

    **Every write mirrors into the engine store.** The two shapes are different -
    the dashboard's is richer (folders, per-signal modes, media counts) and the
    engine's is what a launch actually needs - but they must describe the same
    set of profiles. Before this, they did not: the engine copy was only written
    when a profile was launched, so a profile created in the GUI was invisible to
    `numera list` until someone pressed play, and one created by the CLI never
    appeared in the GUI at all (bug B17).

    Mirroring lives here rather than in each API route because there are five
    separate write paths (create, update, bulk, import, bulk-create). Fixing it
    per-route would leave the next one to be written broken again.
    """

    def __init__(self, root: Path, engine_store=None):
        self.dir = root / "dashboard"
        self.dir.mkdir(parents=True, exist_ok=True)
        self._engine = engine_store

    def _path(self, pid: str) -> Path:
        return self.dir / f"{pid}.json"

    def list(self) -> list[dict]:
        out = []
        for f in sorted(self.dir.glob("*.json")):
            try:
                out.append(json.loads(f.read_text(encoding="utf-8")))
            except json.JSONDecodeError:
                continue
        return sorted(out, key=lambda p: p.get("_created", 0))

    def get(self, pid: str) -> Optional[dict]:
        f = self._path(pid)
        return json.loads(f.read_text(encoding="utf-8")) if f.exists() else None

    def save(self, prof: dict) -> dict:
        if not prof.get("id"):
            prof["id"] = uuid.uuid4().hex[:12]
        prof.setdefault("_created", time.time())
        self._path(prof["id"]).write_text(json.dumps(prof, indent=2), encoding="utf-8")
        self._mirror(prof)
        return prof

    def _mirror(self, prof: dict) -> None:
        """Write the engine-shaped twin. Never fatal: a profile the dashboard
        can hold but the engine cannot convert must still save, or the UI would
        start refusing edits for a reason the user cannot see."""
        if self._engine is None:
            return
        try:
            self._engine.save(to_engine_profile(prof))
        except Exception:       # pragma: no cover - conversion is best-effort
            pass

    def delete(self, pid: str) -> bool:
        f = self._path(pid)
        if not f.exists():
            return False
        f.unlink()
        if self._engine is not None:
            # Leaving the twin behind would resurrect the profile in `numera
            # list` after the GUI said it was gone.
            try:
                self._engine.delete(pid)
            except Exception:   # pragma: no cover
                pass
        return True

    # Dashboard-only settings (folder list, etc.) live in a sibling file rather
    # than inside the dashboard/ dir, so list()'s "*.json" glob never mistakes
    # them for a profile.
    def _meta_path(self) -> Path:
        return self.dir.parent / "dashboard-config.json"

    def get_meta(self) -> dict:
        f = self._meta_path()
        if not f.exists():
            return {}
        try:
            return json.loads(f.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return {}

    def set_meta(self, meta: dict) -> dict:
        self._meta_path().write_text(json.dumps(meta, indent=2), encoding="utf-8")
        return meta


class ProxyPool:
    """A managed list of proxies the dashboard can import, test and assign."""

    def __init__(self, root: Path):
        self.path = root / "proxies.json"

    def list(self) -> list[dict]:
        if not self.path.exists():
            return []
        try:
            return json.loads(self.path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return []

    def _save(self, items: list[dict]):
        self.path.write_text(json.dumps(items, indent=2), encoding="utf-8")

    @staticmethod
    def _key(p: dict) -> tuple:
        # Sticky-session vendors hand out one host:port for the whole pool and
        # select the exit node from the credentials — some vary the username,
        # others vary only the password. Credentials are part of a proxy's
        # identity, so deduping on host:port alone collapses a whole vendor
        # list into a single entry.
        return (p["host"], str(p["port"]), p.get("user") or "", p.get("pass") or "")

    def add_many(self, proxies: list[dict]) -> list[dict]:
        items = self.list()
        seen = {self._key(p) for p in items}
        added = []
        for pr in proxies:
            key = self._key(pr)
            if key in seen:
                continue
            entry = {**pr, "id": uuid.uuid4().hex[:12], "status": "untested"}
            items.append(entry)
            seen.add(key)
            added.append(entry)
        self._save(items)
        return added

    def get(self, pid: str) -> Optional[dict]:
        return next((p for p in self.list() if p.get("id") == pid), None)

    def update(self, pid: str, patch: dict) -> Optional[dict]:
        items = self.list()
        out = None
        for p in items:
            if p.get("id") == pid:
                p.update(patch)
                out = p
        if out:
            self._save(items)
        return out

    def delete(self, pid: str) -> bool:
        items = self.list()
        kept = [p for p in items if p.get("id") != pid]
        if len(kept) == len(items):
            return False
        self._save(kept)
        return True
