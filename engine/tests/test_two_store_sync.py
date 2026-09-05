"""One profile, two copies, and neither is allowed to go stale.

Numera stores every profile twice: `profiles/<id>.json` is what a launch needs,
`dashboard/<id>.json` is the richer shape the GUI edits. Dashboard writes always
mirrored into the engine - but the engine's copy was adopted by the dashboard
only when its id was *unknown*. For an id the dashboard already had:

* `numera edit`, `regen` and `apply` rewrote the engine record and left the
  dashboard's untouched, so the GUI showed values the launcher no longer used;
* `numera delete` removed the engine record and left the dashboard copy, which
  the next `GET /api/profiles` adopted straight back - a deleted profile
  reappearing under its own id.

The second defect was the read-modify-write itself. `resolve()` -> mutate ->
`save()` spans the whole command, so a concurrent edit is acknowledged and then
erased. `ProfileRepo.edit()` runs the mutation inside the transaction instead.
"""
import json

import pytest

from _rendezvous import Rendezvous, finish, start

from numera import cli, generate
from numera.models import Profile
from numera.repo import NameTaken, repo
from numera.store import ProfileStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient        # noqa: E402

from numera.server import create_app             # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def dash_raw(tmp_path, pid):
    path = tmp_path / "dashboard" / f"{pid}.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


# --- the engine -> dashboard direction --------------------------------------

def test_a_cli_edit_reaches_the_dashboard_copy(tmp_path):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "shared", "notes": ""}).json()["id"]

    store = ProfileStore(tmp_path)
    assert cli.main(["--data-dir", str(tmp_path), "edit", pid, "--notes", "from the cli"]) == 0

    assert dash_raw(tmp_path, pid)["notes"] == "from the cli"
    assert c.get("/api/profiles").json()[0]["notes"] == "from the cli"
    assert store.get(pid).notes == "from the cli"


def test_a_cli_regen_reaches_the_dashboard_copy(tmp_path):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "regen"}).json()["id"]
    before = dash_raw(tmp_path, pid)["userAgent"]

    assert cli.main(["--data-dir", str(tmp_path), "regen", pid, "--os", "linux"]) == 0

    after = dash_raw(tmp_path, pid)
    assert after["userAgent"] != before
    assert "Linux" in after["userAgent"]
    assert after["os"] == "Linux"


def test_a_cli_delete_removes_both_copies(tmp_path):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "gone"}).json()["id"]
    assert dash_raw(tmp_path, pid) is not None

    assert cli.main(["--data-dir", str(tmp_path), "delete", pid, "--yes"]) == 0

    assert dash_raw(tmp_path, pid) is None
    # The listing adopts engine-only profiles, so a surviving dashboard copy
    # would have come straight back here.
    assert c.get("/api/profiles").json() == []


def test_a_cli_edit_keeps_dashboard_only_fields(tmp_path):
    """The engine has no `folder`; a naive round trip through the converter
    would drop it, which is what made two-way sync unsafe to add."""
    c = client(tmp_path)
    pid = c.post("/api/profiles",
                 json={"name": "kept", "folder": "Ads", "tags": ["x"]}).json()["id"]

    assert cli.main(["--data-dir", str(tmp_path), "edit", pid, "--notes", "n"]) == 0

    after = dash_raw(tmp_path, pid)
    assert after["folder"] == "Ads"
    assert after["notes"] == "n"


def test_an_api_edit_is_visible_to_the_cli(tmp_path, capsys):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "api-side"}).json()["id"]
    c.put(f"/api/profiles/{pid}", json={"name": "api-side", "notes": "typed in the ui"})

    assert cli.main(["--data-dir", str(tmp_path), "show", pid, "--json"]) == 0
    assert "typed in the ui" in capsys.readouterr().out


# --- uniqueness is decided inside the transaction ----------------------------

def test_two_simultaneous_same_name_creates_have_one_winner(tmp_path):
    body = """
from numera import generate
from numera.models import Profile
from numera.repo import NameTaken, repo
from numera.store import ProfileStore
store = ProfileStore(ROOT)
coordinator = repo(store)
prof = Profile(name="twin", fingerprint=generate(seed=1))
wait()
try:
    coordinator.create(prof)
    print("CREATED")
except NameTaken:
    print("REFUSED")
say("done")
"""
    bar = Rendezvous()
    procs = {}
    try:
        for who in ("first", "second"):
            procs[who] = start(body, tmp_path, who, bar.port)
        for who in procs:
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
    made = [w for w, r in results.items() if r["out"] == "CREATED"]
    assert len(made) == 1, results
    assert len(ProfileStore(tmp_path).list()) == 1


def test_the_repo_refuses_a_duplicate_name(tmp_path):
    store = ProfileStore(tmp_path)
    coordinator = repo(store)
    coordinator.create(Profile(name="one", fingerprint=generate(seed=1)))
    with pytest.raises(NameTaken):
        coordinator.create(Profile(name="one", fingerprint=generate(seed=2)))


# --- restore and import move both copies ------------------------------------

def test_a_restore_over_an_existing_id_replaces_both_copies(tmp_path):
    """`restore` wrote only the engine record, so restoring onto an id the
    dashboard already had left the two describing different profiles - one
    identity for the launcher, the one it replaced for the GUI."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "original"}).json()["id"]
    c.put(f"/api/profiles/{pid}", json={**c.get("/api/profiles").json()[0],
                                        "folder": "clients", "notes": "keep me"})

    store = ProfileStore(tmp_path)
    archive = tmp_path / "backup.zip"
    store.backup(pid, archive)
    repo(store).edit(pid, lambda prof: setattr(prof, "name", "changed") or prof)

    store.restore(archive)                       # same id, the archived identity

    assert ProfileStore(tmp_path).get(pid).name == "original"
    twin = dash_raw(tmp_path, pid)
    assert twin["name"] == "original", "the dashboard kept the identity it replaced"
    assert twin["folder"] == "clients", "a dashboard-only field was dropped"
    assert twin["notes"] == "keep me"


def test_a_failed_restore_commit_leaves_both_copies_alone(tmp_path, monkeypatch):
    """A rollback that put back only the engine record would leave the twin
    describing the archive that never landed."""
    from numera import paths

    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "original"}).json()["id"]
    store = ProfileStore(tmp_path)
    archive = tmp_path / "backup.zip"
    store.backup(pid, archive)
    repo(store).edit(pid, lambda prof: setattr(prof, "name", "current") or prof)
    before = dash_raw(tmp_path, pid)

    real = paths.write_json
    monkeypatch.setattr(paths, "write_json",
                        lambda path, data: (_ for _ in ()).throw(
                            OSError("no space left on device")))
    with pytest.raises(OSError):
        ProfileStore(tmp_path).restore(archive)
    monkeypatch.setattr(paths, "write_json", real)

    assert ProfileStore(tmp_path).get(pid).name == "current"
    assert dash_raw(tmp_path, pid) == before, "the twin moved on a failed restore"


def test_an_import_of_an_existing_id_updates_both_copies(tmp_path):
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "before"}).json()["id"]
    store = ProfileStore(tmp_path)
    exported = tmp_path / "p.json"
    store.export(pid, exported)
    repo(store).edit(pid, lambda prof: setattr(prof, "name", "after") or prof)

    store.import_file(exported)                  # the same id comes back

    assert ProfileStore(tmp_path).get(pid).name == "before"
    assert dash_raw(tmp_path, pid)["name"] == "before"


# --- concurrent edits from the two surfaces ---------------------------------

def test_a_cli_note_and_an_api_tag_edit_both_survive(tmp_path):
    """The CLI mutation runs inside the transaction, so the API edit released
    into the middle of it lands either side - never on top of it."""
    c = client(tmp_path)
    pid = c.post("/api/profiles", json={"name": "both", "notes": "", "tags": []}).json()["id"]

    holder = """
from numera.repo import repo
from numera.store import ProfileStore

def edit(prof):
    say("inside")
    wait()
    prof.notes = "from the cli"
    return prof

repo(ProfileStore(ROOT)).edit({pid!r}, edit)
say("done")
""".format(pid=pid)
    writer = """
from numera.store import ProfileStore
from numera.server.stores import DashboardStore
dash = DashboardStore(ROOT, ProfileStore(ROOT))
wait()
say("trying")
dash.patch({pid!r}, lambda cur: ({{**cur, "tags": ["from-the-api"]}}, None))
say("done")
""".format(pid=pid)

    bar = Rendezvous()
    procs = {}
    try:
        procs["holder"] = start(holder, tmp_path, "holder", bar.port)
        bar.expect("holder", "inside")
        procs["writer"] = start(writer, tmp_path, "writer", bar.port)
        bar.release("writer")
        bar.expect("writer", "trying")
        bar.release("holder")
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results

    final = dash_raw(tmp_path, pid)
    assert final["notes"] == "from the cli", final
    assert final["tags"] == ["from-the-api"], final
