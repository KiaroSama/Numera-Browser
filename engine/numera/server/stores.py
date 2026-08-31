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
from typing import Optional
from .. import vault
from ..paths import _lock_for, id_path, write_json
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
        return id_path(self.dir, pid, ".json")

    def list(self) -> list[dict]:
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
        """
        try:
            f = self._path(pid)
        except ValueError:
            return {}
        if not f.exists():
            return {}
        try:
            return json.loads(f.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}

    def _guard_secrets(self, prof: dict) -> dict:
        """Return a copy that never carries a plaintext secret past the vault.

        The rule itself lives in `vault.encrypt_for_write` - the engine store
        and the proxy pool need exactly the same one, and three copies of it
        drifted apart once already.
        """
        a, b = self.SECRET
        node = prof.get(a)
        if not isinstance(node, dict):
            return prof
        stored = (self._raw(prof.get("id", "")).get(a) or {}).get(b)
        kept = vault.encrypt_for_write(self._engine, node.get(b), stored)
        if kept == node.get(b):
            return prof
        return {**prof, a: {**node, b: kept}}

    def _decrypted(self, prof: dict) -> dict:
        """The record as a caller should see it: the secret, or nothing."""
        a, b = self.SECRET
        node = prof.get(a)
        if not isinstance(node, dict) or not node.get(b):
            return prof
        return {**prof, a: {**node, b: vault.decrypt_for_read(self._engine, node[b])}}

    def save(self, prof: dict) -> dict:
        if not prof.get("id"):
            prof["id"] = uuid.uuid4().hex[:12]
        prof.setdefault("_created", time.time())
        prof = self._guard_secrets(prof)
        path = self._path(prof["id"])
        with _lock_for(path):
            previous = path.read_bytes() if path.exists() else None
            write_json(path, prof)
            try:
                self._mirror(prof)
            except Exception:
                # Both copies or neither. This half was written first and the
                # mirror's failure was swallowed, so a locked vault produced a
                # dashboard that said "saved" over an engine copy still holding
                # the old proxy - and nothing anywhere said the two had parted.
                if previous is None:
                    path.unlink(missing_ok=True)
                else:
                    path.write_bytes(previous)
                raise
        return prof

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
        self._engine.save(twin)

    def delete(self, pid: str) -> bool:
        """Remove both copies of a profile, and its session last.

        The order is the point. Metadata deletion is recoverable - the bytes are
        held and written back - so both metadata files go first and the session
        directory, which cannot be brought back, only afterwards. And the engine
        failure is no longer swallowed: it left the dashboard copy gone and the
        twin behind, so the profile reappeared in `numera list` after the GUI
        said it was deleted, and the call reported success.
        """
        try:
            f = self._path(pid)
        except ValueError:
            return False
        engine_record = None
        if self._engine is not None:
            try:
                engine_record = self._engine._path(pid)
            except ValueError:
                engine_record = None
        if not f.exists() and not (engine_record and engine_record.exists()):
            return False

        undo = []
        try:
            for path in (f, engine_record):
                if path is not None and path.exists():
                    undo.append((path, path.read_bytes()))
                    path.unlink()
        except OSError:
            for path, original in reversed(undo):
                path.write_bytes(original)
            raise

        # Past the point of no return: both records are gone, so the session
        # they described is orphaned whatever happens next.
        if self._engine is not None:
            self._engine.delete_session(pid)
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
        path = self._meta_path()
        with _lock_for(path):
            write_json(path, meta)
        return meta


class ProxyPool:
    """A managed list of proxies the dashboard can import, test and assign."""

    # The pool holds the same kind of secret the profiles do, so it needs the
    # same vault. `engine_store` is how it sees one; without it the pool keeps
    # its old plaintext behaviour, which is right for a caller that has no data
    # root of its own but wrong for the two that do - both pass it.
    def __init__(self, root: Path, engine_store=None):
        self.path = root / "proxies.json"
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
        """
        return [self._decrypted(p) for p in self._stored()]

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
        return _lock_for(self.path)

    @staticmethod
    def _key(p: dict) -> tuple:
        # Sticky-session vendors hand out one host:port for the whole pool and
        # select the exit node from the credentials — some vary the username,
        # others vary only the password. Credentials are part of a proxy's
        # identity, so deduping on host:port alone collapses a whole vendor
        # list into a single entry.
        return (p["host"], str(p["port"]), p.get("user") or "", p.get("pass") or "")

    def add_many(self, proxies: list[dict]) -> list[dict]:
      with self._lock:
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
        self._save_entries(items)
        return added

    def get(self, pid: str) -> Optional[dict]:
        return next((p for p in self.list() if p.get("id") == pid), None)

    def update(self, pid: str, patch: dict) -> Optional[dict]:
      # Re-read inside the lock: the caller's snapshot may already be stale,
      # and writing it back is how a concurrent health check lost a status.
      with self._lock:
        items = self.list()
        out = None
        for p in items:
            if p.get("id") == pid:
                p.update(patch)
                out = p
        if out:
            self._save_entries(items)
        return out

    def delete(self, pid: str) -> bool:
      with self._lock:
        items = self.list()
        kept = [p for p in items if p.get("id") != pid]
        if len(kept) == len(items):
            return False
        self._save_entries(kept)
        return True


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
        self.path = root / "tokens.json"

    def list(self) -> list[dict]:
        if not self.path.exists():
            return []
        try:
            items = json.loads(self.path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            return []
        return items if isinstance(items, list) else []

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
        """Mint a token. The value is returned once and never stored."""
        token = "nmr_" + secrets.token_urlsafe(32)
        entry = {"id": uuid.uuid4().hex[:12], "name": (name or "").strip() or "unnamed",
                 "hash": self.hash(token), "created": time.time(), "lastUsed": None}
        with self._lock:
            items = self.list()
            items.append(entry)
            self._save(items)
        return {**entry, "token": token}

    def revoke(self, tid: str) -> bool:
        with self._lock:
            items = self.list()
            keep = [t for t in items if t.get("id") != tid]
            if len(keep) == len(items):
                return False
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
                # near-miss.
                if secrets.compare_digest(str(t.get("hash", "")), digest):
                    t["lastUsed"] = time.time()
                    # Re-read inside the lock and write that: this used to write
                    # back a list captured before a concurrent `revoke()`, which
                    # brought the revoked token back.
                    self._save(items)
                    return t
        return None
