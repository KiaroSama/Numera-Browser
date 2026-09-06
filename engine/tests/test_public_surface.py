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
    """`save_engine` is what `DashboardStore._mirror` needs: it is already
    writing the dashboard record, and going through the coordinator from there
    would have it rewrite the copy it was called from."""
    store = ProfileStore(tmp_path)
    prof = store.save_engine(made("engine-only"))
    assert engine_raw(tmp_path, prof.id) is not None
    assert dash_raw(tmp_path, prof.id) is None


# --- a legacy duplicate name is refused, not guessed -----------------------

def legacy_twins(tmp_path, name="same"):
    """Two ids under one name, the way an older version would have left them."""
    store = ProfileStore(tmp_path)
    first = store.save(made(name))
    second = store.save_engine(made(name))      # no coordinator, no name check
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
