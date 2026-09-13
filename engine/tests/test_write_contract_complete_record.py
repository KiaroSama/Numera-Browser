"""N04: a complete record is validated completely, at every door that writes one.

The audit's two halves, both reproduced against `ProfileStore.save` before
anything here was written:

**N04.A - an invalid fingerprint container.** `_reader` answered every lookup
with `None` for an object that had no such attribute, so `fingerprint = 123`
produced a reader reporting all thirty-five fields absent, the checks found
nothing to say, and the record was declared writable. It did not reach disk -
`Fingerprint.to_dict()` raised `AttributeError: 'int' object has no attribute
'to_dict'` a few frames later - but a traceback is not a refusal: it names no
field, and over HTTP it is a 500 where a 422 belongs. Measured, not assumed:
all four of `None`, `[]`, `123` and `"x"` crashed rather than wrote.

**N04.B - missing field rules.** These *did* write. Driven through
`ProfileStore.save`, `screen_width=inf`, `viewport_height=-1`, `timezone=[]`,
`user_agent=123`, `seed=None` and `languages=None` all landed on disk, and the
engine file for the first one contained the bare token `Infinity` - which is
not JSON, so `JSON.parse` rejects the whole file, and the dashboard twin
carried `screen: "infx1080"`, which `convert.to_engine_profile` cannot parse
and silently replaces with 1920x1080. Three of the audit's examples were
already refused before this work and are kept here as controls that say so:
`seed=inf`, `hardware_concurrency=True` and `fonts=[1, 2]`.

The rule is now the `field_policy` table, diffed against the dataclasses by
`test_model_policy_coverage`. This file is the other half: that the table is
actually reached, from each door that writes a complete record, and that a
refusal happens before any byte of any file changes.
"""

import json
from dataclasses import replace

import pytest

from numera import generate, write_contract
from numera.models import Fingerprint, Profile, Proxy
from numera.store import ProfileStore

#: One generated fingerprint for the whole module. Every case differs by a
#: single field, so `replace()` says that outright and costs one generate().
FP = generate(seed=1)


def profile(**kw) -> Profile:
    kw.setdefault("fingerprint", replace(FP))
    kw.setdefault("name", "p")
    return Profile(**kw)


def with_fp(name: str = "p", **kw) -> Profile:
    """A profile whose fingerprint differs from the healthy one by `kw`."""
    return profile(name=name, fingerprint=replace(FP, **kw))


def snapshot(root) -> dict:
    """Every file under `root`, by path and exact bytes.

    A refusal that writes nothing has to be asserted against the bytes, not
    against a profile count: a half-written mirror or a config change committed
    by a call that reported failure both leave the count alone.
    """
    return {str(p.relative_to(root)): p.read_bytes()
            for p in sorted(root.rglob("*")) if p.is_file()}


# ---------------------------------------------------------------------------
# N04.A - the container itself
# ---------------------------------------------------------------------------
@pytest.mark.parametrize("bad", [None, [], 123, "x", 1.5, True, ("a",)],
                         ids=["none", "list", "int", "str", "float", "bool",
                              "tuple"])
def test_a_fingerprint_that_is_not_one_is_refused_by_name(tmp_path, bad):
    """The defect: generic attribute lookup turned this into "nothing set".

    The refusal has to name `fingerprint`. `AttributeError: 'int' object has no
    attribute 'to_dict'` names a method of a class the caller never mentioned.
    """
    store = ProfileStore(tmp_path)
    before = snapshot(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=r"'fingerprint'"):
        store.save(profile(fingerprint=bad))

    assert snapshot(tmp_path) == before


def test_an_empty_fingerprint_mapping_is_refused_field_by_field(tmp_path):
    """A mapping *is* a container, so it gets read - and then has nothing in it.

    This is the shape that separates "not a fingerprint" from "a fingerprint
    with required fields missing", and they earn different messages.
    """
    store = ProfileStore(tmp_path)

    with pytest.raises(write_contract.WriteRefused) as refused:
        store.save(profile(fingerprint={}))

    assert "fingerprint.os" in str(refused.value)
    assert "fingerprint.seed" in str(refused.value)


def test_something_that_is_not_a_profile_at_all_is_named_as_such(tmp_path):
    """The same generic-reader defect one level up.

    An integer handed to the validator came back reported as a profile whose
    *name* was unset - a message about a field, for a thing that has no fields.
    """
    problems = write_contract.profile_problems(123)

    assert problems == ["profile: a profile must be a record or a patch, "
                        "not int. Nothing was written."]


# ---------------------------------------------------------------------------
# N04.B - the audit's accepted examples, at the real boundary
# ---------------------------------------------------------------------------
#: (field, value, message fragment). Every one of these was written to disk by
#: `ProfileStore.save` before this work; the three marked `control` below were
#: already refused and are here so a regression in either direction is visible.
WROTE_BEFORE = [
    ("screen_width", float("inf"), "finite"),
    ("screen_height", float("-inf"), "finite"),
    ("viewport_height", -1, "between 1 and 65536"),
    ("viewport_width", 0, "between 1 and 65536"),
    ("timezone", [], "must be text"),
    ("user_agent", 123, "must be text"),
    ("seed", None, "required"),
    ("languages", None, "must be a list"),
    ("os", None, "must be text"),
    ("chrome_version", None, "must be text"),
    ("webrtc", [], "must be text"),
    ("canvas", 7, "must be text"),
    ("locale", b"en", "must be text"),
    ("is_mobile", 2, "true or false"),
    ("do_not_track", 1, "true or false"),
    ("device_memory", 3.5, "whole number"),
    ("hardware_concurrency", 2.5, "whole number"),
    ("max_touch_points", -3, "between 0 and 32"),
    ("os_version", None, "must be text"),
    ("device_model", 4, "must be text"),
]

#: Already refused before N04. Kept as controls: the work must not have made
#: any of them acceptable on the way past.
ALREADY_REFUSED = [
    ("seed", float("inf"), "finite"),
    ("seed", float("nan"), "finite"),
    ("seed", 10 ** 400, "between"),
    ("hardware_concurrency", True, "must be a number"),
    ("device_pixel_ratio", float("nan"), "finite"),
    ("fonts", [1, 2], "every entry"),
    ("languages", ["en", 3], "every entry"),
]


@pytest.mark.parametrize("field,value,fragment", WROTE_BEFORE + ALREADY_REFUSED,
                         ids=lambda v: str(v)[:18])
def test_the_core_write_path_refuses_the_field_and_writes_nothing(
        tmp_path, field, value, fragment):
    """One case per audit example, through `ProfileStore.save` itself."""
    store = ProfileStore(tmp_path)
    before = snapshot(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=fragment) as refused:
        store.save(with_fp(**{field: value}))

    assert field in str(refused.value), \
        "the refusal did not name the field the caller set"
    assert snapshot(tmp_path) == before, "the refusal still wrote something"


def test_a_non_finite_dimension_never_becomes_a_non_json_token(tmp_path):
    """What `screen_width = inf` actually produced, and why it mattered.

    `json.dump` writes `Infinity` for it - which no strict parser accepts, so
    one profile made the whole file unreadable to `JSON.parse` rather than
    merely odd. Asserted on the bytes, because a Python reader accepts the
    token and would report the file as fine.
    """
    store = ProfileStore(tmp_path)
    store.save(profile(name="healthy"))

    with pytest.raises(write_contract.WriteRefused):
        store.save(with_fp(screen_width=float("inf")))

    for path in tmp_path.rglob("*.json"):
        json.loads(path.read_text("utf-8"),
                   parse_constant=lambda c: pytest.fail(
                       f"{path.name} holds the non-JSON token {c!r}"))


# ---------------------------------------------------------------------------
# Collections, bookmarks and the proxy
# ---------------------------------------------------------------------------
@pytest.mark.parametrize("value,fragment", [
    ("https://x.test", "must be a list"),
    ([3], "{'name', 'url'}"),
    (["https://x.test"], "{'name', 'url'}"),
    ([{"name": 1, "url": "https://x.test"}], "bookmarks[].name"),
    ([{"name": "ok"}], "bookmarks[].url"),
    ([{"name": "ok", "url": 7}], "bookmarks[].url"),
], ids=["str", "int-entry", "str-entry", "numeric-name", "no-url", "numeric-url"])
def test_a_malformed_bookmark_is_refused_by_name(tmp_path, value, fragment):
    """`to_dashboard_profile` reads `b.get("name")` and `b["url"]` off each
    entry, so a bare string or number raised `AttributeError` out of the write
    and `{"name": 1}` was formatted into the stored text as a title."""
    store = ProfileStore(tmp_path)
    before = snapshot(tmp_path)

    with pytest.raises(write_contract.WriteRefused) as refused:
        store.save(profile(bookmarks=value))

    assert "bookmarks" in str(refused.value)
    assert fragment in str(refused.value)
    assert snapshot(tmp_path) == before


def test_a_valid_bookmark_list_still_saves(tmp_path):
    """The control: the rule is about shape, not about having bookmarks."""
    store = ProfileStore(tmp_path)

    saved = store.save(profile(bookmarks=[
        {"name": "Docs", "url": "https://docs.test"},
        {"name": "", "url": "https://x.test"},
    ]))

    assert [b["url"] for b in store.get(saved.id).bookmarks] == [
        "https://docs.test", "https://x.test"]


@pytest.mark.parametrize("bad", [123, [], {"server": 1}, 1.5],
                         ids=["int", "list", "numeric-server", "float"])
def test_a_proxy_that_cannot_be_one_is_refused_by_name(tmp_path, bad):
    store = ProfileStore(tmp_path)
    before = snapshot(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=r"proxy"):
        store.save(profile(proxy=bad))

    assert snapshot(tmp_path) == before


def test_a_refusal_never_quotes_the_password_back(tmp_path):
    """These messages reach stderr, the API response body and the logs.

    The wrong *type* is worth naming; the value is not, and a list password
    holding the real secret as one of its entries would otherwise be printed
    in full.
    """
    store = ProfileStore(tmp_path)
    px = Proxy(server="http://h:1")
    px.password = ["hunter2-the-real-one"]

    with pytest.raises(write_contract.WriteRefused) as refused:
        store.save(profile(proxy=px))

    assert "proxy.password" in str(refused.value)
    assert "hunter2" not in str(refused.value)


def test_validating_never_reassigns_the_password(tmp_path):
    """`Proxy.__setattr__` records that the password was *chosen*, and that
    flag is what tells a write to preserve, clear or replace the stored secret.
    A validator that assigned to `password` - even the value already there -
    would turn every untouched proxy into a replacement.

    The nested pass reads every field of the policy table now, `password`
    included, so this is the assertion that says it reads and nothing else.
    """
    px = Proxy.from_dict({"server": "http://h:1", "username": "u",
                          "password": "stored", "country": "DE"})
    assert px.password_intent == Proxy.PRESERVE       # the precondition

    write_contract.profile_problems(profile(proxy=px))

    assert px.password_intent == Proxy.PRESERVE, \
        "validation flipped the proxy's write intent"


# ---------------------------------------------------------------------------
# The three shapes
# ---------------------------------------------------------------------------
def test_the_three_shapes_are_told_apart_explicitly():
    """A patch and a record disagree about what an absent field means, and a
    single pass over both had to pick one answer. It picked "absent", which is
    why a complete profile carrying `seed = None` was stored."""
    assert write_contract.shape(profile()) is write_contract.ENGINE_OBJECT
    assert write_contract.shape(profile().to_dict()) is write_contract.ENGINE_RECORD
    assert write_contract.shape({"name": "n", "cores": 8}) is \
        write_contract.DASHBOARD_PATCH


def test_a_partial_dashboard_patch_is_not_charged_for_what_it_omits():
    """An absent key here means "not changing this one". The API merges the
    patch over the stored record, and the merged result is what reaches the
    complete check through the converter."""
    assert write_contract.profile_problems({"name": "n"}) == []
    assert write_contract.profile_problems({"cores": 8, "memory": 16}) == []


def test_a_complete_record_is_charged_for_what_it_omits():
    """The same omission, in the shape where it is a missing field."""
    record = profile().to_dict()
    del record["fingerprint"]["seed"]

    assert any("fingerprint.seed" in p
               for p in write_contract.profile_problems(record))


def test_the_merged_patch_is_what_gets_complete_validation():
    """The order the brief requires: normalise and merge, *then* check.

    Validating the patch as though it were a record would refuse every
    single-field edit; validating the merge as though it were a patch is how
    the gap stayed open. Both directions asserted on the same pair.
    """
    stored = profile(name="stored").to_dict()
    patch = {"notes": "edited"}

    assert write_contract.profile_problems(patch) == []       # as a patch
    assert write_contract.profile_problems({**stored, **patch}) == []


@pytest.mark.parametrize("value", ["0x0", "-5x-5", "potato", "infx1080",
                                   "1920", "1920x1080x2", "99999x10"])
def test_the_packed_resolution_cannot_walk_past_the_dimension_rule(value):
    """`screen` is the dashboard's one name for two engine fields.

    `convert` writes `f"{w}x{h}"` and parses it back with `int()`, falling back
    to 1920x1080 when that raises - so sending the pair as text was a way round
    every dimension rule the engine record has, and `"potato"` replaced the
    caller's resolution with a default while the request reported success.
    """
    assert any("screen" in p for p in
               write_contract.profile_problems({"name": "n", "screen": value}))


@pytest.mark.parametrize("value", ["1920x1080", "1920X1080", "800x600"])
def test_an_ordinary_resolution_is_left_alone(value):
    assert write_contract.profile_problems({"name": "n", "screen": value}) == []


# ---------------------------------------------------------------------------
# Batch: a bad *later* entry
# ---------------------------------------------------------------------------
def test_a_bad_second_entry_leaves_the_first_one_unwritten(tmp_path):
    """The all-or-nothing property, with the failure deliberately not first.

    A validator called per profile inside the write loop refuses the second one
    after the first is already on disk, and the command reports failure over a
    store that changed - the worst shape to recover from, because the obvious
    retry duplicates whatever landed. Asserted on the exact bytes of every file
    under the root, taken before the call.
    """
    from numera.repo import repo

    store = ProfileStore(tmp_path)
    store.save(profile(name="already-here"))
    before = snapshot(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=r"profiles\[1\]"):
        repo(store).save_many([
            profile(name="first-and-fine"),
            with_fp("second-and-broken", user_agent=123),
            profile(name="third-and-fine"),
        ])

    assert snapshot(tmp_path) == before, "a refused batch wrote part of itself"
    assert [p.name for p in store.list()] == ["already-here"]


def test_a_bad_last_entry_is_named_by_its_position(tmp_path):
    """A ten-profile import that refuses without saying which one is useless."""
    from numera.repo import repo

    store = ProfileStore(tmp_path)
    batch = [profile(name=f"p{i}") for i in range(5)]
    batch[4] = with_fp("p4", seed=None)

    with pytest.raises(write_contract.WriteRefused, match=r"profiles\[4\]"):
        repo(store).save_many(batch)

    assert store.list() == []


def test_a_config_change_does_not_commit_when_the_batch_is_refused(tmp_path):
    """The config rides in the same transaction, so a refusal owes it the same
    answer: `--engine` used to write the global default before the profiles."""
    from numera.repo import repo

    store = ProfileStore(tmp_path)
    store.save(profile(name="seed-profile"))
    before = snapshot(tmp_path)

    with pytest.raises(write_contract.WriteRefused):
        repo(store).save_many([with_fp("bad", timezone=[])],
                              config_change={"default_engine": "camoufox"})

    assert snapshot(tmp_path) == before


# ---------------------------------------------------------------------------
# Valid records: the controls
# ---------------------------------------------------------------------------
def test_a_healthy_profile_round_trips_unchanged_but_for_server_metadata(tmp_path):
    """The contract is about type, not content. `updated_at` is the documented
    exception - every write stamps it, which is why the policy marks it owned."""
    store = ProfileStore(tmp_path)
    original = profile(name="Ünïcodé ✓", notes="note", tags=["a", "b"],
                       startup_urls=["https://x.test"],
                       bookmarks=[{"name": "D", "url": "https://d.test"}],
                       proxy=Proxy(server="socks5://h:1080", username="u",
                                   country="NL"))

    saved = store.save(original)
    read = store.get(saved.id)

    assert read.to_dict() | {"updated_at": 0} == original.to_dict() | {"updated_at": 0}


@pytest.mark.parametrize("value", [1.0, 1.5, 2.0, 2.625, 3, 0.05, 20])
def test_a_fractional_device_pixel_ratio_is_still_accepted(tmp_path, value):
    """The field where a float is not a mistake: 1.5 and 2.625 are ordinary
    values for a real display. The whole-number rule applies to the counts a
    browser API reports as integers, and to this one it must not."""
    store = ProfileStore(tmp_path)

    saved = store.save(with_fp(f"dpr-{value}", device_pixel_ratio=value))

    assert store.get(saved.id).fingerprint.device_pixel_ratio == value


@pytest.mark.parametrize("field,value", [
    ("seed", 0), ("device_memory", 0), ("max_touch_points", 0),
    ("cameras", 0), ("microphones", 0), ("speakers", 0),
    ("os_version", ""), ("device_model", ""), ("fonts", []),
    ("is_mobile", True), ("do_not_track", True),
])
def test_a_legitimate_edge_value_is_not_refused(tmp_path, field, value):
    """Zero, empty and False are values, not omissions. A requiredness rule
    that cannot tell them from `None` refuses a working profile."""
    store = ProfileStore(tmp_path)

    saved = store.save(with_fp(f"edge-{field}", **{field: value}))

    assert getattr(store.get(saved.id).fingerprint, field) == value


def test_an_empty_name_and_no_proxy_are_still_legitimate(tmp_path):
    """`proxy` is the one genuinely optional block on the profile."""
    store = ProfileStore(tmp_path)

    saved = store.save(profile(name="", proxy=None))

    assert store.get(saved.id).proxy is None
