"""The unlock has to reach every process, and locked work must refuse.

Two halves of one defect. `numera --password X serve` unlocked a store and threw
it away - the server built its own locked one, so every browser it launched went
out with no proxy password. That failure is invisible: the profile browses, just
without its proxy, which for an anti-detect tool is the worst possible way to
fail.

The other half is the mirror. A locked read returns `None` for a secret by
design, and several operations read that as "there is no password" rather than
"there is one and I cannot see it" - so a backup claimed to hold a credential it
did not, and a health check tested a blank password and recorded a real failure.
"""
import json
import subprocess

import pytest

from numera import crypto, generate, vault
from numera.models import Profile, Proxy
from numera.paths import child_env
from numera.server.stores import DashboardStore, ProxyPool
from numera.store import ProfileStore

pytest.importorskip("cryptography")

SECRET = "PROXY-PASSWORD"


def vaulted(root, password="master"):
    """A root with one profile and one pool proxy, both holding a secret."""
    engine = ProfileStore(root)
    pool = ProxyPool(root, engine)
    engine.save(Profile(name="p1", fingerprint=generate(seed=1),
                        proxy=Proxy(server="http://1.2.3.4:8080",
                                    username="u", password=SECRET)))
    pool.add_many([{"type": "HTTP", "host": "5.6.7.8", "port": "8080",
                    "user": "u", "pass": SECRET}])
    engine.enable_vault(password)
    return engine


# --- the unlock reaches the surfaces that outlive the command ---------------

def test_serve_gets_an_unlocked_store_when_the_cli_was_unlocked(tmp_path):
    """`create_app` built its own `ProfileStore(data_dir)` with no password, so
    the flag did nothing for the one surface that runs longest."""
    pytest.importorskip("fastapi")
    from numera.server import create_app

    vaulted(tmp_path)
    app = create_app(data_dir=str(tmp_path), start_scheduler=False, password="master")
    assert app.state.engine_store.unlocked(), "the server's own store is locked"
    prof = app.state.engine_store.resolve("p1")
    assert prof.proxy.password == SECRET, "the launcher would get no proxy password"


def test_without_a_password_the_server_store_stays_locked(tmp_path):
    """The other direction: passing nothing must not silently unlock anything."""
    pytest.importorskip("fastapi")
    from numera.server import create_app

    vaulted(tmp_path)
    app = create_app(data_dir=str(tmp_path), start_scheduler=False)
    assert not app.state.engine_store.unlocked()


def test_a_child_inherits_the_unlock_by_environment(tmp_path):
    env = child_env("master")
    assert env["NUMERA_PASSWORD"] == "master"
    # It must be a full environment, not just the one key - a child with an
    # empty environment loses PATH and cannot start.
    assert len(env) > 1


def test_the_password_never_appears_in_a_child_argv(tmp_path, monkeypatch):
    """argv is readable by every process on the machine. The unlock travels by
    environment or not at all."""
    pytest.importorskip("fastapi")
    from numera.server import create_app

    vaulted(tmp_path)
    seen = {}

    from _child import FakeChild

    def capture(cmd, *a, **k):
        seen["argv"] = list(cmd)
        seen["env"] = k.get("env") or {}
        # A real child answers the startup handshake before the parent calls it
        # the owner, so a stand-in that never does would only prove the wait is
        # bounded - which `test_the_wait_is_bounded` already does.
        return FakeChild(cmd, seen["env"])

    monkeypatch.setattr(subprocess, "Popen", capture)
    app = create_app(data_dir=str(tmp_path), start_scheduler=False, password="master")
    from fastapi.testclient import TestClient
    c = TestClient(app)
    pid = c.get("/api/profiles").json()[0]["id"]
    c.post(f"/api/profiles/{pid}/launch", json={})

    assert seen, "no child was spawned"
    assert "master" not in " ".join(seen["argv"]), f"password leaked into argv: {seen['argv']}"
    assert seen["env"].get("NUMERA_PASSWORD") == "master", "the child was left locked"


# --- locked work refuses instead of producing a wrong artifact --------------

def test_a_backup_refuses_rather_than_dropping_the_password(tmp_path):
    """`backup`'s own docstring promises the archive carries the proxy
    password. While locked it would not, and nothing in the file would say so -
    the restore comes back a working-looking profile that cannot reach its
    proxy."""
    vaulted(tmp_path)
    locked = ProfileStore(tmp_path)
    pid = locked.resolve("p1").id
    assert locked.secret_withheld(pid)

    with pytest.raises(crypto.VaultLocked):
        locked.backup(pid, tmp_path / "out.zip")
    assert not (tmp_path / "out.zip").exists(), "a half-truthful archive was written"


def test_an_export_refuses_rather_than_dropping_the_password(tmp_path):
    vaulted(tmp_path)
    locked = ProfileStore(tmp_path)
    pid = locked.resolve("p1").id
    with pytest.raises(crypto.VaultLocked):
        locked.export(pid, tmp_path / "out.json")
    assert not (tmp_path / "out.json").exists()


def test_unlocked_export_and_backup_still_carry_the_password(tmp_path):
    """The refusal must be about the lock, not about the feature."""
    vaulted(tmp_path)
    unlocked = ProfileStore(tmp_path, password="master")
    pid = unlocked.resolve("p1").id

    out = unlocked.export(pid, tmp_path / "ok.json")
    assert SECRET in out.read_text(encoding="utf-8")
    assert unlocked.backup(pid, tmp_path / "ok.zip").exists()


def test_a_profile_without_a_secret_exports_while_locked(tmp_path):
    """Non-secret work stays available - only what needs the credential stops."""
    engine = vaulted(tmp_path)
    plain = engine.save(Profile(name="no-secret", fingerprint=generate(seed=9)))
    locked = ProfileStore(tmp_path)
    assert not locked.secret_withheld(plain.id)
    assert locked.export(plain.id, tmp_path / "plain.json") is not None


def test_a_locked_proxy_test_is_refused_not_recorded_as_failed(tmp_path):
    """A blank password against a real proxy fails, and writing that down marks
    a working proxy broken."""
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    vaulted(tmp_path)
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    entry = c.get("/api/proxies").json()[0]
    assert entry["pass"] is None, "a locked read should hide the secret"

    r = c.post(f"/api/proxies/{entry['id']}/test")
    assert r.status_code == 409, r.status_code
    assert c.get("/api/proxies").json()[0].get("status") != "failed", \
        "a locked check recorded a false failure"


def test_a_locked_assign_is_refused(tmp_path):
    """Assigning would write `pass=None` onto the profile; on one that already
    has a proxy the save path keeps the old ciphertext while host and user
    change, leaving a credential pair belonging to no proxy at all."""
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    engine = vaulted(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    dash.save({"name": "target", "tags": []})
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))
    r = c.post("/api/proxies/assign", json={})
    assert r.status_code == 409, r.status_code


def test_unlocked_assign_still_works(tmp_path):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    engine = vaulted(tmp_path)
    dash = DashboardStore(tmp_path, engine_store=engine)
    dash.save({"name": "target", "tags": []})
    c = TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False,
                              password="master"))
    r = c.post("/api/proxies/assign", json={})
    assert r.status_code == 200, r.json()


def test_is_withheld_only_fires_on_a_real_locked_secret(tmp_path):
    engine = vaulted(tmp_path)
    unlocked = ProfileStore(tmp_path, password="master")
    locked = ProfileStore(tmp_path)

    cipher = json.loads(
        (tmp_path / "profiles" / f"{locked.resolve('p1').id}.json")
        .read_text(encoding="utf-8"))["proxy"]["password"]

    assert crypto.is_encrypted(cipher)
    assert vault.is_withheld(locked, cipher) is True
    assert vault.is_withheld(unlocked, cipher) is False
    assert vault.is_withheld(locked, None) is False
    assert vault.is_withheld(locked, "") is False
    assert vault.is_withheld(locked, "plaintext") is False
    assert engine is not None
