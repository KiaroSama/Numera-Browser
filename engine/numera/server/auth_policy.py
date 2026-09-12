"""Whether this API is protected, as a document with exactly one valid shape.

Split out of `stores.py`, which owns the token table. This owns the *policy*:
one boolean decision, persisted separately from the credentials so that losing
the credentials cannot be mistaken for a decision to stop requiring them.

The whole point of the file is that it is read strictly. `bool(marker.get(
"protected"))` reads `null`, `0`, `""`, `[]` and `{}` all as "open" - five
values that mean the document is damaged or was written by something that did
not understand it, and none of which is anyone deciding to expose an API.
Opening a protected service is the one direction that needs a deliberate,
unambiguous act, so exactly one value does it: JSON `false`.

Everything else - absent key, wrong type, unparseable file, wrong container -
is `AMBIGUOUS`, and ambiguity keeps the door shut.
"""
from __future__ import annotations

import json
import time
from pathlib import Path

from ..paths import write_json

#: The three answers a policy document can give. `ABSENT` is the only one that
#: means "no decision has ever been recorded here"; `AMBIGUOUS` means a
#: decision may have been recorded and can no longer be read, which is not the
#: same thing and must never be treated as one.
OPEN = "open"
PROTECTED = "protected"
ABSENT = "absent"
AMBIGUOUS = "ambiguous"


def policy_path(root) -> Path:
    return Path(root) / "auth-policy.json"


def read(root) -> str:
    """`OPEN`, `PROTECTED`, `ABSENT` or `AMBIGUOUS` for the document at `root`."""
    path = policy_path(root)
    if not path.exists():
        return ABSENT
    try:
        marker = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return AMBIGUOUS
    if not isinstance(marker, dict) or "protected" not in marker:
        return AMBIGUOUS
    value = marker["protected"]
    # `is`, not truthiness: `0`, `""`, `[]`, `{}` and `null` are all falsey and
    # none of them is a decision. Note `False is 0` is False in Python, so the
    # integer zero does not sneak through this comparison.
    if value is False:
        return OPEN
    if value is True:
        return PROTECTED
    return AMBIGUOUS


def write(root, protected: bool) -> None:
    """Record the decision. `protected` is coerced to a real JSON boolean."""
    path = policy_path(root)
    write_json(path, {"protected": bool(protected), "since": time.time()})
    try:
        path.chmod(0o600)
    except OSError:
        pass
