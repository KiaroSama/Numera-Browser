"""A protected API may not become anonymous by losing its credentials.

`auth_required()` was `bool(env_token) or bool(tokens.list())` - a question
about how many credentials exist *right now*, standing in for a question about
policy. Three ways that turns a locked API open, with no restart and no warning:

* revoke the last named token, with no `NUMERA_API_TOKEN` set;
* delete `tokens.json`;
* corrupt it - invalid JSON, or valid JSON that is not a list.

The third is this codebase's recurring defect in a new place: `TokenStore.list()`
answered `[]` for *no file*, *unreadable file* and *genuinely empty* alike, so
"I could not read the credentials" and "there are no credentials" arrived as the
same value. The caller trusted it, because it reads as an answer.

The startup bind check does not help: it runs once, and all three transitions
happen while the process is up.

What is asserted here is the policy, not the count: once this API has been
protected, it stays protected until someone deliberately says otherwise.
"""

import json

import pytest

from numera.server.stores import TokenStore


@pytest.fixture
def store(tmp_path):
    return TokenStore(tmp_path)


def test_a_fresh_install_is_not_protected(store):
    """The control. Nothing has ever been minted, so nothing is claimed."""
    assert store.protected() is False
    assert store.state() == "empty"


def test_minting_the_first_token_turns_protection_on(store):
    store.create("ci")

    assert store.protected() is True
    assert store.state() == "tokens"


def test_protection_survives_revoking_the_last_token(store):
    """The first way it used to fail open."""
    made = store.create("only")
    store.revoke(made["id"])

    assert store.list() == []
    assert store.protected() is True, \
        "revoking the last token turned the API anonymous"


def test_protection_survives_the_file_being_deleted(store):
    """The second: the store is a file, and files go missing."""
    store.create("ci")
    store.path.unlink()

    assert store.protected() is True, \
        "deleting tokens.json turned the API anonymous"


@pytest.mark.parametrize("corrupt", [
    "{not json at all",
    '{"tokens": []}',          # valid JSON, wrong shape
    "null",
    '"a string"',
    "42",
])
def test_unreadable_storage_is_not_an_empty_list(store, corrupt):
    """The third, and the one that reads as an answer.

    `state()` must say it could not look. Reporting "empty" here is what let a
    corrupt file authorise every anonymous request.
    """
    store.create("ci")
    store.path.write_text(corrupt, encoding="utf-8")

    assert store.state() == "unreadable", f"{corrupt!r} was read as a credential count"
    assert store.protected() is True


def test_a_valid_empty_list_is_still_empty(store):
    """The distinction has to cut both ways, or it is just a broader refusal."""
    store.create("ci")
    store.path.write_text("[]", encoding="utf-8")

    assert store.state() == "empty"
    assert store.list() == []


def test_revoking_the_last_token_is_refused_without_an_alternative(store):
    """Fail-closed is correct but unhelpful on its own: a user who revokes
    their last token would lock themselves out of their own API. Refusing the
    revocation says so at the moment it can still be acted on."""
    made = store.create("only")

    with pytest.raises(ValueError, match="last"):
        store.revoke(made["id"], refuse_lockout=True)

    assert len(store.list()) == 1, "the refusal still removed the token"


def test_a_second_token_makes_the_first_revocable(store):
    first = store.create("one")
    store.create("two")

    store.revoke(first["id"], refuse_lockout=True)

    assert [t["name"] for t in store.list()] == ["two"]


def test_protection_can_be_turned_off_deliberately(store):
    """Recovery has to exist, and has to be explicit - otherwise the only way
    out of a corrupt token file is editing application state by hand."""
    store.create("ci")
    store.path.unlink()
    assert store.protected() is True

    store.set_protected(False)

    assert store.protected() is False


def test_the_marker_is_not_the_token_file(store):
    """A corrupt or deleted `tokens.json` must not take the policy with it -
    that is the whole failure being fixed."""
    store.create("ci")
    store.path.unlink()

    assert store.protected() is True
    assert TokenStore(store.path.parent).protected() is True, \
        "the policy did not survive being re-read from disk"
