"""The three doors the write contract still did not reach (N04, recheck).

`FIELD_TYPES` and `check_fields` are the one schema every write is supposed to
pass, and `test_profile_schema.py` holds that line for create, PUT, batch, bulk
and the native import. Three ways in were still open, and all three end the same
way - a value the dashboard cannot render, or a traceback where a field-level
4xx belongs.

**A JSON integer too large for a float was not refused, it raised.**
`math.isfinite` converts its argument to a float first, so `math.isfinite(10 **
400)` is an `OverflowError`, not `False`. `_value_problems` called it for every
key in `NUMBER_RANGES`, so `{"cores": 10 ** 400}` - a perfectly ordinary JSON
integer - blew the validator up instead of being refused by name. An `int` in
Python is exact and unbounded; it can be range-compared without ever becoming a
float.

**Bulk's two tag operations went through the preflight untyped.** `bulk_update`
passes `extra=("addTags", "removeTags")`, which tells `check_fields` to allow
those keys - and allowing a key is not the same as checking it. Only their
list-ness was checked afterwards, never their elements, so `addTags: [{}]`
raised `unhashable type: 'dict'` inside the set that merges the tags, and
`["a", 1]` could raise inside the sort.

**The vendor import branch wrote without the contract at all.** It builds its
own records - `to_dashboard_profile(profile_from_export(exp))` - and called
`dash.save_many` with no `check_fields` anywhere. A vendor payload `{"name":
123}` kept a numeric name all the way to disk; `readable()` does not repair
`name` (it repairs non-finite numbers and list fields), and the profile list
calls `p.name.toLowerCase()`, which throws and blanks the page for every
profile, not just that one. Three other shapes did not even get that far and
raised out of the importer as a 500.

Being tolerant about which *name* a vendor gives a value is this importer's
whole reason to exist and is not what was wrong. Being tolerant about a value
having a type nothing can render is. So the name tolerance is pinned down here
too, beside the refusals, because a fix that quietly narrowed it would pass
every other test in this file.
"""
import json

import pytest

pytest.importorskip("fastapi")

from fastapi.testclient import TestClient                       # noqa: E402

from numera.importers import NATIVE_KEY, NATIVE_VERSION         # noqa: E402
from numera.server import create_app                            # noqa: E402


@pytest.fixture
def c(tmp_path):
    return TestClient(create_app(data_dir=str(tmp_path), start_scheduler=False))


def send(c, method, url, payload):
    """Send a body the way a raw HTTP client would.

    `httpx` will not encode every value this contract has to refuse, and the
    server runs `json.loads` on the received bytes either way, so the text goes
    over directly - the same reason `test_profile_schema.send` exists.
    """
    return c.request(method, url, content=json.dumps(payload),
                     headers={"content-type": "application/json"})


def stored_ids(tmp_path) -> set:
    return {f.stem for f in (tmp_path / "dashboard").glob("*.json")}


def engine_ids(tmp_path) -> set:
    d = tmp_path / "profiles"
    return {f.stem for f in d.glob("*.json")} if d.is_dir() else set()


def dash_raw(tmp_path, pid) -> dict:
    return json.loads((tmp_path / "dashboard" / f"{pid}.json").read_text("utf-8"))


def importing(payload) -> dict:
    """A vendor file body for `POST /api/profiles/import`."""
    return {"text": json.dumps(payload)}


def native(profiles) -> dict:
    """A versioned Numera export body, the other branch of the same route."""
    return {"text": json.dumps({NATIVE_KEY: NATIVE_VERSION, "profiles": profiles})}


# --- (a) an integer past the float range is a refusal, not an exception -----

#: Every bounded number, with an integer no float can hold. Each is valid JSON
#: and each reached `math.isfinite` before this was fixed.
HUGE = [(field, sign * 10 ** 400)
        for field in ("cores", "memory", "mediaVideo", "mediaAudio",
                      "mediaSpeakers", "maxTouchPoints", "devicePixelRatio",
                      "seed")
        for sign in (1, -1)]


@pytest.mark.parametrize("field,value", HUGE,
                         ids=[f"{f}{'' if v > 0 else '-neg'}" for f, v in HUGE])
def test_an_integer_too_large_for_a_float_is_refused_by_field(c, tmp_path,
                                                              field, value):
    """`math.isfinite(10 ** 400)` raises `OverflowError: int too large to
    convert to float`. The value is a plain JSON integer, so the answer owed to
    it is the same 4xx naming the field that `cores: -4` already gets."""
    r = send(c, "POST", "/api/profiles", {"name": "huge", field: value})
    assert 400 <= r.status_code < 500, \
        f"{field}=10**400 was accepted: {r.status_code} {r.text[:200]}"
    assert field in r.text, f"the refusal does not say which field: {r.text[:200]}"
    assert not stored_ids(tmp_path) and not engine_ids(tmp_path)


def test_the_refusal_does_not_echo_the_whole_number_back(c, tmp_path):
    """The message quotes the value, and the value is the client's. Four
    thousand digits is the largest one CPython will decode into an `int` at
    all - `sys.set_int_max_str_digits` stops the rest at the parser, which is
    the other half of why the message can be bounded cheaply."""
    r = send(c, "POST", "/api/profiles", {"name": "huge", "cores": 10 ** 4000})
    assert 400 <= r.status_code < 500, r.text[:200]
    assert "cores" in r.text
    assert len(r.text) < 500, f"the refusal echoed {len(r.text)} bytes back"


def test_an_integer_past_the_decoder_limit_is_a_bad_request_not_a_crash(c, tmp_path):
    """Past CPython's 4300-digit limit the body cannot become an `int` at all,
    so the refusal comes from the JSON parser instead. Asserted because that is
    the reason the check above may assume a value it can print."""
    body = b'{"name": "huge", "cores": 1' + b"0" * 5000 + b"}"
    r = c.request("POST", "/api/profiles", content=body,
                  headers={"content-type": "application/json"})
    assert 400 <= r.status_code < 500, \
        f"a 5001-digit integer was not a client error: {r.status_code} {r.text[:200]}"
    assert not stored_ids(tmp_path)


@pytest.mark.parametrize("route,body", [
    ("/api/profiles/batch", {"profiles": [{"name": "a"}, {"name": "b", "cores": 10 ** 400}]}),
    ("/api/profiles/bulk", {"ids": [], "patch": {"cores": 10 ** 400}}),
])
def test_a_huge_integer_is_refused_on_every_write_route(c, tmp_path, route, body):
    """One validator, five routes: the same value must get the same answer
    wherever it arrives."""
    r = send(c, "POST", route, body)
    assert 400 <= r.status_code < 500, \
        f"{route} accepted 10**400: {r.status_code} {r.text[:200]}"
    assert "cores" in r.text, r.text[:200]
    assert not stored_ids(tmp_path)


def test_a_huge_integer_in_a_native_import_is_refused_too(c, tmp_path):
    r = send(c, "POST", "/api/profiles/import",
             {"text": json.dumps({NATIVE_KEY: NATIVE_VERSION,
                                  "profiles": [{"name": "huge", "seed": 10 ** 400}]})})
    assert 400 <= r.status_code < 500, \
        f"a native import accepted 10**400: {r.status_code} {r.text[:200]}"
    assert "seed" in r.text, r.text[:200]
    assert not stored_ids(tmp_path)


def test_an_integer_inside_the_range_is_still_accepted(c, tmp_path):
    """The bounds exist to refuse a value no device has, not to refuse ints."""
    r = c.post("/api/profiles", json={"name": "fine", "cores": 16, "memory": 32,
                                      "seed": 2 ** 52, "maxTouchPoints": 0})
    assert r.status_code < 400, r.text
    assert dash_raw(tmp_path, r.json()["id"])["seed"] == 2 ** 52


# --- (b) bulk's tag operations are checked, not merely allowed --------------

#: Element shapes that broke the merge rather than being refused by it. A dict
#: is unhashable and raises inside the set; a mixed list can raise inside the
#: sort that follows it.
BAD_TAG_LISTS = [[{}], [{"name": "work"}], ["ok", 17], [None], [["nested"]],
                 ["ok", None]]


@pytest.mark.parametrize("key", ["addTags", "removeTags"])
@pytest.mark.parametrize("value", BAD_TAG_LISTS, ids=lambda v: repr(v)[:24])
def test_a_non_string_tag_operand_is_refused_not_raised(c, tmp_path, key, value):
    """`extra=` tells the preflight to *allow* these two keys. Allowing a key
    and checking it are different things, and only one of them was happening."""
    ids = [c.post("/api/profiles", json={"name": n}).json()["id"]
           for n in ("a", "b", "c")]
    before = {pid: dash_raw(tmp_path, pid) for pid in ids}

    r = send(c, "POST", "/api/profiles/bulk", {"ids": ids, "patch": {key: value}})
    assert 400 <= r.status_code < 500, \
        f"{key}={value!r} was accepted: {r.status_code} {r.text[:200]}"
    assert key in r.text, f"the refusal does not say which field: {r.text[:200]}"
    for pid in ids:
        assert dash_raw(tmp_path, pid) == before[pid], \
            f"{pid} was edited by a request that was refused"


def test_a_tag_operand_that_is_not_a_list_is_still_refused(c, tmp_path):
    """The check that was already there, kept."""
    pid = c.post("/api/profiles", json={"name": "a"}).json()["id"]
    r = send(c, "POST", "/api/profiles/bulk",
             {"ids": [pid], "patch": {"addTags": "work"}})
    assert 400 <= r.status_code < 500, r.text[:200]
    assert "addTags" in r.text


def test_string_tag_operands_still_add_and_remove(c, tmp_path):
    """What the route is for, and what a stricter preflight must not break."""
    ids = [c.post("/api/profiles", json={"name": n, "tags": ["old"]}).json()["id"]
           for n in ("a", "b")]
    r = c.post("/api/profiles/bulk", json={
        "ids": ids, "patch": {"addTags": ["new"], "removeTags": ["old"]}})
    assert r.status_code < 400, r.text
    for pid in ids:
        assert dash_raw(tmp_path, pid)["tags"] == ["new"]


def test_an_empty_tag_operand_is_not_an_error(c, tmp_path):
    pid = c.post("/api/profiles", json={"name": "a", "tags": ["keep"]}).json()["id"]
    r = c.post("/api/profiles/bulk", json={"ids": [pid], "patch": {"addTags": []}})
    assert r.status_code < 400, r.text
    assert dash_raw(tmp_path, pid)["tags"] == ["keep"]


# --- (c) the vendor import branch holds the same contract ------------------

#: One vendor export per shape that reached disk, or raised, without ever being
#: checked. `(label, export, the field the refusal must name)`.
UNRENDERABLE_EXPORTS = [
    ("numeric name", {"name": 123}, "name"),
    ("list name", {"name": ["a"]}, "name"),
    ("numeric notes", {"name": "ok", "notes": 5}, "notes"),
    ("object timezone", {"name": "ok", "timezone": {"zone": "Europe/Berlin"}},
     "timezone"),
    ("numeric webgl vendor", {"name": "ok", "webgl_vendor": 7}, "webglVendor"),
    ("object in the tag list", {"name": "ok", "tags": [{}, "fine"]}, "tags"),
    ("number in the tag list", {"name": "ok", "tags": ["fine", 17]}, "tags"),
    ("object proxy user",
     {"name": "ok", "proxy": {"type": "http", "host": "1.2.3.4", "port": 8080,
                              "user": {"k": 1}}}, "proxy"),
    ("list proxy country",
     {"name": "ok", "proxy": {"type": "http", "host": "1.2.3.4", "port": 8080,
                              "country": [1]}}, "proxy"),
]


@pytest.mark.parametrize("export,field",
                         [(e, f) for _, e, f in UNRENDERABLE_EXPORTS],
                         ids=[lbl for lbl, _, _ in UNRENDERABLE_EXPORTS])
def test_an_unrenderable_vendor_value_is_refused_by_field(c, tmp_path, export,
                                                          field):
    """The import builds its own records and never ran them past the schema, so
    every one of these was written exactly as the vendor wrote it."""
    r = c.post("/api/profiles/import", json=importing([export]))
    assert 400 <= r.status_code < 500, \
        f"{export!r} was imported: {r.status_code} {r.text[:200]}"
    assert field in r.text, f"the refusal does not say which field: {r.text[:200]}"
    assert not stored_ids(tmp_path), "a refused import wrote a dashboard record"
    assert not engine_ids(tmp_path), "a refused import wrote an engine record"


@pytest.mark.parametrize("export,field",
                         [(e, f) for _, e, f in UNRENDERABLE_EXPORTS],
                         ids=[lbl for lbl, _, _ in UNRENDERABLE_EXPORTS])
def test_a_bad_entry_after_the_first_still_refuses_the_whole_import(
        c, tmp_path, export, field):
    """Position matters: a loop that saved as it went, or that only looked at
    the entry it happened to crash on, leaves the good entries before it on
    disk - and the obvious retry then duplicates them."""
    r = c.post("/api/profiles/import", json=importing(
        [{"name": "first"}, {"name": "second"}, export, {"name": "fourth"}]))
    assert 400 <= r.status_code < 500, \
        f"{export!r} at position 2 was imported: {r.status_code} {r.text[:200]}"
    assert "profiles[2]" in r.text, \
        f"the refusal does not say which entry: {r.text[:200]}"
    assert not stored_ids(tmp_path), "part of a refused import landed anyway"
    assert not engine_ids(tmp_path), "part of a refused import landed anyway"


#: Exports whose value made the importer itself raise, before anything could be
#: checked - a 500 over a file a user opened.
UNREADABLE_EXPORTS = [
    ("tag list that is not a list", {"name": "ok", "tags": 17}),
    ("user agent that is not text", {"name": "ok", "userAgent": 99}),
    ("language that is not text", {"name": "ok", "language": {"code": "en-US"}}),
]


@pytest.mark.parametrize("export",
                         [e for _, e in UNREADABLE_EXPORTS],
                         ids=[lbl for lbl, _ in UNREADABLE_EXPORTS])
def test_a_vendor_value_the_importer_cannot_read_is_never_a_traceback(
        c, tmp_path, export):
    """`_chrome_from_ua` runs a regex over the user agent, the locale table is
    looked up by key, and the tag list is iterated - so a number, an object and
    an integer respectively raised `TypeError` straight out of the route.

    Whether the answer is a fallback or a refusal is the importer's call; a
    traceback is not one of the options, and neither is a stored record the
    dashboard cannot draw.
    """
    r = c.post("/api/profiles/import", json=importing([export]))
    assert r.status_code < 500, \
        f"{export!r} raised out of the route: {r.status_code} {r.text[:300]}"
    if r.status_code < 400:
        assert r.json()["results"][0]["issues"], \
            "a value was quietly replaced with no issue on the profile"
        for pid in stored_ids(tmp_path):
            assert isinstance(dash_raw(tmp_path, pid).get("tags"), list)
    else:
        assert not stored_ids(tmp_path) and not engine_ids(tmp_path)


def test_every_imported_profile_passes_the_same_schema_a_created_one_does(
        c, tmp_path):
    """The contract itself, rather than a list of shapes: whatever the vendor
    path writes, `check_fields` must have nothing to say about it - which is
    the property the five other write routes already have."""
    from numera.server.routes_profiles import CLIENT_MAY_NOT_SET, check_fields

    r = c.post("/api/profiles/import", json=importing([
        {"name": "gologin one", "os": "Windows", "tags": "ads, client",
         "navigator": {"userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) "
                                    "AppleWebKit/537.36 (KHTML, like Gecko) "
                                    "Chrome/128.0.0.0 Safari/537.36",
                       "hardwareConcurrency": "8 cores", "resolution": "1920x1080"},
         "proxy": {"mode": "socks5", "host": "1.2.3.4", "port": 1080,
                   "username": "u", "password": "p", "country": "DE"}},
        {"profile_name": "adspower two", "os_type": "macOS",
         "group_name": "Client work", "description": "second"},
    ]))
    assert r.status_code < 400, r.text[:300]
    assert stored_ids(tmp_path) == engine_ids(tmp_path)
    for pid in stored_ids(tmp_path):
        rec = dash_raw(tmp_path, pid)
        problems = check_fields(
            {k: v for k, v in rec.items()
             if k not in CLIENT_MAY_NOT_SET and not k.startswith("_")},
            where="imported")
        assert not problems, f"{pid} was stored in a shape no write route allows: {problems}"


def test_a_vendor_number_outside_the_contract_is_refused_with_its_bounds(
        c, tmp_path):
    """Not unrenderable - out of range, and the same consequence.

    `_as_int` tolerates anything up to 4096 because that is where a hardware
    count stops being one; the dashboard's own bound for `cores` is 512. A
    record stored past it opens fine and is refused by every later save, which
    is the corrupt-record state `test_profile_schema` treats as a defect. So
    the import says the bound instead of creating one.
    """
    r = c.post("/api/profiles/import", json=importing([
        {"name": "ok", "navigator": {"hardwareConcurrency": 2000}}]))
    assert 400 <= r.status_code < 500, \
        f"2000 cores was imported: {r.status_code} {r.text[:200]}"
    assert "cores" in r.text and "512" in r.text, r.text[:200]
    assert not stored_ids(tmp_path)


def test_an_imported_profile_can_still_be_edited_afterwards(c, tmp_path):
    """The consequence of the test above, from the user's side. A record the
    write contract refuses is a profile that can be opened and never saved."""
    c.post("/api/profiles/import", json=importing([{"name": "vendor"}]))
    pid = next(iter(stored_ids(tmp_path)))
    r = c.put(f"/api/profiles/{pid}", json={"name": "renamed"})
    assert r.status_code < 400, \
        f"an imported profile cannot be saved again: {r.status_code} {r.text[:200]}"


def test_a_vendor_name_is_rendered_as_text_by_every_reader(c, tmp_path):
    """The headline symptom of this half. `App.jsx` calls
    `p.name.toLowerCase()` on every row, so one numeric name blanks the page
    for every profile."""
    c.post("/api/profiles/import", json=importing([{"name": 123}]))
    c.post("/api/profiles", json={"name": "innocent"})
    for p in c.get("/api/profiles").json():
        assert isinstance(p["name"], str), \
            f"{p['id']} carries a {type(p['name']).__name__} name"


def test_the_native_branch_holds_the_same_line(c, tmp_path):
    """The other half of the same route. It runs `check_fields` over every
    entry already - this says so out loud, so a later edit cannot quietly take
    it away and leave only the vendor branch covered."""
    r = c.post("/api/profiles/import", json=native([
        {"name": "one"}, {"name": 123}, {"name": "three"}]))
    assert 400 <= r.status_code < 500, \
        f"a native import accepted a numeric name: {r.status_code} {r.text[:200]}"
    assert "name" in r.text and "profiles[1]" in r.text, r.text[:200]
    assert not stored_ids(tmp_path) and not engine_ids(tmp_path)


# --- what the vendor importer is *for*, and must keep doing ----------------

#: The whole point of the tolerant importer: one value, the many names the
#: tools it reads happen to give it. A contract that narrowed this would be a
#: regression, not a fix - so each alias is asserted to still land.
ALIASES = [
    ("name", {"profile_name": "by profile_name"}, "name", "by profile_name"),
    ("title", {"title": "by title"}, "name", "by title"),
    ("notes", {"name": "n", "description": "by description"}, "notes",
     "by description"),
    ("nested user agent",
     {"name": "n", "fingerprint": {"ua": "Mozilla/5.0 (X11; Linux x86_64) "
                                         "AppleWebKit/537.36 (KHTML, like Gecko) "
                                         "Chrome/126.0.0.0 Safari/537.36"}},
     "browserVersion", "126.0.0.0"),
    ("os_type", {"name": "n", "os_type": "macOS"}, "os", "macOS"),
    ("screen_resolution", {"name": "n", "screen_resolution": "2560 x 1440"},
     "screen", "2560x1440"),
    ("nested locale", {"name": "n", "navigator": {"language": "de-DE"}},
     "locale", "de-DE"),
    ("nested timezone", {"name": "n", "timezone": {"timezone": "Europe/Berlin"}},
     "timezone", "Europe/Berlin"),
    ("hardware_concurrency", {"name": "n", "hardware_concurrency": 12}, "cores", 12),
    ("device_memory", {"name": "n", "device_memory": "16"}, "memory", 16),
    ("unmaskedVendor", {"name": "n", "unmaskedVendor": "Google Inc. (NVIDIA)"},
     "webglVendor", "Google Inc. (NVIDIA)"),
    ("comma-separated tags", {"name": "n", "tags": "ads, client"}, "tags",
     ["ads", "client"]),
    ("group_name becomes a tag", {"name": "n", "group_name": "Client work"},
     "tags", ["Client work"]),
]


@pytest.mark.parametrize("export,field,want",
                         [(e, f, w) for _, e, f, w in ALIASES],
                         ids=[lbl for lbl, _, _, _ in ALIASES])
def test_the_importer_is_still_tolerant_about_the_name_a_vendor_uses(
        c, tmp_path, export, field, want):
    """Tolerance about which name a value hides under is the policy. Tolerance
    about a value having a type nothing can render was the defect. A fix that
    took the first with the second would pass every refusal test above."""
    r = c.post("/api/profiles/import", json=importing([export]))
    assert r.status_code < 400, f"{export!r} was refused: {r.text[:200]}"
    pid = next(iter(stored_ids(tmp_path)))
    assert dash_raw(tmp_path, pid)[field] == want


def test_a_missing_field_is_still_filled_in_coherently(c, tmp_path):
    """The importer's other promise: gaps become a coherent generated value,
    not a hole - and it still reports what it could not line up."""
    r = c.post("/api/profiles/import", json=importing([{"name": "sparse"}]))
    assert r.status_code < 400, r.text[:200]
    assert r.json() == {"imported": 1, "format": "vendor",
                        "results": [{"name": "sparse", "issues": []}]}
    rec = dash_raw(tmp_path, next(iter(stored_ids(tmp_path))))
    assert rec["userAgent"] and rec["locale"] and rec["timezone"]
    assert isinstance(rec["tags"], list) and isinstance(rec["seed"], int)


def test_a_proxy_a_vendor_actually_exports_still_arrives_whole(c, tmp_path):
    """Including the credential, and including the protocol it named."""
    r = c.post("/api/profiles/import", json=importing([
        {"name": "with proxy",
         "proxy": {"proxy_type": "socks4", "ip": "203.0.113.9",
                   "proxy_port": "1080", "login": "u", "proxy_password": "p",
                   "geo": "NL"}}]))
    assert r.status_code < 400, r.text[:200]
    px = c.get("/api/profiles").json()[0]["proxy"]
    assert px["type"] == "SOCKS4" and px["host"] == "203.0.113.9"
    assert px["port"] == "1080" and px["user"] == "u" and px["country"] == "NL"
