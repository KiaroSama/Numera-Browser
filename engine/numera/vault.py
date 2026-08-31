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
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Iterator, Optional

from . import crypto
from .paths import _Batch


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

    def rollback(self) -> None:
        """Restore every file this migration wrote, newest first."""
        self._batch.rollback()


@contextmanager
def migration(root: Path):
    """Run a migration that undoes itself if anything inside the block fails.

    The config write belongs *inside* the block: the vault becoming visible is
    the last step, and if it fails the encryption has to come back off.
    """
    step = _Migration(root)
    try:
        yield step
    except BaseException:
        step.rollback()
        raise


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


def encrypt_for_write(engine, value, stored=None):
    """What to store for `value`, given what is already on disk.

    Three cases, and the middle one is the one that keeps biting:

    * no vault - unchanged; there is nothing to protect it from.
    * vault locked - a *new* secret is refused rather than written in the
      clear, and an *unchanged* one keeps the ciphertext already on disk. A
      locked read yields ``None`` for the secret, so an ordinary edit comes back
      carrying ``None``; writing that would destroy the password.
    * vault unlocked - the ciphertext.

    Already-encrypted input is returned untouched, so a value that has been
    round-tripped is never encrypted twice.
    """
    if engine is None or not getattr(engine, "vault_enabled", lambda: False)():
        return value
    if not value:
        return stored if stored else value
    if crypto.is_encrypted(value):
        return value
    if not engine.unlocked():
        if stored:
            return stored
        raise crypto.VaultLocked(
            "the vault is locked, so this proxy password cannot be stored "
            "safely - unlock it with `numera vault unlock` first")
    return crypto.encrypt(engine._password, engine.vault()["salt"], value)
