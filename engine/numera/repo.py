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


class TargetMoved(RuntimeError):
    """The reference named one profile when we looked, and another when we acted.

    `delete("alpha")` resolved a profile, took its lease, and deleted it by id.
    Between those two steps another process could rename that profile to
    `beta` - and the delete went ahead, so both operations reported success
    even though no serial order of the two explains the result: after a
    successful rename, "delete alpha" should not have deleted the profile now
    called `beta`.

    Raised instead of deleting a stale target. Not `NameTaken`, and not a plain
    "not found": the profile is there, the name is there, and they no longer
    refer to each other, which is a different thing to tell someone.
    """

    def __init__(self, ref: str, expected: str, found: str):
        self.ref, self.expected, self.found = ref, expected, found
        super().__init__(
            f"{ref!r} meant profile {expected} when the command started and "
            f"means {found} now - something renamed one of them in between. "
            f"Nothing was deleted; re-run it against the id you meant.")


# Fields the dashboard owns and the engine has no room for. An engine-shaped
# edit must not erase them just by being written back through the converter.
def _merge_dashboard(previous: Optional[dict], fresh: dict) -> dict:
    """`fresh` (from the engine record) over `previous`, keeping what only the
    dashboard knows.

    The converter builds a complete dashboard object from an engine profile, so
    a naive overwrite drops every field the engine cannot represent. Anything
    the engine does not model is carried across from the stored copy.

    **Which copy owns a field is decided by the field, never by its value.** The
    previous version preserved the stored `tags` and `notes` and then let the
    engine's win *only when truthy*, which made clearing either of them
    impossible: `numera edit --notes ""` wrote the empty note to the engine
    record and left the dashboard showing the old one, and removing the last tag
    did the same. An empty string is a value a user chose, not a missing
    argument, and truthiness cannot tell those apart. Both fields are modelled
    by `Profile`, so both are engine-owned and an empty one propagates.
    """
    if not previous:
        return fresh
    merged = {**previous, **fresh}
    for key in ("folder", "_created", "_rev", "_last_active",
                "_trust", "status", "lastActive"):
        if key in previous and key not in _ENGINE_OWNED:
            merged[key] = previous[key]
    return merged


# What an engine-shaped edit is authoritative about. Everything else in the
# dashboard record survives the round trip.
_ENGINE_OWNED = frozenset({
    "name", "engine", "os", "browser", "browserVersion", "userAgent", "screen",
    "cores", "memory", "webglVendor", "webglRenderer", "locale", "timezone",
    "languages", "proxy", "startupUrls", "extensions", "bookmarks", "launchArgs",
    "dns", "ipBased", "clearOnStart", "switchedOff", "mediaVideo", "mediaAudio",
    "canvas", "webrtc", "audio", "fonts", "geo", "dnt",
    # Both shapes carry these, and `Profile` models them, so an engine-shaped
    # edit is authoritative about them - including when it sets them empty.
    "tags", "notes",
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
                self._refuse_taken_names([profile])
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
        from .paths import session_lease

        # A candidate, not a decision. Nothing is locked yet, so this only
        # says which profile to take the lease on.
        profile = self.store.resolve(ref)
        if profile is None:
            return False
        pid = profile.id

        with session_lease(self.root, pid):
            with transaction_lock(self.root):
                # Now that nothing else can move, ask the same question again.
                # `resolve` is id-first, so an exact id re-checks existence -
                # a delete that committed in the gap answers "no such profile"
                # instead of raising out of `_delete_locked` - and a name
                # re-answers *which* profile is called that. A name that has
                # moved to another id is refused rather than deleted, because
                # the request was about the name.
                current = self.store.resolve(ref)
                if current is None:
                    return False
                if current.id != pid:
                    raise TargetMoved(ref, pid, current.id)
                return self._delete_locked(pid)

    def _delete_locked(self, pid: str) -> bool:
        import warnings

        from .paths import RollbackFailed, _restore_bytes
        from .store import SessionStranded

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
            self._undo_delete(pid, tomb, undo, first,
                              RollbackFailed, _restore_bytes, SessionStranded)
            raise

        # Logical commit is done; the tombstone is disposable from here.
        #
        # `delete_session(pid)` was wrong here and silently so: it detaches
        # again, and `user-data/<id>` no longer exists after the detach above,
        # so it found nothing, reclaimed nothing, and left the entire browser
        # identity on disk as `.deleting-*` forever. Every successful delete
        # leaked one. The tombstone we actually made is the thing to discard.
        if not self.store.discard_tombstone(tomb):
            warnings.warn(
                f"{pid} is deleted, but its session is still on disk as {tomb}; "
                f"remove it once nothing holds it", RuntimeWarning, stacklevel=2)
        return True

    def _undo_delete(self, pid, tomb, undo, first,
                     RollbackFailed, _restore_bytes, SessionStranded) -> None:
        """Put back everything this delete had already removed, then report.

        Rollback is an outcome, not a gesture. The previous version restored
        the metadata in a bare loop, so the first `_restore_bytes` failure
        propagated out of the handler - the remaining record was never put
        back, `restore_session` was never called, and what reached the caller
        described neither the delete that failed nor the damage that was left.

        Every step is attempted, every failure is collected, and if any of them
        failed the caller gets `RollbackFailed` carrying the original error and
        each path that could not be restored. A repair needs the list; an
        exception that hides half of it is worse than the failure itself.
        """
        failures = []
        for path, original in reversed(undo):
            try:
                _restore_bytes(path, original)
            except OSError as err:
                failures.append((path, err))
        try:
            self.store.restore_session(pid, tomb)
        except SessionStranded as stranded:
            failures.append((tomb, stranded))
        except OSError as err:                               # pragma: no cover
            failures.append((tomb, err))
        if failures:
            raise RollbackFailed(first, failures) from first

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

    def _name_taken(self, profile: Profile, accepted: list) -> bool:
        """Would writing `profile` claim a name something else already holds?

        The rule itself is `store.name_clash`, and it is the *same call* the
        HTTP surface makes from `DashboardStore._claim_names`. Two
        implementations of "is this name free" is precisely the divergence this
        audit was about: the CLI refused duplicates the API happily created,
        and `resolve(name)` then answered for one of two profiles without ever
        saying which.

        `accepted` is what this batch has already agreed to write, so two
        pending profiles asking for one name collide with each other and not
        only with the disk.

        ponytail: asked once per profile, so a batch of n rebuilds the name map
        n times. A few hundred profiles is nothing; if bulk create ever runs to
        tens of thousands, hoist the map.
        """
        from .store import name_clash, stored_names

        stored = stored_names(self.store.profiles_dir, self.dash_dir)
        stored += [(p.id, p.name) for p in accepted if p.name]
        return name_clash(stored, [(profile.id, profile.name)]) is not None

    def _refuse_taken_names(self, profiles: list) -> None:
        """Raise `NameTaken` for the first profile claiming a taken name."""
        accepted: list = []
        for profile in profiles:
            if self._name_taken(profile, accepted):
                raise NameTaken(f"a profile called {profile.name!r} already exists")
            accepted.append(profile)

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
        with transaction_lock(self.root):
            return self._write_batch(profiles, config_change, on_name_clash)

    def patch_many(self, refs, apply: Callable[[Profile], object], *,
                   config_change: Optional[dict] = None,
                   on_name_clash: str = "reject") -> list:
        """Re-read every profile, change it, and write them all as one transaction.

        The difference from `save_many` is *where the decision is made*, and it
        is the whole point. `apply` and `pool assign` did:

            targets = select(store.list(), ...)   # read
            for prof in targets:                  # mutate stale snapshots
                apply_patch(prof, patch)
            repo.save_many(targets)               # write them back whole

        `save_many` made that write atomic, which fixed the half-applied
        command - but it still wrote objects read before any lock was held, so
        an API edit committing in between was acknowledged and then overwritten
        by a bulk command that never saw it. Serialising a write cannot repair a
        decision taken outside it.

        Here the transaction opens first and everything happens inside it:
        re-read each profile as it is *now*, hand it to `apply`, check names,
        seal secrets, write once. `apply(profile)` mutates the profile in place
        and returns something truthy for "write this one" - `bulk.apply_patch`
        already returns the list of fields it changed, which is exactly that -
        or something falsey to leave that profile alone.

        A ref matching nothing is skipped rather than failing the batch: a
        profile deleted between the selection and the write is not an error, it
        is a profile that is no longer there. Returns the profiles written.
        """
        with transaction_lock(self.root):
            pending = []
            for ref in refs:
                current = self.store.resolve(ref)
                if current is None:
                    continue
                if apply(current):
                    pending.append(current)
            return self._write_batch(pending, config_change, on_name_clash)

    def _write_batch(self, profiles, config_change: Optional[dict],
                     on_name_clash: str) -> list:
        """The shared body of `save_many` and `patch_many`.

        The caller already holds the root transaction, so every decision made
        here - which names are free, what the stored ciphertext is - is made
        under the same lock that performs the write.
        """
        if on_name_clash not in ("reject", "skip"):
            raise ValueError(f"unknown name-clash policy: {on_name_clash!r}")
        pending = []
        for profile in profiles:
            if self._name_taken(profile, pending):
                if on_name_clash == "reject":
                    raise NameTaken(
                        f"a profile called {profile.name!r} already exists")
                continue
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
