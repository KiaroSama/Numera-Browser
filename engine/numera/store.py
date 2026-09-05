"""
Profile storage.

Profiles live as individual JSON files under a data directory (default
``~/.numera/profiles``, or the pre-rename ``~/.persona`` when only that exists).
One file per profile keeps things transparent,
diff-friendly and trivial to back up or sync. Browser user-data (cookies,
localStorage) is kept in a sibling ``user-data/<id>`` directory that the
launcher points the browser at, so sessions persist between runs.
"""

from __future__ import annotations

import json
import os
import time
import uuid
import warnings
import shutil
import tempfile
import zipfile
from pathlib import Path
from typing import Optional

from . import crypto
from . import vault
from .models import Profile
from .paths import (_lock_for, id_path, mutate_json, resolve_data_dir, safe_id,
                    safe_join, session_lease, transaction_lock, write_json)


def unloadable_extensions(paths) -> list[str]:
    """The entries Chromium could not load, one message each.

    This lived in `routes_config.py`, so it guarded the HTTP config route and
    nothing else: `numera config --default-extension` wrote straight through
    `set_default_extensions()`, and `numera ext default-add` checked only
    `is_dir()`. Both could persist a value the API rejects and the launcher
    silently drops - `_extension_args` filters out whatever is not a directory,
    so a Web Store URL or a folder without a manifest was shown as saved and
    then did nothing at launch, with no error anywhere.

    Chromium can load only an *unpacked* extension: a folder containing
    manifest.json.
    """
    problems = []
    for raw in paths:
        entry = str(raw).strip()
        if not entry:
            continue
        if "://" in entry:
            # Store URLs go through the installer, which downloads and unpacks
            # them. Reaching here means a URL was put straight into the list,
            # where the launcher could not use it.
            problems.append(
                f"{entry!r} is a URL. Install it first - paste it on the "
                "Extensions page, where the button reads Install for a link, "
                "or run `numera ext install <url>` - then the unpacked folder "
                "goes in this list.")
            continue
        path = Path(entry).expanduser()
        if not path.is_dir():
            problems.append(f"{entry!r} is not a folder on this machine.")
        elif not (path / "manifest.json").is_file():
            problems.append(
                f"{entry!r} has no manifest.json, so Chromium will refuse it. "
                "Point at the folder that contains the manifest, not its parent.")
    return problems


def _make_data_dirs(root: Path, *dirs: Path) -> None:
    """Create the data directories, or explain why it could not.

    This used to be two bare `mkdir` calls, so a data directory the process
    cannot write to surfaced as a `PermissionError` traceback out of
    `pathlib.mkdir` - forty lines of stack, no mention of which directory, and
    nothing about what to do. The two ways to meet it are both ordinary:

    * a container started with a bind mount (`-v ./data:/data`). The image runs
      as `pwuser`, uid 1001, and a bind mount keeps the *host* directory's
      ownership, so the `chown` in the Dockerfile does not apply to it. The
      documented `docker compose` path uses a named volume and is unaffected.
    * an app unpacked somewhere the user cannot write, which on Windows means
      anywhere under Program Files.

    The exception type stays the same so existing callers behave the same; only
    the message changes.
    """
    for path in dirs:
        try:
            path.mkdir(parents=True, exist_ok=True)
        except PermissionError as e:
            owner = ""
            try:
                st = root.stat()
                owner = f" (it is owned by uid {st.st_uid}:{st.st_gid}"
                owner += f", this process runs as uid {os.getuid()}:{os.getgid()})"
            except (OSError, AttributeError):        # no st_uid/getuid on Windows
                owner = ""
            raise PermissionError(
                f"cannot create {path} - the data directory {root} is not "
                f"writable by this process{owner}. Point --data-dir or "
                f"NUMERA_DATA_DIR at a directory you can write, or fix the "
                f"ownership of that one. In Docker, prefer the named volume the "
                f"bundled docker-compose.yml uses; a bind-mounted host directory "
                f"keeps its own ownership and must already be writable by the "
                f"container's user."
            ) from e


class SessionStranded(RuntimeError):
    """A failed delete could not put its detached session back."""


class ProfileStore:
    def __init__(self, root: Optional[str | Path] = None,
                 password: Optional[str] = None):
        # Where the data lives is one decision, made in paths.py - see the order
        # it resolves. The stopgap that read ~/.persona in place is gone: the
        # first run now imports it into the portable root instead (firstrun.py).
        self.root = resolve_data_dir(root)
        self.profiles_dir = self.root / "profiles"
        self.userdata_dir = self.root / "user-data"
        self.config_path = self.root / "config.json"
        _make_data_dirs(self.root, self.profiles_dir, self.userdata_dir)
        # The master password comes from the caller or the environment, so a
        # launched subprocess inherits it without it ever hitting disk.
        self._password = password or os.environ.get("NUMERA_PASSWORD") or None

    # ---- config (small key/value settings) -------------------------------
    # Preference order for a fresh install, strongest first. `cloak` is NOT in
    # it: CloakBrowser needs a paid subscription and cannot be bundled, so
    # defaulting to it meant the very first launch click failed on every new
    # install. An explicit user choice still wins over this list.
    ENGINE_PREFERENCE = ("camoufox", "patchright", "playwright")
    DEFAULT_ENGINE = "playwright"

    def get_config(self) -> dict:
        """The stored config, re-read only when the file has actually changed.

        `vault()` and `unlocked()` both call this, and `vault.decrypt_for_read`
        calls both - so decrypting a single secret cost two full reads of
        `config.json`. Listing a 60-proxy pool did over a hundred, which is
        where a pool sweep's quadratic cost really came from; the PBKDF2 work
        was only half of it.

        Keyed on the file's size and modification time. Writes go through
        `write_json`, which replaces the file atomically, so any change moves
        one of those. A copy is returned because callers mutate what they get.
        """
        import copy

        try:
            stat = self.config_path.stat()
        except OSError:
            self._config_cache = None
            return {}
        key = (stat.st_mtime_ns, stat.st_size)
        cached = getattr(self, "_config_cache", None)
        if cached is not None and cached[0] == key:
            return copy.deepcopy(cached[1])
        try:
            data = json.loads(self.config_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return {}
        if not isinstance(data, dict):
            return {}
        self._config_cache = (key, data)
        return copy.deepcopy(data)

    def save_config(self, cfg: dict) -> dict:
        with _lock_for(self.config_path):
            write_json(self.config_path, cfg)
        return cfg

    def get_default_engine(self) -> str:
        """The engine used for new profiles when none is given (persisted).

        An explicitly configured engine is returned as-is even when its package
        is missing - silently substituting another one would be the same class
        of bug as a UI control that does nothing, and the launcher already
        raises a clear "needs the 'x' package" error. The preference order only
        decides what a *fresh* install gets."""
        configured = self.get_config().get("default_engine")
        if configured:
            return configured
        from . import drivers  # local: keeps store importable without a driver stack
        for name in self.ENGINE_PREFERENCE:
            if drivers.is_installed(name):
                return name
        return self.DEFAULT_ENGINE

    def set_default_engine(self, name: str) -> str:
        # Read and write inside one lock. Reading outside it meant two settings
        # saved at once each wrote their own copy of the whole config, and the
        # second one silently dropped the first one's change.
        self._set_config("default_engine", name)
        return name

    def get_default_extensions(self) -> list:
        """Extension folders every new profile starts with.

        Chromium loads extensions per browser instance, and each profile is its
        own instance, so there is no "install once, applies everywhere" step to
        hook — a global set has to be copied onto each profile as it is created.
        """
        return list(self.get_config().get("default_extensions") or [])

    def set_default_extensions(self, paths) -> list:
        """Persist the set every new profile starts with, validated.

        Only entries that are *new* are checked. Validating the whole list
        would make an already-stored bad path - one whose folder was deleted
        after it was added - block the very call that removes it.
        """
        wanted = [str(p) for p in paths]
        with transaction_lock(self.root):
            current = set(self.get_default_extensions())
            problems = unloadable_extensions([p for p in wanted if p not in current])
            if problems:
                raise ValueError("; ".join(problems))
            return self._set_config("default_extensions", wanted)

    def get_headless_launch(self) -> bool:
        """Whether the API launches profiles without a window.

        A GUI-less server has no display for Chromium to open a window on, so
        the dashboard's launch button needs this on to work there at all.
        """
        return bool(self.get_config().get("headless_launch"))

    def set_headless_launch(self, on: bool) -> bool:
        return self._set_config("headless_launch", bool(on))

    def set_config_many(self, changes: dict) -> dict:
        """Apply several config keys as one read-modify-write.

        `PUT /api/config` used to call `set_default_engine`, then
        `set_default_extensions`, then `set_headless_launch` in turn - three
        separate writes. A request whose *third* field was invalid had already
        committed the first two and then answered 400, so a rejected request
        still changed the config. One write, after everything is validated.
        """
        if not changes:
            return self.get_config()

        def change(cfg):
            cfg.update(changes)
            return cfg

        # Root transaction lock first, then the config file lock `mutate_json`
        # takes. Config used to be its own lock domain: a profile transaction
        # wrote `config.json` while holding only the transaction lock, and an
        # ordinary settings change held only the file lock - so neither waited
        # for the other, and a rollback could restore config over a settings
        # change that had already succeeded.
        with transaction_lock(self.root):
            return mutate_json(self.config_path, change)

    def _set_config(self, key: str, value):
        """Change one config key without disturbing the others."""
        def change(cfg):
            cfg[key] = value
            return cfg

        # Same hierarchy as `set_config_many`; see the note there.
        with transaction_lock(self.root):
            return mutate_json(self.config_path, change)[key]

    # ---- vault (encrypt-at-rest for secrets) -----------------------------
    # The fields worth encrypting: anything that grants access if leaked. Cookies
    # live in Chromium's own encrypted store, so the one in our plaintext JSON is
    # the proxy password.
    SECRET_PATHS = (("proxy", "password"),)

    def vault(self) -> Optional[dict]:
        """The vault descriptor ({salt, verifier}) if a password was ever set."""
        return self.get_config().get("vault")

    def vault_enabled(self) -> bool:
        return self.vault() is not None

    def unlocked(self) -> bool:
        """True when secrets can actually be read/written (right password set)."""
        v = self.vault()
        if not v:
            return True                      # nothing to unlock
        return bool(self._password) and crypto.check_password(
            self._password, v["salt"], v["verifier"])

    def enable_vault(self, password: str) -> int:
        """Turn on the vault and encrypt every secret under the data root.

        Returns how many secrets were converted.

        This used to walk `self.list()` - the engine's own profiles - and stop
        there, so the dashboard's copy of each profile and the whole shared
        proxy pool kept their passwords in the clear while the command reported
        success. `vault.SITES` is the list of every file that holds one; adding
        a store to the product means adding a line there.

        All or nothing. The vault descriptor is written *last*, inside the same
        block, so a failure anywhere - encrypting, writing, or saving the config
        - restores every touched file and leaves the vault disabled. The other
        order left it switched on over profiles it had not reached, which tells
        the user they are protected while some of them are not.
        """
        crypto._require()
        # The whole migration under the root transaction lock, which every
        # profile, pool and folder write also takes. Without it another process
        # could write a plaintext secret *after* the scan had passed that file
        # and *before* the vault descriptor committed - leaving a store the
        # config says is encrypted with one readable password in it. Two
        # simultaneous `vault enable` calls could also both see "disabled" and
        # race different salts, which is unrecoverable: each secret opens with
        # only one of them.
        with transaction_lock(self.root):
            # Re-check inside the lock. The check outside it is the classic
            # check-then-act: both callers pass it before either commits.
            if self.vault_enabled():
                raise RuntimeError("A vault is already set up. Use `numera vault change` "
                                   "to rotate the master password.")
            salt = crypto.new_salt()
            with vault.migration(self.root) as step:
                converted = step.encrypt_all(password, salt)
                cfg = self.get_config()
                cfg["vault"] = {"salt": salt,
                                "verifier": crypto.make_verifier(password, salt)}
                self.save_config(cfg)
            self._password = password
            return converted

    def change_vault_password(self, old_password: str, new_password: str) -> int:
        """Rotate the master password, or change nothing. Returns secrets moved.

        A new salt as well as a new key: reusing the salt would leave the two
        passwords deriving from the same input, and the point of a rotation is
        that the old password stops being able to open anything.

        The verifier is written last and inside the migration block, exactly as
        `enable_vault` does, so a failure anywhere leaves the old password still
        working rather than a store no password can open.
        """
        crypto._require()
        if not new_password:
            raise ValueError("the new master password cannot be empty")
        # Same transaction boundary as `enable_vault`, and for the same reason:
        # a rotation rewrites every secret, and a write landing inside it leaves
        # one record under the old key that the new password cannot open.
        with transaction_lock(self.root):
            v = self.vault()          # read inside the lock, not before it
            if not v:
                raise RuntimeError("There is no vault to rotate. "
                                   "Use `numera vault enable` first.")
            if not crypto.check_password(old_password, v["salt"], v["verifier"]):
                raise crypto.VaultLocked("that is not the current master password")
            new_salt = crypto.new_salt()
            with vault.migration(self.root) as step:
                moved = step.rekey_all(old_password, v["salt"], new_password, new_salt)
                cfg = self.get_config()
                cfg["vault"] = {"salt": new_salt,
                                "verifier": crypto.make_verifier(new_password, new_salt)}
                self.save_config(cfg)
            self._password = new_password
            return moved

    def _secret_values(self, profile: Profile) -> list:
        vals = []
        d = profile.to_dict()
        for a, b in self.SECRET_PATHS:
            sec = (d.get(a) or {}).get(b) if isinstance(d.get(a), dict) else None
            vals.append(sec)
        return vals

    def _apply_secrets(self, data: dict, mode: str) -> dict:
        """Encrypt (mode='enc') or decrypt (mode='dec') the secret fields in a
        profile dict, in place. A no-op when there's no vault."""
        v = self.vault()
        if not v:
            return data
        pw = self._password
        for a, b in self.SECRET_PATHS:
            node = data.get(a)
            if not isinstance(node, dict) or not node.get(b):
                continue
            val = node[b]
            if mode == "enc":
                if pw and self.unlocked() and not crypto.is_encrypted(val):
                    node[b] = crypto.encrypt(pw, v["salt"], val)
            else:  # dec
                if crypto.is_encrypted(val):
                    # Locked reads yield None, not ciphertext: the launcher takes
                    # this value straight to the browser, and authenticating with
                    # an encrypted blob is worse than not authenticating at all.
                    # `save()` is where the stored value is protected.
                    if pw and self.unlocked():
                        try:
                            node[b] = crypto.decrypt(pw, v["salt"], val)
                        except Exception:               # noqa: BLE001
                            # Encrypted under a *different* key - a backup from
                            # another machine, or a rotated vault. `unlocked()`
                            # only says the current verifier matches, not that
                            # this record does. Degrade to None like
                            # `vault.decrypt_for_read` already does; raising
                            # here took the entire profile list down over one
                            # unreadable row.
                            node[b] = None
                    else:
                        node[b] = None
        return data

    # ---- paths -----------------------------------------------------------
    def _path(self, profile_id: str) -> Path:
        return id_path(self.profiles_dir, profile_id, ".json")

    def user_data_path(self, profile_id: str) -> Path:
        p = id_path(self.userdata_dir, profile_id)
        p.mkdir(parents=True, exist_ok=True)
        return p

    # ---- CRUD ------------------------------------------------------------
    def save(self, profile: Profile) -> Profile:
        profile.updated_at = time.time()
        # The lock spans the read *and* the write: `_keep_stored_secrets` reads
        # the stored record to carry a withheld secret across, so this is a real
        # read-modify-write and was running outside any lock. Two saves of the
        # same profile could interleave and drop one's preserved ciphertext.
        path = self._path(profile.id)
        # Transaction lock first, per-file lock second - the same order every
        # profile mutation uses, so a batch and a single save can never
        # interleave and undo each other.
        #
        # The vault decision is inside it. Reading the descriptor and
        # encrypting outside the lock was a check-then-act across processes: a
        # `vault change` committing in between meant this wrote a blob sealed
        # with the *previous* salt over a record the rotation had just rekeyed,
        # and the new password could never open it again. Re-reading here means
        # a writer holding a stale password is refused instead.
        with transaction_lock(self.root), _lock_for(path):
            locked = self.vault_enabled() and not self.unlocked()
            # Refuse to write a plaintext secret over an encrypted one: if the
            # vault is on but locked, saving would silently downgrade security.
            #
            # Only a *plaintext* one. This used to refuse any secret at all,
            # which made every edit of a profile that merely has a proxy
            # password fail while locked - renaming it, changing its notes -
            # because the value coming back round is the ciphertext the store
            # handed out. Re-writing the ciphertext downgrades nothing.
            if locked and any(v and not crypto.is_encrypted(v)
                              for v in self._secret_values(profile)):
                raise crypto.VaultLocked(
                    "the vault is locked — unlock with the master password before "
                    "saving a profile that has a proxy password")
            data = self._apply_secrets(profile.to_dict(), "enc")
            # A locked read hands back None for the secret, so an ordinary edit
            # - rename the profile, change its notes - came back here carrying
            # None and wrote it straight over the ciphertext. The password was
            # gone and nothing said so. Carry the stored value across instead.
            if locked:
                data = self._keep_stored_secrets(profile.id, data)
            write_json(path, data)
        return profile

    def _keep_stored_secrets(self, profile_id: str, data: dict) -> dict:
        """Put back any secret this save has no value for.

        Only ever restores; it cannot introduce a value the caller did not have,
        because the only source is the record already on disk.
        """
        path = self._path(profile_id)
        if not path.exists():
            return data
        try:
            stored = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return data
        for a, b in self.SECRET_PATHS:
            node = data.get(a)
            if not isinstance(node, dict) or node.get(b):
                continue                      # the caller supplied something
            old = stored.get(a)
            previous = old.get(b) if isinstance(old, dict) else None
            if previous:
                node[b] = previous
        return data

    def get(self, profile_id: str) -> Optional[Profile]:
        try:
            path = self._path(profile_id)
        except ValueError:
            return None
        if not path.exists():
            return None
        # Inside the transaction: this read's meaning depends on the profile
        # file and `config.json` agreeing, and a migration rewrites the first
        # before committing the second. Reentrant, so a caller already holding
        # it (every write path) pays nothing.
        with transaction_lock(self.root):
            data = self._apply_secrets(
                json.loads(path.read_text(encoding="utf-8")), "dec")
        return Profile.from_dict(data)

    def find_by_name(self, name: str) -> Optional[Profile]:
        for p in self.list():
            if p.name == name:
                return p
        return None

    def resolve(self, ref: str) -> Optional[Profile]:
        """Look up a profile by id first, then by name.

        One lock for both attempts: a write landing between them could show a
        profile under neither its old nor its new name.
        """
        with transaction_lock(self.root):
            return self.get(ref) or self.find_by_name(ref)

    def list(self) -> list[Profile]:
        with transaction_lock(self.root):     # one coherent vault state, see `get`
            return self._list_locked()

    def _list_locked(self) -> list[Profile]:
        out: list[Profile] = []
        for f in sorted(self.profiles_dir.glob("*.json")):
            try:
                data = self._apply_secrets(json.loads(f.read_text(encoding="utf-8")), "dec")
                out.append(Profile.from_dict(data))
            except (json.JSONDecodeError, KeyError, TypeError, ValueError):
                # One unreadable record is skipped; it must never end the
                # listing. TypeError/ValueError cover a stored shape a newer
                # version wrote that this one cannot model.
                continue
        return sorted(out, key=lambda p: p.created_at)

    def delete(self, profile_id: str) -> bool:
        """Delete a profile and its session, or leave both exactly as they were.

        The session lease is taken first and *outside* the data transaction, in
        that order deliberately: a busy profile refuses immediately instead of
        queueing behind - or holding up - every write in the data root. The
        rename in `detach_session` was never the proof it looked like, because
        POSIX renames a directory a browser is using without complaint.
        """
        with session_lease(self.root, profile_id), transaction_lock(self.root):
            return self._delete_locked(profile_id)

    def _delete_locked(self, profile_id: str) -> bool:
        path = self._path(profile_id)
        if not path.exists():
            return False
        # Session first: if it is held, this raises before the record is touched.
        tomb = self.detach_session(profile_id)
        original = path.read_bytes()
        try:
            path.unlink()
        except OSError:
            self.restore_session(profile_id, tomb)
            raise
        if tomb is not None and not self.discard_tombstone(tomb):
            # Committed. The profile is gone and the detached bytes are
            # unreachable through it, so this is untidiness, not a failure.
            warnings.warn(f"the deleted session for {profile_id} is still on disk "
                          f"as {tomb.name}; remove it once nothing holds it",
                          RuntimeWarning, stacklevel=2)
        assert original is not None
        return True

    def detach_session(self, profile_id: str) -> "Optional[Path]":
        """Move the session aside and return the tombstone, or None if none.

        This is step one of a delete, and it is deliberately first: renaming
        fails loudly while a browser holds the directory, and at that point
        nothing else has been touched. Deleting the metadata first and
        discovering the session was held afterwards produced the one answer a
        caller cannot act on - a failed request that had already deleted the
        profile.
        """
        import shutil

        assert shutil                                   # used by the siblings
        try:
            ud = self.userdata_dir / safe_id(profile_id)
        except ValueError:
            return None
        if not ud.exists():
            return None
        tomb = ud.with_name(f"{ud.name}.deleting-{uuid.uuid4().hex[:8]}")
        ud.rename(tomb)
        return tomb

    def restore_session(self, profile_id: str, tomb) -> None:
        """Put a detached session back, when the rest of the delete failed.

        Raises `SessionStranded` if it cannot. This used to swallow the error,
        which produced the one state nothing downstream can make sense of: the
        profile record restored and present, its real browser session left under
        a `.deleting-*` name that nothing points at. The caller saw only the
        first error and the profile looked healthy with its session missing.

        The exception names the tombstone so the repair is deterministic rather
        than a hunt.
        """
        if tomb is None or not tomb.exists():
            return
        try:
            self.userdata_dir.mkdir(parents=True, exist_ok=True)
            tomb.rename(self.userdata_dir / safe_id(profile_id))
        except (OSError, ValueError) as e:
            raise SessionStranded(
                f"the delete of {profile_id} failed and its session could not be "
                f"put back: it is still on disk as {tomb}. Move that directory to "
                f"{self.userdata_dir / safe_id(profile_id)} to finish recovering."
            ) from e

    @staticmethod
    def discard_tombstone(tomb) -> bool:
        """Reclaim a detached session. False means it is still on disk.

        A failure here is untidy, not incorrect: the profile is already gone
        and the bytes are no longer reachable through it. Callers report it as
        cleanup-pending rather than turning a committed delete into a failure.
        """
        import shutil

        if tomb is None:
            return True
        shutil.rmtree(tomb, ignore_errors=True)
        return not tomb.exists()

    def delete_session(self, profile_id: str) -> None:
        """Detach and reclaim in one step, for callers deleting nothing else."""
        tomb = self.detach_session(profile_id)
        if tomb is not None and not self.discard_tombstone(tomb):
            raise OSError(
                f"the session for {profile_id} was detached to {tomb.name} but "
                f"could not be deleted - remove it by hand once nothing holds it")


    # ---- import / export -------------------------------------------------
    def secret_withheld(self, profile_id: str) -> bool:
        """Does this profile hold a secret the current lock state hides?

        Reads the stored bytes rather than a decrypted view: the decrypted view
        is exactly the one that cannot tell "locked" from "no password".
        """
        try:
            path = self._path(profile_id)
        except ValueError:
            return False
        if not path.exists():
            return False
        try:
            raw = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return False
        for a, b in self.SECRET_PATHS:
            node = raw.get(a)
            value = node.get(b) if isinstance(node, dict) else None
            if vault.is_withheld(self, value):
                return True
        return False

    def export(self, profile_id: str, dest: str | Path) -> Optional[Path]:
        prof = self.get(profile_id)
        if not prof:
            return None
        # An export of a profile whose password we cannot read would omit it
        # silently, and the file gives no hint that anything is missing.
        if self.secret_withheld(profile_id):
            raise crypto.VaultLocked(
                f"exporting '{prof.name}' would drop its proxy password: "
                + vault.LOCKED_MESSAGE)
        dest = Path(dest)
        dest.write_text(json.dumps(prof.to_dict(), indent=2), encoding="utf-8")
        return dest

    def import_file(self, src: str | Path) -> Profile:
        data = json.loads(Path(src).read_text(encoding="utf-8"))
        prof = Profile.from_dict(data)
        return self.save(prof)

    # ---- backup / restore ------------------------------------------------
    # ``export``/``import_file`` move only the profile *config* (a JSON file).
    # A backup is the whole identity: the config **and** its browser session
    # (cookies, localStorage — everything under user-data), zipped into one
    # portable file you can archive or carry to another machine.
    def backup(self, profile_id: str, dest: str | Path) -> Optional[Path]:
        """Zip a profile and its entire browser session into one file.

        The profile JSON inside the archive holds any proxy password in
        *plaintext* (secrets are decrypted on read), so treat a backup file as
        sensitive — it grants everything the profile does. Returns the path, or
        ``None`` if no such profile exists."""
        # A live Chromium rewrites its databases continuously; zipping one
        # mid-write produces an archive that restores into a corrupt profile,
        # with nothing at backup time to say so.
        with session_lease(self.root, profile_id):
            prof = self.get(profile_id)
            if not prof:
                return None
            # The docstring above promises the archive carries the proxy password.
            # While locked it would not, and nothing in the file would say so - the
            # restore would come back a working-looking profile that cannot reach
            # its proxy.
            if self.secret_withheld(profile_id):
                raise crypto.VaultLocked(
                    f"backing up '{prof.name}' would drop its proxy password: "
                    + vault.LOCKED_MESSAGE)
            dest = Path(dest)
            with zipfile.ZipFile(dest, "w", zipfile.ZIP_DEFLATED) as z:
                z.writestr("profile.json", json.dumps(prof.to_dict(), indent=2))
                ud = self.userdata_dir / profile_id
                if ud.exists():
                    for f in ud.rglob("*"):
                        if f.is_file():
                            z.write(f, arcname=str(Path("user-data") / f.relative_to(ud)))
            return dest

    def restore(self, src: str | Path, new_id: bool = False) -> Profile:
        """Recreate a profile and its session from a :meth:`backup` archive.

        By default the profile keeps its original id, overwriting any profile
        already under that id (a true restore). Pass ``new_id=True`` to bring it
        in as a fresh copy instead — useful for cloning a warmed-up session
        onto a second machine without the two fighting over one id."""
        src = Path(src)
        # Everything in the archive is untrusted: it may have come from another
        # machine, or from someone else entirely. Validate the whole thing and
        # stage it before touching a single profile file - the previous version
        # saved the profile first and wrote members as it went, so a hostile or
        # merely truncated archive left a half-made profile behind.
        try:
            zf = zipfile.ZipFile(src, "r")
        except zipfile.BadZipFile as e:
            raise ValueError(f"{src.name} is not a readable archive: {e}") from e

        with zf as z:
            try:
                data = json.loads(z.read("profile.json").decode("utf-8"))
            except KeyError:
                raise ValueError(f"{src.name} has no profile.json - not a Numera backup") from None
            except (json.JSONDecodeError, UnicodeDecodeError) as e:
                raise ValueError(f"{src.name} has an unreadable profile.json: {e}") from e
            try:
                prof = Profile.from_dict(data)
            except (KeyError, TypeError, ValueError) as e:
                raise ValueError(f"{src.name} does not contain a usable profile: {e}") from e
            if new_id:
                prof.id = uuid.uuid4().hex[:12]

            # The lease, now that the archive has named the profile: a restore
            # replaces a whole session directory, and must not replace one
            # another process is using.
            with session_lease(self.root, prof.id):
                staging = Path(tempfile.mkdtemp(prefix="numera-restore-", dir=self.root))
                try:
                    for info in z.infolist():
                        name = info.filename
                        if not name.startswith("user-data/") or name.endswith("/"):
                            continue
                        if info.is_dir():
                            continue
                        # Raises before anything is written if the member escapes.
                        target = safe_join(staging, name[len("user-data/"):])
                        target.parent.mkdir(parents=True, exist_ok=True)
                        with z.open(info) as fh:
                            target.write_bytes(fh.read())

                    # Replace, do not merge: a session that is half the old
                    # identity and half the restored one is worse than either.
                    #
                    # Commit order matters, and it used to be wrong. The profile was
                    # saved first, then the old session was removed with
                    # `ignore_errors=True`, then staging was moved in. On Windows a
                    # running Chromium holds its files, so the delete failed
                    # silently - and `shutil.move` onto a directory that still
                    # exists moves the source *inside* it, leaving
                    # `user-data/<id>/numera-restore-xxxx/`. The metadata was
                    # already updated, so the store described a session it did not
                    # have. Now the old session is renamed aside (which fails loudly
                    # if it is in use), the new one is moved into place, the profile
                    # is written last, and any failure puts all of it back.
                    ud = self.user_data_path(prof.id)
                    record = self._path(prof.id)
                    previous_json = record.read_bytes() if record.exists() else None
                    aside = None
                    if ud.exists():
                        aside = ud.with_name(f"{ud.name}.replaced-{uuid.uuid4().hex[:8]}")
                        ud.rename(aside)
                    try:
                        ud.parent.mkdir(parents=True, exist_ok=True)
                        shutil.move(str(staging), str(ud))
                        staging = None
                        self.save(prof)
                    except BaseException:
                        shutil.rmtree(ud, ignore_errors=True)
                        if aside is not None:
                            aside.rename(ud)
                        if previous_json is not None:
                            record.write_bytes(previous_json)
                        else:
                            record.unlink(missing_ok=True)
                        raise
                    if aside is not None:
                        shutil.rmtree(aside, ignore_errors=True)
                finally:
                    if staging is not None:
                        shutil.rmtree(staging, ignore_errors=True)
            return prof
