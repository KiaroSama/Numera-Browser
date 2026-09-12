"""The shared contract checked the dashboard's field names, over engine objects.

`write_contract.NUMBER_RANGES` is keyed by the dashboard's vocabulary - `cores`,
`memory`, `seed` - and `profile_problems` read every one of them off the *top
level* of whatever it was handed. An engine `Profile` keeps those numbers inside
`profile.fingerprint`, under the names the browser API uses
(`hardware_concurrency`, `device_memory`). So every numeric check saw `None` on
the one shape the core write path actually carries, and skipped it:
`fingerprint.seed = float("inf")` was written by `ProfileStore.save` while the
flat `{"seed": inf}` was refused over HTTP. The advertised numeric contract was
enforced on the shape that already had a second schema in front of it, and on
nothing else.

The proxy had no core check at all. `Proxy.server = 123` reached
`to_playwright()`, and a list password reached the vault.

The read repair had the mirror-image gap. `readable()` iterates the keys a
record *has*, so a record written before `name` was mandatory came back with no
`name` at all - and `App.jsx` calls `p.name.toLowerCase()` on every row, which
throws and blanks the page for every profile, not just that one. Repairing a
present-and-wrong name while leaving an absent one alone fixes the symptom
somebody reported and not the one that is left.

One set of bounds, one name map, three adapters - engine object, serialized
engine mapping, dashboard payload - because the reason the two doors drifted
apart the first time was a second copy of the rule.
"""

import json
from dataclasses import replace

import pytest

from numera import generate, write_contract
from numera.models import Profile, Proxy
from numera.store import ProfileStore

#: One generated fingerprint for the whole module. Every case differs by a
#: single field, so `replace()` says that outright and costs one generate().
FP = generate(seed=1)


def profile(fingerprint=None, **kw):
    kw.setdefault("name", "alpha")
    return Profile(fingerprint=fingerprint or FP, **kw)


def engine_files(root):
    return sorted(p.name for p in (root / "profiles").glob("*.json"))


# ---------------------------------------------------------------------------
# 1. The engine object: numbers that live inside `fingerprint`
# ---------------------------------------------------------------------------

#: `(engine field on Fingerprint, value, the fragment the refusal must carry)`.
#: Every one of these is written today, because the check looked for the
#: dashboard's name for it at the top level and found nothing.
BAD_FINGERPRINT = [
    ("seed", float("inf"), "finite"),
    ("seed", float("nan"), "finite"),
    ("seed", 10 ** 400, "between"),
    ("hardware_concurrency", 10 ** 400, "between"),
    ("hardware_concurrency", 0, "between"),
    ("device_memory", -5, "between"),
    ("device_pixel_ratio", float("-inf"), "finite"),
    ("device_pixel_ratio", 999, "between"),
    ("cameras", "two", "must be a number"),
    ("max_touch_points", True, "must be a number"),
    ("speakers", 10 ** 400, "between"),
]


@pytest.mark.parametrize("field,value,fragment", BAD_FINGERPRINT,
                         ids=[f"{f}={v!r}"[:28] for f, v, _ in BAD_FINGERPRINT])
def test_a_nested_number_is_refused_before_anything_is_written(
        tmp_path, field, value, fragment):
    """The regression. `ProfileStore.save` holds an engine `Profile`, so this
    is the shape the contract has to police, not the one it was policing."""
    store = ProfileStore(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=fragment):
        store.save(profile(fingerprint=replace(FP, **{field: value})))

    assert engine_files(tmp_path) == [], "the refusal still wrote the profile"


@pytest.mark.parametrize("field,value,fragment", BAD_FINGERPRINT,
                         ids=[f"{f}={v!r}"[:28] for f, v, _ in BAD_FINGERPRINT])
def test_the_refusal_names_the_field_the_caller_actually_sent(
        field, value, fragment):
    """A message naming `cores` for a caller who set `hardware_concurrency` is
    a message about a field that caller cannot find."""
    problems = write_contract.profile_problems(
        profile(fingerprint=replace(FP, **{field: value})))

    assert any(f"fingerprint.{field}" in p for p in problems), problems


#: Fingerprint collections the product joins or iterates - `", ".join(langs)`
#: and the font probe list both assume strings throughout.
BAD_FINGERPRINT_LISTS = [
    ("languages", ["en-US", 7], "every entry"),
    ("languages", [None], "every entry"),
    ("languages", "en-US", "must be a list"),
    ("fonts", [{}], "every entry"),
    ("fonts", "Arial", "must be a list"),
]


@pytest.mark.parametrize("field,value,fragment", BAD_FINGERPRINT_LISTS,
                         ids=[f"{f}={v!r}"[:24] for f, v, _ in BAD_FINGERPRINT_LISTS])
def test_a_nested_collection_element_is_refused(tmp_path, field, value, fragment):
    store = ProfileStore(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match=fragment):
        store.save(profile(fingerprint=replace(FP, **{field: value})))

    assert engine_files(tmp_path) == []


# ---------------------------------------------------------------------------
# 2. The proxy, which had no core check at all
# ---------------------------------------------------------------------------

BAD_PROXY = [
    ("server", 123),
    ("server", ["http://x:1"]),
    ("username", []),
    ("username", 7),
    ("password", {}),
    ("country", 7),
]


@pytest.mark.parametrize("field,value", BAD_PROXY,
                         ids=[f"{f}={v!r}"[:20] for f, v in BAD_PROXY])
def test_a_proxy_field_that_is_not_text_is_refused(tmp_path, field, value):
    """`to_playwright()` hands these straight to the driver, and the vault
    stores the password. Neither survives a list."""
    store = ProfileStore(tmp_path)
    px = Proxy(server="http://h:1")
    setattr(px, field, value)

    with pytest.raises(write_contract.WriteRefused, match="must be text"):
        store.save(profile(proxy=px))

    assert engine_files(tmp_path) == []


def test_a_proxy_that_is_not_a_proxy_is_refused_not_a_traceback(tmp_path):
    """`repo._secrets_for` asks the proxy for `password_intent` before anything
    is written, so a string proxy raised `AttributeError: 'str' object has no
    attribute 'password_intent'` out of the writer, where a refusal naming the
    field belongs."""
    store = ProfileStore(tmp_path)

    with pytest.raises(write_contract.WriteRefused, match="proxy"):
        store.save(profile(proxy="http://host:8080"))

    assert engine_files(tmp_path) == []


def test_validating_a_proxy_does_not_decide_what_to_do_with_its_secret(tmp_path):
    """`Proxy.__setattr__` records that the password was *chosen*, and that flag
    is what tells a write to preserve, clear or replace the stored secret. A
    validator that assigned to `password` - even the value already there -
    would silently turn every untouched proxy into a replacement."""
    px = Proxy.from_dict({"server": "http://h:1", "username": "u",
                          "password": "stored", "country": "DE"})
    assert px.password_intent == Proxy.PRESERVE      # the precondition

    write_contract.profile_problems(profile(proxy=px))

    assert px.password_intent == Proxy.PRESERVE, \
        "validation flipped the proxy's write intent"


def test_an_ordinary_proxy_still_saves(tmp_path):
    """The control: the contract is about type, not about having a proxy."""
    store = ProfileStore(tmp_path)

    saved = store.save(profile(proxy=Proxy(server="socks5://h:1080",
                                           username="u", password="p",
                                           country="NL")))

    assert saved.proxy.server == "socks5://h:1080"
    assert store.list()[0].proxy.username == "u"


# ---------------------------------------------------------------------------
# 3. The same rule over the other two shapes
# ---------------------------------------------------------------------------

def test_the_serialized_engine_mapping_is_checked_the_same_way():
    """`profile.to_dict()` is what a batch path and a backup carry. It is the
    same nesting under the same names, so it must not need a second rule."""
    record = profile().to_dict()
    record["fingerprint"]["seed"] = float("inf")

    problems = write_contract.profile_problems(record)

    assert any("fingerprint.seed" in p and "finite" in p for p in problems), problems


def test_a_serialized_proxy_is_checked_the_same_way():
    record = profile(proxy=Proxy(server="http://h:1")).to_dict()
    record["proxy"]["username"] = []

    problems = write_contract.profile_problems(record)

    assert any("proxy.username" in p for p in problems), problems


def test_the_dashboard_payload_keeps_its_own_flat_names():
    """The shape the API already had a schema for. It must keep working, and
    keep naming the field the dashboard shows."""
    problems = write_contract.profile_problems(
        {"name": "n", "cores": 10 ** 400, "seed": float("inf")})

    assert any("'cores'" in p and "between" in p for p in problems), problems
    assert any("'seed'" in p and "finite" in p for p in problems), problems


def test_a_dashboard_proxy_is_checked_under_its_own_key_names():
    """`host`/`user`/`pass` here, `server`/`username`/`password` on the engine
    object - one table covering both, because two would be the drift again."""
    problems = write_contract.profile_problems(
        {"name": "n", "proxy": {"type": "HTTP", "host": 123, "user": [],
                                "port": "8080"}})

    assert any("proxy.host" in p for p in problems), problems
    assert any("proxy.user" in p for p in problems), problems
    assert not any("port" in p for p in problems), \
        "a port the vendor exported as text was refused"


def test_a_healthy_engine_profile_has_nothing_said_about_it():
    """The control for all three shapes at once."""
    p = profile(proxy=Proxy(server="http://h:1", username="u", country="DE"))

    assert write_contract.profile_problems(p) == []
    assert write_contract.profile_problems(p.to_dict()) == []


def test_the_bounds_are_not_duplicated_per_shape():
    """Whatever the mapping looks like, both vocabularies must resolve to the
    one range table - a second copy is how `cores` and `hardware_concurrency`
    came to disagree in the first place."""
    low, high = write_contract.NUMBER_RANGES["cores"]

    flat = write_contract.profile_problems({"name": "n", "cores": high + 1})
    nested = write_contract.profile_problems(
        profile(fingerprint=replace(FP, hardware_concurrency=high + 1)))

    assert f"between {low} and {high}" in flat[0]
    assert f"between {low} and {high}" in nested[0]


# ---------------------------------------------------------------------------
# 4. The real write paths: batches, the CLI, and zero partial writes
# ---------------------------------------------------------------------------

def test_a_batch_with_a_bad_second_entry_writes_nothing(tmp_path):
    """A refusal after the first entry is the one that matters: a loop that
    saved as it went leaves the good entries on disk, and the obvious retry
    then duplicates them."""
    from numera.repo import repo

    store = ProfileStore(tmp_path)
    bad = profile(name="second", fingerprint=replace(FP, device_memory=-5))

    with pytest.raises(write_contract.WriteRefused, match=r"profiles\[1\]"):
        repo(store).save_many([profile(name="first"), bad, profile(name="third")])

    assert store.list() == [], "part of a refused batch landed anyway"
    assert engine_files(tmp_path) == []


def test_the_cli_import_door_refuses_the_same_nested_value_the_api_does(tmp_path):
    """The two-doors regression, at the nested level this time.

    `profile_from_export` tolerates a hardware count up to 4096 because that is
    where one stops being a hardware count; the product's own bound for `cores`
    is 512. The API import route already refuses 2000 by name and says the
    bound. `numera import-from` reaches `repo.save_many` without that route, so
    the same file wrote a record every later save refuses - a profile that can
    be opened and never saved again.
    """
    from numera import importers

    export = tmp_path / "vendor.json"
    export.write_text(json.dumps([
        {"name": "ok", "navigator": {"hardwareConcurrency": 2000}}]),
        encoding="utf-8")
    store = ProfileStore(tmp_path / "root")

    with pytest.raises(write_contract.WriteRefused, match="512"):
        importers.import_file(export, store)

    assert store.list() == [], "a 2000-core record reached the engine store"


def test_a_good_vendor_export_still_imports_from_the_cli(tmp_path):
    """The guard must not cost the feature it guards."""
    from numera import importers

    export = tmp_path / "vendor.json"
    export.write_text(json.dumps([{
        "name": "from-gologin", "os": "win", "userAgent": "Mozilla/5.0",
        "resolution": "1920x1080"}]), encoding="utf-8")
    store = ProfileStore(tmp_path / "root")

    importers.import_file(export, store)

    assert [p.name for p in store.list()] == ["from-gologin"]


def test_an_edit_that_touches_nothing_numeric_still_saves(tmp_path):
    """A stored profile is re-read, renamed and written back. Every nested
    number rides along untouched, so a contract that mis-read one of them
    would make ordinary edits impossible rather than refusing bad input."""
    from numera.repo import repo

    store = ProfileStore(tmp_path)
    saved = store.save(profile(name="before",
                               proxy=Proxy(server="http://h:1", password="p")))

    def rename(p):
        p.name = "after"
        return p

    updated = repo(store).edit(saved.id, rename)

    assert updated.name == "after"
    assert store.get(saved.id).fingerprint.seed == FP.seed


# ---------------------------------------------------------------------------
# 5. Reading back a record that has no name at all
# ---------------------------------------------------------------------------

def test_a_record_with_no_name_key_gets_one_to_render(tmp_path):
    """The repair only helped a name that was *present* and wrong: it walked
    `record.items()`, and an absent key has no item. `p.name.toLowerCase()`
    throws on `undefined` exactly as it does on a number."""
    view = write_contract.readable({"id": "x", "tags": ["a"]})

    assert view["name"] == "", "an older record came back with no name at all"
    assert isinstance(view["name"], str)


def test_supplying_a_missing_name_does_not_touch_the_stored_record():
    record = {"id": "x", "tags": ["a"]}
    before = json.dumps(record, sort_keys=True)

    write_contract.readable(record)

    assert json.dumps(record, sort_keys=True) == before


def test_a_healthy_record_is_still_handed_back_by_identity():
    """No copy, no churn on the common path - and the repair above must not
    quietly turn every read into one."""
    record = {"id": "x", "name": "alpha", "tags": ["a"]}

    assert write_contract.readable(record) is record


def test_only_a_field_this_vocabulary_owns_is_supplied():
    """`readable` is called with two field lists. A shape that does not have a
    `name` must not grow one."""
    view = write_contract.readable({"id": "x"}, text_fields=("notes",),
                                   list_fields=())

    assert "name" not in view


# ---------------------------------------------------------------------------
# 6. The same two halves at the API boundary
# ---------------------------------------------------------------------------
#
# One app for the whole section. Every case below either asserts that *no new*
# record landed, or asserts on a record it planted itself by id, so a shared
# store proves the same thing a fresh app per case would - at one app instead
# of a dozen.

#: A record an older build left on disk: no `name` key at all. Planted as raw
#: bytes because no current write path can produce it any more, which is the
#: whole reason the read view has to cope with it.
LEGACY = [
    ("nameless", {}),
    ("numeric", {"name": 123}),
    ("nulled", {"name": None}),
]


@pytest.fixture(scope="module")
def api(tmp_path_factory):
    """One TestClient, plus the legacy records planted underneath it."""
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    root = tmp_path_factory.mktemp("api")
    dash = root / "dashboard"
    dash.mkdir(parents=True, exist_ok=True)
    for pid, extra in LEGACY:
        (dash / f"{pid}.json").write_text(
            json.dumps({"id": pid, "_rev": 1, **extra}), encoding="utf-8")
    client = TestClient(create_app(data_dir=str(root), start_scheduler=False))
    client.numera_root = root
    return client


def dash_ids(api):
    return {f.stem for f in (api.numera_root / "dashboard").glob("*.json")}


def engine_ids(api):
    d = api.numera_root / "profiles"
    return {f.stem for f in d.glob("*.json")} if d.is_dir() else set()


#: `(route, body, the field the refusal must name)`. The flat vocabulary is
#: what a dashboard client sends, and it must keep getting the same answer now
#: that the nested pass exists beside it.
API_REFUSALS = [
    ("/api/profiles", {"name": "a", "cores": 10 ** 400}, "cores"),
    ("/api/profiles", {"name": "a", "seed": float("inf")}, "seed"),
    ("/api/profiles", {"name": "a", "memory": -5}, "memory"),
    ("/api/profiles", {"name": "a", "tags": [{}]}, "tags"),
]


@pytest.mark.parametrize("route,body,field", API_REFUSALS,
                         ids=[f"{b.get('name')}-{f}" for _, b, f in API_REFUSALS])
def test_the_api_still_refuses_by_field_and_writes_nothing(api, route, body, field):
    before_dash, before_engine = dash_ids(api), engine_ids(api)

    # Raw bytes: `httpx` will not encode `Infinity`, and the server runs
    # `json.loads` on what arrives either way.
    r = api.request("POST", route, content=json.dumps(body),
                    headers={"content-type": "application/json"})

    assert 400 <= r.status_code < 500, f"{body!r}: {r.status_code} {r.text[:200]}"
    assert field in r.text, r.text[:200]
    assert dash_ids(api) == before_dash and engine_ids(api) == before_engine


def test_api_create_and_update_still_work(api):
    """The control for the whole change: the nested pass runs on every write
    now, so an ordinary create and an ordinary edit have to be untouched."""
    r = api.post("/api/profiles", json={"name": "control", "cores": 8,
                                        "proxy": {"type": "HTTP", "host": "1.2.3.4",
                                                  "port": "8080", "user": "u",
                                                  "pass": "p", "country": "DE"}})
    assert r.status_code < 400, r.text[:300]
    pid = r.json()["id"]

    r = api.put(f"/api/profiles/{pid}", json={"name": "control renamed"})
    assert r.status_code < 400, r.text[:300]
    rows = {p["id"]: p for p in api.get("/api/profiles").json()}
    assert rows[pid]["name"] == "control renamed"
    assert rows[pid]["proxy"]["host"] == "1.2.3.4", "the edit dropped the proxy"


def test_a_proxy_the_core_cannot_read_is_refused_over_http(api):
    before = dash_ids(api)

    r = api.request("POST", "/api/profiles",
                    content=json.dumps({"name": "bad proxy",
                                        "proxy": {"host": ["1.2.3.4"]}}),
                    headers={"content-type": "application/json"})

    assert 400 <= r.status_code < 500, f"{r.status_code} {r.text[:200]}"
    assert "proxy" in r.text, r.text[:200]
    assert dash_ids(api) == before


def test_a_bad_later_entry_refuses_the_whole_batch_over_http(api):
    """Zero partial writes, with the bad entry second - the position a loop
    that saves as it goes gets wrong."""
    before = dash_ids(api)

    r = api.request("POST", "/api/profiles/batch",
                    content=json.dumps({"profiles": [
                        {"name": "batch one"},
                        {"name": "batch two", "seed": float("inf")},
                        {"name": "batch three"}]}),
                    headers={"content-type": "application/json"})

    assert 400 <= r.status_code < 500, f"{r.status_code} {r.text[:200]}"
    assert dash_ids(api) == before, "part of a refused batch landed anyway"
    assert not {"batch one", "batch three"} & {
        p["name"] for p in api.get("/api/profiles").json()}


def test_every_row_of_the_listing_has_a_name_the_dashboard_can_render(api):
    """`App.jsx` calls `p.name.toLowerCase()` on every row, and its search box
    calls it again on every keystroke. Missing, numeric and null all reach it,
    and all three used to throw - the missing one still did after the first
    repair, because it walked the keys the record had."""
    rows = {p["id"]: p for p in api.get("/api/profiles").json()}

    for pid, _ in LEGACY:
        assert pid in rows, f"{pid} vanished from the listing instead of rendering"
    for pid, row in rows.items():
        assert isinstance(row["name"], str), \
            f"{pid} carries a {type(row['name']).__name__} name"
    assert rows["numeric"]["name"] == "123"
    assert rows["nameless"]["name"] == ""


def test_the_health_page_renders_the_same_legacy_names(api):
    """The other reader over the same records. It took `name` straight off the
    stored record, so the page that exists to show you a damaged profile was
    the one handing the grid the value that breaks a row."""
    rows = {r["id"]: r for r in api.get("/api/profiles/health").json()["profiles"]}

    for pid, _ in LEGACY:
        assert pid in rows, f"{pid} is missing from the health overview"
        assert isinstance(rows[pid]["name"], str), \
            f"{pid} carries a {type(rows[pid]['name']).__name__} name"
    assert rows["numeric"]["name"] == "123"


def test_a_legacy_record_keeps_its_original_bytes(api):
    """Rendered, not repaired: a real fix must still be able to see what was
    on disk, and a read must never rewrite a file."""
    api.get("/api/profiles")
    api.get("/api/profiles/health")

    for pid, extra in LEGACY:
        stored = json.loads(
            (api.numera_root / "dashboard" / f"{pid}.json").read_text("utf-8"))
        assert stored == {"id": pid, "_rev": 1, **extra}, \
            f"reading {pid} rewrote it"
