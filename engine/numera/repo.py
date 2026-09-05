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
import time
import uuid
from typing import Callable, Optional

from . import vault
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
    def dash_path(self, profile_id: str):
        from .paths import id_path

        return id_path(self.dash_dir, profile_id, ".json")

    def _dash_raw(self, profile_id: str) -> Optional[dict]:
        try:
            path = self.dash_path(profile_id)
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
        """Remove a profile from both stores and end its session - or nothing.

        The single deletion primitive. Both surfaces call it, because two
        implementations of "delete a profile" is what let the copies diverge in
        the first place.

        The order is deliberate and each step earns its place:

        1. **The session lease first, outside the transaction.** A profile
           another process has open must refuse immediately, before anything is
           touched, and without holding up every write in the data root while
           it does. It also closes the window the previous two-transaction
           version left open: between deleting the engine record and unlinking
           the dashboard copy, an API edit could recreate the engine twin, and
           the profile came back engine-only.
        2. **The root transaction**, so no reader sees one copy gone.
        3. **Detach the session**, which is reversible - a tombstone rename.
        4. **Both metadata files**, holding their bytes for rollback.
        5. On any failure, put the metadata back through the atomic writer and
           restore the session. A plain `write_bytes` truncates first, so a
           crash mid-rollback would leave an empty profile file - which reads
           as corruption rather than as the delete that did not happen.
        """
        from .paths import _restore_bytes, session_lease
        from .store import SessionStranded

        profile = self.store.resolve(ref)
        if profile is None:
            return False
        pid = profile.id

        with session_lease(self.root, pid):
            with transaction_lock(self.root):
                return self._delete_locked(pid, _restore_bytes, SessionStranded)

    def _delete_locked(self, pid: str, _restore_bytes, SessionStranded) -> bool:
        engine_record = self.store._path(pid)
        dash_record = self.dash_path(pid)
        if not dash_record.exists() and not engine_record.exists():
            return False

        tomb = self.store.detach_session(pid)

        undo = []
        try:
            for path in (dash_record, engine_record):
                if path.exists():
                    undo.append((path, path.read_bytes()))
                    path.unlink()
        except OSError as first:
            for path, original in reversed(undo):
                _restore_bytes(path, original)
            try:
                self.store.restore_session(pid, tomb)
            except SessionStranded as stranded:
                # Both halves failed. Surfacing only `first` would describe a
                # profile that exists with a session nothing points at, so the
                # message naming the tombstone is the one to raise.
                raise stranded from first
            raise

        # Logical commit is done; the tombstone is disposable from here. A
        # cleanup failure is a warning, not an ambiguous delete.
        self.store.delete_session(pid)
        return True

    def _engine_raw(self, profile_id: str) -> Optional[dict]:
        """The engine record exactly as it is on disk - ciphertext and all."""
        try:
            path = self.store._path(profile_id)
        except ValueError:
            return None
        if not path.exists():
            return None
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return None

    def _seal(self, profile: Profile, previous_dash: Optional[dict]):
        """The one value both schemas must store for this profile's password.

        Everything about secrets goes through `vault.encrypt_for_write`, which
        is the only place that gets all four cases right:

        * no vault - unchanged;
        * locked and the caller has nothing (a locked read yields `None`) -
          keep the ciphertext already on disk;
        * locked and the caller has a NEW plaintext - refuse, write nothing;
        * unlocked - encrypt, and never encrypt ciphertext twice.

        Writing `profile.to_dict()` straight into the batch skipped all of it.
        Two ways that lost data: a locked edit wrote `None` over the stored
        ciphertext in *both* copies, destroying the credential; and an unlocked
        edit wrote the plaintext into `dashboard/<id>.json`, because the
        dashboard converter carries `proxy.pass` in the clear - reintroducing a
        readable secret under an enabled vault.

        Returns the sealed value, or None when there is no proxy at all.
        """
        if profile.proxy is None:
            return None
        stored = ((self._engine_raw(profile.id) or {}).get("proxy") or {}).get("password")
        if not stored:
            # Fall back to the dashboard's copy: the two hold the same secret,
            # and either may be the one that survived.
            stored = ((previous_dash or {}).get("proxy") or {}).get("pass") or None
        return vault.encrypt_for_write(self.store, profile.proxy.password, stored)

    def _records(self, profile: Profile) -> tuple:
        """The exact contents both copies of one profile are about to get.

        Sealing happens here, before any batch opens, so a refusal means
        nothing was written - including nothing of the *other* profiles when a
        batch is being prepared.
        """
        previous = self._dash_raw(profile.id)
        sealed = self._seal(profile, previous)

        profile.updated_at = time.time()
        engine_data = profile.to_dict()
        if engine_data.get("proxy") is not None:
            engine_data["proxy"]["password"] = sealed

        twin = to_dashboard_profile(profile)
        twin["id"] = profile.id
        if twin.get("proxy") is not None:
            twin["proxy"]["pass"] = sealed or ""
        merged = _merge_dashboard(previous, twin)
        merged["_rev"] = int((previous or {}).get("_rev") or 0) + 1
        merged.setdefault("_created", time.time())
        return engine_data, merged

    def _write_both(self, profile: Profile,
                    config_change: Optional[dict] = None) -> Profile:
        """Engine record, dashboard twin and any config change, in one batch."""
        engine_data, merged = self._records(profile)

        engine_path = self.store._path(profile.id)
        dash_path = self.dash_path(profile.id)
        config_path = self.store.config_path
        with _lock_for(engine_path), _lock_for(dash_path):
            with self._config_lock(config_change), write_batch() as batch:
                batch.write(engine_path, engine_data)
                batch.write(dash_path, merged)
                if config_change:
                    batch.write(config_path,
                                {**self.store.get_config(), **config_change})
        return profile

    def materialize_current(self, profile_id: str) -> Optional[Profile]:
        """Bring the engine twin into step with the dashboard record as it is NOW.

        The browser routes used to do:

            d = dash.get(pid)                        # a snapshot
            ...                                      # seconds, or minutes
            engine_store.save(to_engine_profile(d))

        with real work in between. An edit committed in that window - by the
        API or the CLI, to both copies - was undone on the engine side by the
        older object being written back, and the browser then opened an
        identity the dashboard no longer described. Re-reading inside the
        transaction is the whole fix: what gets written is derived from the
        record that exists at the moment of writing, never from one read
        before it.

        The engine copy is the derived one, so only it is written, and only
        when it differs - dashboard writes already mirror into it, so usually
        there is nothing to do. Returns the profile a browser would open, or
        None when no such dashboard record exists.
        """
        with transaction_lock(self.root):
            raw = self._dash_raw(profile_id)
            if raw is None:
                return None
            profile = to_engine_profile(raw)
            profile.id = profile_id
            current = self._engine_raw(profile_id)
            # Not an edit. Keeping the engine record's own timestamps is what
            # lets an unchanged profile compare equal, so the common case
            # writes nothing at all.
            if current:
                profile.created_at = current.get("created_at", profile.created_at)
                profile.updated_at = current.get("updated_at", profile.updated_at)
            engine_data = profile.to_dict()
            if engine_data.get("proxy") is not None:
                engine_data["proxy"]["password"] = self._seal(profile, raw)
            if current == engine_data:
                return profile
            engine_path = self.store._path(profile_id)
            with _lock_for(engine_path), write_batch() as batch:
                batch.write(engine_path, engine_data)
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

    def save_many(self, profiles, *, config_change: Optional[dict] = None,
                  on_name_clash: str = "reject") -> list:
        """Write several profiles - and any config change - as one transaction.

        The CLI's `apply`, `bulk create` and vendor import each looped `save()`,
        so a failure on the Nth profile left the first N-1 written and the
        command reporting failure. Same contract as the HTTP bulk paths.

        Names are unique per store, and that decision belongs *here*.
        `bulk create` used to read the taken names, choose around them, and
        then call this - so two of them running at once both passed a check
        neither repeated under the lock, and created different ids with
        identical names, after which `resolve(name)` is ambiguous forever. The
        root lock serialised the writes; it could not re-make the decision.

        `on_name_clash` is the contract, and every caller states it:

        ``reject``  a taken name refuses the whole call with `NameTaken`, and
                    nothing is written - what `apply` and the vendor import
                    want, both of which are already all-or-nothing;
        ``skip``    a taken name drops that one profile and the rest are
                    written - what `bulk create` wants, generating a run of
                    names and already reporting how many it passed over.

        A collision *within* the batch counts either way: two pending profiles
        called the same thing produce exactly the ambiguity this prevents.

        Only a *new* claim is policed. A profile still carrying the name it
        already has on disk is not asking for anything, so `apply` over every
        profile stays a rename-free edit even in a store that already holds two
        profiles with one name - `restore --new-id` makes exactly that, on
        purpose - and an unrelated bulk edit does not fail on a duplicate it
        did not create.
        """
        if on_name_clash not in ("reject", "skip"):
            raise ValueError(f"unknown name-clash policy: {on_name_clash!r}")
        with transaction_lock(self.root):
            held, taken = {}, {}
            for existing in self.store.list():
                held[existing.id] = existing.name
                if existing.name:
                    taken.setdefault(existing.name, existing.id)
            pending = []
            for profile in profiles:
                claiming = profile.name and held.get(profile.id) != profile.name
                owner = taken.get(profile.name) if claiming else None
                if owner is not None and owner != profile.id:
                    if on_name_clash == "reject":
                        raise NameTaken(
                            f"a profile called {profile.name!r} already exists")
                    continue
                if claiming:
                    taken[profile.name] = profile.id
                pending.append(profile)

            # Every secret sealed before the batch opens, for the same reason
            # as `_write_both`: one locked replacement anywhere in the set must
            # refuse the whole request without having written any of it.
            saved = [(p, *self._records(p)) for p in pending]

            with self._config_lock(config_change), write_batch() as batch:
                for profile, engine_data, merged in saved:
                    batch.write(self.store._path(profile.id), engine_data)
                    batch.write(self.dash_path(profile.id), merged)
                if config_change:
                    batch.write(self.store.config_path,
                                {**self.store.get_config(), **config_change})
            return [p for p, _e, _m in saved]


def repo(store) -> ProfileRepo:
    """The coordinator for `store`'s data root."""
    return ProfileRepo(store)
