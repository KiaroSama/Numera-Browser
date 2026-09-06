"""The repository decides, and it decides under the lock that writes.

Three defects, one shape between them: a decision made somewhere other than
where its consequence lands.

* **Delete reclaimed the wrong thing.** `detach_session()` renames
  `user-data/<id>` aside and hands back the tombstone; the delete then called
  `delete_session(pid)`, which detaches *again* - and after the first detach
  there is nothing at that path, so it found nothing, reclaimed nothing, and
  left the whole browser identity on disk as `.deleting-*`. Every successful
  delete leaked one. Its rollback had the same shape in reverse: a bare loop
  that stopped at the first failure, so the second record was never restored
  and the session never came back, and what reached the caller described
  neither the delete that failed nor the damage left behind.

* **The merge decided by truthiness.** `notes` and `tags` are modelled by
  `Profile`, but the merge preserved the stored value and let the engine's win
  only when it was truthy - so clearing either of them was impossible through
  any repo-backed path. An empty string is a value a user chose; truthiness
  cannot tell it from an argument nobody passed.

* **Bulk commands decided outside the transaction.** `apply` and `pool assign`
  selected profiles, mutated those snapshots, and handed the finished objects
  to `save_many`. The write was atomic; the decision was not. An API edit that
  committed in between was acknowledged and then overwritten.
"""

import json

import pytest

from numera import cli, generate
from numera.models import Profile, Proxy
from numera.paths import RollbackFailed
from numera.repo import repo
from numera.store import ProfileStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient        # noqa: E402

from numera.server import create_app             # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def dash_raw(tmp_path, pid):
    path = tmp_path / "dashboard" / f"{pid}.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


def made(name="p", **kw) -> Profile:
    return Profile(name=name, fingerprint=generate(seed=1), **kw)


def with_a_session(store: ProfileStore, prof: Profile) -> "object":
    """Give the profile a real session directory with real bytes in it."""
    ud = store.userdata_dir / prof.id
    (ud / "Default").mkdir(parents=True, exist_ok=True)
    (ud / "Default" / "Cookies").write_bytes(b"not really sqlite, but real bytes")
    return ud


def tombstones(store: ProfileStore) -> list:
    return sorted(p.name for p in store.userdata_dir.glob("*.deleting-*"))


# --- delete reclaims what it detached --------------------------------------

def test_a_successful_delete_leaves_no_tombstone(tmp_path):
    store = ProfileStore(tmp_path)
    prof = repo(store).create(made("doomed"))
    ud = with_a_session(store, prof)

    assert repo(store).delete(prof.id) is True
    assert not ud.exists()
    assert tombstones(store) == [], "the detached session was never reclaimed"


def test_a_tombstone_that_will_not_go_is_reported_not_swallowed(tmp_path, monkeypatch):
    """The profile is gone and its bytes are unreachable through it, so this is
    cleanup-pending rather than a failed delete - but the exact path has to
    reach someone, or nothing ever removes it."""
    store = ProfileStore(tmp_path)
    prof = repo(store).create(made("stubborn"))
    with_a_session(store, prof)
    monkeypatch.setattr(ProfileStore, "discard_tombstone",
                        staticmethod(lambda tomb: False))

    with pytest.warns(RuntimeWarning, match=r"\.deleting-"):
        assert repo(store).delete(prof.id) is True
    assert store.get(prof.id) is None, "a cleanup warning must not undo the delete"
    assert dash_raw(tmp_path, prof.id) is None


def test_a_failed_metadata_delete_puts_everything_back(tmp_path, monkeypatch):
    store = ProfileStore(tmp_path)
    prof = repo(store).create(made("kept"))
    ud = with_a_session(store, prof)
    engine_before = store._path(prof.id).read_bytes()
    dash_before = dash_raw(tmp_path, prof.id)

    real = type(store._path(prof.id)).unlink

    def second_unlink_fails(self, *a, **kw):
        if self.suffix == ".json" and self.parent.name == "profiles":
            raise OSError("no")
        return real(self, *a, **kw)

    monkeypatch.setattr(type(store._path(prof.id)), "unlink", second_unlink_fails)
    with pytest.raises(OSError):
        repo(store).delete(prof.id)
    monkeypatch.undo()

    assert store._path(prof.id).read_bytes() == engine_before
    assert dash_raw(tmp_path, prof.id) == dash_before
    assert ud.exists() and (ud / "Default" / "Cookies").exists(), \
        "the session was not restored"
    assert tombstones(store) == []


def test_a_rollback_that_also_fails_names_every_casualty(tmp_path, monkeypatch):
    """The one outcome a caller cannot act on is a half-undone delete reported
    as the original error. Both failures reach them, with the paths."""
    store = ProfileStore(tmp_path)
    prof = repo(store).create(made("wreck"))
    with_a_session(store, prof)

    path_type = type(store._path(prof.id))
    real_unlink = path_type.unlink

    def engine_unlink_fails(self, *a, **kw):
        if self.suffix == ".json" and self.parent.name == "profiles":
            raise OSError("the metadata delete failed")
        return real_unlink(self, *a, **kw)

    monkeypatch.setattr(path_type, "unlink", engine_unlink_fails)
    monkeypatch.setattr("numera.paths._restore_bytes",
                        lambda path, blob: (_ for _ in ()).throw(
                            OSError("and so did putting it back")))

    with pytest.raises(RollbackFailed) as caught:
        repo(store).delete(prof.id)
    monkeypatch.undo()

    assert "the metadata delete failed" in str(caught.value.cause)
    assert caught.value.failures, "a rollback failure with no casualties named"
    assert any("dashboard" in str(path) for path, _err in caught.value.failures)


# --- the merge is authoritative by field, not by value ---------------------

def test_a_cli_edit_can_clear_notes(tmp_path):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "noted", "notes": "remember me"}).json()["id"]
    store = ProfileStore(tmp_path)

    repo(store).edit(pid, lambda prof: setattr(prof, "notes", "") or prof)

    assert store.get(pid).notes == ""
    assert dash_raw(tmp_path, pid)["notes"] == "", \
        "the dashboard kept a note the engine cleared"


def test_removing_the_last_tag_reaches_both_copies(tmp_path):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "tagged", "tags": ["keep"]}).json()["id"]
    store = ProfileStore(tmp_path)

    repo(store).edit(pid, lambda prof: setattr(prof, "tags", []) or prof)

    assert store.get(pid).tags == []
    assert dash_raw(tmp_path, pid)["tags"] == [], \
        "the dashboard kept a tag the engine removed"


def test_clearing_urls_extensions_and_proxy_reaches_both_copies(tmp_path):
    store = ProfileStore(tmp_path)
    prof = repo(store).create(made(
        "full", startup_urls=["https://a.test"], extensions=[],
        proxy=Proxy(server="http://1.2.3.4:8080")))

    def strip(current):
        current.startup_urls = []
        current.proxy = None
        return current

    repo(store).edit(prof.id, strip)

    saved = store.get(prof.id)
    assert saved.startup_urls == [] and saved.proxy is None
    twin = dash_raw(tmp_path, prof.id)
    assert twin["startupUrls"] in ("", [], None), twin["startupUrls"]
    assert not twin.get("proxy"), "the dashboard kept a proxy the engine removed"


# --- the bulk decision happens inside the transaction ----------------------

def edit_during_selection(monkeypatch, c, pid, body):
    """Commit an API edit in the exact window between selection and write.

    `select()` is the last thing a bulk command does before it decides what to
    write, so an edit landing here is the one the old code read past. The
    interleaving is stated rather than raced: a sleep would make this a flaky
    test of the scheduler instead of a real test of the lock.
    """
    from numera import bulk

    real, fired = bulk.select, []

    def select_then_edit(*a, **kw):
        chosen = real(*a, **kw)
        if not fired:
            fired.append(True)
            current = {p["id"]: p for p in c.get("/api/profiles").json()}[pid]
            c.put(f"/api/profiles/{pid}", json={**current, **body})
        return chosen

    monkeypatch.setattr(bulk, "select", select_then_edit)
    return fired


def test_apply_does_not_overwrite_an_edit_it_never_saw(tmp_path, monkeypatch, capsys):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "shared", "tags": []}).json()["id"]
    fired = edit_during_selection(monkeypatch, c, pid, {"notes": "written by the API"})

    cli.main(["--data-dir", str(tmp_path), "apply", "--all", "--add-tag", "cli", "-y",
              "--json"])
    capsys.readouterr()
    assert fired, "the concurrent edit never ran - the test proves nothing"

    saved = ProfileStore(tmp_path).get(pid)
    assert "cli" in saved.tags, "the bulk change did not land"
    assert saved.notes == "written by the API", "the API edit was overwritten"
    assert dash_raw(tmp_path, pid)["notes"] == "written by the API"


def test_pool_assign_does_not_overwrite_an_edit_it_never_saw(tmp_path, monkeypatch, capsys):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "assigned"}).json()["id"]
    proxies = tmp_path / "proxies.txt"
    proxies.write_text("http://u:p@5.6.7.8:3128:US\n", encoding="utf-8")
    cli.main(["--data-dir", str(tmp_path), "pool", "add", str(proxies), "--json"])
    fired = edit_during_selection(monkeypatch, c, pid, {"notes": "still mine"})

    cli.main(["--data-dir", str(tmp_path), "pool", "assign", "--all", "--json"])
    capsys.readouterr()
    assert fired, "the concurrent edit never ran - the test proves nothing"

    saved = ProfileStore(tmp_path).get(pid)
    assert saved.proxy and "5.6.7.8" in saved.proxy.server, "the assignment did not land"
    assert saved.notes == "still mine", "the API edit was overwritten"


def test_a_failing_write_rolls_the_whole_patch_back(tmp_path, monkeypatch):
    """All-or-none survives the move inside the transaction."""
    store = ProfileStore(tmp_path)
    for name in ("one", "two", "three"):
        repo(store).create(made(name))
    before = {p.id: p.notes for p in store.list()}

    from numera import paths

    real, calls = paths.write_json, {"n": 0}

    def fail_late(path, data):
        calls["n"] += 1
        if calls["n"] == 4:
            raise OSError("no space left on device")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_late)
    with pytest.raises(OSError):
        repo(store).patch_many([p.id for p in store.list()],
                               lambda prof: setattr(prof, "notes", "bulk") or True)
    monkeypatch.undo()

    assert {p.id: p.notes for p in ProfileStore(tmp_path).list()} == before


def test_patch_many_skips_a_profile_that_is_already_gone(tmp_path):
    store = ProfileStore(tmp_path)
    kept = repo(store).create(made("kept"))
    written = repo(store).patch_many(
        [kept.id, "nosuchprofile00"],
        lambda prof: setattr(prof, "notes", "here") or True)
    assert [p.id for p in written] == [kept.id]


# --- the target and the answer are decided under the lock ------------------

def test_a_delete_committing_before_the_lock_is_a_clean_miss(tmp_path, monkeypatch):
    """`get()` checked `exists()` outside the transaction it then took.

    A delete committing in that gap turned a question whose honest answer is
    "no such profile" into a `FileNotFoundError` at a caller that had no reason
    to expect one. The delete here lands in exactly that gap.
    """
    from numera import store as store_mod

    store = ProfileStore(tmp_path)
    prof = repo(store).create(made("doomed"))
    # Patched where `get` looks it up: `store.py` imports the name, so the call
    # resolves in its own module globals and never through `paths`.
    real, fired = store_mod.transaction_lock, []

    def deleting_lock(root):
        if not fired:
            fired.append(True)
            (tmp_path / "profiles" / f"{prof.id}.json").unlink()
        return real(root)

    monkeypatch.setattr(store_mod, "transaction_lock", deleting_lock)
    assert ProfileStore(tmp_path).get(prof.id) is None
    assert fired, "the delete never ran - the test proves nothing"


def test_a_rename_before_the_lease_refuses_the_delete(tmp_path, monkeypatch):
    """`delete("alpha")` resolved a profile, took its lease, and deleted it by
    id - so a rename committing in between let both operations report success
    with no serial order that explains them."""
    from numera import paths
    from numera.repo import TargetMoved

    store = ProfileStore(tmp_path)
    prof = repo(store).create(made("alpha"))
    real, fired = paths.session_lease, []

    def racing(root, profile_id):
        if not fired:
            fired.append(True)
            repo(ProfileStore(tmp_path)).edit(
                profile_id, lambda p: setattr(p, "name", "beta") or p)
            repo(ProfileStore(tmp_path)).create(made("alpha"))   # a new holder
        return real(root, profile_id)

    monkeypatch.setattr(paths, "session_lease", racing)
    with pytest.raises(TargetMoved):
        repo(ProfileStore(tmp_path)).delete("alpha")
    assert fired, "the rename never ran - the test proves nothing"
    assert ProfileStore(tmp_path).get(prof.id) is not None,         "the renamed profile was deleted by a command that named its old name"


def test_a_delete_by_id_after_a_rename_still_works(tmp_path, monkeypatch):
    """Revalidation must not make an ordinary delete fail: an id means the same
    profile whatever it is called."""
    from numera import paths

    store = ProfileStore(tmp_path)
    prof = repo(store).create(made("alpha"))
    real, fired = paths.session_lease, []

    def racing(root, profile_id):
        if not fired:
            fired.append(True)
            repo(ProfileStore(tmp_path)).edit(
                profile_id, lambda p: setattr(p, "name", "beta") or p)
        return real(root, profile_id)

    monkeypatch.setattr(paths, "session_lease", racing)
    assert repo(ProfileStore(tmp_path)).delete(prof.id) is True
    assert fired
