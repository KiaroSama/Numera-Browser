"""An edit that says nothing about engines must not change the global default.

The default engine is what every future profile is created with. It could be
changed by a rename, a tag edit, or a bulk note change - the last profile
touched won. A user who set `camoufox` and then renamed one old `playwright`
profile silently got `playwright` for everything afterwards, with nothing in the
UI saying so.
"""
import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera.server import create_app                            # noqa: E402
from numera.store import ProfileStore                           # noqa: E402


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def default_of(tmp_path):
    return ProfileStore(tmp_path).get_default_engine()


def test_renaming_a_profile_does_not_change_the_default(c, tmp_path):
    """The headline case: an edit that never mentions an engine."""
    ProfileStore(tmp_path).set_default_engine("camoufox")
    made = c.post("/api/profiles", json={"name": "a", "engine": "playwright"}).json()
    ProfileStore(tmp_path).set_default_engine("camoufox")

    r = c.put(f"/api/profiles/{made['id']}", json={"name": "renamed"})
    assert r.status_code == 200, r.json()
    assert default_of(tmp_path) == "camoufox", "a rename moved the global default"


def test_an_explicit_engine_change_does_update_the_default(c, tmp_path):
    """The feature this must not break."""
    made = c.post("/api/profiles", json={"name": "a", "engine": "camoufox"}).json()
    assert made["engine"] == "camoufox"
    ProfileStore(tmp_path).set_default_engine("camoufox")
    c.put(f"/api/profiles/{made['id']}", json={"name": "a", "engine": "playwright"})
    assert default_of(tmp_path) == "playwright"


def test_resaving_the_same_engine_is_not_a_change(c, tmp_path):
    """Round-tripping a profile unchanged should be inert."""
    made = c.post("/api/profiles", json={"name": "a", "engine": "playwright"}).json()
    ProfileStore(tmp_path).set_default_engine("camoufox")
    c.put(f"/api/profiles/{made['id']}", json={"name": "a", "engine": "playwright"})
    assert default_of(tmp_path) == "camoufox"


def test_a_tag_only_bulk_edit_does_not_change_the_default(c, tmp_path):
    """Bulk called `_remember_engine` per touched profile whatever the patch
    contained, so the last profile in the selection decided the default."""
    a = c.post("/api/profiles", json={"name": "a", "engine": "playwright"}).json()
    b = c.post("/api/profiles", json={"name": "b", "engine": "playwright"}).json()
    ProfileStore(tmp_path).set_default_engine("camoufox")

    r = c.post("/api/profiles/bulk",
               json={"ids": [a["id"], b["id"]], "patch": {"addTags": ["x"]}})
    assert r.status_code == 200, r.json()
    assert default_of(tmp_path) == "camoufox", "a tag edit moved the global default"


def test_a_bulk_patch_that_names_an_engine_still_updates_it(c, tmp_path):
    a = c.post("/api/profiles", json={"name": "a"}).json()
    ProfileStore(tmp_path).set_default_engine("camoufox")
    c.post("/api/profiles/bulk", json={"ids": [a["id"]], "patch": {"engine": "playwright"}})
    assert default_of(tmp_path) == "playwright"


def test_an_auto_filled_batch_entry_cannot_override_an_explicit_one(c, tmp_path):
    """`last_engine` was read after the auto-fill, so entry 2 - which named
    nothing and was filled from the current default - overrode entry 1's real
    choice."""
    ProfileStore(tmp_path).set_default_engine("camoufox")
    r = c.post("/api/profiles/batch",
               json={"profiles": [{"name": "one", "engine": "playwright"},
                                  {"name": "two"}]})
    assert r.status_code == 200, r.json()
    assert default_of(tmp_path) == "playwright", \
        "an entry that named no engine overrode one that did"


def test_a_batch_that_names_no_engine_leaves_the_default_alone(c, tmp_path):
    ProfileStore(tmp_path).set_default_engine("camoufox")
    c.post("/api/profiles/batch", json={"profiles": [{"name": "one"}, {"name": "two"}]})
    assert default_of(tmp_path) == "camoufox"
