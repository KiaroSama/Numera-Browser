"""The vault descriptor that is actually in force, and what makes one usable.

Three questions live here and they are the same question asked at three moments:

* **Is this a descriptor at all** — `usable()`. A salt alone is not one.
* **Is the generation currently in force allowed to be, for the transition the
  journal describes** — `require_transition_config()`, asked before recovery
  writes anything.
* **Is the generation that reached the config the one that was verified** —
  `verify_persisted_completion()`, asked after recovery writes, by reading the
  file back rather than trusting the callback that wrote it.

Separated from `vault_recovery` because that module is about *finishing an
interrupted migration* while this one is about *the descriptor itself*, and
because the recovery module is at the size where new code belongs in the
responsibility it names rather than appended to the end.

`vault_recovery` owns refusal and completion scoring, so those two helpers are
imported inside the functions that need them - the same local-import convention
`disposal` uses for `proctree`, and for the same reason: the dependency runs
both ways at module scope and only one way at call time.
"""

from __future__ import annotations

import base64
import json
from pathlib import Path

from .vault_problems import (
    DESCRIPTOR_DISAGREEMENT,
    DESCRIPTOR_UNUSABLE,
    Problem,
    UNREADABLE_FILES,
    VaultIncomplete,
    same_descriptor,
)


def _text(value) -> bool:
    """A non-empty string, and nothing that merely looks like one."""
    return isinstance(value, str) and bool(value.strip())


def usable(desc) -> bool:
    """Is this a descriptor, or just a salt someone wrote down?

    Both halves are load-bearing. The salt says which key the ciphertext is
    under; the verifier is the only way to tell a correct password from one
    that will decrypt every record into noise. A journal carrying only the
    first let recovery commit a vault whose password could never be checked -
    and `crypto.check_password(pw, salt, None)` answers False for every
    password, so the store it produced could not be unlocked by anyone.
    """
    if not (isinstance(desc, dict)
            and _text(desc.get("salt")) and _text(desc.get("verifier"))):
        return False
    # The salt has to be the encoding the key derivation will feed to
    # `b64decode`. Checking only that it is a nonempty string let a damaged
    # descriptor through to `binascii.Error`, raised out of what every caller
    # asks as a yes/no question.
    try:
        base64.b64decode(desc["salt"], validate=True)
    except (ValueError, TypeError):
        return False
    return True


def require_transition_config(root, cfg, new, old, op) -> None:
    """Validate the active generation before either recovery path can write.

    A missing active descriptor is legitimate only for an enable. Rotation
    starts from a configured generation, so absent/damaged/unrelated config
    is unresolved evidence, not permission to choose the journal's key.
    """
    from .vault_recovery import _refuse_incomplete

    if not isinstance(cfg, dict):
        _refuse_incomplete(root, "migration", [Problem(
            DESCRIPTOR_UNUSABLE, "the active config is not a settings object")])
    active = cfg.get("vault")
    if active is None and op == "enable":
        return
    if not usable(active):
        _refuse_incomplete(root, "migration", [Problem(
            DESCRIPTOR_UNUSABLE, "the active config has no usable vault descriptor")])
    permitted = (new, old) if op == "rotate" else (new,)
    if not any(same_descriptor(active, desc) for desc in permitted if desc is not None):
        _refuse_incomplete(root, "migration", [Problem(
            DESCRIPTOR_DISAGREEMENT,
            "the active descriptor is neither an allowed starting generation "
            "nor the journal's target; neither record has been discarded")])


def verify_persisted_completion(root, new, password, what) -> None:
    """Read back the descriptor the next opener will use, then prove its data.

    A successful callback is not evidence that it wrote the intended config.
    This check stays inside migration's rollback boundary when writes occur.
    """
    from .vault_recovery import _refuse_incomplete, completion_problems

    try:
        cfg = json.loads((Path(root) / "config.json").read_text(encoding="utf-8"))
    except (OSError, ValueError, UnicodeError, RecursionError) as error:
        raise VaultIncomplete(
            "The persisted config could not be read; migration completion "
            "has not been established and recovery evidence is retained.",
            [Problem(UNREADABLE_FILES, "config.json cannot be read")]) from error
    active = cfg.get("vault") if isinstance(cfg, dict) else None
    if not usable(active) or not same_descriptor(active, new):
        _refuse_incomplete(root, what, [Problem(
            DESCRIPTOR_DISAGREEMENT,
            "the descriptor persisted in config.json does not match the "
            "verified target generation")])
    problems = completion_problems(root, active, password)
    if problems:
        _refuse_incomplete(root, what, problems)
