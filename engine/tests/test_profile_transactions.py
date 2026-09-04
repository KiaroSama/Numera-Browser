"""A profile mutation either lands whole or leaves nothing behind.

Two failures with the same root. First, ordering: the profile files were made
transactional but `config.json` was not, and it was written *first* - so a
storage failure rolled the profiles back and left the default engine set by a
request that had failed. Second, locking: a batch snapshots each file, writes
them all, and on failure restores the old bytes; a single save landing in that
window was overwritten by the rollback, which is data loss caused by an
unrelated request failing.

Everything here injects a real failure or runs a real second thread. A
transaction that is only tested on the happy path is not tested.
"""
import json
import threading
import time

import pytest

from numera import folders, generate, paths
from numera.models import Profile
from numera.store import ProfileStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient       # noqa: E402

from numera.server import create_app            # noqa: E402
from numera.server.stores import DashboardStore  # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def make(c, name, **extra):
    r = c.post("/api/profiles", json={"name": name, **extra})
    assert r.status_code < 400, r.text
    return r.json()["id"]


def one(c, pid):
    """One profile out of the list route.

    There is no single-profile GET, so reading one that way returned a 404 body
    and made every before/after comparison compare two identical error objects.
    """
    for prof in c.get("/api/profiles").json():
        if prof["id"] == pid:
            return prof
    return None


def default_engine(tmp_path):
    cfg = tmp_path / "config.json"
    if not cfg.is_file():
        return None
    return json.loads(cfg.read_text(encoding="utf-8")).get("default_engine")


# --- 3a. the config change commits only after the data does -----------------

def test_a_failing_bulk_leaves_neither_profiles_nor_the_default_engine_changed(
        tmp_path, monkeypatch):
    """`_remember_engine` ran inside the preparation loop.

    `config.json` is not part of the profile batch, so when the batch failed the
    profiles came back and the new default engine stayed - chosen by a request
    that returned 500.
    """
    c = client(tmp_path)
    ids = [make(c, "a"), make(c, "b"), make(c, "c")]
    before_engine = default_engine(tmp_path)
    before = {i: one(c, i) for i in ids}

    real = paths.write_json
    calls = {"n": 0}

    def fail_on_the_third_write(path, data):
        calls["n"] += 1
        if calls["n"] == 3:
            raise OSError("no space left on device")
        return real(path, data)

    monkeypatch.setattr(paths, "write_json", fail_on_the_third_write)
    monkeypatch.setattr("numera.server.stores.write_json", fail_on_the_third_write)

    with pytest.raises(OSError):
        c.post("/api/profiles/bulk",
               json={"ids": ids, "patch": {"engine": "camoufox", "notes": "x"}})

    monkeypatch.undo()
    assert default_engine(tmp_path) == before_engine, \
        "the default engine changed although the write failed"
    for i in ids:
        assert one(c, i) == before[i], \
            f"profile {i} kept part of a failed bulk edit"


# --- 3b. bulk validates exactly as a single update does ---------------------

def test_bulk_refuses_an_unknown_engine_and_changes_nothing(tmp_path):
    """`check_fields` only asks whether `engine` is a string. "bogus" is."""
    c = client(tmp_path)
    ids = [make(c, "a"), make(c, "b")]
    before_engine = default_engine(tmp_path)
    before = {i: one(c, i) for i in ids}

    r = c.post("/api/profiles/bulk", json={"ids": ids, "patch": {"engine": "bogus"}})

    assert 400 <= r.status_code < 500, f"an unknown engine was accepted: {r.status_code}"
    assert default_engine(tmp_path) == before_engine, "the default engine was poisoned"
    for i in ids:
        assert one(c, i) == before[i]


def test_bulk_still_applies_a_known_engine(tmp_path):
    """The guard must not refuse the valid case."""
    c = client(tmp_path)
    ids = [make(c, "a")]

    r = c.post("/api/profiles/bulk", json={"ids": ids, "patch": {"engine": "camoufox"}})

    assert r.status_code < 400, r.text
    assert one(c, ids[0])["engine"] == "camoufox"
    assert default_engine(tmp_path) == "camoufox", "an explicit choice was not remembered"


# --- 3c. an explicitly named engine is imported, not only detected ----------

def test_an_export_naming_an_engine_produces_a_profile_using_it(tmp_path):
    """The export was recognised as explicit and then thrown away: the profile
    fell back to Playwright while the *default* was set to what the export
    said."""
    c = client(tmp_path)
    export = {"name": "imported", "engine": "camoufox", "os": "win"}

    r = c.post("/api/profiles/import", json={"text": json.dumps(export)})

    assert r.status_code < 400, r.text
    made = [p for p in c.get("/api/profiles").json() if p["name"] == "imported"]
    assert made, "nothing was imported"
    assert made[0]["engine"] == "camoufox", \
        f"the export named camoufox but the profile uses {made[0]['engine']!r}"


def test_an_export_naming_something_unsupported_is_not_an_explicit_choice(tmp_path):
    """A vendor export names a *browser*. That is not one of our engines and
    must not move the default."""
    c = client(tmp_path)
    before = default_engine(tmp_path)

    r = c.post("/api/profiles/import",
               json={"text": json.dumps({"name": "vendor", "browserEngine": "Chrome"})})

    assert r.status_code < 400, r.text
    assert default_engine(tmp_path) == before, \
        "a vendor browser name was treated as an engine choice"


# --- 4. concurrency: a rollback may not undo somebody else's success --------

def test_a_failing_batch_rollback_does_not_erase_a_concurrent_save(tmp_path):
    """The batch holds the old bytes of every file it wrote.

    Without one lock covering snapshot-through-rollback, a `save()` that
    succeeded in that window was overwritten by the rollback of a transaction
    it had nothing to do with.
    """
    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine)
    victim = dash.save({"name": "victim", "notes": "original"})
    other = dash.save({"name": "other", "notes": "original"})

    started, done = threading.Event(), threading.Event()
    real = paths.write_json

    def slow_then_fail(path, data):
        # Fail on the *second* file so the first is already snapshotted and
        # written - the exact window the rollback has to cover.
        if str(path).endswith(f"{other['id']}.json"):
            started.set()
            done.wait(5)
            raise OSError("no space left on device")
        return real(path, data)

    import numera.server.stores as stores_mod
    stores_mod.write_json = slow_then_fail
    paths.write_json = slow_then_fail

    def concurrent_edit():
        started.wait(5)
        stores_mod.write_json = real          # this one must actually land
        paths.write_json = real
        dash.save({**victim, "notes": "EDITED-BY-THE-OTHER-REQUEST"})
        done.set()

    editor = threading.Thread(target=concurrent_edit, daemon=True)
    editor.start()
    try:
        with pytest.raises(OSError):
            dash.save_many([{**victim, "notes": "batch"}, {**other, "notes": "batch"}])
    finally:
        done.set()
        editor.join(10)
        stores_mod.write_json = real
        paths.write_json = real

    assert not editor.is_alive(), "the concurrent save never finished"
    survived = dash.get(victim["id"])["notes"]
    assert survived == "EDITED-BY-THE-OTHER-REQUEST", \
        f"the rollback erased a successful concurrent save (notes={survived!r})"


def test_a_folder_rename_and_a_new_profile_in_it_cannot_split(tmp_path):
    """The folder transaction used to enumerate profile files and then lock
    them, so a profile created after the snapshot belonged to the folder being
    renamed and was held by nothing."""
    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine)
    folders.create(tmp_path, "source")
    dash.save({"name": "existing", "folder": "source"})

    made = []

    def add_one():
        made.append(dash.save({"name": "late", "folder": "source"}))

    writer = threading.Thread(target=add_one, daemon=True)
    writer.start()
    folders.rename(tmp_path, "source", "dest")
    writer.join(10)
    assert not writer.is_alive() and made, "the concurrent create never finished"

    names = folders.list_folders(tmp_path) if hasattr(folders, "list_folders") \
        else folders._read_meta(tmp_path).get("folders", [])
    assert "source" not in names, "the old folder name survived the rename"
    for prof in dash.list():
        assert prof.get("folder") in (None, "", "dest"), \
            f"{prof['name']} was left pointing at {prof.get('folder')!r}"


def test_delete_racing_a_save_leaves_one_coherent_state(tmp_path):
    """Never a split state: dashboard copy gone but the engine twin behind, or
    the reverse. Either the profile is fully there or fully gone."""
    engine = ProfileStore(tmp_path)
    dash = DashboardStore(tmp_path, engine)
    prof = dash.save({"name": "contested", "notes": "v1"})
    pid = prof["id"]

    errors = []

    def saver():
        try:
            dash.save({**prof, "notes": "v2"})
        except Exception as e:                                  # noqa: BLE001
            errors.append(e)

    t = threading.Thread(target=saver, daemon=True)
    t.start()
    dash.delete(pid)
    t.join(10)
    assert not t.is_alive(), "the concurrent save never finished"

    in_dash = dash.get(pid) is not None
    in_engine = engine._path(pid).exists()
    assert in_dash == in_engine, (
        f"split state: dashboard={in_dash}, engine={in_engine} - one copy of the "
        f"profile outlived the other")
