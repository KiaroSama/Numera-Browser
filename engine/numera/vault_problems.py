"""What a refused vault migration says, as data rather than as prose.

Split out of `vault_recovery` in 0.35.0. That module owns the *protocol* -
classify the journal, decide whether the data root is in the generation a
descriptor names, commit or refuse. This one owns the **vocabulary** the
refusal is expressed in: the exception types, the stable cause codes, and the
pairing of a code with the sentence a person reads.

They separate cleanly because they have different readers. A sentence is
written for whoever is looking at a failed migration and is free to be
rewritten the moment it reads better. A code is what a route, a CLI, or a test
matches on, and it must not move when the wording does. Keeping both in one
`Problem` and the codes in one reviewable list is what lets a caller ask *why*
a migration refused without parsing English.

Nothing here touches the disk.
"""

from __future__ import annotations

from typing import NamedTuple

from . import crypto


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


class Problem(NamedTuple):
    """One reason a migration is not complete: a stable code and a sentence.

    Two fields because they have two audiences. `detail` is for the person
    reading the refusal and is free to be rewritten whenever it reads better;
    `code` is what anything *checking* the refusal matches on, and it does not
    change with the prose.

    The separation is not decoration. A caller that asserts on the sentence
    passes wherever the phrasing it expected happens to be produced and fails
    where an equally correct refusal arrives by a different route - and the
    routes differ by what the machine has on disk, so the check is green
    exactly where it is least needed. `completion_problems` can return several
    of these, and which one comes first depends on the order they are appended.
    """
    code: str
    detail: str


#: Every code a completion check can return. Named here so a caller can match
#: one without importing a sentence, and so the set is reviewable in one place.
DESCRIPTOR_UNUSABLE = "descriptor_unusable"
UNREADABLE_FILES = "unreadable_files"
PLAINTEXT_LEFT = "plaintext_left"
NO_PASSWORD = "no_password"
NEEDS_BOTH_PASSWORDS = "needs_both_passwords"
VERIFIER_MISMATCH = "verifier_mismatch"
WRONG_GENERATION = "wrong_generation"


def codes(problems) -> set:
    """The causes in a problem list, without their wording."""
    return {p.code for p in problems}


def locked(role: str, message: str) -> crypto.VaultLocked:
    """A `VaultLocked` that says *which* password was rejected, as data.

    A resumed rotation is handed two passwords and either can be wrong. The
    sentence has always said which; `role` is `"old"` or `"new"` so a caller
    can prompt for the right one again without reading the sentence.
    """
    error = crypto.VaultLocked(message)
    error.role = role
    return error


class VaultIncomplete(VaultRecoveryRequired):
    """A refusal that carries its causes as data, not only as a sentence.

    `problems` is the list `completion_problems` produced, so a caller - a
    route turning this into a response, or a test - can ask *why* without
    parsing the message.
    """

    def __init__(self, message: str, problems: list):
        super().__init__(message)
        self.problems = list(problems)

    @property
    def codes(self) -> set:
        return {p.code for p in self.problems}
