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


class ProfileDiverged(RuntimeError):
    """The two copies of one profile hold different secrets.

    Every current write moves both copies together, so a disagreement means an
    older version, a hand edit or a restored backup left them apart. For
    ordinary fields that is answerable - the dashboard record is the authority,
    because it is the superset shape and the launch path already derives the
    engine copy from it - and the repair is silent.

    A password is different. Replacing a stored credential with a different one
    destroys something the user may not have anywhere else, and under a locked
    vault the two cannot even be compared. So this is raised instead of a
    write, and it is raised by the *launch* path rather than by reads: a
    listing or a lookup with one field in doubt is still useful, while opening
    a browser against a proxy credential Numera cannot justify is exactly the
    thing not to guess at.
    """

    def __init__(self, profile_id: str, field: str = "proxy.password"):
        self.profile_id, self.field = profile_id, field
        super().__init__(
            f"the two stored copies of profile {profile_id} hold different "
            f"proxy passwords, and neither can be shown to be the one you "
            f"meant, so Numera will not choose. Set the proxy password on this "
            f"profile again - one write moves both copies - or clear it. "
            f"Launches have been using the engine copy.")


#: What a record derived from the dashboard cannot be expected to match. The id
#: and the timestamps belong to the record rather than to the profile.
_NOT_A_DISAGREEMENT = frozenset({"id", "created_at", "updated_at"})

#: The engine fields the dashboard shape has no room for. It stores a screen
#: size and no viewport, so a viewport derived back from it differs by the
#: browser's own chrome - a conversion artefact, not a disagreement, and not
#: something a repair may overwrite.
_ENGINE_ONLY_FINGERPRINT = ("viewport_width", "viewport_height")


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
        from . import sessions
        from .paths import session_lease

        # A candidate, not a decision. Nothing is locked yet, so this only
        # says which profile to take the lease on.
        profile = self.store.resolve(ref)
        if profile is None:
            return False
        pid = profile.id

        with session_lease(self.root, pid):
            # The lease is free the moment a launcher dies; the browser it
            # started is not. Deleting a profile whose Chromium is still open
            # renames the directory out from under it.
            sessions.refuse_if_busy(self.root, pid)
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
                gone, tomb = self._delete_locked(pid)

        # Both locks are released here, and the delete is already committed.
        # What is left is garbage collection: a Chromium profile directory can
        # be hundreds of megabytes, and recursively deleting it under the root
        # transaction blocked *every* unrelated profile, config and vault write
        # in the data root - long enough to push them into the lock timeout.
        # Nothing can reference the tombstone any more, so nothing needs the
        # lock to reclaim it.
        if gone:
            sessions.settle(self.root, pid)
            self._reclaim(pid, tomb)
        return gone

    @staticmethod
    def _reclaim(pid: str, tomb) -> None:
        """Remove a detached session tree, and say so if it will not go."""
        import warnings

        from .store import ProfileStore

        if tomb is None:
            return
        if not ProfileStore.discard_tombstone(tomb):
            warnings.warn(
                f"{pid} is deleted, but its session is still on disk as {tomb}; "
                f"remove it once nothing holds it", RuntimeWarning, stacklevel=2)

    def _delete_locked(self, pid: str) -> tuple:
        from .paths import RollbackFailed, _restore_bytes
        from .store import SessionStranded

        engine_record = self.store._path(pid)
        dash_record = self.dash_path(pid)
        if not dash_record.exists() and not engine_record.exists():
            return False, None

        tomb = self.store._detach_session(pid)

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

        # Logical commit is done. The tombstone is handed back rather than
        # reclaimed here: removing it is a recursive delete of a whole browser
        # profile, and doing that under the root transaction made every
        # unrelated write in the data root wait for it. `delete()` reclaims it
        # once both locks are released - see `_reclaim`.
        #
        # (`delete_session(pid)` used to stand here, and was wrong silently: it
        # detaches again, and `user-data/<id>` no longer exists after the
        # detach above, so it found nothing and left the entire browser
        # identity on disk as `.deleting-*` forever.)
        return True, tomb

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
            self.store._restore_session(pid, tomb)
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

    def _secrets_for(self, profile: Profile, previous) -> tuple:
        """What each copy should store for the proxy password: `(engine, dash)`.

        One value used to be computed and written to both, which is right for a
        write that *says* something about the secret and wrong for one that
        does not. Under a locked vault the two copies hold two ciphertexts -
        `vault enable` encrypts each file separately - and this process cannot
        tell whether they are the same password. Writing one over the other to
        mirror a note change would destroy a credential to no purpose, so a
        withheld write leaves each copy exactly as it found it.

        A write that *does* speak - a replacement or an explicit clear - is one
        decision and both copies take it.
        """
        if profile.proxy is None:
            return None, None
        stored = ((self._engine_raw(profile.id) or {}).get("proxy") or {}).get("password")
        twin = ((previous or {}).get("proxy") or {}).get("pass")
        incoming = profile.proxy.password
        # Withheld, or an echo of what this profile already holds. `resolve()`
        # hands the caller the stored password and `edit()` passes that whole
        # object back, so "the value we just gave you" is not a decision about
        # the secret - it is the ordinary shape of every unrelated edit.
        if incoming is None or self._same_secret(incoming, stored):
            self.refuse_unresolved_secret(profile.id, stored, twin)
            return stored, twin
        chosen = vault.encrypt_for_write(self.store, incoming, stored)
        return chosen, chosen

    def refuse_unresolved_secret(self, profile_id: str, stored, twin) -> None:
        """Refuse a withheld write over a conflict only a person can settle.

        Three states, and the third is the one that must stay writable:

        * the two agree, or neither holds anything - nothing to settle;
        * one side holds a credential the other does not, or both are readable
          and different - a conflict, and an edit that says nothing about the
          secret may not proceed past it. `resolve()` hands back one copy, and
          before this an unrelated note or rename carried that copy into the
          other file and the second credential was gone;
        * both hold something and at least one cannot be read - a locked
          vault, where two ciphertexts of one password look exactly like two
          passwords. That is not a decision this process can make, so it makes
          none: the write goes through and `_secrets_for` leaves each copy
          alone.
        """
        if self._same_secret(stored, twin) or (not stored and not twin):
            return
        both_readable = (bool(stored) and bool(twin)
                         and vault.decrypt_for_read(self.store, stored) is not None
                         and vault.decrypt_for_read(self.store, twin) is not None)
        if both_readable or not stored or not twin:
            raise ProfileDiverged(profile_id)

    def _for_derivation(self, profile_id: str, stored, fresh):
        """What a *derived* engine record should store for the proxy password.

        A derivation is not an edit. It rebuilds the engine copy from the
        dashboard one, so it must never be the reason a credential changes.

        Three answers, and the middle one is the one that matters. If the two
        agree - the same string, or the same plaintext under two encryptions -
        the derived value stands. If neither can be read, the stored one stands:
        `vault enable` encrypts each file separately, so two *different*
        ciphertexts of one password is the ordinary state of every profile in
        an encrypted store, and a locked engine cannot tell that apart from a
        real conflict. Only when both are readable and genuinely different is
        there something to settle, and then nothing is written.
        """
        if not stored or self._same_secret(stored, fresh):
            return fresh
        if not fresh:
            raise ProfileDiverged(profile_id)
        readable = vault.decrypt_for_read(self.store, stored)
        if readable is None or vault.decrypt_for_read(self.store, fresh) is None:
            return stored                    # locked: keep, do not choose
        raise ProfileDiverged(profile_id)

    def _secret_disagrees(self, stored, fresh) -> bool:
        """Is there a secret disagreement here worth acting on?

        Not the same question as "are these two strings different". A locked
        store cannot read either of them, and after `vault enable` - which
        encrypts every file separately - two different ciphertexts of one
        password is what every profile in it looks like. Calling that a
        disagreement would have every listing try to repair every encrypted
        profile, forever, over nothing.
        """
        if self._same_secret(stored, fresh):
            return False
        if not stored:
            return bool(fresh)      # the dashboard has one the engine lacks
        if not fresh:
            return False            # a derivation keeps the stored one anyway
        return (vault.decrypt_for_read(self.store, stored) is not None
                and vault.decrypt_for_read(self.store, fresh) is not None)

    def _same_secret(self, stored, fresh) -> bool:
        """Do these two stored values hold the same secret?

        Ciphertext cannot be compared as text: every encryption uses a fresh
        nonce, so one password encrypted twice gives two different strings.
        With the vault open they are compared as what they mean. With it
        locked, neither can be read, and "I cannot tell" is answered as "not
        the same" - the conservative direction, because the caller uses this to
        decide whether it is safe to overwrite one of them.
        """
        if stored == fresh:
            return True
        if not stored or not fresh:
            return False
        left = vault.decrypt_for_read(self.store, stored)
        right = vault.decrypt_for_read(self.store, fresh)
        return left is not None and left == right

    @staticmethod
    def _keep_engine_only(profile: Profile, current: dict) -> None:
        """Carry across the fields only the engine record models.

        The dashboard shape stores a screen size and no viewport, so an engine
        record derived from it has a viewport recomputed from that screen. A
        profile created through the CLI, whose viewport was the screen less the
        browser's own chrome, therefore had its viewport quietly changed every
        time this ran - and it runs on every launch. The dashboard is the
        authority for the fields it *has*; one it cannot express is not a
        disagreement, and the stored value stands.

        Only while the screen still matches, though: a screen the user actually
        changed leaves the old viewport incoherent - larger than the display it
        sits in - so that case takes the derived one.
        """
        stored = (current or {}).get("fingerprint") or {}
        fresh = profile.fingerprint
        if (stored.get("screen_width"), stored.get("screen_height")) != \
                (fresh.screen_width, fresh.screen_height):
            return
        for field in _ENGINE_ONLY_FINGERPRINT:
            if stored.get(field):
                setattr(fresh, field, stored[field])

    def coherent(self, profile_id: str) -> Optional[Profile]:
        """The profile a browser may open, both copies brought into step first.

        The universal gate. Every path that opens a browser - `numera launch`,
        `attach`, a warm-up, a probe, the API, the scheduler - goes through
        `drivers.open_session`, and this is what it asks. Before, only the HTTP
        route reconciled and only the HTTP route could refuse, so a direct CLI
        launch would open a browser with a proxy credential the API had already
        decided it could not justify.

        Returns None when there is no such profile. Raises `ProfileDiverged`
        when the two copies hold different readable secrets, which is the one
        disagreement no derivation may settle.
        """
        with transaction_lock(self.root):
            engine, dash = self._engine_raw(profile_id), self._dash_raw(profile_id)
            if engine is None and dash is None:
                return None
            if dash is not None:
                self.materialize_current(profile_id)
            else:
                self.adopt_engine_record(profile_id)
            return self.store.get(profile_id)

    def adopt_engine_record(self, profile_id: str) -> bool:
        """Give an engine-only profile the dashboard twin it is missing.

        The other half of the union listing. A dashboard-only record has been
        repaired since 0.25 because `materialize_current` derives the engine
        copy from it; an engine-only one was listed and left one-sided, so
        "two copies, one invariant" quietly depended on someone opening the
        GUI, whose listing adopts it. A CLI-only machine never did.

        The engine record is the source and is not rewritten - it is healthy,
        and touching it would change timestamps for nothing. The secret is
        carried across exactly as stored, so an encrypted one stays encrypted
        and a locked vault is no obstacle to making the twin.
        """
        with transaction_lock(self.root):
            engine = self._engine_raw(profile_id)
            if engine is None or self._dash_raw(profile_id) is not None:
                return False
            twin = to_dashboard_profile(Profile.from_dict(engine))
            twin["id"] = profile_id
            if twin.get("proxy") is not None:
                twin["proxy"]["pass"] = (engine.get("proxy") or {}).get("password") or ""
            twin["_rev"] = 1
            twin.setdefault("_created", engine.get("created_at") or time.time())
            dash_path = self.dash_path(profile_id)
            with _lock_for(dash_path), write_batch() as batch:
                batch.write(dash_path, twin)
            return True

    def divergence(self, profile_id: str) -> list:
        """The fields the two stored copies of one profile disagree about.

        Empty for a healthy profile, and for one that exists on only one side -
        there is nothing to compare against, and that case is a repair rather
        than a conflict.

        The comparison is made in the engine shape, which is what normalises
        the two: the dashboard stores a port as a string and a proxy type in
        whatever case the client sent, and a record written through the GUI
        carries only the keys that client set, with the rest filled in on read.
        Comparing the raw JSON would call all of that a disagreement. Converting
        first asks the only question that matters - do these two describe the
        same profile.
        """
        return self._disagreements(self._engine_raw(profile_id),
                                   self._dash_raw(profile_id))

    def _disagreements(self, engine: Optional[dict], dash: Optional[dict]) -> list:
        """The same question, for two records already read.

        `list()` walks every profile and has both copies in hand, so it asks
        this rather than paying for the two reads a second time.
        """
        if not engine or not dash:
            return []
        try:
            fresh = to_engine_profile(dash).to_dict()
        except Exception:      # noqa: BLE001 - a record this cannot read
            return []          # cannot be compared, and will not be repaired
        out = []
        for key in sorted(set(engine) | set(fresh)):
            if key in _NOT_A_DISAGREEMENT:
                continue
            left, right = engine.get(key), fresh.get(key)
            if key == "fingerprint":
                left = {k: v for k, v in (left or {}).items()
                        if k not in _ENGINE_ONLY_FINGERPRINT}
                right = {k: v for k, v in (right or {}).items()
                         if k not in _ENGINE_ONLY_FINGERPRINT}
            elif key == "proxy":
                left, right = dict(left or {}), dict(right or {})
                if self._secret_disagrees(left.pop("password", None),
                                          right.pop("password", None)):
                    out.append("proxy.password")
                left, right = left or None, right or None
            if left != right:
                out.append(key)
        return out

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

    def refuse_conflicting_write(self, profile_id: str, incoming,
                                 has_proxy: bool) -> None:
        """The dashboard's half of the same rule. `incoming` is None when the
        write said nothing about the secret - see `refuse_unresolved_secret`.
        """
        if not has_proxy:
            return                       # the proxy is being removed outright
        stored = ((self._engine_raw(profile_id) or {}).get("proxy") or {}).get("password")
        twin = ((self._dash_raw(profile_id) or {}).get("proxy") or {}).get("pass")
        if incoming is not None and not self._same_secret(incoming, twin):
            return                       # a replacement or an explicit clear
        # Otherwise the write echoed this record's own stored value, which is
        # what a patch built from the stored record carries for every field it
        # is not changing.
        self.refuse_unresolved_secret(profile_id, stored, twin)

    def _records(self, profile: Profile) -> tuple:
        """The exact contents both copies of one profile are about to get.

        Sealing happens here, before any batch opens, so a refusal means
        nothing was written - including nothing of the *other* profiles when a
        batch is being prepared.
        """
        previous = self._dash_raw(profile.id)
        engine_secret, dash_secret = self._secrets_for(profile, previous)

        profile.updated_at = time.time()
        engine_data = profile.to_dict()
        if engine_data.get("proxy") is not None:
            engine_data["proxy"]["password"] = engine_secret

        twin = to_dashboard_profile(profile)
        twin["id"] = profile.id
        if twin.get("proxy") is not None:
            twin["proxy"]["pass"] = dash_secret or ""
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

        This is also the repair for a store whose two copies have drifted
        apart, so it is careful about the two things a derivation must not do:
        it does not reset a field the dashboard shape cannot express, and it
        does not replace a stored proxy password with a different one. The
        second raises `ProfileDiverged` rather than writing.
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
                self._keep_engine_only(profile, current)
            engine_data = profile.to_dict()
            if engine_data.get("proxy") is not None:
                engine_data["proxy"]["password"] = self._for_derivation(
                    profile_id, ((current or {}).get("proxy") or {}).get("password"),
                    self._seal(profile, raw))
            elif ((current or {}).get("proxy") or {}).get("password"):
                # The dashboard says there is no proxy at all while the engine
                # copy holds a credential. Writing the derivation would delete
                # it, and there is nowhere to keep it either.
                raise ProfileDiverged(profile_id)
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
