"""One name, one profile - on every surface, not only the CLI's.

`ProfileRepo.create/save_many` have refused a duplicate name since 0.21.0, so
`numera create` and `numera bulk-create` honour the contract. Every HTTP write
went through `DashboardStore` instead and none of them did: single create,
batch create, `PUT` rename, bulk update with a name in the patch, and the
vendor import could each produce a second profile under a name that was already
taken. `restore --new-id` could too, and the suite even documented that as
allowed.

The consequence is not cosmetic. `ProfileStore.resolve(ref)` falls back to
`find_by_name()`, which returns the *first* match, and `numera delete <name>`
acts on what it returns - so a duplicate makes a destructive command destroy an
arbitrary one of two profiles and say nothing about the choice.

These tests drive the real routes and the real CLI. The check they exercise
lives in one place, inside the transaction that writes, which is why a route
added tomorrow inherits it.
"""
import json
import zipfile

import pytest

from numera import cli, generate
from numera.models import Profile
from numera.repo import repo
from numera.store import ProfileStore

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient        # noqa: E402

from numera.server import create_app             # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def names(root) -> list:
    return sorted(p.name for p in ProfileStore(root).list())


def dash_names(root) -> list:
    return sorted(json.loads(f.read_text(encoding="utf-8"))["name"]
                  for f in (root / "dashboard").glob("*.json"))


# --- the five HTTP write paths ---------------------------------------------

def test_a_second_create_with_a_taken_name_is_refused(tmp_path):
    c = client(tmp_path)
    first = c.post("/api/profiles", json={"name": "alpha"})
    assert first.status_code < 400, first.text

    second = c.post("/api/profiles", json={"name": "alpha"})

    assert second.status_code == 409, second.text
    assert "alpha" in second.json()["detail"]
    assert names(tmp_path) == ["alpha"], "the API created a duplicate name"
    assert dash_names(tmp_path) == ["alpha"], "the dashboard copy was written anyway"


def test_two_entries_of_one_batch_cannot_share_a_name(tmp_path):
    """Nothing on disk is needed for the ambiguity - two entries of one request
    produce it between themselves."""
    c = client(tmp_path)

    r = c.post("/api/profiles/batch",
               json={"profiles": [{"name": "twin"}, {"name": "twin"}]})

    assert r.status_code == 409, r.text
    assert names(tmp_path) == [], "a refused batch wrote part of itself"


def test_a_batch_cannot_claim_a_name_the_store_already_holds(tmp_path):
    c = client(tmp_path)
    c.post("/api/profiles", json={"name": "taken"})

    r = c.post("/api/profiles/batch",
               json={"profiles": [{"name": "fresh"}, {"name": "taken"}]})

    assert r.status_code == 409, r.text
    assert names(tmp_path) == ["taken"], "the entries before the clash were kept"


def test_renaming_a_profile_onto_a_taken_name_is_refused(tmp_path):
    c = client(tmp_path)
    c.post("/api/profiles", json={"name": "alpha"})
    other = c.post("/api/profiles", json={"name": "beta"}).json()

    r = c.put(f"/api/profiles/{other['id']}", json={**other, "name": "alpha"})

    assert r.status_code == 409, r.text
    assert names(tmp_path) == ["alpha", "beta"], "the rename landed anyway"


def test_a_profile_may_still_be_saved_under_the_name_it_already_has(tmp_path):
    """The check is on a *claim*, not on the name appearing twice in one
    request and one file. Every dashboard edit posts the whole profile back,
    name included, so treating that as a claim would refuse every save."""
    c = client(tmp_path)
    made = c.post("/api/profiles", json={"name": "alpha"}).json()

    r = c.put(f"/api/profiles/{made['id']}", json={**made, "notes": "edited"})

    assert r.status_code < 400, r.text
    assert r.json()["notes"] == "edited"


def test_a_bulk_rename_of_many_profiles_onto_one_name_is_refused(tmp_path):
    """The fastest way to make a dozen duplicates: select them all and put one
    name in the patch."""
    c = client(tmp_path)
    ids = [c.post("/api/profiles", json={"name": n}).json()["id"]
           for n in ("one", "two", "three")]

    r = c.post("/api/profiles/bulk", json={"ids": ids, "patch": {"name": "merged"}})

    assert r.status_code == 409, r.text
    assert names(tmp_path) == ["one", "three", "two"], \
        "part of the selection was renamed before the clash was noticed"


def test_a_bulk_patch_that_does_not_touch_names_still_works(tmp_path):
    c = client(tmp_path)
    ids = [c.post("/api/profiles", json={"name": n}).json()["id"]
           for n in ("one", "two")]

    r = c.post("/api/profiles/bulk", json={"ids": ids, "patch": {"notes": "swept"}})

    assert r.status_code < 400, r.text
    assert [p["notes"] for p in r.json()] == ["swept", "swept"]


def test_importing_the_same_vendor_file_twice_is_refused(tmp_path):
    """The obvious user error - importing the export again - used to double
    every profile in it under the same names."""
    c = client(tmp_path)
    payload = json.dumps([{"name": "vendor-a", "os": "win"},
                          {"name": "vendor-b", "os": "win"}])

    first = c.post("/api/profiles/import", json={"text": payload})
    assert first.status_code < 400, first.text

    second = c.post("/api/profiles/import", json={"text": payload})

    assert second.status_code == 409, second.text
    assert names(tmp_path) == ["vendor-a", "vendor-b"], \
        "the second import wrote a duplicate set"


def test_one_vendor_file_naming_a_profile_twice_is_refused(tmp_path):
    c = client(tmp_path)
    payload = json.dumps([{"name": "same", "os": "win"}, {"name": "same", "os": "mac"}])

    r = c.post("/api/profiles/import", json={"text": payload})

    assert r.status_code == 409, r.text
    assert names(tmp_path) == []


# --- the two surfaces agree ------------------------------------------------

def test_the_api_refuses_the_name_the_cli_created(tmp_path):
    """The point of a single contract: which surface got there first must not
    change the answer."""
    store = ProfileStore(tmp_path)
    repo(store).create(Profile(name="shared", fingerprint=generate(seed=1)))

    r = client(tmp_path).post("/api/profiles", json={"name": "shared"})

    assert r.status_code == 409, r.text


def test_the_cli_refuses_the_name_the_api_created(tmp_path):
    c = client(tmp_path)
    c.post("/api/profiles", json={"name": "shared"})

    assert cli.main(["--data-dir", str(tmp_path), "create", "shared"]) != 0
    assert names(tmp_path) == ["shared"]


def test_listing_still_adopts_a_cli_made_profile(tmp_path):
    """`GET /api/profiles` writes: it copies anything the CLI created into the
    dashboard store. That write carries the profile's existing name, so a check
    that read it as a fresh claim would turn the listing endpoint into a 500
    for every store the CLI has ever touched."""
    store = ProfileStore(tmp_path)
    repo(store).create(Profile(name="from-the-cli", fingerprint=generate(seed=2)))

    r = client(tmp_path).get("/api/profiles")

    assert r.status_code == 200, r.text
    assert [p["name"] for p in r.json()] == ["from-the-cli"]
    assert dash_names(tmp_path) == ["from-the-cli"], "the profile was not adopted"


# --- restore --new-id is a clone, and a clone needs its own name -----------

def backup_of(store, prof, dest):
    ud = store.user_data_path(prof.id)
    (ud / "Cookies").write_text("session", encoding="utf-8")
    return store.backup(prof.id, dest)


def test_restoring_a_clone_takes_a_free_name(tmp_path):
    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="warmed", fingerprint=generate(seed=3)))
    archive = backup_of(store, prof, tmp_path / "b.zip")

    copy = store.restore(archive, new_id=True)

    assert copy.id != prof.id
    assert copy.name == "warmed (copy)", f"the clone kept a taken name: {copy.name!r}"
    assert names(tmp_path) == ["warmed", "warmed (copy)"]


def test_a_second_clone_does_not_collide_with_the_first(tmp_path):
    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="warmed", fingerprint=generate(seed=3)))
    archive = backup_of(store, prof, tmp_path / "b.zip")

    store.restore(archive, new_id=True)
    third = store.restore(archive, new_id=True)

    assert third.name == "warmed (copy 2)", third.name
    assert sorted(names(tmp_path)) == sorted(
        ["warmed", "warmed (copy)", "warmed (copy 2)"])


def test_restoring_over_the_same_id_keeps_the_archived_name(tmp_path):
    """A restore is asked for by id. The name in the archive is the one that
    profile already holds, so nothing is being claimed and nothing is renamed -
    this must not become `alpha (copy)` on every restore."""
    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="alpha", fingerprint=generate(seed=4)))
    archive = backup_of(store, prof, tmp_path / "b.zip")
    repo(store).edit(prof.id, lambda p: setattr(p, "name", "renamed since") or p)

    back = store.restore(archive)

    assert back.id == prof.id
    assert back.name == "alpha"
    assert names(tmp_path) == ["alpha"]


def test_a_name_based_command_after_a_clone_targets_one_profile(tmp_path):
    """The whole reason the contract exists. Before this, `restore --new-id`
    left two profiles called `warmed`, and `numera show/edit/delete warmed`
    each acted on whichever the directory listed first."""
    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="warmed", fingerprint=generate(seed=5)))
    archive = backup_of(store, prof, tmp_path / "b.zip")
    clone = store.restore(archive, new_id=True)
    root = str(tmp_path)

    assert cli.main(["--data-dir", root, "show", "warmed"]) == 0
    assert cli.main(["--data-dir", root, "edit", "warmed", "--notes", "the original"]) == 0
    assert ProfileStore(tmp_path).get(prof.id).notes == "the original"
    assert ProfileStore(tmp_path).get(clone.id).notes != "the original", \
        "the edit landed on the clone"

    assert cli.main(["--data-dir", root, "delete", "warmed (copy)", "--yes"]) == 0
    assert names(tmp_path) == ["warmed"], "the wrong profile was deleted"
    assert ProfileStore(tmp_path).get(prof.id) is not None


def test_importing_an_export_whose_name_is_taken_is_refused(tmp_path):
    """`numera import` was the last write that skipped the check."""
    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="exported", fingerprint=generate(seed=6)))
    out = tmp_path / "p.json"
    store.export(prof.id, out)
    data = json.loads(out.read_text(encoding="utf-8"))
    data["id"] = "adifferentid"                   # the same name, a second profile
    out.write_text(json.dumps(data), encoding="utf-8")

    with pytest.raises(ValueError):
        ProfileStore(tmp_path).import_file(out)
    assert names(tmp_path) == ["exported"]


def test_reimporting_an_export_over_its_own_id_still_works(tmp_path):
    store = ProfileStore(tmp_path)
    prof = repo(store).create(Profile(name="exported", fingerprint=generate(seed=7)))
    out = tmp_path / "p.json"
    store.export(prof.id, out)
    repo(store).edit(prof.id, lambda p: setattr(p, "notes", "changed") or p)

    back = ProfileStore(tmp_path).import_file(out)

    assert back.id == prof.id and back.name == "exported"
    assert ProfileStore(tmp_path).get(prof.id).notes == ""


def test_a_hand_made_archive_can_still_be_restored_into_an_empty_store(tmp_path):
    """No name is taken, so nothing is renamed. Guards the free-name search
    against turning every first restore into a copy."""
    store = ProfileStore(tmp_path)
    path = tmp_path / "hand.zip"
    with zipfile.ZipFile(path, "w") as z:
        z.writestr("profile.json", json.dumps(
            Profile(name="fresh", fingerprint=generate(seed=8)).to_dict()))

    assert store.restore(path).name == "fresh"
