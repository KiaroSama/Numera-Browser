"""What an outside script gets, and what an upgraded data root does.

Two defects with one shape: the invariants Numera enforces internally stopped
at the edge of the code that enforces them.

* **The advertised Python API bypassed the coordinator.** `numera.__doc__`
  tells a caller to `store.save(Profile(...))`, and that wrote
  `profiles/<id>.json` and nothing else - so a profile created by a script was
  engine-only until a dashboard listing happened to adopt it, an *update* to an
  id the dashboard already knew never reached the GUI (the listing does not
  re-adopt a known id), and a delete left the twin behind so the GUI kept
  showing a profile the script had been told was gone. Every other surface -
  CLI, HTTP - had been routed through `ProfileRepo`; the one documented as the
  public API had not.

* **Name uniqueness protected only new writes.** 0.22 refuses to *create* a
  duplicate, which does nothing for a data root that already has one: an older
  Numera, a hand edit, a restored backup. `find_by_name` returned the first
  match in directory order, so `numera delete <name>` destroyed an arbitrary
  one of two profiles and reported success.

The imports below are deliberately the public ones, exactly as an external
script would write them.
"""

import json

import pytest

from numera import Profile, ProfileStore, cli, generate
from numera.store import AmbiguousProfile


def engine_raw(root, pid):
    path = root / "profiles" / f"{pid}.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


def dash_raw(root, pid):
    path = root / "dashboard" / f"{pid}.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else None


def made(name="acct-01", **kw) -> Profile:
    return Profile(name=name, fingerprint=generate(seed=1), **kw)


# --- the public mutators keep both copies ----------------------------------

def test_the_quick_start_example_writes_both_copies(tmp_path):
    """`numera.__doc__` is a contract: what it shows has to be enough."""
    store = ProfileStore(tmp_path)
    prof = store.save(made())

    assert engine_raw(tmp_path, prof.id) is not None
    twin = dash_raw(tmp_path, prof.id)
    assert twin is not None, "a script-created profile was engine-only"
    assert twin["name"] == "acct-01"


def test_updating_an_id_the_dashboard_knows_reaches_it(tmp_path):
    """The case adoption cannot rescue: `GET /api/profiles` adopts an *unknown*
    id, so an engine-only update to a known one is invisible forever."""
    store = ProfileStore(tmp_path)
    prof = store.save(made(notes="before"))
    assert dash_raw(tmp_path, prof.id)["notes"] == "before"

    prof.notes = "after"
    store.save(prof)

    assert engine_raw(tmp_path, prof.id)["notes"] == "after"
    assert dash_raw(tmp_path, prof.id)["notes"] == "after", \
        "the dashboard kept a note the script changed"


def test_delete_removes_both_copies(tmp_path):
    store = ProfileStore(tmp_path)
    prof = store.save(made())

    assert store.delete(prof.id) is True
    assert engine_raw(tmp_path, prof.id) is None
    assert dash_raw(tmp_path, prof.id) is None, \
        "the GUI would still show a profile the script deleted"


def test_the_public_api_shares_the_name_contract(tmp_path):
    """One name, one profile - on every surface, including this one."""
    from numera.repo import NameTaken

    store = ProfileStore(tmp_path)
    store.save(made("taken"))
    with pytest.raises(NameTaken):
        store.save(made("taken"))


def test_the_engine_only_primitive_is_still_there_for_the_mirror(tmp_path):
    """`_save_engine` is what `DashboardStore._mirror` needs: it is already
    writing the dashboard record, and going through the coordinator from there
    would have it rewrite the copy it was called from."""
    store = ProfileStore(tmp_path)
    prof = store._save_engine(made("engine-only"))
    assert engine_raw(tmp_path, prof.id) is not None
    assert dash_raw(tmp_path, prof.id) is None


# --- a legacy duplicate name is refused, not guessed -----------------------

def legacy_twins(tmp_path, name="same"):
    """Two ids under one name, the way an older version would have left them."""
    store = ProfileStore(tmp_path)
    first = store.save(made(name))
    second = store._save_engine(made(name))      # no coordinator, no name check
    return store, first, second


def test_a_duplicate_name_refuses_instead_of_choosing(tmp_path):
    store, first, second = legacy_twins(tmp_path)
    with pytest.raises(AmbiguousProfile) as caught:
        store.resolve("same")
    assert sorted(caught.value.ids) == sorted([first.id, second.id])
    assert first.id in str(caught.value) and second.id in str(caught.value), \
        "the error has to name the ids, or it cannot be acted on"


def test_an_id_still_resolves_in_an_ambiguous_store(tmp_path):
    """The way out of the ambiguity has to keep working, or the store is stuck."""
    store, first, second = legacy_twins(tmp_path)
    assert store.resolve(first.id).id == first.id
    assert store.resolve(second.id).id == second.id


@pytest.mark.parametrize("argv", [
    ["show", "same"],
    ["edit", "same", "--notes", "hello"],
    ["delete", "same", "-y"],
    ["regen", "same"],
])
def test_destructive_commands_by_name_refuse_and_change_nothing(tmp_path, argv, capsys):
    store, first, second = legacy_twins(tmp_path)
    before = (engine_raw(tmp_path, first.id), engine_raw(tmp_path, second.id))

    code = cli.main(["--data-dir", str(tmp_path), *argv, "--json"])
    out = capsys.readouterr().out

    assert code == 1, f"{argv[0]} did not refuse"
    assert first.id in out and second.id in out, "the ids were not reported"
    assert (engine_raw(tmp_path, first.id), engine_raw(tmp_path, second.id)) == before, \
        f"{argv[0]} changed something despite refusing"


def test_a_listing_still_works_in_an_ambiguous_store(tmp_path):
    """Discovering the problem must not make the store unusable: listing is how
    a user sees both ids in order to fix it."""
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    store, first, second = legacy_twins(tmp_path)
    assert len(store.list()) == 2

    with TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False)) as c:
        rows = c.get("/api/profiles")
        assert rows.status_code == 200, rows.text
        assert {first.id, second.id} <= {r["id"] for r in rows.json()}


# --- the CLI deletes what was named, and says what it did ------------------

def test_the_cli_refuses_a_delete_whose_target_moved(tmp_path, monkeypatch, capsys):
    """`delete alpha` resolved A and then deleted `A` by id, which threw away
    the coordinator's own protection: by the time the lease was taken, `alpha`
    could name a different profile and A could be called something else."""
    from numera import paths
    from numera.repo import repo

    store = ProfileStore(tmp_path)
    a = store.save(made("alpha"))
    real, fired = paths.session_lease, []

    def racing(root, profile_id):
        if not fired:
            fired.append(True)
            fresh = ProfileStore(tmp_path)
            repo(fresh).edit(profile_id, lambda p: setattr(p, "name", "beta") or p)
            fresh.save(made("alpha"))          # someone else takes the name
        return real(root, profile_id)

    monkeypatch.setattr(paths, "session_lease", racing)
    code = cli.main(["--data-dir", str(tmp_path), "delete", "alpha", "-y", "--json"])
    out = capsys.readouterr().out
    monkeypatch.undo()

    assert fired, "the rename never ran - the test proves nothing"
    assert code == 1, "a moved target was reported as deleted"
    assert engine_raw(tmp_path, a.id) is not None, "the renamed profile was deleted"
    assert "meant profile" in out or "moved" in out.lower(), out


def test_the_cli_deletes_one_profile_named_twice_once(tmp_path, capsys):
    store = ProfileStore(tmp_path)
    prof = store.save(made("dup"))

    code = cli.main(["--data-dir", str(tmp_path), "delete", "dup", prof.id,
                     "-y", "--json"])
    out = capsys.readouterr().out

    assert code == 0, out
    assert out.count(prof.id) <= 2, "the same profile was deleted twice"
    assert engine_raw(tmp_path, prof.id) is None


def test_a_delete_that_found_nothing_is_not_reported_as_one(tmp_path, monkeypatch, capsys):
    """`coordinator.delete()` answers False when the profile had already gone.
    Ignoring that printed "Deleted" for a deletion this command did not do."""
    from numera.repo import ProfileRepo

    store = ProfileStore(tmp_path)
    prof = store.save(made("ghost"))
    monkeypatch.setattr(ProfileRepo, "delete", lambda self, ref: False)

    code = cli.main(["--data-dir", str(tmp_path), "delete", "ghost", "-y", "--json"])
    out = capsys.readouterr().out
    monkeypatch.undo()

    assert code == 1, "a delete that removed nothing reported success"
    assert '"removed": []' in out.replace(" ", "") or '"removed":[]' in out.replace(" ", ""), out
    assert engine_raw(tmp_path, prof.id) is not None


def test_a_detached_command_reports_a_bad_reference_as_json(tmp_path, capsys):
    """`--detach` resolved its target outside the CLI's error handler, so the
    foreground command printed a clean JSON error and the detached one printed
    a traceback and spawned nothing anyone could ask about."""
    from numera import jobs

    legacy_twins(tmp_path)
    code = cli.main(["--data-dir", str(tmp_path), "show", "same",
                     "--detach", "--json"])
    out = capsys.readouterr().out

    assert code == 1
    assert "Traceback" not in out, out
    payload = json.loads(out.strip().splitlines()[-1])
    assert payload["ok"] is False and "same" in payload["error"]
    assert jobs.listing(tmp_path) == [], "a job was created for a reference that named nothing"


def test_a_dashboard_only_twin_makes_a_name_ambiguous(tmp_path):
    """The name invariant is cross-store, and the *read* was not.

    `list()` reads `profiles/` alone, so an engine profile called `same` beside
    a dashboard-only one called `same` looked unambiguous - and a name-based
    delete went ahead while the GUI went on showing the other. The write-time
    check has scanned both directories since 0.22; this is the read catching up.
    """
    store = ProfileStore(tmp_path)
    engine_side = store.save(made("same"))
    # A dashboard-only record, the way an older version could leave one.
    (tmp_path / "dashboard").mkdir(exist_ok=True)
    (tmp_path / "dashboard" / "legacy00legacy.json").write_text(
        json.dumps({"id": "legacy00legacy", "name": "same", "os": "Windows"}),
        encoding="utf-8")

    with pytest.raises(AmbiguousProfile) as caught:
        store.resolve("same")
    assert sorted(caught.value.ids) == sorted([engine_side.id, "legacy00legacy"])


def test_a_dashboard_only_profile_is_repaired_rather_than_denied(tmp_path):
    """Answering "no such profile" for an id the GUI is showing would be this
    store disagreeing with the one the user is looking at. The two-copy model
    says repair it."""
    store = ProfileStore(tmp_path)
    (tmp_path / "dashboard").mkdir(exist_ok=True)
    (tmp_path / "dashboard" / "orphan00orphan.json").write_text(
        json.dumps({"id": "orphan00orphan", "name": "orphan", "os": "Windows"}),
        encoding="utf-8")

    found = store.resolve("orphan00orphan")
    assert found is not None and found.id == "orphan00orphan"
    assert engine_raw(tmp_path, "orphan00orphan") is not None,         "the missing engine copy was not written"
    assert store.resolve("orphan") is not None, "and it resolves by name now"
