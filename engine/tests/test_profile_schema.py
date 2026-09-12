"""One schema for a writable profile, and a mirror that is never silently absent.

Two halves of the same defect (N04).

**The schema had holes.** `FIELD_TYPES` named twenty-six of the forty-odd fields
the converter exposes and checked none of their elements, so `tags: [{}]` and
`fontList: 17` were both stored happily. A non-string tag is not cosmetic: the
profile list renders `p.tags` and searches it, and a dict there takes the page
down for every profile, not just the poisoned one. Nor was a number checked for
finiteness - `cores: NaN` passed `isinstance(x, (int, float))` and is not even
JSON.

**And the write path swallowed the consequence.** `_validate_profile` wrapped
`to_engine_profile` in a bare `except Exception: complete = {}`, and
`DashboardStore._twin` did the same and returned `None`. So a value the engine
could not model produced a dashboard record with *no engine copy at all*: the
API answered 200, the GUI showed the profile, and `numera list` had never heard
of it. The two stores are meant to move together or not at all.

The policy these tests pin down:

* every field the converter exposes is typed, including the element type of
  each list and the finiteness and range of each number;
* a malformed value is refused with an actionable 4xx on every write route -
  create, PUT, batch, bulk and import - and leaves nothing at all on disk;
* a conversion that fails is refused *before* the write rather than swallowed
  after it, so a stored dashboard profile always has its engine twin;
* a record that was already corrupt on disk is handled explicitly: the views
  that merely read it do not crash, and a write that would carry the bad value
  onward says which field it is.
"""
import json
import math

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera.server import create_app                            # noqa: E402
from numera.store import ProfileStore                           # noqa: E402


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def send(c, method, url, payload):
    """Send a body the way a raw HTTP client would.

    `httpx` refuses to *encode* `NaN` and `Infinity`, so `json=` cannot express
    the values this schema exists to refuse. `json.loads` - which is what the
    server runs on the received bytes - accepts all three happily, so handing
    over the text directly is the case that actually reaches the route.
    """
    return c.request(method, url, content=json.dumps(payload),
                     headers={"content-type": "application/json"})


def stored_ids(tmp_path) -> set:
    """Every profile id with a dashboard record on disk."""
    return {f.stem for f in (tmp_path / "dashboard").glob("*.json")}


def engine_ids(tmp_path) -> set:
    """Every profile id with an engine record on disk."""
    d = tmp_path / "profiles"
    return {f.stem for f in d.glob("*.json")} if d.is_dir() else set()


def dash_raw(tmp_path, pid) -> dict:
    return json.loads((tmp_path / "dashboard" / f"{pid}.json").read_text("utf-8"))


# Values the dashboard has no way to render and the engine has no way to model.
# Each is `(field, value)` and each was accepted before this schema existed.
MALFORMED = [
    ("tags", [{}]),                     # p.tags.map(...) over an object
    ("tags", ["ok", 17]),               # one bad element is enough
    ("tags", "work"),                   # a string is iterable, and wrong
    ("ipBased", [{"a": 1}]),
    ("clearOnStart", [None]),
    ("switchedOff", [["nested"]]),
    ("fontList", 17),                   # split on newlines at launch
    ("fontList", {"Arial": True}),
    ("cores", float("nan")),            # not JSON, and int(nan) raises
    ("cores", float("inf")),
    ("cores", -4),                      # a browser with minus four cores
    ("memory", float("-inf")),
    ("seed", float("nan")),
    ("mediaVideo", float("inf")),
    ("devicePixelRatio", "big"),
    ("devicePixelRatio", 0),            # a zero ratio scales everything away
    ("maxTouchPoints", []),
    ("canvas", 5),
    ("webglRenderer", {}),
    ("osVersion", []),
    ("deviceModel", 12),
    ("browser", ["Chrome"]),
    ("proxy", "1.2.3.4:8080"),          # the shape is an object
]


@pytest.mark.parametrize("field,value", MALFORMED)
def test_a_malformed_value_is_refused_on_create(c, tmp_path, field, value):
    """`POST /api/profiles` is the route that mints the record, so a value it
    accepts is a value every later read has to survive."""
    r = send(c, "POST", "/api/profiles", {"name": "poisoned", field: value})
    assert 400 <= r.status_code < 500, \
        f"{field}={value!r} was accepted: {r.status_code} {r.text[:200]}"
    assert field in r.text, f"the refusal does not say which field: {r.text[:200]}"
    assert not stored_ids(tmp_path), "a refused create still wrote a profile"
    assert not engine_ids(tmp_path), "a refused create still wrote an engine copy"


@pytest.mark.parametrize("field,value", MALFORMED)
def test_a_malformed_value_is_refused_on_update(c, tmp_path, field, value):
    """A PUT is a patch onto stored state, and the merged record is what gets
    written - so the check has to survive the merge."""
    made = c.post("/api/profiles", json={"name": "clean"}).json()
    before = dash_raw(tmp_path, made["id"])

    r = send(c, "PUT", f"/api/profiles/{made['id']}", {field: value})
    assert 400 <= r.status_code < 500, \
        f"{field}={value!r} was accepted: {r.status_code} {r.text[:200]}"
    assert dash_raw(tmp_path, made["id"]) == before, \
        "a refused update changed the stored profile anyway"


@pytest.mark.parametrize("field,value", MALFORMED)
def test_a_malformed_entry_refuses_the_whole_batch(c, tmp_path, field, value):
    """Half a batch on disk is worse than none: nothing tells the user which
    half landed, and the obvious retry duplicates the entries that did."""
    r = send(c, "POST", "/api/profiles/batch", {"profiles": [
        {"name": "first"}, {"name": "second", field: value}, {"name": "third"}]})
    assert 400 <= r.status_code < 500, \
        f"{field}={value!r} was accepted: {r.status_code} {r.text[:200]}"
    assert not stored_ids(tmp_path), "a refused batch wrote part of itself"
    assert not engine_ids(tmp_path), "a refused batch wrote part of itself"


@pytest.mark.parametrize("field,value", MALFORMED)
def test_a_malformed_bulk_patch_reaches_no_profile(c, tmp_path, field, value):
    """One patch, many profiles: an unchecked field is written to every one of
    them before anything notices."""
    ids = [c.post("/api/profiles", json={"name": n}).json()["id"]
           for n in ("a", "b", "c")]
    before = {pid: dash_raw(tmp_path, pid) for pid in ids}

    r = send(c, "POST", "/api/profiles/bulk", {"ids": ids, "patch": {field: value}})
    assert 400 <= r.status_code < 500, \
        f"{field}={value!r} was accepted: {r.status_code} {r.text[:200]}"
    for pid in ids:
        assert dash_raw(tmp_path, pid) == before[pid], \
            f"{pid} was edited by a request that was refused"


def test_a_non_string_tag_never_reaches_the_rendered_listing(c, tmp_path):
    """The headline symptom. `App.jsx` lowercases and searches every tag, so
    one object in one profile's tag list blanks the whole page."""
    c.post("/api/profiles", json={"name": "poisoned", "tags": [{}, 17, None]})
    for p in c.get("/api/profiles").json():
        for tag in p.get("tags", []):
            assert isinstance(tag, str), f"{p['id']} carries a {type(tag).__name__} tag"


def test_no_non_finite_number_is_ever_stored(c, tmp_path):
    """`NaN` and `Infinity` are not JSON. Python writes them and reads them
    back, so they survive a round trip here and break every other consumer."""
    for field in ("cores", "memory", "seed", "mediaVideo", "mediaAudio"):
        for bad in (float("nan"), float("inf"), float("-inf")):
            send(c, "POST", "/api/profiles", {"name": f"{field}-{bad}", field: bad})
    for pid in stored_ids(tmp_path):
        for key, value in dash_raw(tmp_path, pid).items():
            if isinstance(value, float):
                assert math.isfinite(value), f"{pid}.{key} is {value}"


# --- the mirror is never silently missing -----------------------------------

def test_a_profile_the_engine_cannot_model_is_refused_not_half_written(c, tmp_path):
    """The swallowed half of the defect.

    `fontList: 17` made `to_engine_profile` raise; `_validate_profile` caught
    it and carried on, and `_twin` caught it again and returned `None`. The
    result was a dashboard record with no engine copy - a profile the GUI
    showed and `numera list` had never heard of.
    """
    r = send(c, "POST", "/api/profiles", {"name": "odd", "fontList": 17})
    assert 400 <= r.status_code < 500, \
        f"an unconvertible profile was stored: {r.status_code} {r.text[:200]}"
    assert not stored_ids(tmp_path) and not engine_ids(tmp_path)


def test_a_conversion_failure_refuses_the_write_rather_than_dropping_the_twin(
        tmp_path, monkeypatch):
    """Straight at the store, because that is the choke point every one of the
    five write routes goes through - including the import route, which builds
    its records itself."""
    from numera.server import stores as stores_mod
    from numera.server.stores import DashboardStore

    dash = DashboardStore(tmp_path, engine_store=ProfileStore(tmp_path))
    monkeypatch.setattr(stores_mod, "to_engine_profile",
                        lambda d: (_ for _ in ()).throw(ValueError("unconvertible")))
    with pytest.raises(Exception):
        dash.save({"name": "odd"})
    assert not stored_ids(tmp_path), \
        "the dashboard copy was written without its engine twin"


def test_an_import_that_cannot_be_mirrored_is_refused_as_a_client_error(
        c, tmp_path, monkeypatch):
    """An import is all-or-nothing too, and a failed mirror must answer 4xx
    rather than a traceback."""
    from numera.server import stores as stores_mod

    monkeypatch.setattr(stores_mod, "to_engine_profile",
                        lambda d: (_ for _ in ()).throw(ValueError("unconvertible")))
    payload = json.dumps([{"name": "vendor-one", "os": "Windows"},
                          {"name": "vendor-two", "os": "Windows"}])
    r = c.post("/api/profiles/import", json={"text": payload})
    assert 400 <= r.status_code < 500, \
        f"an unmirrorable import answered {r.status_code}: {r.text[:200]}"
    assert not stored_ids(tmp_path), "a refused import wrote part of itself"


@pytest.mark.parametrize("route,body", [
    ("/api/profiles", {"name": "single"}),
    ("/api/profiles/batch", {"profiles": [{"name": "batched"}]}),
])
def test_every_stored_profile_has_its_engine_twin(c, tmp_path, route, body):
    """Two-store equality: the point of refusing a failed conversion."""
    r = c.post(route, json=body)
    assert r.status_code < 400, r.text
    assert stored_ids(tmp_path), "nothing was created"
    assert stored_ids(tmp_path) == engine_ids(tmp_path), \
        "the two stores disagree about which profiles exist"
    for pid in stored_ids(tmp_path):
        engine = json.loads((tmp_path / "profiles" / f"{pid}.json").read_text("utf-8"))
        assert engine["name"] == dash_raw(tmp_path, pid)["name"]


# --- what must keep working -------------------------------------------------

def test_a_minimal_payload_is_still_a_valid_profile(c, tmp_path):
    """A schema that refuses `{"name": "x"}` would be worse than no schema."""
    r = c.post("/api/profiles", json={"name": "minimal"})
    assert r.status_code < 400, r.text
    body = r.json()
    assert isinstance(body["tags"], list) and isinstance(body["startupUrls"], str)
    assert stored_ids(tmp_path) == engine_ids(tmp_path)


def test_a_fully_populated_profile_round_trips(c, tmp_path):
    """Every field the dashboard actually sends, with a plausible value."""
    full = {
        "name": "full", "folder": "Ads", "os": "macOS", "engine": "camoufox",
        "browser": "Chrome", "browserVersion": "131.0.0.0",
        "userAgent": "Mozilla/5.0 (Macintosh)", "screen": "2560x1440",
        "cores": 12, "memory": 32, "webglVendor": "Apple",
        "webglRenderer": "Apple M2", "canvas": "Noise", "webrtc": "Altered",
        "audio": "Noise", "fonts": "Masked", "clientRects": "Noise",
        "webglImage": "Noise", "mediaDevices": "Masked", "locale": "en-GB",
        "timezone": "Europe/London", "geo": "Prompt", "mediaVideo": 2,
        "mediaAudio": 2, "mediaSpeakers": 1, "fontList": "Arial\nHelvetica",
        "osVersion": "14.0.0", "deviceModel": "", "maxTouchPoints": 0,
        "devicePixelRatio": 2.0, "seed": 4242, "dnt": True,
        "proxy": {"type": "HTTP", "host": "127.0.0.1", "port": "8080",
                  "user": "u", "pass": "p", "country": "GB"},
        "notes": "hello", "tags": ["work", "ads"],
        "startupUrls": "https://example.test/", "extensions": "",
        "launchArgs": "--mute-audio", "bookmarks": "Home | https://example.test/",
        "dns": "1.1.1.1", "ipBased": ["timezone"], "clearOnStart": ["cookies"],
        "switchedOff": ["googleServices"],
    }
    r = c.post("/api/profiles", json=full)
    assert r.status_code < 400, r.text
    saved = r.json()
    for key in ("name", "folder", "os", "engine", "screen", "cores", "seed",
                "tags", "dns", "ipBased", "clearOnStart", "switchedOff"):
        assert saved[key] == full[key], f"{key} did not survive the write"
    assert stored_ids(tmp_path) == engine_ids(tmp_path)


def test_an_empty_list_and_an_empty_string_are_still_valid(c, tmp_path):
    r = c.post("/api/profiles", json={"name": "empties", "tags": [],
                                      "notes": "", "fontList": "",
                                      "ipBased": [], "startupUrls": ""})
    assert r.status_code < 400, r.text


def test_an_explicit_null_is_still_a_missing_field_not_a_bad_one(c, tmp_path):
    """The dashboard sends `proxy: null` for a profile with no proxy, and
    `null` for fields it has not filled in. Refusing those would refuse its own
    output."""
    r = c.post("/api/profiles", json={"name": "nulls", "proxy": None,
                                      "notes": None, "tags": None})
    assert r.status_code < 400, r.text


# --- records that were already corrupt on disk ------------------------------

def corrupt_on_disk(c, tmp_path) -> str:
    """A profile stored by an older version, with values today's schema refuses."""
    pid = c.post("/api/profiles", json={"name": "legacy"}).json()["id"]
    f = tmp_path / "dashboard" / f"{pid}.json"
    rec = json.loads(f.read_text("utf-8"))
    rec["fontList"] = 17
    rec["tags"] = [{}, "work"]
    rec["cores"] = float("inf")
    f.write_text(json.dumps(rec), encoding="utf-8")
    return pid


# `(route, how many profiles its body should account for)`. The count matters
# as much as the status: a view that survives by dropping the corrupt record
# has hidden a profile the user owns.
COUNTED_VIEWS = [
    ("/api/profiles", lambda body: len(body)),
    ("/api/profiles/health", lambda body: len(body["profiles"])),
    ("/api/profiles/collisions", lambda body: body["total"]),
]


@pytest.mark.parametrize("path,count", COUNTED_VIEWS,
                         ids=[p for p, _ in COUNTED_VIEWS])
def test_a_corrupt_record_does_not_take_an_unrelated_view_down(c, tmp_path,
                                                               path, count):
    """`profiles_health` and `profiles_collisions` convert every profile, and
    one bad record raised straight out of the route - so a single poisoned
    profile made the Health page unreachable for all of them. `GET
    /api/profiles` had a second way to fail: `Infinity` is not JSON, so the
    response could not even be serialised."""
    corrupt_on_disk(c, tmp_path)
    c.post("/api/profiles", json={"name": "innocent"})
    r = c.get(path)
    assert r.status_code < 500, f"{path} raised on a corrupt record: {r.text[:200]}"
    assert count(r.json()) == 2, \
        f"{path} lost a profile rather than raising on it: {r.text[:200]}"


def test_writing_a_corrupt_record_back_says_which_field_is_wrong(c, tmp_path):
    """Explicit handling, not a silent repair: the stored value is not quietly
    deleted and not quietly carried forward."""
    pid = corrupt_on_disk(c, tmp_path)
    r = c.put(f"/api/profiles/{pid}", json={"name": "renamed"})
    assert 400 <= r.status_code < 500, \
        f"a corrupt record was rewritten unchanged: {r.status_code}"
    assert "fontList" in r.text or "tags" in r.text or "cores" in r.text, r.text[:200]
    assert dash_raw(tmp_path, pid)["fontList"] == 17, \
        "the refused write deleted the stored value anyway"


def test_a_corrupt_record_can_be_repaired_by_sending_good_values(c, tmp_path):
    """The way out. A refusal that cannot be cleared is a dead profile."""
    pid = corrupt_on_disk(c, tmp_path)
    r = c.put(f"/api/profiles/{pid}", json={"name": "repaired", "fontList": "Arial",
                                            "tags": ["work"], "cores": 8})
    assert r.status_code < 400, r.text
    assert dash_raw(tmp_path, pid)["cores"] == 8
    assert pid in engine_ids(tmp_path), "the repaired profile got no engine twin"


# --- protections this schema must not have loosened -------------------------

def test_the_revision_conflict_still_refuses_a_stale_write(c, tmp_path):
    """`_rev` optimistic concurrency: a client holding an old copy is refused
    rather than silently erasing the other writer's fields."""
    made = c.post("/api/profiles", json={"name": "a"}).json()
    c.put(f"/api/profiles/{made['id']}", json={"name": "b"})
    r = c.put(f"/api/profiles/{made['id']}", json={**made, "name": "c"})
    assert r.status_code == 409, r.text
    assert r.json().get("kind") == "StaleProfile"


def test_an_unrelated_edit_still_preserves_the_proxy_password(c, tmp_path):
    """PRESERVE. The body says nothing about the secret, so the stored one
    stands."""
    px = {"type": "HTTP", "host": "127.0.0.1", "port": "8080",
          "user": "u", "pass": "secret"}
    made = c.post("/api/profiles", json={"name": "a", "proxy": px}).json()
    c.put(f"/api/profiles/{made['id']}", json={"name": "renamed"})
    assert c.get("/api/profiles").json()[0]["proxy"]["pass"] == "secret"


def test_an_empty_password_still_clears_it(c, tmp_path):
    """CLEAR."""
    px = {"type": "HTTP", "host": "127.0.0.1", "port": "8080", "pass": "secret"}
    made = c.post("/api/profiles", json={"name": "a", "proxy": px}).json()
    c.put(f"/api/profiles/{made['id']}", json={"proxy": {**px, "pass": ""}})
    assert c.get("/api/profiles").json()[0]["proxy"]["pass"] == ""


def test_a_new_password_still_replaces_it(c, tmp_path):
    """REPLACE."""
    px = {"type": "HTTP", "host": "127.0.0.1", "port": "8080", "pass": "old"}
    made = c.post("/api/profiles", json={"name": "a", "proxy": px}).json()
    c.put(f"/api/profiles/{made['id']}", json={"proxy": {**px, "pass": "new"}})
    assert c.get("/api/profiles").json()[0]["proxy"]["pass"] == "new"


def test_a_null_proxy_still_removes_it(c, tmp_path):
    """REMOVE. `proxy: null` is a decision, and distinct from an absent key."""
    px = {"type": "HTTP", "host": "127.0.0.1", "port": "8080", "pass": "old"}
    made = c.post("/api/profiles", json={"name": "a", "proxy": px}).json()
    r = c.put(f"/api/profiles/{made['id']}", json={"proxy": None})
    assert r.status_code < 400, r.text
    assert not c.get("/api/profiles").json()[0].get("proxy")


def test_the_schema_covers_every_writable_field():
    """The hole that made this defect possible, closed mechanically.

    `client_fields()` is derived from the converter's own output, so a field
    added to the dashboard shape is accepted the moment it exists. It was only
    *typed* if somebody remembered to add it to `FIELD_TYPES`, and seventeen of
    them - `fontList`, `devicePixelRatio`, every per-signal mode - had not
    been. This fails when the next one is added untyped.
    """
    from numera.server.routes_profiles import (CLIENT_MAY_NOT_SET, FIELD_TYPES,
                                               client_fields)

    untyped = client_fields() - set(FIELD_TYPES) - CLIENT_MAY_NOT_SET
    assert not untyped, f"writable but unchecked: {sorted(untyped)}"


def test_an_unknown_field_is_still_refused(c, tmp_path):
    r = c.post("/api/profiles", json={"name": "a", "nonsuch": 1})
    assert r.status_code == 422 and "nonsuch" in r.text


def test_an_unknown_engine_is_still_refused(c, tmp_path):
    r = c.post("/api/profiles", json={"name": "a", "engine": "bogus"})
    assert r.status_code == 422 and "bogus" in r.text
