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
from .config_store import ConfigStore
from .models import Profile
from .paths import (_lock_for, _restore_bytes, id_path, mutate_json,
                    resolve_data_dir, safe_id, safe_join, session_lease,
                    transaction_lock, write_json)


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


# ---- one name, one profile -------------------------------------------------
# The product contract: a profile name identifies exactly one profile, on every
# surface. It has to, because `resolve(ref)` falls back to `find_by_name()` and
# `numera delete <name>` acts on what that returns - so two profiles under one
# name make a destructive command pick one of them and say nothing.
#
# `ProfileRepo` enforced it for the CLI, inside the transaction that writes.
# The HTTP writes went through `DashboardStore` and did not, so the API created
# duplicates the CLI refused. These two functions are the check itself, kept
# apart from either store so both can make the same decision from the same
# facts rather than growing two policies that drift.
def stored_names(*dirs) -> list:
    """`(id, name)` for every profile record in `dirs`, exactly as stored.

    Raw JSON on purpose. The question is about names, which are not secrets, so
    going through `list()` would decrypt every proxy password to answer it -
    and would answer it differently depending on whether the vault happens to
    be unlocked.

    A record that cannot be parsed contributes nothing: it holds no name this
    store can honour, and refusing every write because one file is corrupt
    would be worse than the ambiguity being guarded against.
    """
    out = []
    for directory in dirs:
        if directory is None:
            continue
        for path in sorted(Path(directory).glob("*.json")):
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                continue
            if isinstance(data, dict) and data.get("name"):
                out.append((data.get("id") or path.stem, data["name"]))
    return out


def name_clash(stored, pending):
    """The first pending profile that claims a name another one holds, or None.

    `stored` is `(id, name)` for what is already on disk - both copies of a
    profile may appear, and they may even disagree after an older version left
    them diverged, so every name an id is known by counts as its own.
    `pending` is `(id, name)` in write order. Returns the offending
    `(id, name)`.

    Only a *new* claim is policed, the same rule `ProfileRepo.save_many` uses:
    a profile re-saving the name it already holds is not asking for anything,
    so an ordinary edit never fails on a duplicate it did not create, and
    `GET /api/profiles` can still adopt a CLI-made profile into the dashboard
    without the adoption looking like a second claim on its own name.

    Two pending profiles with one name clash with each other: that is the
    ambiguity, whether or not anything on disk has the name yet.
    """
    held, taken = {}, {}
    for pid, name in stored:
        held.setdefault(pid, set()).add(name)
        taken.setdefault(name, pid)
    for pid, name in pending:
        if not name or name in held.get(pid, ()):
            continue
        owner = taken.get(name)
        if owner is not None and owner != pid:
            return (pid, name)
        taken[name] = pid
        held.setdefault(pid, set()).add(name)
    return None


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


class AmbiguousProfile(RuntimeError):
    """A name matches more than one profile, so no lookup can answer it.

    0.22 stopped *new* duplicate names from being created; it did nothing for a
    data root that already had them - an older Numera, a hand edit, a restored
    backup, a clone from a version that allowed it. `find_by_name` returned the
    first match in directory order, so `numera delete <name>` on such a store
    destroyed an arbitrary one of the two and reported success.

    Failing closed is the only safe answer: the store cannot know which profile
    was meant, and picking one is the outcome nothing can undo. `RuntimeError`,
    like `SessionBusy`, because the CLI already renders those as a message for
    the user rather than a traceback.
    """

    def __init__(self, name: str, ids):
        self.name = name
        self.ids = list(ids)
        super().__init__(
            f"{len(self.ids)} profiles are called {name!r}: "
            + ", ".join(self.ids)
            + ". Use an id instead, or rename one of them - a command that "
              "changes or deletes 'by name' cannot be told which you mean.")


class ProfileStore(ConfigStore):
    def __init__(self, root: Optional[str | Path] = None,
                 password: Optional[str] = None):
        # Where the data lives is one decision, made in paths.py - see the order
        # it resolves. The stopgap that read ~/.persona in place is gone: the
        # first run now imports it into the portable root instead (firstrun.py).
        self.root = resolve_data_dir(root)
        self.profiles_dir = self.root / "profiles"
        # The dashboard's copies. The engine store does not write them - the
        # coordinator does - but it has to *see* them: a name belongs to a
        # profile, not to a directory, and an older version could leave a
        # record in one and not the other.
        self.dash_dir = self.root / "dashboard"
        self.userdata_dir = self.root / "user-data"
        self.config_path = self.root / "config.json"
        _make_data_dirs(self.root, self.profiles_dir, self.userdata_dir)
        # The master password comes from the caller or the environment, so a
        # launched subprocess inherits it without it ever hitting disk.
        self._password = password or os.environ.get("NUMERA_PASSWORD") or None

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
        """Save a profile - **both** copies of it - and return it.

        This is the public mutator, and it is what `numera.__doc__` tells an
        external script to call. It used to write `profiles/<id>.json` and
        nothing else, which recreated the exact divergence `ProfileRepo` exists
        to prevent: a profile created this way was engine-only until a
        dashboard listing happened to adopt it, and an *update* to an id the
        dashboard already knew changed only the engine copy - the listing does
        not re-adopt a known id, so the GUI kept showing the old identity
        indefinitely.

        Name uniqueness applies here for the same reason it applies to the CLI
        and the API: one name, one profile, on every surface. A profile
        re-saved under the name it already holds is not making a claim, so
        ordinary updates are unaffected.

        `_save_engine` is the engine-only primitive this used to be. It is not
        part of the supported surface: the only thing that should write one
        copy without the other is the dashboard mirror, which is writing the
        other copy at that moment.
        """
        from .repo import repo

        return repo(self).create(profile)

    def _save_engine(self, profile: Profile) -> Profile:
        """Write ONLY `profiles/<id>.json`. Internal - see `save`.

        The dashboard mirror is the one legitimate caller: it is already
        writing the dashboard record, and going through the coordinator from
        there would have it rewrite the copy it was called from.
        """
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
        # Existence and content are decided by the same transaction. Checking
        # `exists()` outside it was a TOCTOU with delete: the file could be
        # unlinked between the check and the lock, and this raised
        # `FileNotFoundError` at a caller that had asked a question whose
        # honest answer was "no such profile". `get()` returning None is how
        # every caller expects a missing profile to arrive.
        #
        # The read is in there for its own reason too: its meaning depends on
        # the profile file and `config.json` agreeing, and a migration rewrites
        # the first before committing the second. Reentrant, so a caller
        # already holding it (every write path) pays nothing.
        with transaction_lock(self.root):
            if not path.exists():
                return None
            try:
                raw = path.read_text(encoding="utf-8")
            except FileNotFoundError:                # pragma: no cover
                return None                          # lost a race we cannot lock
            data = self._apply_secrets(json.loads(raw), "dec")
        return Profile.from_dict(data)

    def find_by_name(self, name: str) -> Optional[Profile]:
        """The profile with this name, None if there is none - or raise.

        Every match is collected rather than the first one returned, because
        "the first one in directory order" is not an answer to "which profile
        is called this". See `AmbiguousProfile`.

        Both directories are searched, and that is the point. `list()` reads
        `profiles/` alone, so an engine profile called `same` beside a
        *dashboard-only* one called `same` looked unambiguous from here - and a
        name-based delete went ahead, while the GUI went on showing the other.
        The write-time check has scanned both since 0.22; the read had not
        caught up. An id present in both copies counts once.
        """
        with transaction_lock(self.root):
            ids = sorted({pid for pid, stored
                          in stored_names(self.profiles_dir, self.dash_dir)
                          if stored == name})
            if len(ids) > 1:
                raise AmbiguousProfile(name, ids)
            if not ids:
                return None
            return self.get(ids[0]) or self._adopt_legacy(ids[0])

    def _adopt_legacy(self, profile_id: str) -> Optional[Profile]:
        """Give a dashboard-only profile the engine copy it is missing.

        An older version could write one side and not the other. The GUI still
        lists such a profile and can still launch it, so answering "no such
        profile" from here would be this store disagreeing with the one the
        user is looking at - and the two-copy invariant says the answer is to
        repair it, not to report it. Materialising is idempotent and derives
        the engine record from the dashboard one, which is the only copy that
        exists.
        """
        from .repo import repo

        try:
            path = id_path(self.dash_dir, profile_id, ".json")
        except ValueError:
            return None            # not an id at all - `resolve` will try it as a name
        if not path.exists():
            return None
        return repo(self).materialize_current(profile_id)

    def resolve(self, ref: str) -> Optional[Profile]:
        """Look up a profile by id first, then by name.

        One lock for both attempts: a write landing between them could show a
        profile under neither its old nor its new name.

        Id first is what keeps an ambiguous store usable: two profiles may
        share a name, but never an id, so `numera <command> <id>` still works
        on both of them and is how the ambiguity gets fixed.

        An id the *dashboard* alone knows resolves too, by repairing it - see
        `_adopt_legacy`. Returning None there would have been this store
        denying a profile the GUI was showing at that moment.
        """
        with transaction_lock(self.root):
            found = self.get(ref)
            if found is not None:
                return found
            adopted = self._adopt_legacy(ref)
            return adopted if adopted is not None else self.find_by_name(ref)

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

    def delete(self, ref: str) -> bool:
        """Delete a profile - both copies and its session - or nothing.

        The public deleter, and it goes through the coordinator. It used to
        remove the engine record and the session only, so a caller told the
        profile was deleted still had its dashboard twin on disk and the GUI
        went on showing a profile that no longer existed for the engine.

        `_delete_engine` is the engine-only primitive this used to be, kept
        private for the same reason as `_save_engine`.
        """
        from .repo import repo

        return repo(self).delete(ref)

    def _delete_engine(self, profile_id: str) -> bool:
        """Delete ONLY the engine record and its session. Internal - see `delete`.

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
        # The record and the vault state are one decision, so they are read
        # under one lock. `is_withheld` asks whether the *current* key can open
        # what is stored; a rotation rewrites the record before it commits the
        # new descriptor, so reading the bytes outside the transaction and
        # judging them against `config.json` afterwards compared a value to a
        # key state that never coexisted. The dashboard store and the proxy
        # pool were given this boundary in 0.22; the engine store was missed.
        with transaction_lock(self.root):
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

    # ---- moving a profile in and out of a file --------------------------
    # The four archive operations live in `archive.py`: they are the only code
    # in the store that speaks zip, staging and rollback, and each is built out
    # of the primitives above rather than being one of them. These stay as the
    # public entry points, so no caller had to change.
    def export(self, profile_id: str, dest: str | Path) -> Optional[Path]:
        """Write this profile's config to `dest` as JSON. See `archive.export`."""
        from . import archive

        return archive.export(self, profile_id, dest)

    def import_file(self, src: str | Path) -> Profile:
        """Read one exported profile into BOTH copies. See `archive.import_file`."""
        from . import archive

        return archive.import_file(self, src)

    def backup(self, profile_id: str, dest: str | Path) -> Optional[Path]:
        """Zip the profile and its whole browser session. See `archive.backup`."""
        from . import archive

        return archive.backup(self, profile_id, dest)

    def restore(self, src: str | Path, new_id: bool = False) -> Profile:
        """Recreate a profile and its session from a backup. See `archive.restore`."""
        from . import archive

        return archive.restore(self, src, new_id=new_id)
