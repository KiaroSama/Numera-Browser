"""The one place a profile changes, whichever surface asked.

Numera keeps a profile twice. `profiles/<id>.json` is what a launch needs;
`dashboard/<id>.json` is the richer shape the GUI edits, with folders, per-signal
modes and media counts the engine has no field for. Two copies of one thing is a
design decision, not an accident - but it only works if every write moves both.

It did not. Dashboard writes mirrored into the engine, and the engine's copy was
adopted by the dashboard only when its id was *unknown*. For an id the dashboard
already had, `numera edit`, `regen`, `apply` and `align` rewrote the engine
record and left the dashboard's stale; `numera delete` removed the engine record
and left a dashboard copy that the next listing happily re-adopted, recreating
the profile the user had deleted.

The second half is the read-modify-write. Core CLI callers did:

    prof = store.resolve(ref)     # read
    prof.notes = "..."            # mutate a snapshot
    store.save(prof)              # write it whole, minutes later

which loses any edit made in between - the acknowledged-then-erased write the
storage locks alone cannot prevent, because the decision was made outside them.

`edit()` closes both: one transaction, read inside it, both copies written
inside it, dashboard-only fields preserved across an engine-shaped edit.

It lives here rather than under `server/` because the CLI needs it and must not
grow a dependency on the optional HTTP stack. Nothing in this module imports
FastAPI.
"""

from __future__ import annotations

import json
import uuid
from typing import Callable, Optional

from .convert import to_dashboard_profile, to_engine_profile
from .models import Profile
from .paths import _lock_for, transaction_lock, write_batch


class NameTaken(ValueError):
    """A profile with that name already exists."""


# Fields the dashboard owns and the engine has no room for. An engine-shaped
# edit must not erase them just by being written back through the converter.
def _merge_dashboard(previous: Optional[dict], fresh: dict) -> dict:
    """`fresh` (from the engine record) over `previous`, keeping what only the
    dashboard knows.

    The converter builds a complete dashboard object from an engine profile, so
    a naive overwrite drops every field the engine cannot represent. Anything
    the engine does not model is carried across from the stored copy.
    """
    if not previous:
        return fresh
    merged = {**previous, **fresh}
    for key in ("folder", "tags", "notes", "_created", "_rev", "_last_active",
                "_trust", "status", "lastActive"):
        if key in previous and key not in _ENGINE_OWNED:
            merged[key] = previous[key]
    # `tags` and `notes` exist in both shapes; the engine's win only when set.
    for key in ("tags", "notes"):
        if fresh.get(key):
            merged[key] = fresh[key]
    return merged


# What an engine-shaped edit is authoritative about. Everything else in the
# dashboard record survives the round trip.
_ENGINE_OWNED = frozenset({
    "name", "engine", "os", "browser", "browserVersion", "userAgent", "screen",
    "cores", "memory", "webglVendor", "webglRenderer", "locale", "timezone",
    "languages", "proxy", "startupUrls", "extensions", "bookmarks", "launchArgs",
    "dns", "ipBased", "clearOnStart", "switchedOff", "mediaVideo", "mediaAudio",
    "canvas", "webrtc", "audio", "fonts", "geo", "dnt",
})


class ProfileRepo:
    """Both copies of every profile, changed together or not at all.

    Built on the engine store; the dashboard copy is a sibling directory, so
    this needs no HTTP machinery and no second store object.
    """

    def __init__(self, store):
        self.store = store
        self.root = store.root
        self.dash_dir = self.root / "dashboard"

    # ---- paths -----------------------------------------------------------
    def _dash_path(self, profile_id: str):
        from .paths import id_path

        return id_path(self.dash_dir, profile_id, ".json")

    def _dash_raw(self, profile_id: str) -> Optional[dict]:
        try:
            path = self._dash_path(profile_id)
        except ValueError:
            return None
        if not path.exists():
            return None
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return None

    # ---- the one mutation ------------------------------------------------
    def edit(self, ref: str, apply: Callable[[Profile], Optional[Profile]]) -> Optional[Profile]:
        """Read, mutate and write both copies inside one transaction.

        `apply(profile)` receives the profile as it is *now* - not a snapshot
        taken before some slow work - and may mutate it in place or return a
        replacement. Returns the saved profile, or None when `ref` matches
        nothing.
        """
        with transaction_lock(self.root):
            profile = self.store.resolve(ref)
            if profile is None:
                return None
            updated = apply(profile) or profile
            return self._write_both(updated)

    def create(self, profile: Profile, *, unique_name: bool = True,
               config_change: Optional[dict] = None) -> Profile:
        """Save a new profile in both stores, refusing a duplicate name.

        The name check is inside the transaction. `find_by_name()` before
        `save()` is the same check-then-act two processes can both pass.

        `config_change` rides in the same transaction - `--engine` used to
        write the global default *before* the profile, so a failed save left
        the default changed by a command that reported failure.
        """
        with transaction_lock(self.root):
            if unique_name and profile.name:
                existing = self.store.find_by_name(profile.name)
                if existing is not None and existing.id != profile.id:
                    raise NameTaken(f"a profile called {profile.name!r} already exists")
            return self._write_both(profile, config_change)

    def delete(self, ref: str) -> bool:
        """Remove a profile from *both* stores and end its session.

        The dashboard copy used to survive an engine-side delete, and the next
        `GET /api/profiles` adopted it back - a deleted profile reappearing
        with the same id.
        """
        profile = self.store.resolve(ref)
        if profile is None:
            return False
        pid = profile.id
        gone = self.store.delete(pid)             # takes the lease, then the lock
        with transaction_lock(self.root):
            path = self._dash_path(pid)
            with _lock_for(path):
                path.unlink(missing_ok=True)
        return gone

    def _write_both(self, profile: Profile,
                    config_change: Optional[dict] = None) -> Profile:
        """Engine record, dashboard twin and any config change, in one batch."""
        previous = self._dash_raw(profile.id)
        twin = to_dashboard_profile(profile)
        twin["id"] = profile.id
        merged = _merge_dashboard(previous, twin)
        merged["_rev"] = int((previous or {}).get("_rev") or 0) + 1

        engine_path = self.store._path(profile.id)
        dash_path = self._dash_path(profile.id)
        config_path = self.store.config_path
        with _lock_for(engine_path), _lock_for(dash_path):
            with self._config_lock(config_change), write_batch() as batch:
                batch.write(engine_path,
                            self.store._apply_secrets(profile.to_dict(), "enc"))
                batch.write(dash_path, merged)
                if config_change:
                    batch.write(config_path,
                                {**self.store.get_config(), **config_change})
        return profile

    @staticmethod
    def _noop():
        from contextlib import nullcontext

        return nullcontext()

    def _config_lock(self, config_change):
        """`config.json`'s own lock, on the same hierarchy as everywhere else:
        root transaction first (the caller holds it), file lock second."""
        if not config_change:
            return self._noop()
        return _lock_for(self.store.config_path)

    def save_many(self, profiles, *, config_change: Optional[dict] = None) -> list:
        """Write several profiles - and any config change - as one transaction.

        The CLI's `apply`, `bulk create` and vendor import each looped `save()`,
        so a failure on the Nth profile left the first N-1 written and the
        command reporting failure. Same contract as the HTTP bulk paths.
        """
        with transaction_lock(self.root):
            saved = []
            for profile in profiles:
                previous = self._dash_raw(profile.id)
                twin = to_dashboard_profile(profile)
                twin["id"] = profile.id
                merged = _merge_dashboard(previous, twin)
                merged["_rev"] = int((previous or {}).get("_rev") or 0) + 1
                saved.append((profile, merged))

            with self._config_lock(config_change), write_batch() as batch:
                for profile, merged in saved:
                    batch.write(self.store._path(profile.id),
                                self.store._apply_secrets(profile.to_dict(), "enc"))
                    batch.write(self._dash_path(profile.id), merged)
                if config_change:
                    batch.write(self.store.config_path,
                                {**self.store.get_config(), **config_change})
            return [p for p, _ in saved]


def repo(store) -> ProfileRepo:
    """The coordinator for `store`'s data root."""
    return ProfileRepo(store)
