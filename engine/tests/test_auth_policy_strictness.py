"""Falsey is not false, and a damaged credential file is still a credential file.

`0.32.0` moved "is this API protected" into `auth-policy.json` and adopted an
existing token store before serving it. Two gaps stayed open.

**The document was read with `bool()`.** `null`, `0`, `""`, `[]` and `{}` are
all falsey, and every one of them meant "open this service". None of them is
anyone deciding that: they are what a damaged file, a partial write or a
different writer leaves behind. Exposing an API is the one direction that needs
an unambiguous act, so exactly one value does it - JSON `false`.

**Adoption skipped the already-damaged case.** A legacy `tokens.json` that will
not parse *at startup* was protected only by the unreadable-state fallback, and
never written into the policy. Delete that file afterwards and the markerless
store went anonymous - the same upgrade gap the marker exists to close, one
state further along.

Also here, because it is the same request path: `compare_digest` on two `str`
raises `TypeError` unless both are ASCII. Checked at the layer where it is
actually reachable - a hand-edited `tokens.json`, not an HTTP header, which
cannot carry those bytes at all.

Most of this is store-level and runs in-process in milliseconds. Only the cases
that are genuinely about the running app use the TestClient, once each.
"""

import json

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient  # noqa: E402

from numera.server import auth_policy, create_app  # noqa: E402
from numera.server.stores import TokenStore  # noqa: E402

POLICY = "auth-policy.json"
TOKENS = "tokens.json"


@pytest.fixture(autouse=True)
def _no_env_token(monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)


def store(tmp_path):
    return TokenStore(tmp_path)


def legacy_tokens(tmp_path, body=None):
    """A 0.30.1-shaped credential file, with no policy marker beside it."""
    s = TokenStore(tmp_path)
    (tmp_path / TOKENS).write_text(
        body if body is not None else json.dumps([
            {"id": "aaaaaaaaaaaa", "name": "CI", "hash": s.hash("nmr_legacy"),
             "created": 1.0, "lastUsed": None}]),
        encoding="utf-8")
    assert not (tmp_path / POLICY).exists()
    return s


# ---------------------------------------------------------------------------
# 1. Only an unambiguous `false` opens the service
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("value", ["null", "0", '""', "[]", "{}", '"false"', "0.0"])
def test_a_falsey_policy_value_does_not_open_the_service(tmp_path, value):
    """The regression. Every one of these used to disable protection."""
    s = store(tmp_path)
    (tmp_path / POLICY).write_text('{"protected": %s}' % value, encoding="utf-8")

    assert auth_policy.read(tmp_path) == auth_policy.AMBIGUOUS
    assert s.protected() is True, f"protected:{value} was read as a decision to open"


def test_only_a_real_boolean_false_opens_it(tmp_path):
    """The escape hatch has to keep working, or a damaged marker is a trap."""
    s = store(tmp_path)
    (tmp_path / POLICY).write_text('{"protected": false}', encoding="utf-8")

    assert auth_policy.read(tmp_path) == auth_policy.OPEN
    assert s.protected() is False


def test_a_real_boolean_true_protects(tmp_path):
    s = store(tmp_path)
    s.set_protected(True)

    assert auth_policy.read(tmp_path) == auth_policy.PROTECTED
    assert s.protected() is True
    assert json.loads((tmp_path / POLICY).read_text("utf-8"))["protected"] is True


@pytest.mark.parametrize("body", ["{not json", "[]", '"a string"', "17"])
def test_a_damaged_document_is_ambiguous_not_open(tmp_path, body):
    s = store(tmp_path)
    (tmp_path / POLICY).write_text(body, encoding="utf-8")

    assert auth_policy.read(tmp_path) == auth_policy.AMBIGUOUS
    assert s.protected() is True


def test_no_document_is_absent_not_open(tmp_path):
    """`absent` and `open` must stay distinct: a brand-new loopback install has
    made no decision, and adoption depends on telling those apart."""
    assert auth_policy.read(tmp_path) == auth_policy.ABSENT
    assert store(tmp_path).protected() is False


# ---------------------------------------------------------------------------
# 2. A credential file that is already damaged is still adopted
# ---------------------------------------------------------------------------

def test_an_already_damaged_legacy_store_is_adopted(tmp_path):
    """The regression. This store *had* credentials; that they are now
    unreadable is damage, and damage is not a reason to stop asking for one."""
    s = legacy_tokens(tmp_path, body="{truncated")
    assert s.state() == "unreadable"

    assert s.adopt() is True
    assert auth_policy.read(tmp_path) == auth_policy.PROTECTED


def test_deleting_a_damaged_legacy_file_after_adoption_stays_closed(tmp_path):
    """The consequence: without adoption, removing the damaged file took the
    last thing holding the door shut with it."""
    s = legacy_tokens(tmp_path, body="{truncated")
    s.adopt()

    (tmp_path / TOKENS).unlink()

    assert s.protected() is True
    assert TokenStore(tmp_path).protected() is True


def test_a_healthy_legacy_store_is_still_adopted(tmp_path):
    s = legacy_tokens(tmp_path)

    assert s.adopt() is True
    assert auth_policy.read(tmp_path) == auth_policy.PROTECTED


def test_adoption_leaves_an_existing_decision_alone(tmp_path):
    """Including a deliberate `false`: adopting over it would silently undo the
    one documented way out of a fail-closed API."""
    s = legacy_tokens(tmp_path)
    s.set_protected(False)
    before = (tmp_path / POLICY).read_text("utf-8")

    assert s.adopt() is False
    assert (tmp_path / POLICY).read_text("utf-8") == before


def test_a_new_install_is_not_adopted(tmp_path):
    """Adoption must not invent a policy where no credential ever existed."""
    assert store(tmp_path).adopt() is False
    assert not (tmp_path / POLICY).exists()


# ---------------------------------------------------------------------------
# 3. The running app, where the app is actually the point
# ---------------------------------------------------------------------------

def test_a_damaged_legacy_root_stays_closed_across_a_restart(tmp_path):
    """End to end, because serving is what adoption is timed against."""
    legacy_tokens(tmp_path, body="{truncated")
    first = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    assert first.get("/api/health").status_code == 401

    (tmp_path / TOKENS).unlink()
    second = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))

    assert second.get("/api/health").status_code == 401, \
        "an upgraded root with damaged credentials went anonymous"


@pytest.mark.parametrize("stored", ["tökén", "密码", "plain-but-wrong"])
def test_a_non_ascii_stored_hash_is_a_miss_not_an_exception(tmp_path, stored):
    """`compare_digest` on two `str` raises `TypeError` unless both are ASCII.

    Not reachable through a conformant HTTP client: an `Authorization` header
    cannot carry these bytes and `httpx` refuses to send them, so the earlier
    claim that this was a live 500 would have been wrong. It *is* reachable
    through the file - a hand-edited or corrupted `tokens.json` puts an
    arbitrary string where the hash belongs, and every authenticated request
    then compares against it. A damaged credential row must read as "no match",
    never as a crash in the auth path.
    """
    s = TokenStore(tmp_path)
    (tmp_path / TOKENS).write_text(json.dumps(
        [{"id": "aaaaaaaaaaaa", "name": "hand-edited", "hash": stored,
          "created": 1.0, "lastUsed": None}]), encoding="utf-8")

    assert s.verify("nmr_anything") is None


def test_a_damaged_row_does_not_hide_a_healthy_one(tmp_path):
    """The comparison must keep working past the bad row, or one corrupt entry
    locks out every valid token beside it."""
    s = TokenStore(tmp_path)
    made = s.create("real")
    rows = json.loads((tmp_path / TOKENS).read_text("utf-8"))
    rows.insert(0, {"id": "bbbbbbbbbbbb", "name": "bad", "hash": "tökén",
                    "created": 1.0, "lastUsed": None})
    (tmp_path / TOKENS).write_text(json.dumps(rows), encoding="utf-8")

    assert s.verify(made["token"]) is not None

def test_the_right_token_still_authenticates(tmp_path, monkeypatch):
    """The control for the encoding change."""
    monkeypatch.setenv("NUMERA_API_TOKEN", "ascii-token-value")
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))

    assert c.get("/api/health",
                 headers={"Authorization": "Bearer ascii-token-value"}).status_code == 200
