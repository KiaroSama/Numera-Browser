"""Two doors into one store must agree about what a profile is.

`0.32.0` gave the API a full field schema and the core a *name* check. So
`tags: [{}]` was refused over HTTP and accepted by `numera import-from`, which
reaches `repo.save_many` without passing through any of it. A record that one
surface calls invalid and the other writes is not a validated store.

The second half is the installations that already hold a bad record. Refusing
new writes repairs nothing that is on disk: `name: 123` and `name: null` were
still handed to the dashboard, which calls `p.name.toLowerCase()` on them. The
read view has to render what it finds without touching the file, and without
hiding the record.

`write_contract` is the shared rule, deliberately free of any optional
dependency so the engine can import it with no server extra installed. These
tests exercise it directly and through the core write path; the API side is
already covered by `test_write_contract_entry_points.py`, and re-testing the
same invariant through a TestClient would cost an app per case to prove
something the same function already proved.
"""

import json

import pytest

from numera import generate, write_contract
from numera.models import Profile
from numera.store import ProfileStore


def profile(**kw):
    kw.setdefault("name", "alpha")
    kw.setdefault("fingerprint", generate(seed=1))
    return Profile(**kw)


# ---------------------------------------------------------------------------
# 1. The core refuses what the API refuses
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("field,value,fragment", [
    ("name", 123, "must be text"),
    ("name", None, "must be text"),
    ("name", ["a"], "must be text"),
    ("notes", 7, "must be text"),
    ("engine", {}, "must be text"),
    ("tags", [{}], "every entry"),
    ("tags", ["ok", 17], "every entry"),
    ("tags", "notalist", "must be a list"),
    ("startup_urls", [None], "every entry"),
    ("extensions", [1], "every entry"),
    ("launch_args", [[]], "every entry"),
    ("ip_based", [{}], "every entry"),
    ("clear_on_start", [2.5], "every entry"),
    ("switched_off", [b"x"], "every entry"),
])
def test_the_core_write_path_refuses_what_the_api_refuses(tmp_path, field, value, fragment):
    """The regression, one case per field the product dereferences."""
    store = ProfileStore(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=fragment):
        store.save(profile(**{field: value}))

    assert list((tmp_path / "profiles").glob("*.json")) == [], \
        "the refusal still wrote the profile"


def test_a_batch_names_the_entry_that_failed(tmp_path):
    """A ten-profile import that refuses without saying which one is useless."""
    from numera.repo import repo

    store = ProfileStore(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=r"profiles\[1\]"):
        repo(store).save_many([profile(name="good"), profile(name="bad", tags=[{}])])

    assert store.list() == [], "a refused batch wrote part of itself"


def test_an_ordinary_profile_is_untouched(tmp_path):
    """The control. The contract is about type, not content."""
    store = ProfileStore(tmp_path)

    saved = store.save(profile(name="Ünïcodé ✓", notes="", tags=["a", "b"],
                               startup_urls=["https://x.test"]))

    assert saved.name == "Ünïcodé ✓"
    assert store.list()[0].tags == ["a", "b"]


def test_an_empty_list_and_an_empty_string_are_fine(tmp_path):
    store = ProfileStore(tmp_path)

    saved = store.save(profile(name="", tags=[], notes=""))

    assert saved.name == ""


# ---------------------------------------------------------------------------
# 2. Numbers, without the overflow
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("value", [10 ** 400, -(10 ** 400), 2 ** 200])
def test_an_enormous_integer_is_refused_not_an_overflow(value):
    """`math.isfinite` builds a float first, so this used to raise
    `OverflowError` out of the check instead of being refused by it."""
    problems = write_contract.profile_problems({"name": "n", "cores": value})

    assert any("between" in p for p in problems), problems


@pytest.mark.parametrize("value", [float("nan"), float("inf"), float("-inf")])
def test_a_non_finite_float_is_refused(value):
    problems = write_contract.profile_problems({"name": "n", "memory": value})

    assert any("finite" in p for p in problems), problems


def test_a_number_inside_the_range_is_accepted():
    assert write_contract.profile_problems({"name": "n", "cores": 8, "memory": 16}) == []


def test_a_boolean_is_not_a_number():
    """`True` is an `int` in Python and would otherwise pass the bounds."""
    problems = write_contract.profile_problems({"name": "n", "cores": True})

    assert any("must be a number" in p for p in problems), problems


# ---------------------------------------------------------------------------
# 3. Reading back a record an older build stored
# ---------------------------------------------------------------------------

@pytest.mark.parametrize("stored,rendered", [
    (123, "123"),
    (None, ""),
    (12.5, "12.5"),
    (True, "true"),
    ({}, ""),
    ([], ""),
])
def test_a_legacy_name_renders_as_text(stored, rendered):
    """The UI calls `.toLowerCase()` on this. Refusing new writes did nothing
    for the installations that already hold one."""
    view = write_contract.readable({"id": "x", "name": stored})

    assert view["name"] == rendered
    assert isinstance(view["name"], str)


def test_the_read_view_never_touches_the_stored_record(tmp_path):
    """Quarantine for display, not repair on read: the original bytes stay, so
    a real fix can still see what was there."""
    record = {"id": "x", "name": 123, "tags": ["a", {}]}
    before = json.dumps(record, sort_keys=True)

    write_contract.readable(record)

    assert json.dumps(record, sort_keys=True) == before


def test_a_healthy_record_passes_through_unchanged():
    """No copy, no churn: the common path returns the same object."""
    record = {"id": "x", "name": "alpha", "tags": ["a"]}

    assert write_contract.readable(record) is record


def test_the_dashboard_view_uses_its_own_field_names():
    """The two shapes differ - `startupUrls` is a string here and a list in the
    engine - so the vocabulary is a parameter rather than a second copy."""
    from numera.server.routes_profiles import readable

    view = readable({"id": "x", "name": None, "folder": 7,
                     "startupUrls": 5, "tags": ["a", {}]})

    assert view["name"] == "" and view["folder"] == "7"
    assert view["startupUrls"] == "5", "the dashboard's string field was treated as a list"
    assert view["tags"] == ["a"]


def test_a_bad_record_does_not_hide_the_healthy_ones(tmp_path):
    """The whole point of rendering rather than dropping: one damaged profile
    must not remove itself, or its neighbours, from the listing."""
    from numera.server.routes_profiles import readable

    rows = [{"id": "a", "name": 123}, {"id": "b", "name": "fine"}]

    view = [readable(r) for r in rows]

    assert [r["id"] for r in view] == ["a", "b"]
    assert view[0]["name"] == "123"
