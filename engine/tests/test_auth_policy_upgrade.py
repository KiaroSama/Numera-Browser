"""An upgrade may not quietly unprotect an API that was already protected.

`0.31.0` moved "is this API protected" out of the credential count and into
`auth-policy.json`, so losing the credentials could not open the door. It
works - for a data root created after the change. Only `TokenStore.create`
writes the marker, and an existing installation's tokens were minted before it
existed.

So a 0.30.1 data root upgrades into a server with `tokens.json` and no marker
at all. It looks protected, because `protected()` falls back to "are there
tokens". Take the tokens away and the fallback goes with them:

* delete `tokens.json` - anonymous `/api/health` goes 401 -> 200;
* corrupt it - the same, and this is the worse one, because "I could not read
  the credentials" is being answered as "there are no credentials";
* empty it to `[]` - the same.

Both while the process runs and after a restart. The new-install case stays
protected throughout, which is what makes this specifically an upgrade gap and
not a reason to distrust the policy file.

Two rules are asserted here:

1. An already-protected token store is **adopted** into durable policy before
   it is served, so the marker exists by the time anything can remove it.
2. Unreadable is not open. A damaged credential list, or a damaged marker, is
   a reason to keep asking for a credential - never a configuration choice to
   stop.
"""

import json

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient  # noqa: E402

from numera.server import create_app  # noqa: E402
from numera.server.stores import TokenStore  # noqa: E402

POLICY = "auth-policy.json"
TOKENS = "tokens.json"


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def legacy_root(tmp_path):
    """A data root exactly as 0.30.1 left it: tokens, and no policy marker.

    Written as bytes rather than through `TokenStore.create`, because the
    whole point is that this store was minted by a build that had no marker
    to write.
    """
    store = TokenStore(tmp_path)
    (tmp_path / TOKENS).write_text(json.dumps([
        {"id": "aaaaaaaaaaaa", "name": "CI",
         "hash": store.hash("nmr_legacy-token-value"),
         "created": 1.0, "lastUsed": None}]), encoding="utf-8")
    assert not (tmp_path / POLICY).exists(), "the fixture is not a legacy root"
    return tmp_path


@pytest.fixture(autouse=True)
def _no_env_token(monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)


# ---------------------------------------------------------------------------
# 1. An existing protected store is adopted before it is served
# ---------------------------------------------------------------------------

def test_a_legacy_root_is_protected_to_begin_with(tmp_path):
    """The control: nothing below is interesting if this was already open."""
    c = client(legacy_root(tmp_path))

    assert c.get("/api/health").status_code == 401


def test_serving_a_legacy_root_records_the_policy(tmp_path):
    """The fix, stated directly. The marker has to exist *before* anything can
    take the credentials away, which means at the moment the store is served."""
    root = legacy_root(tmp_path)

    client(root)

    assert (root / POLICY).exists(), \
        "an already-protected store was served without recording that it is protected"
    assert json.loads((root / POLICY).read_text("utf-8"))["protected"] is True


@pytest.mark.parametrize("damage,label", [
    (lambda p: p.unlink(), "deleting tokens.json"),
    (lambda p: p.write_text("{not json", encoding="utf-8"), "corrupting tokens.json"),
    (lambda p: p.write_text("[]", encoding="utf-8"), "emptying tokens.json"),
    (lambda p: p.write_text('{"tokens": []}', encoding="utf-8"), "reshaping tokens.json"),
])
def test_losing_the_credentials_of_a_legacy_root_keeps_it_closed(tmp_path, damage, label):
    """The regression, all four ways, while the process is running."""
    root = legacy_root(tmp_path)
    c = client(root)
    assert c.get("/api/health").status_code == 401, "the fixture started open"

    damage(root / TOKENS)

    assert c.get("/api/health").status_code == 401, \
        f"{label} turned a protected API anonymous without a restart"


@pytest.mark.parametrize("damage,label", [
    (lambda p: p.unlink(), "deleting tokens.json"),
    (lambda p: p.write_text("{not json", encoding="utf-8"), "corrupting tokens.json"),
    (lambda p: p.write_text("[]", encoding="utf-8"), "emptying tokens.json"),
])
def test_the_policy_survives_a_restart(tmp_path, damage, label):
    """A restart re-reads from disk, so this is the question of whether the
    decision was ever written down."""
    root = legacy_root(tmp_path)
    client(root)                       # first run: adopts
    damage(root / TOKENS)

    assert client(root).get("/api/health").status_code == 401, \
        f"{label} turned a protected API anonymous across a restart"


# ---------------------------------------------------------------------------
# 2. Unreadable is not open
# ---------------------------------------------------------------------------

def test_an_unreadable_credential_list_is_not_an_open_configuration(tmp_path):
    """The recurring defect in its newest place: a corrupt file answering the
    question "how many credentials are there" with "none"."""
    store = TokenStore(tmp_path)
    (tmp_path / TOKENS).write_text("{not json", encoding="utf-8")

    assert store.state() == "unreadable"
    assert store.protected() is True, \
        "a corrupt credential list was read as a decision to require nothing"


def test_a_damaged_policy_marker_is_not_an_open_configuration(tmp_path):
    """The same rule one level up. A marker that cannot be parsed is damage."""
    store = TokenStore(tmp_path)
    store.set_protected(True)
    (tmp_path / POLICY).write_text("{not json", encoding="utf-8")

    assert store.protected() is True, \
        "a corrupt policy marker was read as a decision to require nothing"


def test_turning_protection_off_is_still_possible_and_explicit(tmp_path):
    """Fail-closed needs a documented way out, or the only recovery from a
    damaged file is editing application state by hand."""
    store = TokenStore(tmp_path)
    store.set_protected(True)
    (tmp_path / TOKENS).unlink(missing_ok=True)

    store.set_protected(False)

    assert store.protected() is False
    assert client(tmp_path).get("/api/health").status_code == 200


# ---------------------------------------------------------------------------
# A new install is a different thing, and must stay that way
# ---------------------------------------------------------------------------

def test_a_genuinely_new_install_is_open_and_unmarked(tmp_path):
    """Adoption must not invent a policy where no credential ever existed -
    that would lock a first-run loopback install out of its own dashboard."""
    c = client(tmp_path)

    assert c.get("/api/health").status_code == 200
    assert not (tmp_path / POLICY).exists(), \
        "a brand-new install was marked protected with no credential to use"


def test_an_environment_token_still_opens_a_locked_root(tmp_path, monkeypatch):
    """The recovery path the docs name. It has to keep working after the token
    file is gone, or fail-closed becomes a trap."""
    root = legacy_root(tmp_path)
    client(root)
    (root / TOKENS).unlink()
    monkeypatch.setenv("NUMERA_API_TOKEN", "env-token-value")

    c = client(root)

    assert c.get("/api/health").status_code == 401
    assert c.get("/api/health",
                 headers={"Authorization": "Bearer env-token-value"}).status_code == 200


def test_adoption_does_not_disturb_a_store_that_already_has_a_marker(tmp_path):
    """Idempotence, and no rewriting of a deliberate decision."""
    store = TokenStore(tmp_path)
    store.set_protected(False)
    before = (tmp_path / POLICY).read_text("utf-8")

    client(tmp_path)

    assert (tmp_path / POLICY).read_text("utf-8") == before, \
        "serving the store overwrote an explicit decision to stay open"
