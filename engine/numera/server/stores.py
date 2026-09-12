"""The two JSON-file stores the API owns.

`DashboardStore` holds the richer shape the GUI edits and mirrors every
write into the engine store, so a profile created here is visible to the
CLI and the other way round. `ProxyPool` is the shared pool both surfaces
read."""

from __future__ import annotations

import hashlib
import json
import secrets
import time
import uuid
from pathlib import Path
from contextlib import contextmanager
from typing import Optional
from .. import vault
from ..proxypool import identity as vault_identity
from ..paths import _lock_for, id_path, transaction_lock, write_json
from . import auth_policy
from ..repo import NameTaken
from ..store import name_clash, stored_names
from .convert import to_engine_profile


@contextmanager
def _config_lock(engine, config_change):
    """Hold `config.json`'s own lock for a batch that writes it.

    The batch wrote config through `batch.write`, which takes no file lock, so
    a profile transaction and an ordinary settings change were two independent
    lock domains: neither waited for the other, and a rollback could restore
    config over a settings change that had already succeeded. Config writers now
    take `transaction_lock(root)` first and this second - one order, one
    hierarchy. The lock spans the whole batch so the rollback is covered too,
    and the read of the current config happens under the lock that protects its
    write.
    """
    if not config_change or engine is None:
        yield
        return
    with _lock_for(engine.config_path):
        yield


class StaleProfile(RuntimeError):
    """A full-profile write carried a revision older than the stored one."""


class UnconvertibleProfile(ValueError):
    """This dashboard record has no engine shape, so it was not written.

    The two stores are meant to hold the same set of profiles. `_twin` used to
    swallow a conversion failure and return `None`, which wrote the dashboard
    copy on its own: the API answered 200, the GUI showed the profile and
    `numera list` had never heard of it. Refusing is the honest answer, and the
    API turns it into a 422 naming the reason.
    """


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
        self.root = Path(root)
        self.dir = self.root / "dashboard"
        self.dir.mkdir(parents=True, exist_ok=True)
        self._engine = engine_store

    def _path(self, pid: str) -> Path:
        return id_path(self.dir, pid, ".json")

    # Every read below joins the root transaction, for the reason
    # `ProfileStore.get` does: what a decrypted record *means* depends on the
    # record and `config.json` agreeing, and a vault migration rewrites every
    # secret-bearing file before committing the descriptor that describes the
    # key. A read that crossed that boundary saw a sealed record under a config
    # that said "no vault", and `decrypt_for_read` answers `None` for it -
    # indistinguishable from "this profile has no proxy password". The engine
    # store was given this boundary and these two were not, so the same request
    # could be told both stories at once.
    #
    # The lock is reentrant, so the write paths that already hold it pay
    # nothing, and a read is only ever delayed by a migration actually in
    # flight.
    def list(self) -> list[dict]:
        with transaction_lock(self.root):
            out = []
            for f in sorted(self.dir.glob("*.json")):
                try:
                    out.append(self._decrypted(json.loads(f.read_text(encoding="utf-8"))))
                except json.JSONDecodeError:
                    continue
            return sorted(out, key=lambda p: p.get("_created", 0))

    def get(self, pid: str) -> Optional[dict]:
        # An id that cannot name a file is a miss, not an error: routes look
        # profiles up by whatever the caller sent. Writes still refuse.
        try:
            f = self._path(pid)
        except ValueError:
            return None
        with transaction_lock(self.root):
            if not f.exists():
                return None
            return self._decrypted(json.loads(f.read_text(encoding="utf-8")))

    # The vault encrypts the proxy password in the engine's own profile file.
    # This store writes a second copy of the same profile, and it was writing
    # that password in plaintext - so turning the vault on protected one file
    # and left the secret readable in the one beside it.
    SECRET = ("proxy", "pass")

    def _raw(self, pid: str) -> dict:
        """The stored record, undecrypted.

        `get()` yields `None` for the secret while the vault is locked, which is
        deliberate - handing the ciphertext to a caller would send it onwards as
        if it were the password. But deciding what to *keep* on a locked save
        needs the ciphertext itself, so that one caller reads the file directly.

        In the transaction like its siblings: the value read here decides
        whether a secret is preserved or replaced, and a migration rewriting
        this file is exactly the writer that must not be observed half-done.
        """
        try:
            f = self._path(pid)
        except ValueError:
            return {}
        with transaction_lock(self.root):
            if not f.exists():
                return {}
            try:
                return json.loads(f.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                return {}

    def _guard_secrets(self, prof: dict, intent: str = None) -> dict:
        """Return a copy that never carries a plaintext secret past the vault.

        The rule itself lives in `vault.encrypt_for_write` - the engine store
        and the proxy pool need exactly the same one, and three copies of it
        drifted apart once already. `intent` says which of its three cases this
        write is; without one it is read from the record, which is what a
        caller outside the request path (a mirror, a migration) has.
        """
        from ..repo import CLEAR, PRESERVE, REPLACE

        a, b = self.SECRET
        node = prof.get(a)
        if not isinstance(node, dict):
            return prof
        stored = (self._raw(prof.get("id", "")).get(a) or {}).get(b)
        if intent is None:
            asked = node.get(b)
        else:
            asked = {PRESERVE: None, CLEAR: "", REPLACE: node.get(b)}[intent]
        kept = vault.encrypt_for_write(self._engine, asked, stored)
        if kept == node.get(b):
            return prof
        return {**prof, a: {**node, b: kept}}

    def _decrypted(self, prof: dict) -> dict:
        """Readable text, empty for no password, or null for a withheld secret."""
        a, b = self.SECRET
        node = prof.get(a)
        if not isinstance(node, dict):
            return prof
        if node.get(b) is None:
            return {**prof, a: {**node, b: ""}}
        return {**prof, a: {**node, b: vault.decrypt_for_read(self._engine, node[b])}}

    REVISION = "_rev"

    def save(self, prof: dict, *, config_change=None, expect_rev=None,
             secret_intent=None) -> dict:
        """Write one profile, and optionally a config change, as one unit.

        `config_change` exists so a request that owns both - "save this profile
        *and* make its engine the default" - commits them together. They were
        two separate writes, and both orders were wrong: config first meant a
        failed profile write left the default changed; config second meant a
        failed config write reported failure over profile changes that had
        already landed, so the obvious retry applied them twice.

        `expect_rev` is the optimistic-concurrency check. A client that submits
        a whole profile - which the dashboard does, even for an inline rename -
        carries a copy that may already be stale, and without this the later
        writer silently erases the earlier one's fields.
        """
        # Transaction lock outermost, per-file lock inside - the one order every
        # profile mutation uses. Without it a single save could land between a
        # batch's snapshot and its rollback, and the rollback then wrote the old
        # bytes over a change that had already succeeded.
        with transaction_lock(self.root):
            path = self._path(prof.get("id") or uuid.uuid4().hex[:12])
            with _lock_for(path):
                return self._save_locked(prof, config_change, expect_rev,
                                         secret_intent)

    def patch(self, pid: str, apply, *, expect_rev=None, secret_intent=None):
        """Read, merge, validate and write one profile inside one lock.

        `apply(stored)` returns `(merged, config_change)` and runs while the
        lock is held, so what it merges onto cannot change under it.

        The route used to do the read and the merge itself and only then call
        `save()`, where the lock was taken. `expect_rev` closed that for a
        client sending a whole profile, but a *patch* carries no revision - so
        two concurrent patches could read the same record, build different full
        merged copies, and the second write would carry stale values for every
        field the first had just changed. Moving the merge inside the lock is
        what fixes the patch case; the revision remains for the stale-client
        case.

        Returns None when the profile does not exist.
        """
        with transaction_lock(self.root):
            path = self._path(pid)
            with _lock_for(path):
                stored = self._raw(pid)
                if not stored:
                    return None
                merged, config_change = apply(dict(stored))
                return self._save_locked(merged, config_change, expect_rev,
                                         secret_intent)

    def patch_many(self, ids, apply, *, config_change=None,
                   secret_intent=None) -> list:
        """The same for a bulk edit: every record is read under the lock.

        `apply(pid, current)` returns the merged profile, or None to skip.
        `save_many()` was called with a `pending` list the route had already
        built from unlocked reads, so a bulk edit could carry stale values just
        as a single patch could.
        """
        with transaction_lock(self.root):
            pending = []
            for pid in ids:
                current = self.get(pid)
                if not current:
                    continue
                merged = apply(pid, current)
                if merged is not None:
                    pending.append(merged)
            return self._save_many_locked(pending, config_change, secret_intent)

    def _claim_names(self, profiles: list) -> None:
        """Refuse a write that would give one name to a second profile.

        The check has to be here, inside the transaction that writes, and it
        has to be on *this* path: `ProfileRepo` has held the same invariant for
        the CLI since 0.21.0, but every HTTP write - single create, batch
        create, `PUT` rename, bulk update including a name patch, and the
        vendor import - goes through this store instead. So the API happily
        created the duplicate names `numera create` refuses, and from then on
        `resolve(name)` returned whichever of them the directory listed first:
        `numera delete <name>` destroyed an arbitrary one of the two.

        Ids are minted first because a profile with no id yet is a new profile
        by definition, and the check needs to know which id is claiming what.

        Refuses before any file is opened. A batch that raised mid-write would
        unwind correctly, but a rollback is a real filesystem operation with
        its own failure mode, and there is nothing to roll back if the request
        never starts.
        """
        for prof in profiles:
            if not prof.get("id"):
                prof["id"] = uuid.uuid4().hex[:12]
        # Both directories: a dashboard write mirrors into the engine store and
        # a repo write mirrors back, so either copy may be the one carrying a
        # name, and a name held in only one of them is still held.
        known = [self.dir] + ([self._engine.profiles_dir] if self._engine else [])
        clash = name_clash(stored_names(*known),
                           [(p.get("id"), p.get("name")) for p in profiles])
        if clash:
            raise NameTaken(f"a profile called {clash[1]!r} already exists")

    def _save_locked(self, prof: dict, config_change, expect_rev,
                     secret_intent=None) -> dict:
        from ..paths import write_batch

        if not prof.get("id"):
            prof["id"] = uuid.uuid4().hex[:12]
        self._claim_names([prof])
        prof.setdefault("_created", time.time())
        stored = self._raw(prof["id"]) or {}
        if expect_rev is not None and stored:
            current = stored.get(self.REVISION)
            if current is not None and str(current) != str(expect_rev):
                raise StaleProfile(
                    f"profile {prof['id']} changed since you loaded it "
                    f"(revision {current}, you sent {expect_rev})")
        # Both of these read the stored record, so both belong inside the lock.
        # `_guard_secrets` ran outside it and could decide whether to preserve a
        # secret from a record another writer was already replacing;
        # `_resolve_folder` had the same problem with a folder rename.
        prof, twin = self._prepare(prof, secret_intent)
        prof[self.REVISION] = int(stored.get(self.REVISION) or 0) + 1

        with _config_lock(self._engine, config_change), write_batch() as batch:
            batch.write(self._path(prof["id"]), prof)
            if twin is not None:
                # Both copies or neither. The mirror's failure used to be
                # swallowed, so a locked vault produced a dashboard that said
                # "saved" over an engine copy still holding the old proxy.
                batch.write(*twin)
            if config_change:
                batch.write(self._engine.config_path,
                            {**self._engine.get_config(), **config_change})
        return prof

    def _prepare(self, prof: dict, intent: str):
        """The dashboard record and its engine twin, with the secret rule applied.

        The single-profile path grew this in 0.27 and the bulk path did not, so
        `POST /api/profiles/bulk` - a notes or tags edit across a selection -
        went round every safeguard the single write had: no refusal on an
        unresolved conflict, and the dashboard's blob mirrored over the
        engine's. One primitive, both callers, because the second copy of a
        rule is the one that silently stops matching.

        `intent` is what the *request* said about the secret, not what the
        merged record happens to hold - see `secret_intent`.
        """
        from ..repo import PRESERVE, REMOVE, repo as _repo, secret_intent

        if intent is None:
            # No caller said. Read it from the record being written, which is
            # right for a create or an adoption - the record *is* the request -
            # and is why a request path must pass its own: a patch built on the
            # stored profile echoes the stored password, and that is not a
            # decision about it.
            intent = secret_intent(prof, self.SECRET[0])
        if self._engine is not None:
            _repo(self._engine).refuse_conflicting_write(prof["id"], intent)
        if intent == REMOVE:
            self._refuse_unmeant_removal(prof.get("id", ""))
        prof = self._guard_secrets(prof, intent)
        prof = self._resolve_folder(prof)
        return prof, self._twin(prof, keep_engine_secret=intent == PRESERVE)

    def _refuse_unmeant_removal(self, pid: str) -> None:
        """The two ways `proxy: null` can destroy something nobody chose.

        **A credential this process cannot read.** Removing the proxy removes
        the password with it, so it is the same destructive act as an explicit
        clear and takes the same rule: under a locked vault a stored secret is
        not destroyed by a process that cannot tell what it is.
        `_guard_secrets` cannot enforce that here - a write that removes the
        proxy carries no password field for it to look at - so this asks the
        one primitive that owns the rule, once per stored copy.

        **A proxy the caller was never shown.** A removal settles a readable
        two-password divergence, because the client saw a proxy and chose to
        delete it. It settles nothing when only the *engine* copy has one: the
        listing shows no proxy at all in that case, so a client sending the
        whole record back sends `null` for a proxy it never knew about - and
        reading that as a decision would erase a credential on an unrelated
        edit. That is the same defect as the password echo, and it gets the
        same answer: this is not a decision, so it is refused.
        """
        a, b = self.SECRET
        dash = (self._raw(pid).get(a) or {})
        sealed = [dash.get(b)]
        if self._engine is not None:
            from ..repo import ProfileDiverged, repo as _repo

            engine = ((_repo(self._engine)._engine_raw(pid) or {}).get(a) or {})
            sealed.append(engine.get("password"))
            if engine and not dash:
                raise ProfileDiverged(pid, "proxy")
        for stored in sealed:
            vault.encrypt_for_write(self._engine, "", stored)

    def _twin(self, prof: dict, keep_engine_secret: bool = False):
        """`(path, data)` for the engine's copy of `prof`, or None with no engine.

        A profile the engine cannot *model* is refused, not stored alone. The
        opposite used to be the rule - "refusing would take the UI down for a
        reason the user cannot see" - and what it actually produced was a
        dashboard record with no engine copy: a 200, a profile visible in the
        GUI, and nothing at all for the CLI or a launch. The reason the user
        could not see is now in the refusal, and the API routes validate the
        same shape before they get here, so the honest failure arrives as a 422
        on the field rather than as a store that quietly holds half a profile.

        `keep_engine_secret` is set when the write said nothing about the proxy
        password. The engine copy then keeps its own stored value instead of
        taking the dashboard's: under a locked vault the two are different
        ciphertexts that may or may not be the same password, and mirroring a
        note change is not a reason to decide.
        """
        if self._engine is None:
            return None
        try:
            twin = to_engine_profile(prof)
        except Exception as exc:                # noqa: BLE001 - reported, not hidden
            raise UnconvertibleProfile(
                f"profile {prof.get('id') or prof.get('name') or '?'} cannot be "
                f"converted for the engine, so neither copy was written: {exc}"
            ) from exc
        if twin is None:                        # pragma: no cover - defensive
            raise UnconvertibleProfile(
                f"profile {prof.get('id') or '?'} has no engine shape")
        data = self._engine._apply_secrets(twin.to_dict(), "enc")
        if keep_engine_secret and isinstance(data.get("proxy"), dict):
            from ..repo import repo as _repo

            stored = ((_repo(self._engine)._engine_raw(twin.id) or {})
                      .get("proxy") or {}).get("password")
            data["proxy"]["password"] = stored
        return (self._engine._path(twin.id), data)

    def _resolve_folder(self, prof: dict) -> dict:
        """Follow a folder rename that retired this profile's folder name.

        `folders.listing()` derives the folder list from profiles as well as
        from the meta file, so one profile still naming a renamed-away folder
        brings the old name back into the sidebar with that profile alone in it.
        """
        from .. import folders

        name = prof.get("folder")
        if not name:
            return prof
        moved = folders.resolve_name(self.root, name)
        return prof if moved == name else {**prof, "folder": moved}

    def save_many(self, profiles: list, *, config_change=None,
                  secret_intent=None) -> list:
        """Save a batch as one transaction: all of them, or none.

        Validation was already all-or-nothing, but the writes were a plain loop,
        so a storage failure part-way (disk full, a permission error, a failing
        mirror) left the earlier profiles on disk while the request reported
        failure - and the obvious retry then produced duplicates.

        Both copies of every profile are registered with one `write_batch`, so a
        failure anywhere unwinds the dashboard files and the engine twins
        together.
        """
        with transaction_lock(self.root):
            return self._save_many_locked(profiles, config_change, secret_intent)

    def _save_many_locked(self, profiles: list, config_change,
                          secret_intent=None) -> list:
        from ..paths import write_batch

        # Before the batch opens, and over the whole set at once: two entries of
        # one batch naming the same profile are the ambiguity just as surely as
        # a collision with something already stored, and a bulk rename of a
        # selection onto one name is the fastest way to produce a dozen of them.
        self._claim_names(profiles)
        saved = []
        with _config_lock(self._engine, config_change), write_batch() as batch:
            for prof in profiles:
                if not prof.get("id"):
                    prof["id"] = uuid.uuid4().hex[:12]
                prof.setdefault("_created", time.time())
                prof, twin = self._prepare(prof, secret_intent)
                # Before the write, and from the record as it was *before* it.
                # This ran after `batch.write`, so the revision never reached
                # disk: a batch create returned `_rev=1` over a file with no
                # revision at all, and a bulk update returned N+1 while disk
                # still said N. Every one of those answers was a revision a
                # client could hold and be wrongly rejected - or wrongly
                # accepted - on later.
                prof[self.REVISION] = int(
                    (self._raw(prof["id"]) or {}).get(self.REVISION) or 0) + 1
                batch.write(self._path(prof["id"]), prof)
                if twin is not None:
                    batch.write(*twin)
                saved.append(prof)
            if config_change:
                # In the same batch, so a failed config write rolls the profiles
                # back with it. Writing config afterwards meant the API could
                # report failure over changes that had already committed.
                batch.write(self._engine.config_path,
                            {**self._engine.get_config(), **config_change})
        return saved

    def _mirror(self, prof: dict) -> None:
        """Write the engine-shaped twin.

        A profile the dashboard can hold but the engine cannot *model* still
        saves - refusing would take the UI down for a reason the user cannot
        see. A failure to *write* one it modelled fine is a different thing and
        is raised: that is the store telling us it could not store, and
        continuing would leave the two copies describing different profiles.
        """
        if self._engine is None:
            return
        try:
            twin = to_engine_profile(prof)
        except Exception:       # pragma: no cover - conversion is best-effort
            return
        # `_save_engine`, not `save`: this IS the mirror, and the coordinator
        # would write the dashboard record we are already inside.
        self._engine._save_engine(twin)

    def delete(self, pid: str) -> bool:
        """Remove both copies of a profile, and its session last.

        Delegates to the core primitive: `ProfileRepo.delete` owns the order,
        the rollback and - the part this path was missing - the session lease.
        Without it, `detach_session`'s rename was not proof of ownership on
        POSIX, so an API DELETE could move a directory another process had a
        browser open on.

        The old implementation lived below this one for a while, and Python
        keeps the LAST definition of a name in a class body - so this method
        was dead the moment it was added and every API delete still ran the
        unleased path. There is one `delete` in this class, and this is it.
        """
        from ..repo import repo

        if self._engine is None:                          # pragma: no cover
            return False
        return repo(self._engine).delete(pid)

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
        path = self._meta_path()
        with _lock_for(path):
            write_json(path, meta)
        return meta


@contextmanager
def _pool_lock(root, path):
    with transaction_lock(root), _lock_for(path):
        yield


class ProxyPool:
    """A managed list of proxies the dashboard can import, test and assign."""

    # The pool holds the same kind of secret the profiles do, so it needs the
    # same vault. `engine_store` is how it sees one; without it the pool keeps
    # its old plaintext behaviour, which is right for a caller that has no data
    # root of its own but wrong for the two that do - both pass it.
    def __init__(self, root: Path, engine_store=None):
        self.root = Path(root)
        self.path = self.root / "proxies.json"
        self._engine = engine_store

    def _stored(self) -> list[dict]:
        """The file as written, secrets still encrypted."""
        if not self.path.exists():
            return []
        try:
            raw = json.loads(self.path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return []
        return raw if isinstance(raw, list) else []

    def list(self) -> list[dict]:
        """Every proxy, with its password readable - or `None` while locked.

        Decrypting here rather than at each call site is what keeps the rest of
        the pool working now that the vault covers `proxies.json`. Dedupe, the
        "used by N profiles" count and `identity()` all key on the credentials,
        and encryption is randomised: the same password stored twice produces
        two different ciphertexts, so comparing those would make every proxy
        look unique and every usage count fall to zero.

        In the root transaction, like every other read that decrypts. The pool
        *writers* already took it; the readers did not, so `proxies.json` -
        which `vault.SITES` names and a migration rewrites - could be read
        after it had been sealed and before the descriptor committed. Every
        credential came back `None`, which the callers cannot tell apart from
        "this proxy has no password".
        """
        with transaction_lock(self.root):
            return [self._decrypted(p) for p in self._stored()]

    def secret_withheld(self, pid: str) -> bool:
        """Does this pool entry have a password the lock state hides?

        Reads the stored bytes: `list()` hands out `None` for a locked secret,
        which is exactly what a caller cannot tell apart from "no password".
        This is the read whose answer a half-committed migration corrupts most
        directly, so it takes the same boundary.
        """
        with transaction_lock(self.root):
            for entry in self._stored():
                if entry.get("id") == pid:
                    return vault.is_withheld(self._engine, entry.get("pass"))
            return False

    def _decrypted(self, entry: dict) -> dict:
        if not entry.get("pass"):
            return entry
        return {**entry, "pass": vault.decrypt_for_read(self._engine, entry["pass"])}

    def _encrypted(self, entry: dict, stored: dict) -> dict:
        """What to write for `entry`, given what is already on disk for it."""
        kept = vault.encrypt_for_write(self._engine, entry.get("pass"),
                                       (stored or {}).get("pass"))
        return entry if kept == entry.get("pass") else {**entry, "pass": kept}

    def _save_entries(self, items: list[dict]) -> None:
        """Write a decrypted view back, re-encrypting as it goes.

        `list()` hands out plaintext, so every caller that reads-modifies-writes
        would otherwise put the plaintext straight back and quietly undo the
        vault.
        """
        by_id = {e.get("id"): e for e in self._stored()}
        self._save([self._encrypted(e, by_id.get(e.get("id"))) for e in items])

    def _save(self, items: list[dict]):
        write_json(self.path, items)

    @property
    def _lock(self):
        """Root transaction first, then `proxies.json`.

        Every writer here goes through this, so the whole pool serialises
        against a vault migration - which rewrites `proxies.json` under the
        transaction lock alone. With only the file lock, a credential written
        during a migration landed in plaintext after the migration had already
        passed that file, and the config then said the store was encrypted.
        """
        return _pool_lock(self.root, self.path)

    # Identity lives in `proxypool.identity()`, and only there. This class used
    # to carry its own copy that left the protocol out, so an HTTP and a SOCKS5
    # entry with the same host, port and credentials were one proxy to the
    # deduper and two to the "used by N profiles" count - an import could drop
    # one silently.

    def add_many(self, proxies: list[dict]) -> list[dict]:
      with self._lock:
        items = self.list()
        # A locked entry reads back with `pass=None`, which is indistinguishable
        # from a proxy that genuinely has no password - so a new passwordless
        # proxy would dedupe against a credentialed one and be silently dropped.
        # Entries whose secret is hidden take no part in the comparison.
        seen = {vault_identity(p) for p in items
                if not self.secret_withheld(p.get("id", ""))}
        added = []
        for pr in proxies:
            key = vault_identity(pr)
            if key in seen:
                continue
            entry = {**pr, "id": uuid.uuid4().hex[:12], "status": "untested"}
            items.append(entry)
            seen.add(key)
            added.append(entry)
        self._save_entries(items)
        return added

    def get(self, pid: str) -> Optional[dict]:
        return next((p for p in self.list() if p.get("id") == pid), None)

    def _read_for_write(self):
        """`(decrypted view, {id: stored entry})` from a single file read.

        `update`/`delete` used to read the file twice - once through `list()`
        for a decrypted view, and again inside `_save_entries()` to rebuild the
        map needed to re-encrypt. That doubled the cost of every single-field
        edit on a pool of any size. Callers hold the lock around this.
        """
        stored = self._stored()
        return [self._decrypted(e) for e in stored], {e.get("id"): e for e in stored}

    def update(self, pid: str, patch: dict) -> Optional[dict]:
      # Re-read inside the lock: the caller's snapshot may already be stale,
      # and writing it back is how a concurrent health check lost a status.
      with self._lock:
        items, stored = self._read_for_write()
        out = None
        for p in items:
            if p.get("id") == pid:
                p.update(patch)
                out = p
        if out:
            self._save([self._encrypted(e, stored.get(e.get("id"))) for e in items])
        return out

    def update_many(self, patches: dict) -> list:
        """Apply `{id: patch}` in one read-modify-write.

        A pool sweep updated each proxy separately, so testing N proxies did N
        locked read-modify-write cycles over the whole file. One pass instead.
        """
        if not patches:
            return []
        with self._lock:
            items, stored = self._read_for_write()
            touched = []
            for p in items:
                patch = patches.get(p.get("id"))
                if patch:
                    p.update(patch)
                    touched.append(p)
            if touched:
                self._save([self._encrypted(e, stored.get(e.get("id"))) for e in items])
            return touched

    def delete(self, pid: str) -> bool:
      with self._lock:
        items, stored = self._read_for_write()
        kept = [p for p in items if p.get("id") != pid]
        if len(kept) == len(items):
            return False
        self._save([self._encrypted(e, stored.get(e.get("id"))) for e in kept])
        return True


# How stale a token's `lastUsed` may be before it is worth a disk write. The
# field exists so a user can spot a token nobody uses; it is never read at finer
# resolution than "which day".
LAST_USED_RESOLUTION = 60.0


class DuplicateToken(ValueError):
    """A token with that name already exists."""


class TokenStore:
    """Named API tokens, stored as hashes.

    `NUMERA_API_TOKEN` is one shared secret: rotating it locks out every client
    at once and nothing records what a token was for. A table of named tokens
    lets one integration be revoked without disturbing the rest.

    Only the hash is kept. A token that can be read back out of a file is a
    token the file has to be guarded like the secret itself, and there is no
    reason to store it - verification only needs the digest.
    """

    def __init__(self, root: Path):
        self.path = Path(root) / "tokens.json"
        #: Whether this API is protected is a *policy*, and policy does not
        #: live in the credential list. It used to be inferred from that list
        #: being non-empty, so revoking the last token, deleting the file or
        #: corrupting it each turned a locked API anonymous while it ran.
        self.policy_path = Path(root) / "auth-policy.json"

    def state(self) -> str:
        """`"empty"`, `"tokens"` or `"unreadable"` - three different facts.

        `list()` answered `[]` for all three, which is this codebase's recurring
        defect: "there is nothing" and "I could not look" arriving as one value.
        Here it decided whether requests needed a credential at all, so a
        truncated write or a hand-edit authorised everyone.
        """
        if not self.path.exists():
            return "empty"
        try:
            items = json.loads(self.path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return "unreadable"
        if not isinstance(items, list):
            return "unreadable"
        return "tokens" if items else "empty"

    def list(self) -> list[dict]:
        """The tokens, or an empty list when there are none *or* none readable.

        Callers that render a table are fine with that. Callers deciding whether
        to demand a credential are not - they ask `state()` or `protected()`.
        """
        if self.state() != "tokens":
            return []
        return json.loads(self.path.read_text(encoding="utf-8"))

    def protected(self) -> bool:
        """Has this API ever been protected, and not deliberately opened since?

        Sticky on purpose. Losing the credentials is not a decision to stop
        requiring them, and all three ways of losing them happen without anyone
        choosing.
        """
        if self.state() in ("tokens", "unreadable"):
            # "I could not read the credentials" is not "there are none". A
            # truncated write or a hand-edit used to reach the marker lookup
            # below and, on a store that predates the marker, answer False.
            return True
        # Strictly, and in one place: `bool(marker["protected"])` read `null`,
        # `0`, `""`, `[]` and `{}` as a decision to open the service. Every one
        # of those means the document is damaged or was written by something
        # that did not understand it. Only an unambiguous JSON `false` opens it.
        # `in`, not `!= OPEN`: `ABSENT` is a fresh loopback install that has
        # made no decision and must stay open, while `AMBIGUOUS` is a document
        # that exists and cannot be read - those two are the pair this whole
        # module exists to keep apart.
        return auth_policy.read(self.path.parent) in (
            auth_policy.PROTECTED, auth_policy.AMBIGUOUS)

    def adopt(self) -> bool:
        """Record protection for a store that was protected before markers existed.

        True when something was adopted.

        The marker is only ever written by `create`, so every data root whose
        tokens were minted by an earlier build arrives with credentials and no
        policy. It *looks* protected, because `protected()` falls back to "are
        there tokens" - and that fallback disappears with the tokens. Deleting
        or corrupting the file then turned an upgraded installation anonymous.

        Called when the store is put into service, which is the last moment
        that reliably precedes anything being able to remove the credentials.
        An explicit `{"protected": false}` is a decision and is left alone.
        """
        if auth_policy.read(self.path.parent) != auth_policy.ABSENT:
            return False               # a decision already exists; leave it
        # `unreadable` as well as `tokens`: a legacy credential file that is
        # *already* damaged at startup still says this installation had
        # credentials. Adopting only the healthy case left such a store
        # protected by the unreadable-state fallback alone, so deleting the
        # damaged file afterwards made a markerless root anonymous.
        if self.state() not in ("tokens", "unreadable"):
            return False
        self.set_protected(True)
        return True

    def set_protected(self, on: bool) -> None:
        """Turn the policy on or off explicitly.

        Off is the documented way out of a corrupt token file: without it the
        only recovery from a fail-closed API is editing application state by
        hand, which is worse than the problem it solves.
        """
        auth_policy.write(self.path.parent, on)

    def _save(self, items: list[dict]):
        write_json(self.path, items)
        # The digests are not secrets, but the file names every integration that
        # can reach this machine. Keep it owner-only where the OS supports it.
        # This sat after a `return` and so had never run.
        try:
            self.path.chmod(0o600)
        except OSError:
            pass

    @property
    def _lock(self):
        return _lock_for(self.path)

    @staticmethod
    def hash(token: str) -> str:
        return hashlib.sha256(token.encode("utf-8")).hexdigest()

    def create(self, name: str) -> dict:
        """Mint a token. The value is returned once and never stored.

        The name check is here, inside the lock, because both callers used to
        do it themselves and then call this: two requests could each read a
        table without the name, both pass, and both append. The OS lock made
        the *writes* safe, never the decision.
        """
        wanted = (name or "").strip() or "unnamed"
        token = "nmr_" + secrets.token_urlsafe(32)
        entry = {"id": uuid.uuid4().hex[:12], "name": wanted,
                 "hash": self.hash(token), "created": time.time(), "lastUsed": None}
        with self._lock:
            items = self.list()
            if any(str(t.get("name", "")).strip() == wanted for t in items):
                raise DuplicateToken(f"a token called {wanted!r} already exists")
            items.append(entry)
            self._save(items)
            # Protection starts the instant a credential exists, and is
            # recorded separately so losing that credential cannot undo it.
            self.set_protected(True)
        return {**entry, "token": token}

    def revoke(self, tid: str, refuse_lockout: bool = False) -> bool:
        """Remove one token. True when it was there to remove.

        `refuse_lockout` is what the HTTP route passes: removing the *last*
        credential from a protected API locks its owner out of their own
        server, and saying so while the token still exists is more use than
        failing closed afterwards. Off by default so internal callers and
        migrations are unchanged.
        """
        with self._lock:
            items = self.list()
            keep = [t for t in items if t.get("id") != tid]
            if len(keep) == len(items):
                return False
            if refuse_lockout and not keep:
                raise ValueError(
                    "this is the last API token, and removing it would lock "
                    "this server to nobody. Create another token first, or set "
                    "NUMERA_API_TOKEN, or turn protection off deliberately.")
            self._save(keep)
        return True

    def verify(self, token: str) -> Optional[dict]:
        """The matching entry, with its last-used stamp refreshed."""
        if not token:
            return None
        digest = self.hash(token)
        with self._lock:
            items = self.list()
            for t in items:
                # compare_digest on the hashes: the stored value is not secret,
                # but the comparison still should not leak timing about a
                # near-miss. Encoded first, because `compare_digest` on two
                # strings raises `TypeError` unless both are ASCII - our own
                # digest always is, a hand-edited `tokens.json` need not be.
                if secrets.compare_digest(
                        str(t.get("hash", "")).encode("utf-8", "surrogateescape"),
                        digest.encode("utf-8", "surrogateescape")):
                    now = time.time()
                    # Only persist when the stamp actually moves. Every
                    # authenticated request used to rewrite the whole token file
                    # - an atomic temp write, rename and chmod under the lock -
                    # to advance `lastUsed` by milliseconds, putting disk churn
                    # and lock contention on the hot auth path. Minute
                    # granularity is all this field is read at.
                    if now - float(t.get("lastUsed") or 0) >= LAST_USED_RESOLUTION:
                        t["lastUsed"] = now
                        # Re-read inside the lock and write that: this used to
                        # write back a list captured before a concurrent
                        # `revoke()`, which brought the revoked token back.
                        self._save(items)
                    return t
        return None
