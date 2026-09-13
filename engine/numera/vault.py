"""Where a data root keeps its secrets, and how the vault covers all of them.

The vault used to encrypt `profiles/*.json` and stop there, because that is the
file `ProfileStore` owns. But the same proxy password is written to two more
places under the same root - the dashboard's own copy of each profile, and the
shared proxy pool - and both stayed in the clear. `numera vault enable` reported
success and the product claimed proxy passwords were encrypted at rest while two
files still held them readable.

The migration works on *files*, not on store objects, for a reason that is not
stylistic: `vault enable` is a CLI command and the server package is an optional
extra (`engine[api]`). The engine cannot import `server.stores` to ask the
dashboard store to migrate itself, and it must not skip that store just because
it is not importable.

Adding a secret-bearing file to the product means adding a line to ``SITES``.
"""
from __future__ import annotations

import json
import os
import time
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator, Optional

from . import crypto, proctree
from .paths import RollbackFailed, _Batch, write_json


@dataclass(frozen=True)
class Site:
    """One place secrets live, relative to the data root.

    `node` is the key holding the record's secret container, or ``None`` when
    the secret sits directly on the record. `many` marks a document that is a
    list of records rather than a single one - the proxy pool is one file
    holding every proxy.
    """
    glob: str
    field: str
    node: Optional[str] = None
    many: bool = False


# The engine profile, the dashboard's copy of it, and the shared proxy pool.
# The two profile stores disagree about the field name - `password` against
# `pass` - which is why this is a table rather than one constant.
SITES = (
    Site(glob="profiles/*.json", node="proxy", field="password"),
    Site(glob="dashboard/*.json", node="proxy", field="pass"),
    Site(glob="proxies.json", field="pass", many=True),
)


def holders(data, site: Site) -> Iterator[dict]:
    """Every dict in `data` that carries a non-empty secret for `site`."""
    records = data if site.many and isinstance(data, list) else [data]
    for record in records:
        if not isinstance(record, dict):
            continue
        holder = record.get(site.node) if site.node else record
        if isinstance(holder, dict) and holder.get(site.field):
            yield holder


def secret_files(root: Path) -> Iterator[tuple]:
    """`(path, site)` for every file under `root` that may hold a secret."""
    root = Path(root)
    for site in SITES:
        for path in sorted(root.glob(site.glob)):
            if path.is_file():
                yield path, site


def plaintext_secrets(root: Path) -> list:
    """Every readable secret still under `root`, as `(path, value)`.

    Exists so the guarantee can be *checked* rather than asserted: after a
    successful `vault enable` this must be empty.
    """
    out = []
    for path, site in secret_files(root):
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        for holder in holders(data, site):
            value = holder[site.field]
            if not crypto.is_encrypted(value):
                out.append((path, value))
    return out


class _Migration:
    """Encrypts every secret under a root, and can put it all back.

    Keeps the original bytes of each file it writes. A half-migrated store is
    the one state nothing downstream knows how to read - some profiles
    encrypted, some not, and no way to tell which without trying both.
    """

    def __init__(self, root: Path):
        self.root = Path(root)
        self._batch = _Batch()

    def encrypt_all(self, password: str, salt: str) -> int:
        """Encrypt every plaintext secret. Returns how many were converted."""
        converted = 0
        for path, site in secret_files(self.root):
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                # A file that cannot be read cannot be migrated, and pretending
                # otherwise would leave a secret behind under a vault that says
                # it covered everything.
                raise ValueError(f"{path} is unreadable, so its secrets cannot be encrypted")
            changed = 0
            for holder in holders(data, site):
                value = holder[site.field]
                if not crypto.is_encrypted(value):
                    holder[site.field] = crypto.encrypt(password, salt, value)
                    changed += 1
            if changed:
                self._batch.write(path, data)
                converted += changed
        return converted

    def rekey_all(self, old_password: str, old_salt: str,
                  new_password: str, new_salt: str, resume: bool = False) -> int:
        """Re-encrypt every secret under a new key. Returns how many moved.

        Decrypt-then-encrypt, file by file, through the same batch as
        `encrypt_all` - so a failure at any point puts every file back and the
        vault keeps opening with the old password. There is no password
        recovery, which makes a half-rotated store unrecoverable: some secrets
        under the old key, some under the new, and no single password that
        opens them all.

        A record this build cannot decrypt (a foreign key, a newer version)
        stops the rotation rather than being dropped: rotating past it would
        leave a secret nothing can ever open.

        `resume` is for finishing a rotation a crash interrupted. Records
        already moved are indistinguishable from records still waiting - both
        are ciphertext - so each is tried against the new key first and skipped
        if it opens. Without that, a resume would hand an already-rotated value
        to the old password and refuse the whole thing.
        """
        moved = 0
        for path, site in secret_files(self.root):
            try:
                data = json.loads(path.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError):
                raise ValueError(f"{path} is unreadable, so its secrets cannot be rotated")
            changed = 0
            for holder in holders(data, site):
                value = holder[site.field]
                if not crypto.is_encrypted(value):
                    # Plaintext under an enabled vault should not happen; leaving
                    # it alone is right - `enable` is what encrypts.
                    continue
                if resume:
                    try:
                        crypto.decrypt(new_password, new_salt, value)
                    except Exception:                           # noqa: BLE001
                        pass            # still under the old key; move it below
                    else:
                        continue        # already moved by the interrupted run
                try:
                    plain = crypto.decrypt(old_password, old_salt, value)
                except Exception as e:                          # noqa: BLE001
                    raise ValueError(
                        f"{path} holds a secret this password cannot open, so the "
                        f"rotation would leave it unreadable forever ({e})")
                holder[site.field] = crypto.encrypt(new_password, new_salt, plain)
                changed += 1
            if changed:
                self._batch.write(path, data)
                moved += changed
        return moved

    def commit_config(self, cfg: dict, save_config) -> None:
        """Write the descriptor as part of *this* transaction.

        `save_config` is the store's own writer - it takes the config file lock
        and asserts the root transaction, neither of which the batch knows
        about - so the batch remembers the file rather than writing it. What
        that buys is the thing the split protocol lacked: a failure after this
        point restores the config together with the secret files, instead of
        leaving one half of a rotation committed over the other half's data.
        """
        self._batch.remember(self.root / "config.json")
        save_config(cfg)

    def rollback(self) -> list:
        """Restore every file this migration wrote, newest first.

        Returns the files that could not be restored, `[(path, error), ...]`.
        """
        return self._batch.rollback()


# --------------------------------------------------------------------------
# Recovery protocol
# --------------------------------------------------------------------------
# The journal, its classification and the finish-or-refuse decision live in
# `vault_recovery`. They are one responsibility - "what state is this data root
# in, and may it be served" - and a large one; this module owns the sites and
# the migration machinery instead. Re-exported here because `vault.recover`,
# `vault.journal_path` and the rest are the names the store, the CLI and the
# existing suites already use.
from .vault_recovery import (                                   # noqa: E402,F401
    JOURNAL_VERSION,
    KNOWN_JOURNAL_VERSIONS,
    JournalUnreadable,
    SUPPORTED_OPS,
    VaultRecoveryRequired,
    begin_journal,
    classify,
    commit,
    commit_and_verify,
    completion_problems,
    structural_problems,
    descriptors,
    end_journal,
    fsync_path,
    journal_path,
    needs_recovery,
    recover,
    sync_config,
    sync_data,
    sync_dir,
    unconverted,
    unreadable_secret_files,
)


@contextmanager
def migration(root: Path, keep_journal: bool = False):
    """Run a migration that undoes itself if anything inside the block fails.

    `keep_journal` is set when the block is *resuming* an interrupted
    migration. Rolling back a resume does not restore a clean store - it
    restores the half-migrated state the crash left, whose ciphertext opens
    only with the journal's salt. Clearing the journal there would destroy the
    last copy of it for a store that still needs it.

    The config write belongs *inside* the block: the vault becoming visible is
    the last step, and if it fails the encryption has to come back off.

    The journal is separate from that rollback and covers the case rollback
    cannot: a kill that runs no `except` at all. It is cleared on both exits,
    so a rolled-back migration leaves nothing for `recover()` to act on.
    """
    step = _Migration(root)
    try:
        yield step
    except BaseException as exc:
        failures = step.rollback()
        if not failures:
            # The restored state has to reach the disk before the journal that
            # describes it is allowed to go, for the same reason the forward
            # direction does: a rollback sitting in the page cache and a
            # deleted journal is the one combination nothing can repair.
            try:
                sync_data(root)
                sync_config(root)
            except OSError as sync_exc:
                failures = [(Path(root), sync_exc)]
        if failures:
            # Worse here than anywhere else: a half-restored migration leaves
            # some secrets under the new key and some under the old, and the
            # config says one password opens them all. The caller has to be
            # told which files, not just that the migration failed.
            #
            # The journal stays. It is the only record of the salt those
            # half-restored files are encrypted under, and clearing it here -
            # which this did - threw that away at the exact moment it was the
            # last copy.
            raise RollbackFailed(exc, failures) from exc
        if not keep_journal:
            end_journal(root)
        raise
    end_journal(root)


# --------------------------------------------------------------------------
# The read/write rule every secret-bearing store shares
# --------------------------------------------------------------------------
def decrypt_for_read(engine, value):
    """The usable secret, or ``None`` while the vault is locked.

    Never the ciphertext. A locked read that handed the encrypted blob back
    would send it onwards as if it were the password - the launcher would
    authenticate with it and the dashboard would print it in a password field.
    """
    if not value or not crypto.is_encrypted(value):
        return value
    if not (engine and engine.unlocked() and engine._password):
        return None
    vault_desc = engine.vault()
    try:
        return crypto.decrypt(engine._password, vault_desc["salt"], value)
    except Exception:                                           # noqa: BLE001
        return None                 # wrong password, or a corrupted record


def is_withheld(engine, stored_value) -> bool:
    """True when `stored_value` is a secret this process cannot currently read.

    A locked read yields ``None`` by design - handing the ciphertext onwards
    would be worse. But ``None`` is indistinguishable from "this proxy has no
    password", and several operations believed the second reading: a backup
    claimed to contain a credential it did not have, a health check tested a
    blank password and recorded a real failure, and pool assignment wrote
    ``None`` over a working profile.

    Callers use this to refuse rather than produce a confidently wrong artifact.
    Reads and non-secret edits stay available while locked; only work that
    *needs* the credential is blocked.
    """
    if not stored_value or not crypto.is_encrypted(stored_value):
        return False
    if not (engine and engine.unlocked() and engine._password):
        return True
    # An unlocked vault is not the same as a readable record. One entry can be
    # encrypted under a different key (a record pasted in from another install),
    # truncated by a bad write, or written by a newer build than this one. This
    # used to answer "not withheld" purely because the *verifier* opened, so
    # export, health-check and assign carried on and silently dropped exactly
    # the credential they could not read.
    return decrypt_for_read(engine, stored_value) is None


LOCKED_MESSAGE = ("the vault is locked, so this needs the master password - "
                  "set NUMERA_PASSWORD or pass --password")


def encrypt_for_write(engine, value, stored=None):
    """What to store for `value`, given what is already on disk.

    **`value` carries intent, and the three intents are different writes.**

    * ``None`` - withheld. The caller is not saying anything about the secret,
      which is what a locked read hands back and what an unrelated edit passes
      through, so the stored value stands.
    * ``""`` - an explicit clear. There was no way to say this: empty meant
      "keep", so removing a proxy password while keeping the proxy endpoint was
      impossible and the credential simply stayed. It now removes it.
    * anything else - a replacement.

    Then the vault:

    * no vault - the value as given; there is nothing to protect it from.
    * locked - a replacement is refused rather than written in the clear, and
      so is a clear: destroying a credential this process cannot even read is
      the one irreversible thing to get wrong, and unlocking first is a small
      price. Withheld is always fine.
    * unlocked - the ciphertext.

    Already-encrypted input is returned untouched, so a value that has been
    round-tripped is never encrypted twice.
    """
    if value is None:
        return stored                   # withheld, vault or no vault
    if engine is None or not getattr(engine, "vault_enabled", lambda: False)():
        return value                    # including "" - an explicit clear
    if value == "":
        # Clearing nothing is not a decision. A proxy with no password stores
        # `""`, so every ordinary write on one carries `""` back, and refusing
        # those would take locked editing away from the profiles with the least
        # to protect.
        if stored and not engine.unlocked():
            raise crypto.VaultLocked(
                "the vault is locked, so this proxy password cannot be "
                "cleared: unlock first, so that what is being removed is "
                "something you can see")
        return ""
    if crypto.is_encrypted(value):
        return value
    if not engine.unlocked():
        # Refuse, whether or not something is already stored. Returning the
        # existing ciphertext here meant a locked password *change* reported
        # success and changed nothing - the caller sent a new plaintext, got a
        # 200 back, and the old password was still the one on disk. Only an
        # empty/absent value means "keep what is there", and that is handled
        # above; a non-empty plaintext is always a replacement.
        raise crypto.VaultLocked(
            "the vault is locked, so this proxy password cannot be stored "
            "safely - set NUMERA_PASSWORD or pass --password first")
    return crypto.encrypt(engine._password, engine.vault()["salt"], value)
