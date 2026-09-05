"""Config joins the profile transaction, and a failed rollback says so.

Three residual holes after the first transaction pass.

`POST /api/profiles/batch` created the profiles in one transaction and then set
the default engine in a second write. A failure there returned 500 over
profiles that were already on disk, and the obvious retry duplicated every one
of them.

`config.json` was written from inside the batch with `batch.write`, which takes
no file lock, while ordinary settings writes took the file lock and no
transaction lock. Two independent lock domains: neither waited for the other,
so a rollback could restore config over a settings change that had already
succeeded.

`_Batch.rollback()` caught `OSError` per file and passed. A forward failure
followed by a rollback failure left partial state on disk while the caller saw
only the original error - which reads exactly like a clean refusal.
"""
import json

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import folders, paths
from numera.paths import RollbackFailed, write_batch

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient        # noqa: E402

from numera.server import create_app             # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def config_of(tmp_path) -> dict:
    path = tmp_path / "config.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}


def failing_write(after: int, real=None):
    """A `write_json` that raises on the n-th call and works before it."""
    real = real or paths.write_json
    calls = {"n": 0}

    def write(path, data):
        calls["n"] += 1
        if calls["n"] == after:
            raise OSError("no space left on device")
        return real(path, data)
    return write


# --- item 4a: batch create commits its engine change in the same transaction --

def test_a_failing_config_write_leaves_no_profiles_from_a_batch_create(
        tmp_path, monkeypatch):
    c = client(tmp_path)
    before = config_of(tmp_path)

    # Two profiles, two dashboard writes and two engine twins, then config.
    monkeypatch.setattr(paths, "write_json", failing_write(5))
    monkeypatch.setattr("numera.server.stores.write_json", failing_write(5))
    with pytest.raises(OSError):
        c.post("/api/profiles/batch",
               json={"profiles": [{"name": "a", "engine": "camoufox"},
                                  {"name": "b", "engine": "camoufox"}]})
    monkeypatch.undo()

    assert c.get("/api/profiles").json() == [], \
        "the batch failed but left profiles behind, so a retry duplicates them"
    assert config_of(tmp_path).get("default_engine") == before.get("default_engine"), \
        "the default engine changed although the request failed"


# --- item 4b: one lock hierarchy for config ---------------------------------

def test_a_profile_transaction_and_a_settings_change_both_survive(tmp_path):
    """`headless_launch` is written by the settings route, `default_engine` by
    the profile transaction. They used to be in different lock domains."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "a"}).json()["id"]

    holder = """
from numera import paths
from numera.store import ProfileStore
from numera.server.stores import DashboardStore
engine = ProfileStore(ROOT)
dash = DashboardStore(ROOT, engine)

def apply(current):
    say("inside")                       # transaction lock held
    wait()
    return {{**current, "engine": "camoufox"}}, {{"default_engine": "camoufox"}}

dash.patch({pid!r}, apply)
say("done")
""".format(pid=pid)
    setter = """
from numera.store import ProfileStore
store = ProfileStore(ROOT)
wait()
say("trying")
store.set_headless_launch(True)
say("done")
"""
    bar = Rendezvous()
    procs = {}
    try:
        procs["holder"] = start(holder, tmp_path, "holder", bar.port)
        procs["setter"] = start(setter, tmp_path, "setter", bar.port)
        bar.expect("holder", "inside")
        bar.release("setter")
        bar.expect("setter", "trying")
        bar.release("holder")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results

    cfg = config_of(tmp_path)
    assert cfg.get("default_engine") == "camoufox", cfg
    assert cfg.get("headless_launch") is True, \
        f"the settings change was lost to the profile transaction: {cfg}"


def test_a_rolled_back_transaction_cannot_undo_a_committed_settings_change(tmp_path):
    """The rollback restores config's *old* bytes. Under one hierarchy it can
    only do that while it owns the file, so a settings change either lands
    before the transaction started or after it finished - never inside it."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "a"}).json()["id"]

    holder = """
import numera.paths as paths
from numera.store import ProfileStore
from numera.server.stores import DashboardStore
engine = ProfileStore(ROOT)
dash = DashboardStore(ROOT, engine)
_write = paths.write_json
_seen = []

def hooked(path, data):
    _write(path, data)
    if str(path).endswith("config.json") and not _seen:
        _seen.append(path)
        say("inside")               # config committed inside the transaction
        wait()
        raise OSError("no space left on device")

paths.write_json = hooked
import numera.server.stores as stores
stores.write_json = hooked

def apply(current):
    return {{**current, "engine": "camoufox"}}, {{"default_engine": "camoufox"}}

try:
    dash.patch({pid!r}, apply)
    print("WROTE")
except OSError as e:
    print("FAILED", e)
say("done")
""".format(pid=pid)
    setter = """
from numera.store import ProfileStore
store = ProfileStore(ROOT)
wait()
say("trying")
store.set_headless_launch(True)
say("done")
"""
    bar = Rendezvous()
    procs = {}
    try:
        procs["holder"] = start(holder, tmp_path, "holder", bar.port)
        procs["setter"] = start(setter, tmp_path, "setter", bar.port)
        bar.expect("holder", "inside")
        bar.release("setter")
        bar.expect("setter", "trying")
        bar.release("holder")           # the transaction now fails and rolls back
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    assert results["holder"]["out"].startswith("FAILED"), results["holder"]

    cfg = config_of(tmp_path)
    assert cfg.get("headless_launch") is True, \
        f"a rollback overwrote a settings change that had succeeded: {cfg}"
    assert cfg.get("default_engine") != "camoufox", \
        f"the failed transaction left its config change behind: {cfg}"


# --- item 5: a rollback that failed is not a rollback -----------------------

def test_a_failed_rollback_is_raised_with_the_files_it_could_not_restore(
        tmp_path, monkeypatch):
    first, second = tmp_path / "one.json", tmp_path / "two.json"
    paths.write_json(first, {"keep": "original"})

    def refuse(path, original):
        raise OSError("the volume is read-only")

    # Through `monkeypatch`, so the module is restored even if an assertion
    # below raises - a bare assignment leaks a broken `_restore_bytes` into
    # every test that runs after this one in the same session.
    monkeypatch.setattr(paths, "_restore_bytes", refuse)
    with pytest.raises(RollbackFailed) as caught:
        with write_batch() as batch:
            batch.write(first, {"keep": "changed"})
            batch.write(second, {"any": "thing"})
            raise OSError("no space left on device")
    monkeypatch.undo()

    error = caught.value
    assert isinstance(error.cause, OSError), error.cause
    assert "no space left on device" in str(error), str(error)
    assert [str(p) for p, _ in error.failures] == [str(first)], error.failures
    assert str(first) in str(error), "the message must name the file to repair"
    # The file the rollback could not restore is still the failed write's, and
    # saying so is the whole point: silence here read as a clean refusal.
    assert json.loads(first.read_text(encoding="utf-8")) == {"keep": "changed"}
    assert not second.exists(), "deletes still worked, so they still ran"


def test_a_failed_folder_rollback_is_raised_too(tmp_path, monkeypatch):
    """Folder rename rewrites every profile in the folder through the batch."""
    c = client(tmp_path)
    c.post("/api/profiles", json={"name": "a", "folder": "Old"})
    c.post("/api/profiles", json={"name": "b", "folder": "Old"})

    real_write = paths.write_json
    calls = {"n": 0}

    def fail_late(path, data):
        calls["n"] += 1
        if calls["n"] >= 2:
            raise OSError("no space left on device")
        return real_write(path, data)

    def refuse(path, original):
        raise OSError("the volume is read-only")

    monkeypatch.setattr(paths, "write_json", fail_late)
    monkeypatch.setattr(paths, "_restore_bytes", refuse)
    with pytest.raises((RollbackFailed, OSError)) as caught:
        folders.rename(tmp_path, "Old", "New")
    monkeypatch.undo()

    assert isinstance(caught.value, RollbackFailed), \
        f"a rollback that could restore nothing was reported as {caught.value!r}"
    assert caught.value.failures, "RollbackFailed must carry what to repair"


# --- item 9: token names are unique because `create()` decides ---------------

def test_two_processes_cannot_create_the_same_token_name(tmp_path):
    body = """
from numera.server import DuplicateToken, TokenStore
tokens = TokenStore(ROOT)
wait()
try:
    tokens.create("CI runner")
    print("CREATED")
except DuplicateToken as e:
    print("REFUSED", e)
say("done")
"""
    bar = Rendezvous()
    procs = {}
    try:
        for who in ("first", "second"):
            procs[who] = start(body, tmp_path, who, bar.port)
        for who in procs:                   # both are checked in and blocked
            bar.release(who)
        for who in procs:
            bar.expect(who, "done")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results

    made = [w for w, r in results.items() if r["out"].startswith("CREATED")]
    assert len(made) == 1, f"exactly one create must win: {results}"
    kept = json.loads((tmp_path / "tokens.json").read_text(encoding="utf-8"))
    assert [t["name"] for t in kept] == ["CI runner"], kept


def test_the_api_and_the_cli_report_the_same_conflict(tmp_path):
    from numera.server import DuplicateToken, TokenStore

    tokens = TokenStore(tmp_path)
    made = tokens.create("CI runner")
    with pytest.raises(DuplicateToken):
        tokens.create("  CI runner  ")       # normalised, so it is the same name

    # The first named token locks the API, so the duplicate has to carry one.
    c = client(tmp_path)
    auth = {"Authorization": f"Bearer {made['token']}"}
    r = c.post("/api/tokens", json={"name": "CI runner"}, headers=auth)
    assert r.status_code == 409, r.text
    assert "already exists" in r.json()["detail"]
