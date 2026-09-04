"""
Secret encryption for the profile store.

Profiles are stored as readable JSON on purpose — you can list, search, diff and
back them up without ceremony. But a couple of fields are genuinely sensitive:
proxy passwords, above all. Keeping those in plaintext next to everything else is
the gap SECURITY.md calls out.

So the store encrypts *only the secrets*, leaving the rest of each profile
readable. You get a vault without losing the transparency that makes the store
nice to work with: names, OS, tags stay visible; the proxy password becomes an
opaque token until you unlock with the master password.

The actual cryptography is delegated to the ``cryptography`` package (Fernet:
AES-128-CBC + HMAC, authenticated) so we don't hand-roll a cipher. The key is
derived from the master password with PBKDF2-HMAC-SHA256. Both live behind an
optional extra — the engine core stays dependency-free — and every entry point
degrades gracefully when the package isn't installed.
"""

from __future__ import annotations

import base64
import hashlib
from functools import lru_cache
import importlib.util
import os
import re
from typing import Optional

# Current OWASP guidance for PBKDF2-HMAC-SHA256. New secrets are written at
# this cost; `v1` tokens written at the old 200,000 keep opening at 200,000,
# because the round count is part of what the key was derived from and there is
# no way to re-derive an existing token without the password in hand.
PBKDF2_ROUNDS = 600_000
ROUNDS_BY_VERSION = {"v1": 200_000, "v2": PBKDF2_ROUNDS}
CURRENT_VERSION = "v2"

# Marks an encrypted string in the stored JSON, so decrypt knows what to touch
# and a value is never double-encrypted. The version travels in the prefix so a
# stronger KDF can be adopted without a migration that needs every vault
# unlocked at once - which is exactly why the count could not be raised before.
ENC_PREFIX = "enc:v1:"
ENC_PREFIXES = tuple(f"enc:{v}:" for v in ROUNDS_BY_VERSION)
WRITE_PREFIX = f"enc:{CURRENT_VERSION}:"


class VaultLocked(RuntimeError):
    """Raised when a secret is needed but no (correct) master password is set."""


def available() -> bool:
    """Is the crypto backend installed?"""
    return importlib.util.find_spec("cryptography") is not None


def _require():
    if not available():
        raise RuntimeError(
            "Encrypted storage needs the 'cryptography' package.\n"
            'Install it with:  pip install -e ".[secure]"'
        )


def new_salt() -> str:
    """A fresh random salt (base64) to store alongside the vault."""
    return base64.b64encode(os.urandom(16)).decode("ascii")


@lru_cache(maxsize=16)
def _derive_key(password: str, salt_b64: str, rounds: int = PBKDF2_ROUNDS) -> bytes:
    """The Fernet key for this password and salt.

    Cached because it is a pure function of its two arguments and costs 200,000
    rounds of PBKDF2 - and a vault has exactly one salt, fixed when it was
    created. Without the cache every single encrypt and decrypt re-derived the
    same key: listing profiles decrypts every profile, listing the pool decrypts
    every proxy, and saving a pool re-encrypts every entry, so one "test all"
    sweep across N proxies paid it on the order of N-squared times.

    Caching the derived key adds no exposure the process does not already have -
    the master password itself is held in memory as `ProfileStore._password` for
    exactly as long. Bounded so a long-lived server cannot grow it without limit.
    """
    salt = base64.b64decode(salt_b64)
    return base64.urlsafe_b64encode(
        hashlib.pbkdf2_hmac("sha256", password.encode("utf-8"), salt, rounds))


def _fernet(password: str, salt_b64: str, rounds: int = PBKDF2_ROUNDS):
    from cryptography.fernet import Fernet
    return Fernet(_derive_key(password, salt_b64, rounds))


# Any `enc:v<N>:` marker, including a version this build cannot open. Matching
# only the *known* prefixes made an `enc:v9:` record read back as ordinary
# plaintext, and every caller then did the wrong thing with it: handed it to a
# launcher as the password, counted it as a plaintext leak, wrapped it a second
# time. A future envelope is still an envelope.
_ENVELOPE = re.compile(r"^enc:v\d+:")


def is_envelope(value) -> bool:
    """Any versioned secret envelope, readable by this build or not."""
    return isinstance(value, str) and _ENVELOPE.match(value) is not None


def is_supported_envelope(value) -> bool:
    """An envelope whose KDF version this build knows how to open."""
    return isinstance(value, str) and value.startswith(ENC_PREFIXES)


# The question almost every caller means is the wide one: "is this a secret
# marker rather than a password?" Answering it narrowly is what let a future
# version through.
is_encrypted = is_envelope


def encrypt(password: str, salt_b64: str, plaintext: str) -> str:
    _require()
    token = _fernet(password, salt_b64).encrypt(plaintext.encode("utf-8"))
    return WRITE_PREFIX + token.decode("ascii")


def decrypt(password: str, salt_b64: str, value: str) -> str:
    _require()
    from cryptography.fernet import InvalidToken

    version = value.split(":")[1] if value.count(":") >= 2 else CURRENT_VERSION
    rounds = ROUNDS_BY_VERSION.get(version)
    if rounds is None:
        raise VaultLocked(f"this secret was written by a newer version "
                          f"({version}) than this engine understands")
    try:
        token = value[len(f"enc:{version}:"):].encode("ascii")
        return _fernet(password, salt_b64, rounds).decrypt(token).decode("utf-8")
    except InvalidToken as e:
        raise VaultLocked("wrong master password") from e


def make_verifier(password: str, salt_b64: str) -> str:
    """A token we can later decrypt to confirm a password is correct."""
    return encrypt(password, salt_b64, "numera-vault-ok")


def check_password(password: str, salt_b64: str, verifier: str) -> bool:
    """Does ``password`` match the one the vault was set up with?"""
    try:
        return decrypt(password, salt_b64, verifier) == "numera-vault-ok"
    except VaultLocked:
        return False
