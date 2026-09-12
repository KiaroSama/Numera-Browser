"""A create that names no engine gets the engine the user actually chose.

`config.json` holds `default_engine` - the engine every new profile is created
with. `POST /api/profiles` read it, but only as a fallback:

    _validate_profile(profile)                      # fills engine in
    if not profile.get("engine"):                   # never true afterwards
        profile["engine"] = engine_store.get_default_engine()

Normalisation converts the body through `to_engine_profile`, which answers
`playwright` for a profile that names no engine, and copies every field the
body left empty back onto it. So the fallback's condition was already false by
the time it was read, and a request omitting the engine created a **Playwright**
profile while the config still said Camoufox - with nothing on screen saying
so, because the config page kept showing the user's real choice. `/batch` had
the same shape of bug with the same cause.

The distinction that makes this fixable is between an engine the caller
*chose* and one we filled in. It already exists for the other half of this
feature - "last engine wins" must not let a filled-in guess overwrite the real
default (`test_default_engine.py`) - and these tests are the other side of it:
the saved default has to be applied *before* the conversion, and an update must
never reset an engine a profile already has.
"""
import json

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera.server import create_app                            # noqa: E402
from numera.store import ProfileStore                           # noqa: E402


#: Every engine that is not the converter's own fallback. Parametrised rather
#: than hard-coded to one, because `get_default_engine()` prefers the first
#: *installed* engine when nothing is configured - so a test pinned to
#: `camoufox` alone could pass on a machine where camoufox is what an unfixed
#: fallback would have picked anyway.
NOT_THE_CONVERTER_DEFAULT = ["camoufox", "patchright", "cloak"]


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def with_default(tmp_path, engine: str):
    ProfileStore(tmp_path).set_default_engine(engine)
    return engine


def stored_engine(tmp_path, pid: str) -> str:
    """The engine on the profile as written, not the one in the response."""
    f = tmp_path / "dashboard" / f"{pid}.json"
    return json.loads(f.read_text(encoding="utf-8")).get("engine")


def engine_twin_engine(tmp_path, pid: str) -> str:
    """And the same field on the engine store's copy of it."""
    f = tmp_path / "profiles" / f"{pid}.json"
    return json.loads(f.read_text(encoding="utf-8")).get("engine")


@pytest.mark.parametrize("engine", NOT_THE_CONVERTER_DEFAULT)
def test_a_minimal_create_uses_the_saved_default(c, tmp_path, engine):
    """The headline case: the profile, not the config value."""
    with_default(tmp_path, engine)
    made = c.post("/api/profiles", json={"name": "minimal"}).json()
    assert made["engine"] == engine, \
        f"a create that named no engine got {made['engine']!r}"
    assert stored_engine(tmp_path, made["id"]) == engine
    assert engine_twin_engine(tmp_path, made["id"]) == engine, \
        "the engine store's copy disagrees with the dashboard's"


@pytest.mark.parametrize("engine", NOT_THE_CONVERTER_DEFAULT)
def test_an_explicit_null_engine_is_the_same_as_omitting_it(c, tmp_path, engine):
    """The dashboard sends `null` for a field it has not filled in, and a
    client echoing a profile back sends whatever it was given."""
    with_default(tmp_path, engine)
    made = c.post("/api/profiles", json={"name": "nulled", "engine": None}).json()
    assert made["engine"] == engine


@pytest.mark.parametrize("engine", NOT_THE_CONVERTER_DEFAULT)
def test_a_minimal_batch_entry_uses_the_saved_default(c, tmp_path, engine):
    """`/batch` is how the dashboard creates profiles at scale, so this is the
    route that would produce a hundred wrong-engine profiles at once."""
    with_default(tmp_path, engine)
    r = c.post("/api/profiles/batch",
               json={"profiles": [{"name": "one"}, {"name": "two"}]})
    assert r.status_code < 400, r.text
    for pid in r.json()["ids"]:
        assert stored_engine(tmp_path, pid) == engine, \
            f"{pid} was created with {stored_engine(tmp_path, pid)!r}"
        assert engine_twin_engine(tmp_path, pid) == engine


def test_a_mixed_batch_keeps_each_entry_s_own_answer(c, tmp_path):
    """An explicit choice is a choice; the entries beside it are not, and must
    not borrow it."""
    with_default(tmp_path, "camoufox")
    r = c.post("/api/profiles/batch", json={"profiles": [
        {"name": "implicit-first"},
        {"name": "explicit", "engine": "playwright"},
        {"name": "implicit-last"},
    ]})
    assert r.status_code < 400, r.text
    by_name = {p["name"]: p["engine"] for p in c.get("/api/profiles").json()}
    assert by_name["explicit"] == "playwright", "an explicit choice was overwritten"
    assert by_name["implicit-first"] == "camoufox"
    assert by_name["implicit-last"] == "camoufox", \
        "an entry that named nothing inherited the entry beside it"


def test_an_explicit_engine_still_wins_over_the_default(c, tmp_path):
    """The fix must not start ignoring what the caller asked for."""
    with_default(tmp_path, "camoufox")
    made = c.post("/api/profiles", json={"name": "a", "engine": "playwright"}).json()
    assert made["engine"] == "playwright"
    assert stored_engine(tmp_path, made["id"]) == "playwright"


# --- the default-update policy this must not disturb ------------------------

def test_a_create_that_names_no_engine_leaves_the_default_alone(c, tmp_path):
    """An engine we filled in is not a choice, so it must not be persisted as
    the new default - otherwise a client's silence rewrites the user's setting.
    """
    with_default(tmp_path, "camoufox")
    c.post("/api/profiles", json={"name": "minimal"})
    assert ProfileStore(tmp_path).get_default_engine() == "camoufox"


def test_a_create_that_names_an_engine_still_updates_the_default(c, tmp_path):
    with_default(tmp_path, "camoufox")
    c.post("/api/profiles", json={"name": "a", "engine": "playwright"})
    assert ProfileStore(tmp_path).get_default_engine() == "playwright"


def test_a_batch_that_names_no_engine_leaves_the_default_alone(c, tmp_path):
    with_default(tmp_path, "camoufox")
    c.post("/api/profiles/batch", json={"profiles": [{"name": "one"}, {"name": "two"}]})
    assert ProfileStore(tmp_path).get_default_engine() == "camoufox"


# --- an update is not a create ----------------------------------------------

def test_a_rename_does_not_move_an_existing_profile_to_the_default(c, tmp_path):
    """The mirror image of the bug: applying a creation default on a patch
    would silently re-engine every profile edited after the setting changed."""
    made = c.post("/api/profiles", json={"name": "a", "engine": "playwright"}).json()
    with_default(tmp_path, "camoufox")

    r = c.put(f"/api/profiles/{made['id']}", json={"name": "renamed"})
    assert r.status_code < 400, r.text
    assert r.json()["engine"] == "playwright", \
        "a rename moved the profile onto the creation default"
    assert stored_engine(tmp_path, made["id"]) == "playwright"


def test_an_unrelated_bulk_edit_does_not_re_engine_the_selection(c, tmp_path):
    a = c.post("/api/profiles", json={"name": "a", "engine": "playwright"}).json()
    b = c.post("/api/profiles", json={"name": "b", "engine": "playwright"}).json()
    with_default(tmp_path, "camoufox")

    r = c.post("/api/profiles/bulk",
               json={"ids": [a["id"], b["id"]], "patch": {"addTags": ["x"]}})
    assert r.status_code < 400, r.text
    for row in r.json():
        assert row["engine"] == "playwright", "a tag edit changed the engine"


def test_an_update_that_names_an_engine_still_changes_it(c, tmp_path):
    """The feature on the other side of the same line."""
    made = c.post("/api/profiles", json={"name": "a", "engine": "playwright"}).json()
    r = c.put(f"/api/profiles/{made['id']}", json={"engine": "camoufox"})
    assert r.status_code < 400, r.text
    assert stored_engine(tmp_path, made["id"]) == "camoufox"
