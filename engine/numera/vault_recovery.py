"""Finishing, or refusing, a vault migration that was interrupted.

Split out of `vault.py`, which owns the *sites* and the migration machinery.
This module owns one question and its consequences: **what state is this data
root in, and may it be served?**

The protocol has three moving parts.

*The journal* records, before the first destructive write, the descriptors the
migration is moving between. It holds the salt and verifier - the same two
values `config.json` carries openly once the vault is on - and never the master
password or a plaintext secret. Rolling *backward* would need the plaintext on
disk, which is a worse bug than the crash it repairs, so recovery rolls forward
or refuses.

*The classification* decides what the journal means. An earlier version asked
only "does the file parse into a dict with a salt", which collapsed four very
different states into two: a journal that is absent, one that is valid, one
that is **unreadable**, and one written by a build this one does not
understand. The last two were treated as "nothing to do", so a store with an
unresolved migration opened normally - and a direct `recover` would delete the
damaged journal, destroying the only record of the salt its ciphertext needs.

*The completion check* decides whether an interrupted `enable` actually
finished. It cannot be a best-effort scan: a secret file that will not parse is
not a file with no secrets in it. Committing the descriptor because the scanner
could not see the remaining plaintext is how a vault reports protection it does
not have.

Durability ordering, which the whole protocol rests on: journal → data →
descriptor → journal removal, each step pushed to stable storage before the
next begins. A sync failure is a real failure and is raised; only the documented
"this platform cannot sync this object" errors are tolerated.
"""
from __future__ import annotations

import errno
import json
import os
import time
from pathlib import Path
from typing import Optional

from . import crypto, proctree
from .paths import write_json

#: The journal shapes this build knows how to finish.
SUPPORTED_OPS = ("enable", "rotate")

#: Journal format. A future build that changes the protocol bumps this; an
#: older build then refuses the journal by name instead of acting on a record
#: whose fields it would misread.
JOURNAL_VERSION = 1


class VaultRecoveryRequired(RuntimeError):
    """An unresolved migration is on disk and cannot be settled unattended.

    Raised instead of opening the store, because a store whose config and disk
    disagree cannot answer honestly: some credentials are under one key, some
    under another, or some still in plaintext. Committing the descriptor anyway
    is what the first version of this journal did, and it reported an enabled
    vault over readable passwords.

    Finishing needs the key material, which is deliberately not on disk. So the
    store refuses and the message names what it needs.
    """


class JournalUnreadable(VaultRecoveryRequired):
    """The journal exists but cannot be trusted, so nothing may be assumed.

    Its own class because the recovery *action* differs: an unreadable journal
    can never be finished by supplying a password - the salt it was protecting
    is what is missing - so the only honest outcomes are a restore from backup
    or a deliberate local decision. It is never deleted automatically.
    """


def journal_path(root) -> Path:
    """Where a migration records what it is about to do.

    Next to the config it commits into, so the two are lost or kept together.
    """
    return Path(root) / "vault-migration.json"


def descriptors(record: dict) -> tuple:
    """`(new, old)` for a journal record; `old` is None for an enable.

    0.31.0 wrote the salt and verifier at the top level with no `new`/`old`
    pair, because it only ever journalled an enable. Those journals are still
    readable here - a store upgraded mid-crash is exactly when this matters.
    """
    new = record.get("new") or {"salt": record.get("salt"),
                                "verifier": record.get("verifier")}
    return new, record.get("old")


def _descriptor_ok(desc) -> bool:
    return isinstance(desc, dict) and isinstance(desc.get("salt"), str) and desc["salt"]


def classify(root) -> tuple:
    """`(state, record)` for the journal under `root`.

    States, and why each is separate:

    * ``"absent"`` - no journal. Nothing happened, or everything finished.
    * ``"valid"`` - a record this build can act on.
    * ``"unreadable"`` - the file exists and is not a usable record: invalid
      JSON, a list, `{}`, a missing or non-string salt, a rotation with no old
      descriptor. Previously indistinguishable from "absent", which let a store
      with unresolved state open normally.
    * ``"unsupported"`` - it names a format or operation this build does not
      implement. Acting on it would mean reading a newer record with older
      rules.

    The file is never modified here; classification is read-only by design so
    that the damaged case cannot lose evidence to the act of inspecting it.
    """
    path = journal_path(root)
    if not path.exists():
        return "absent", None
    try:
        record = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return "unreadable", None
    if not isinstance(record, dict) or not record:
        return "unreadable", None

    version = record.get("version", JOURNAL_VERSION)
    op = record.get("op", "enable")
    if not isinstance(version, int) or version > JOURNAL_VERSION:
        return "unsupported", record
    if op not in SUPPORTED_OPS:
        return "unsupported", record

    new, old = descriptors(record)
    if not _descriptor_ok(new):
        return "unreadable", record
    if op == "rotate" and not _descriptor_ok(old):
        # A rotation whose *old* descriptor is missing cannot be resumed: the
        # records still under the old key could not be told apart, let alone
        # decrypted. Refusing keeps the new salt on disk for a manual repair.
        return "unreadable", record
    return "valid", record


def begin_journal(root, op: str, *, new: dict, old: Optional[dict] = None) -> None:
    """Record the commit *before* the first destructive write.

    `_Batch` keeps its undo bytes in memory and `migration()` rolls back from
    an `except` clause. Neither exists after `kill -9`, a power cut or a closed
    lid - and the window between "every secret file now holds new ciphertext"
    and "the config names the salt" is exactly where that leaves an
    unrecoverable store.

    A rotation records `old` as well. Both keys are then on disk at once, which
    is the only way a resume can tell an already-rotated record from one still
    waiting - they are both ciphertext, and nothing but a trial decryption
    distinguishes them.
    """
    record = {"version": JOURNAL_VERSION, "op": op,
              # Kept at the top level as well as inside `new`, because a
              # 0.31.0 journal on disk has only this form.
              "salt": new["salt"], "verifier": new.get("verifier"),
              "new": {"salt": new["salt"], "verifier": new.get("verifier")},
              "old": ({"salt": old["salt"], "verifier": old.get("verifier")}
                      if old else None),
              "started": time.time(),
              # Who is doing it, so a *concurrent* reader can tell a migration
              # in flight from one that was abandoned.
              "pid": os.getpid(), "token": proctree.started_at(os.getpid())}
    write_json(journal_path(root), record)
    fsync_path(journal_path(root))


#: `fsync` is not defined for every object on every platform. These are the
#: documented "this cannot be synced here" answers - a directory handle on
#: Windows, a filesystem that does not implement the call - and only these are
#: tolerated. Anything else is a real I/O failure: a full disk, a disconnected
#: volume, a failing drive. Swallowing those was the durability equivalent of
#: an empty `except`.
_UNSUPPORTED_SYNC = frozenset(
    e for e in (getattr(errno, name, None)
                for name in ("EINVAL", "ENOTSUP", "EOPNOTSUPP", "EBADF", "EACCES"))
    if e is not None)


def fsync_path(path: Path, *, directory: bool = True) -> None:
    """Push `path`, and the directory entry that names it, to stable storage.

    `write_json` is atomic against a *failed write* - it serialises, writes a
    temporary file and replaces. It is not durable against a *power loss*: the
    replace can still be sitting in the OS cache. For ordinary profile writes
    that trade is right. For the files a recovery protocol depends on, it is
    not, and the ordering between them is the whole guarantee.

    A genuine sync error propagates. The previous version returned quietly on
    any `OSError`, so a failing disk produced a migration that reported success
    with nothing durably on it.
    """
    path = Path(path)
    try:
        fd = os.open(path, os.O_RDONLY)
    except OSError as e:
        if e.errno in _UNSUPPORTED_SYNC:
            return
        raise
    try:
        os.fsync(fd)
    except OSError as e:
        if e.errno not in _UNSUPPORTED_SYNC:
            raise
    finally:
        os.close(fd)
    if directory and hasattr(os, "O_DIRECTORY"):   # POSIX: sync the rename too
        try:                                       # pragma: no cover - not Windows
            dfd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        except OSError as e:
            if e.errno in _UNSUPPORTED_SYNC:
                return
            raise
        try:
            os.fsync(dfd)
        except OSError as e:
            if e.errno not in _UNSUPPORTED_SYNC:
                raise
        finally:
            os.close(dfd)


def end_journal(root) -> None:
    """Drop the journal. The migration either committed or was rolled back.

    Last in the ordering, and only after the descriptor it was protecting is
    durably on disk - otherwise a crash between the two leaves ciphertext whose
    salt is in neither place.
    """
    try:
        journal_path(root).unlink(missing_ok=True)
    except OSError:                                          # pragma: no cover
        pass


def needs_recovery(root) -> bool:
    """Is there anything unresolved here? Asked before taking any lock.

    True for a damaged or unsupported journal as well as a valid one: all three
    mean the store may not be served until something decides what they are. The
    earlier version answered False for the damaged cases, which is how a store
    with an unresolved migration opened as if nothing had happened.

    The liveness check has to happen out here. A migration holds the root
    transaction for its whole run, so a reader that took that lock first and
    asked afterwards would simply block in its constructor until the migration
    finished - which is not recovery, it is a stall on every concurrent read.
    """
    state, record = classify(root)
    if state == "absent":
        return False
    if state == "valid":
        owner = record.get("pid")
        if owner and owner != os.getpid() and proctree.alive(owner, record.get("token")):
            return False                # still running; nothing to repair
    return True


def unreadable_secret_files(root) -> list:
    """Secret-bearing files under `root` that will not parse, as paths.

    `plaintext_secrets` skips these, because for its original job - "prove
    nothing readable is left" - a file it cannot read contributes nothing to
    find. For deciding whether an interrupted migration *finished*, that same
    skip is the defect: an unparseable `proxies.json` made recovery conclude
    the enable was complete and commit the descriptor over it.
    """
    from . import vault

    bad = []
    for path, _site in vault.secret_files(Path(root)):
        try:
            json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            bad.append(path)
    return bad


def recover(root, get_config, save_config,
            password: Optional[str] = None,
            old_password: Optional[str] = None) -> bool:
    """Settle an abandoned migration, or refuse to serve the store.

    True when something was recovered. Called on every store open, which is the
    only moment that reliably follows a crash.

    The first version answered one question - "is there a vault in the config?"
    - and committed the journal's descriptor when there was not. That is
    correct for exactly one crash point: after the last secret file was
    written. Anywhere earlier it marks the vault enabled over credentials still
    in plaintext, which is a false claim of protection.
    """
    state, record = classify(root)
    if state == "absent":
        return False
    if state == "unreadable":
        raise JournalUnreadable(
            f"{journal_path(root).name} exists but is not a usable recovery "
            f"record, so this data root holds a migration whose state cannot be "
            f"determined. Nothing has been changed or deleted. Restore the data "
            f"directory from a backup, or - if the secrets are known to be "
            f"intact - remove that file deliberately to accept the config as it "
            f"stands.")
    if state == "unsupported":
        raise JournalUnreadable(
            f"{journal_path(root).name} was written by a newer Numera "
            f"(format {record.get('version')!r}, operation {record.get('op')!r}). "
            f"This build will not act on it. Update this install, or finish the "
            f"migration with the one that started it.")

    owner = record.get("pid")
    if owner and owner != os.getpid() and proctree.alive(owner, record.get("token")):
        return False

    new, old = descriptors(record)
    cfg = get_config()
    if record.get("op") == "rotate":
        return _finish_rotation(root, cfg, new, old, save_config,
                                password, old_password)
    return _finish_enable(root, cfg, new, save_config, password)


def _sync_config(root) -> None:
    """The committed descriptor, durably on disk before the journal goes.

    Only when the file is there: `save_config` is a caller-supplied callback
    and this module does not own where it persists. A store that writes
    `config.json` gets the ordering guarantee; one that does not is not an I/O
    failure to report from here.
    """
    path = Path(root) / "config.json"
    if path.exists():
        fsync_path(path)


def _finish_enable(root, cfg, new, save_config, password) -> bool:
    from . import vault

    damaged = unreadable_secret_files(root)
    if damaged:
        raise VaultRecoveryRequired(
            f"A `vault enable` was interrupted, and {len(damaged)} secret-bearing "
            f"file(s) under this data root cannot be read: "
            f"{', '.join(p.name for p in damaged)}. Whether the migration "
            f"finished cannot be established, so the vault is not switched on "
            f"and the recovery record is kept. Repair or restore those files, "
            f"then open the store again.")

    leftover = vault.plaintext_secrets(root)
    if not leftover:
        if not cfg.get("vault"):
            save_config({**cfg, "vault": dict(new)})
            _sync_config(root)
        end_journal(root)
        return True
    if not password:
        raise VaultRecoveryRequired(
            f"A `vault enable` was interrupted with {len(leftover)} credential(s) "
            f"still unencrypted, so this data root is half migrated. Open it with "
            f"the master password that migration was using (NUMERA_PASSWORD, or "
            f"`--password`) and it will be finished. Nothing is lost; the vault is "
            f"not on until it is complete.")
    if not crypto.check_password(password, new["salt"], new.get("verifier")):
        raise crypto.VaultLocked(
            "that is not the master password the interrupted migration was using, "
            "so it cannot be finished with it")
    with vault.migration(root, keep_journal=True) as step:
        step.encrypt_all(password, new["salt"])
        _sync_secret_files(root)
        save_config({**cfg, "vault": dict(new)})
        _sync_config(root)
    return True


def _finish_rotation(root, cfg, new, old, save_config, password, old_password) -> bool:
    from . import vault

    current = cfg.get("vault") or {}
    if current.get("salt") == new["salt"]:
        # The descriptor committed; only `end_journal` was missed.
        end_journal(root)
        return True
    damaged = unreadable_secret_files(root)
    if damaged:
        raise VaultRecoveryRequired(
            f"A master-password rotation was interrupted and "
            f"{len(damaged)} secret-bearing file(s) cannot be read: "
            f"{', '.join(p.name for p in damaged)}. The rotation is not finished "
            f"and its record is kept. Repair or restore those files first.")
    if not (password and old_password):
        raise VaultRecoveryRequired(
            "A master-password rotation was interrupted, so some secrets are "
            "under the new password and some under the old one. Finish it with "
            "`numera vault resume`, which needs both passwords - they are the "
            "only way to tell the two apart, because both are ciphertext. This "
            "data root is not served until then, and the record of the new salt "
            "is kept.")
    if not crypto.check_password(password, new["salt"], new.get("verifier")):
        raise crypto.VaultLocked(
            "that is not the new master password the rotation was moving to")
    if old and not crypto.check_password(old_password, old["salt"], old.get("verifier")):
        raise crypto.VaultLocked(
            "that is not the master password the rotation was moving away from")
    with vault.migration(root, keep_journal=True) as step:
        step.rekey_all(old_password, (old or {}).get("salt"),
                       password, new["salt"], resume=True)
        _sync_secret_files(root)
        save_config({**cfg, "vault": dict(new)})
        _sync_config(root)
    return True


def _sync_secret_files(root) -> None:
    """Every file the migration just rewrote, durably on disk.

    The ordering the protocol promises is journal → data → descriptor →
    journal removal. Syncing the journal alone proved only that the *plan*
    survived a power cut, not the work it described: without this the
    descriptor could be committed while the ciphertext it refers to was still
    in the page cache.
    """
    from . import vault

    for path, _site in vault.secret_files(Path(root)):
        try:
            fsync_path(path, directory=False)
        except OSError:
            raise
