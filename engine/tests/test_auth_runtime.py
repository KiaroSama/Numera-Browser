"""Auth has to be a runtime decision, and a non-loopback bind has to be refused.

Two findings from the audit, both of which the previous tests missed because
they set the world up *before* `create_app` and then asked whether the app was
locked. The real flow is the other way round: the app is already running when
the first token is minted, and that is exactly the moment protection has to
begin.
"""
import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient  # noqa: E402

from numera import server  # noqa: E402
from numera.server import create_app  # noqa: E402
from numera.server.stores import TokenStore  # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def auth(token):
    return {"Authorization": f"Bearer {token}"}


# --- auth is evaluated per request -----------------------------------------

def test_minting_the_first_token_protects_the_running_app(tmp_path, monkeypatch):
    """The whole point of the feature. Auth used to be decided once inside
    `create_app`, so the first token protected nothing until a restart - and the
    user was told the API now required one."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    c = client(tmp_path)
    assert c.get("/api/health").status_code == 200, "should start open"

    made = c.post("/api/tokens", json={"name": "CI"}).json()

    assert c.get("/api/health").status_code == 401, \
        "the same running app still answers without a token"
    assert c.get("/api/health", headers=auth(made["token"])).status_code == 200


def test_revoking_the_last_token_reopens_the_same_running_app(tmp_path, monkeypatch):
    """The mirror image: the state has to be read, not remembered."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    c = client(tmp_path)
    made = c.post("/api/tokens", json={"name": "CI"}).json()
    assert c.get("/api/health").status_code == 401

    c.delete(f"/api/tokens/{made['id']}", headers=auth(made["token"]))
    assert c.get("/api/health").status_code == 200


def test_a_revoked_token_stops_working_immediately(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    c = client(tmp_path)
    keep = c.post("/api/tokens", json={"name": "keep"}).json()
    drop = c.post("/api/tokens", json={"name": "drop"}, headers=auth(keep["token"])).json()
    c.delete(f"/api/tokens/{drop['id']}", headers=auth(keep["token"]))
    assert c.get("/api/health", headers=auth(drop["token"])).status_code == 401
    assert c.get("/api/health", headers=auth(keep["token"])).status_code == 200


def test_the_dashboard_itself_stays_reachable_when_the_api_is_locked(tmp_path, monkeypatch):
    """Gating the SPA behind the token would leave no page on which to enter
    the token. `/api/*` is the protected surface; the shell is not."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    c = client(tmp_path)
    TokenStore(tmp_path).create("CI")
    r = c.get("/")
    assert r.status_code in (200, 404), r.status_code   # 404 only if dist is absent
    assert r.status_code != 401


# --- CORS posture ----------------------------------------------------------

def test_a_locked_api_never_advertises_a_wildcard_origin(tmp_path, monkeypatch):
    """`*` plus a bearer token is a token any web page can spend. The env-token
    path already narrowed CORS; the named-token path did not, because the
    decision was made before any token existed."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    monkeypatch.delenv("NUMERA_ALLOWED_ORIGINS", raising=False)
    TokenStore(tmp_path).create("CI")
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    origins = [m for m in app.user_middleware if "CORS" in str(m)]
    assert origins, "no CORS middleware installed"
    assert "*" not in (origins[0].kwargs.get("allow_origins") or [])


def test_the_default_is_same_origin_not_a_wildcard(tmp_path, monkeypatch):
    """The engine serves the dashboard itself, so the normal case needs no CORS
    at all. The wildcard used to be chosen once at startup, which meant a server
    that started open kept advertising it after the first token locked the API."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    monkeypatch.delenv("NUMERA_ALLOWED_ORIGINS", raising=False)
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    cors = [m for m in app.user_middleware if "CORS" in str(m)][0]
    assert cors.kwargs.get("allow_origins") == []


def test_creating_the_first_token_cannot_leave_a_wildcard_behind(tmp_path, monkeypatch):
    """The transition the audit named: open at startup, locked at runtime."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    monkeypatch.delenv("NUMERA_ALLOWED_ORIGINS", raising=False)
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    c = TestClient(app)
    c.post("/api/tokens", json={"name": "CI"})
    cors = [m for m in app.user_middleware if "CORS" in str(m)][0]
    assert "*" not in (cors.kwargs.get("allow_origins") or [])


def test_an_explicit_origin_is_honoured(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    monkeypatch.setenv("NUMERA_ALLOWED_ORIGINS", "http://localhost:5173")
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    cors = [m for m in app.user_middleware if "CORS" in str(m)][0]
    assert cors.kwargs.get("allow_origins") == ["http://localhost:5173"]


# --- readiness -------------------------------------------------------------

def test_ready_answers_while_the_api_is_locked(tmp_path, monkeypatch):
    """The desktop shell polls this. It used to poll `/api/health`, which is
    behind the token - so an engine with one configured looked like it had
    failed to start, and the screen that asks for a token was unreachable."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    TokenStore(tmp_path).create("CI")
    r = client(tmp_path).get("/api/ready")
    assert r.status_code == 200, r.status_code
    body = r.json()
    assert body["service"] == "numera-browser"
    assert body["authRequired"] is True
    assert body["ok"] is True


def test_ready_says_when_no_token_is_needed(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    assert client(tmp_path).get("/api/ready").json()["authRequired"] is False


def test_ready_answers_with_an_env_token_too(tmp_path, monkeypatch):
    monkeypatch.setenv("NUMERA_API_TOKEN", "secret")
    body = client(tmp_path).get("/api/ready").json()
    assert body["authRequired"] is True and body["ok"] is True


def test_ready_exposes_nothing_protected(tmp_path, monkeypatch):
    """It is public, so it may only carry what a launcher needs."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    TokenStore(tmp_path).create("CI")
    body = client(tmp_path).get("/api/ready").json()
    assert set(body) == {"ok", "service", "version", "authRequired"}


def test_health_is_still_protected(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    TokenStore(tmp_path).create("CI")
    assert client(tmp_path).get("/api/health").status_code == 401


# --- binding ---------------------------------------------------------------

@pytest.mark.parametrize("host", ["0.0.0.0", "::", "192.168.1.10"])
def test_a_non_loopback_bind_without_auth_is_refused(tmp_path, monkeypatch, host):
    """The API creates profiles, launches browsers and exports cookies. It is
    only safe unauthenticated because nothing outside the machine can reach it."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    monkeypatch.delenv("NUMERA_ALLOW_INSECURE_BIND", raising=False)
    with pytest.raises(SystemExit) as e:
        server.serve(host=host, data_dir=str(tmp_path))
    assert "token" in str(e.value).lower()


@pytest.mark.parametrize("host", ["127.0.0.1", "localhost", "::1"])
def test_loopback_needs_no_token(tmp_path, monkeypatch, host):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    assert server._refuse_insecure_bind(host, tmp_path) is None


def test_a_non_loopback_bind_is_allowed_once_a_token_exists(tmp_path, monkeypatch):
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    TokenStore(tmp_path).create("CI")
    assert server._refuse_insecure_bind("0.0.0.0", tmp_path) is None


def test_the_env_token_also_satisfies_the_bind_check(tmp_path, monkeypatch):
    monkeypatch.setenv("NUMERA_API_TOKEN", "secret")
    assert server._refuse_insecure_bind("0.0.0.0", tmp_path) is None


def test_the_override_exists_but_has_to_be_asked_for(tmp_path, monkeypatch):
    """Refusing outright would strand someone who genuinely fronts this with
    their own auth; making it silent would defeat the check."""
    monkeypatch.delenv("NUMERA_API_TOKEN", raising=False)
    monkeypatch.setenv("NUMERA_ALLOW_INSECURE_BIND", "1")
    assert server._refuse_insecure_bind("0.0.0.0", tmp_path) is None
