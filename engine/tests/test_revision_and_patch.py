"""The revision on disk is the revision the API returns, and a patch is atomic.

Two defects with one shape - a decision made outside the lock that protects the
write depending on it.

`save_many()` bumped `_rev` *after* `batch.write`, so the number went back to
the client and never to disk: a batch create answered `_rev=1` over a file with
no revision at all, and a bulk update answered N+1 while disk still said N.
Each of those is a revision a client can hold and be wrongly rejected - or,
worse, wrongly accepted - on later.

`PUT /api/profiles/{id}` read the stored record and merged the patch onto it
*before* `save()` took the lock. `expect_rev` closes the lost update for a
client that sends a whole profile, but a patch carries no revision: two
concurrent patches could merge onto the same old record and the second write
would carry stale values for every field the first had just changed. `/bulk`
was worse - it built its whole pending list from unlocked reads.

The concurrency tests run two real processes against one data root and state
the interleaving with a barrier, so the result is a fact and not a coin toss.
"""
import json

import pytest

from _rendezvous import Rendezvous, finish, start

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient        # noqa: E402

from numera.server import create_app             # noqa: E402


def client(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def stored(tmp_path, pid: str) -> dict:
    """The record as it actually is on disk, not as the API described it."""
    return json.loads((tmp_path / "dashboard" / f"{pid}.json").read_text(encoding="utf-8"))


def served(c, pid: str) -> dict:
    """One profile as the API serves it. There is no single-profile GET."""
    return next(p for p in c.get("/api/profiles").json() if p["id"] == pid)


# --- item 2: the revision reaches disk --------------------------------------

def test_batch_create_persists_the_revision_it_returns(tmp_path):
    c = client(tmp_path)
    r = c.post("/api/profiles/batch", json={"profiles": [{"name": "a"}, {"name": "b"}]})
    assert r.status_code < 400, r.text
    for pid in r.json()["ids"]:
        api = served(c, pid)["_rev"]
        disk = stored(tmp_path, pid).get("_rev")
        assert api == disk == 1, f"{pid}: API says {api}, disk says {disk}"


def test_import_persists_the_revision_it_returns(tmp_path):
    c = client(tmp_path)
    export = json.dumps({"profiles": [{"name": "imported", "engine": "playwright",
                                       "fingerprint": {"os": "windows"}}]})
    r = c.post("/api/profiles/import", json={"text": export})
    assert r.status_code < 400, r.text
    listed = c.get("/api/profiles").json()
    assert listed, "the import stored nothing"
    for prof in listed:
        disk = stored(tmp_path, prof["id"]).get("_rev")
        assert prof["_rev"] == disk == 1, f"API says {prof['_rev']}, disk says {disk}"


def test_two_consecutive_bulk_updates_persist_every_revision(tmp_path):
    c = client(tmp_path)
    ids = c.post("/api/profiles/batch",
                 json={"profiles": [{"name": "a"}, {"name": "b"}]}).json()["ids"]
    for expected in (2, 3):
        r = c.post("/api/profiles/bulk",
                   json={"ids": ids, "patch": {"notes": f"pass {expected}"}})
        assert r.status_code < 400, r.text
        for prof in r.json():
            disk = stored(tmp_path, prof["id"]).get("_rev")
            assert prof["_rev"] == disk == expected, \
                f"{prof['id']}: API says {prof['_rev']}, disk says {disk}"


def test_a_revision_held_from_before_a_bulk_edit_is_refused(tmp_path):
    """The point of persisting it: a stale client has to be told."""
    c = client(tmp_path)
    pid = c.post("/api/profiles/batch", json={"profiles": [{"name": "a"}]}).json()["ids"][0]
    before = served(c, pid)
    assert before["_rev"] == 1, before

    c.post("/api/profiles/bulk", json={"ids": [pid], "patch": {"notes": "moved on"}})
    r = c.put(f"/api/profiles/{pid}", json={**before, "notes": "mine"})
    assert r.status_code == 409, f"a stale full-profile write was accepted: {r.text}"
    assert served(c, pid)["notes"] == "moved on"


# --- item 3: read, merge and write are one critical section -----------------
#
# The holder freezes inside `apply`, which the store calls while it holds the
# lock. The other process is then released to patch the same profile: it can
# only be reading the record the holder is about to replace if the merge is
# outside the lock, which is exactly the lost update.

HOLDER = """
from numera.store import ProfileStore
from numera.server.stores import DashboardStore
dash = DashboardStore(ROOT, ProfileStore(ROOT))

def apply(current):
    say("inside")          # the lock is held for as long as this takes
    wait()
    return {{**current, {change}}}, None

dash.{call}
say("done")
"""

LATE_WRITER = """
from numera.store import ProfileStore
from numera.server.stores import DashboardStore
dash = DashboardStore(ROOT, ProfileStore(ROOT))
wait()
say("trying")
dash.{call}
say("done")
"""


def race(holder: str, writer: str, root):
    bar = Rendezvous()
    procs = {}
    try:
        procs["holder"] = start(holder, root, "holder", bar.port)
        procs["writer"] = start(writer, root, "writer", bar.port)
        bar.expect("holder", "inside")     # mid-merge, lock held
        bar.release("writer")              # the second patch starts now
        bar.expect("writer", "trying")
        bar.release("holder")              # the first patch commits
        results = finish(procs)
    finally:
        bar.close()
        for pr in procs.values():
            if pr.poll() is None:
                pr.kill()
    assert all(r["code"] == 0 for r in results.values()), results
    return results


def one_profile(tmp_path) -> str:
    c = client(tmp_path)
    return c.post("/api/profiles", json={"name": "shared", "notes": "", "tags": []}).json()["id"]


def test_a_notes_patch_and_a_tags_patch_both_survive(tmp_path):
    pid = one_profile(tmp_path)
    race(HOLDER.format(change='"notes": "from the notes patch"',
                       call=f'patch({pid!r}, apply)'),
         LATE_WRITER.format(
             call=f'patch({pid!r}, lambda c: ({{**c, "tags": ["late"]}}, None))'),
         tmp_path)

    final = stored(tmp_path, pid)
    assert final["notes"] == "from the notes patch", \
        f"the tags patch merged onto a stale record and erased the notes: {final}"
    assert final["tags"] == ["late"], final
    assert final["_rev"] == 3, f"both patches must be counted: {final['_rev']}"


def test_a_bulk_edit_and_a_single_patch_both_survive(tmp_path):
    """`/bulk` built its pending list from unlocked reads, so it was worse."""
    pid = one_profile(tmp_path)
    race(HOLDER.format(change='"notes": "from the single patch"',
                       call=f'patch({pid!r}, apply)'),
         LATE_WRITER.format(
             call=f'patch_many([{pid!r}], lambda p, c: {{**c, "tags": ["bulk"]}})'),
         tmp_path)

    final = stored(tmp_path, pid)
    assert final["notes"] == "from the single patch", \
        f"the bulk edit merged onto a stale record: {final}"
    assert final["tags"] == ["bulk"], final
    assert final["_rev"] == 3, final["_rev"]


def test_a_delete_is_not_undone_by_a_concurrent_listing(tmp_path):
    """`GET /api/profiles` writes: it adopts engine-only profiles.

    A delete could remove the dashboard copy, this listing could observe the
    engine copy that had not been removed yet, the delete could finish, and the
    listing would then `save()` the stale engine object - resurrecting a
    profile the caller had been told was gone.
    """
    pid = one_profile(tmp_path)
    holder = """
from numera import paths
from numera.store import ProfileStore
from numera.server.stores import DashboardStore
engine = ProfileStore(ROOT)
dash = DashboardStore(ROOT, engine)
_delete = dash.delete

def hooked(pid):
    # Between the two copies going away - the window the listing fell into.
    dash._path(pid).unlink(missing_ok=True)
    say("inside")
    wait()
    return _delete(pid)

dash.delete = hooked
hooked({pid!r})
say("done")
""".format(pid=pid)
    lister = """
from numera.server import create_app
from fastapi.testclient import TestClient
c = TestClient(create_app(data_dir=ROOT, start_scheduler=False))
wait()
say("trying")
print(len(c.get("/api/profiles").json()))
say("done")
"""
    race(holder, lister, tmp_path)

    c = client(tmp_path)
    assert c.get("/api/profiles").json() == [], "a deleted profile came back"
    assert not (tmp_path / "dashboard" / f"{pid}.json").exists()
