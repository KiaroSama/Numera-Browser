"""Every door that writes a complete profile reaches the same contract.

N04's other requirement: not just that the rule is right, but that no supported
writer gets past it. The doors are `ProfileStore.save`, the batch save behind
`repo.save_many`, `numera import-from`, and the API's create / update / batch /
bulk routes. `test_write_contract_complete_record` covers the first two; this
file covers the CLI and HTTP ones, plus the two properties that only show up
from outside: that a refusal is a named 4xx or a non-zero exit rather than a
traceback, and that a record damaged by an *older* build still reads.

One app and one data root for the whole module. Building a FastAPI application
per test case costs about a second each and proves nothing a shared one does
not - the cases here differ by request, not by server. The tests that need a
store nobody else has touched say so by taking `tmp_path` and building their
own.
"""

import json
from dataclasses import replace

import pytest

from numera import generate, write_contract
from numera.models import Profile
from numera.store import ProfileStore

fastapi = pytest.importorskip("fastapi")            # noqa: E402
from fastapi.testclient import TestClient           # noqa: E402

from numera.server import create_app               # noqa: E402

FP = generate(seed=1)


def profile(**kw) -> Profile:
    kw.setdefault("fingerprint", replace(FP))
    kw.setdefault("name", "p")
    return Profile(**kw)


@pytest.fixture(scope="module")
def client(tmp_path_factory):
    """One application for the module. See the file docstring."""
    root = tmp_path_factory.mktemp("api")
    with TestClient(create_app(data_dir=str(root), start_scheduler=False),
                    raise_server_exceptions=False) as c:
        c.data_root = root
        yield c


def created(client, **fields) -> dict:
    body = {"name": f"p-{len(list((client.data_root / 'profiles').glob('*')))}"}
    response = client.post("/api/profiles", json={**body, **fields})
    assert response.status_code == 200, response.json()
    return response.json()


# ---------------------------------------------------------------------------
# The API: a refusal names the field, with a 4xx
# ---------------------------------------------------------------------------
@pytest.mark.parametrize("field,value", [
    ("screen", "0x0"), ("screen", "-5x-5"), ("screen", "potato"),
    ("screen", "infx1080"), ("screen", "1920"), ("screen", "99999x10"),
    ("cores", -4), ("cores", 10 ** 400), ("memory", -1),
    ("maxTouchPoints", 99), ("devicePixelRatio", 0),
    ("userAgent", 123), ("timezone", []), ("tags", [{}]),
    ("name", 123), ("dnt", "yes"),
], ids=lambda v: str(v)[:14])
def test_the_create_route_refuses_by_field_without_writing(client, field, value):
    """A 4xx naming the field. A 500 is the failure mode this replaces: the
    client cannot tell a rejected value from a broken server, and a retry of
    the same request is the reasonable next move for exactly one of them."""
    before = sorted((client.data_root / "profiles").glob("*.json"))

    response = client.post("/api/profiles", json={"name": "refused", field: value})

    assert 400 <= response.status_code < 500, response.json()
    assert field in response.json()["detail"]
    assert sorted((client.data_root / "profiles").glob("*.json")) == before


@pytest.mark.parametrize("field,value", [
    ("screen", "0x0"), ("cores", -4), ("userAgent", 123),
], ids=["screen", "cores", "userAgent"])
def test_the_update_route_refuses_the_same_way(client, field, value):
    """The same rule on the edit door. An update that is refused must also
    leave the stored record exactly as it was, revision included."""
    stored = created(client)
    path = client.data_root / "dashboard" / f"{stored['id']}.json"
    before = path.read_bytes()

    response = client.put(f"/api/profiles/{stored['id']}",
                          json={**stored, field: value})

    assert 400 <= response.status_code < 500, response.json()
    assert field in response.json()["detail"]
    assert path.read_bytes() == before


def test_a_refused_request_never_answers_with_a_traceback(client):
    """The acceptance criterion, stated once as its own case.

    `fingerprint = 123` used to crash in `Fingerprint.to_dict()`, which over
    HTTP is a 500 with a stack trace in the log and nothing actionable in the
    body.
    """
    response = client.post("/api/profiles", json={"name": "x", "cores": -4})

    assert response.status_code != 500
    assert "Traceback" not in json.dumps(response.json())


# ---------------------------------------------------------------------------
# The API: the healthy paths still work
# ---------------------------------------------------------------------------
def test_an_ordinary_create_and_edit_still_work(client):
    """The control for every refusal above."""
    stored = created(client, notes="first")

    edited = client.put(f"/api/profiles/{stored['id']}",
                        json={**stored, "notes": "second"})

    assert edited.status_code == 200
    assert edited.json()["notes"] == "second"
    assert edited.json()["_rev"] == stored["_rev"] + 1


def test_a_stale_revision_still_conflicts(client):
    """The optimistic-concurrency check must survive the new validation: it
    runs on the same request and a validator that refused first would turn
    every conflict into a 422 about a field the client did not change."""
    stored = created(client)
    client.put(f"/api/profiles/{stored['id']}", json={**stored, "notes": "a"})

    late = client.put(f"/api/profiles/{stored['id']}",
                      json={**stored, "notes": "b"})

    assert late.status_code == 409


def test_a_duplicate_name_still_conflicts(client):
    stored = created(client, name="one-of-a-kind")

    again = client.post("/api/profiles", json={"name": stored["name"]})

    assert again.status_code == 409


def test_the_folder_survives_a_validated_write(client):
    """`folder` is a dashboard-only field with no engine twin, so a validator
    that rebuilt the record from the engine shape would drop it."""
    stored = created(client, folder="Research")

    edited = client.put(f"/api/profiles/{stored['id']}",
                        json={**stored, "notes": "moved"})

    assert edited.json()["folder"] == "Research"


def test_an_untouched_proxy_password_is_preserved_across_an_edit(client):
    """The intent machinery, end to end.

    `Proxy.__setattr__` records that the password was *chosen*; the validator
    reads that field now, and reading it must not count as choosing it. An edit
    that resubmits the profile it was handed would otherwise be indistinguish-
    able from a deliberate replacement.
    """
    stored = created(client, proxy={"type": "HTTP", "host": "h", "port": "1",
                                    "user": "u", "pass": "keep-me"})

    edited = client.put(f"/api/profiles/{stored['id']}",
                        json={**stored, "notes": "unrelated edit"})

    assert edited.status_code == 200
    assert edited.json()["proxy"]["pass"] == "keep-me"


def test_a_deliberate_password_clear_still_clears(client):
    """The other side of the same machinery: empty means empty, on purpose."""
    stored = created(client, proxy={"type": "HTTP", "host": "h", "port": "1",
                                    "user": "u", "pass": "remove-me"})

    edited = client.put(f"/api/profiles/{stored['id']}",
                        json={**stored, "proxy": {**stored["proxy"], "pass": ""}})

    assert edited.json()["proxy"]["pass"] == ""


# ---------------------------------------------------------------------------
# The mirror door
# ---------------------------------------------------------------------------
def test_every_engine_twin_the_api_writes_satisfies_the_contract(client):
    """The API writes its engine record through `ProfileStore._save_engine`,
    which is the one write that does not go through `repo` - so
    `_require_writable` is not on this path, and only the API's own schema plus
    the converter stand between a request and an engine file.

    That makes this an invariant worth pinning rather than an implementation
    detail: today the converter normalises every schema-valid payload into a
    record the contract accepts, and the first dashboard field whose conversion
    does not is the day the missing guard starts costing something. Checked
    over every profile the module has created by the time it runs.
    """
    twins = sorted((client.data_root / "profiles").glob("*.json"))
    assert twins, "the module created no profiles, so this proves nothing"

    for path in twins:
        stored = Profile.from_dict(json.loads(path.read_text("utf-8")))
        assert write_contract.profile_problems(stored) == [], path.name


# ---------------------------------------------------------------------------
# The CLI import door
# ---------------------------------------------------------------------------
def vendor_file(path, *entries):
    path.write_text(json.dumps(list(entries)), encoding="utf-8")
    return path


BASE = {"os": "win", "userAgent": "Mozilla/5.0", "resolution": "1920x1080"}


@pytest.mark.parametrize("entry,fragment", [
    ({**BASE, "name": 123}, "name"),
    ({**BASE, "name": "ok", "resolution": "0x0"}, "screen"),
    ({**BASE, "name": "ok", "tags": [{}]}, "tags"),
], ids=["numeric-name", "zero-resolution", "object-tag"])
def test_the_cli_import_exits_non_zero_and_writes_nothing(tmp_path, entry,
                                                          fragment, capsys):
    """`import-from` is the fourth door, and the one with no schema in front of
    it. Driven through `cli.main` so the exit code is the real one."""
    from numera import cli

    export = vendor_file(tmp_path / "vendor.json", entry)
    root = tmp_path / "root"

    code = cli.main(["--data-dir", str(root), "import-from", str(export)])

    assert code == 1
    assert fragment in capsys.readouterr().out
    assert list(root.rglob("profiles/*.json")) == []


def test_a_bad_later_entry_refuses_the_whole_import(tmp_path):
    """Whole file or nothing. A partial import is the worst shape to recover
    from, because re-running it duplicates whatever did land."""
    from numera import cli

    export = vendor_file(tmp_path / "vendor.json",
                         {**BASE, "name": "first"},
                         {**BASE, "name": "second", "resolution": "0x0"},
                         {**BASE, "name": "third"})
    root = tmp_path / "root"

    assert cli.main(["--data-dir", str(root), "import-from", str(export)]) == 1
    assert list(root.rglob("profiles/*.json")) == []


def test_a_good_vendor_export_still_imports(tmp_path):
    """The control: the guard must not cost the feature it guards."""
    from numera import cli

    export = vendor_file(tmp_path / "vendor.json", {**BASE, "name": "from-vendor"})
    root = tmp_path / "root"

    assert cli.main(["--data-dir", str(root), "import-from", str(export)]) == 0
    assert [p.name for p in ProfileStore(root).list()] == ["from-vendor"]


def test_an_unsupported_format_is_refused_rather_than_reinterpreted(tmp_path,
                                                                    capsys):
    """A Numera export read by the vendor importer would be accepted and
    quietly stripped of its seed, signal modes, startup URLs and folder. Being
    named and refused is the requirement; silently reading it is the defect."""
    from numera import cli, importers

    store = ProfileStore(tmp_path / "src")
    store.save(profile(name="native"))
    native = tmp_path / "native.json"
    native.write_text(json.dumps({
        importers.NATIVE_KEY: importers.NATIVE_VERSION,
        "profiles": [{"id": p.id, "name": p.name} for p in store.list()]}),
        encoding="utf-8")
    root = tmp_path / "root"

    code = cli.main(["--data-dir", str(root), "import-from", str(native)])

    assert code == 1
    assert "Numera profile export" in capsys.readouterr().out
    assert list(root.rglob("profiles/*.json")) == []


def test_an_export_from_a_newer_build_is_refused_by_version(tmp_path, capsys):
    """The other half of the same refusal: reading a newer file with older
    rules and reporting success is the defect, not the missing feature."""
    from numera import cli, importers

    future = tmp_path / "future.json"
    future.write_text(json.dumps({
        importers.NATIVE_KEY: importers.NATIVE_VERSION + 1, "profiles": []}),
        encoding="utf-8")
    root = tmp_path / "root"

    assert cli.main(["--data-dir", str(root), "import-from", str(future)]) == 1
    assert "newer Numera" in capsys.readouterr().out


# ---------------------------------------------------------------------------
# What an older build already stored
# ---------------------------------------------------------------------------
#: Damaged display fields, as a build without this contract could have written
#: them. Refusing new writes repairs none of these, and the reader still has to
#: cope: `p.name.toLowerCase()` throws on every one.
DAMAGED = [
    {"id": "aaaaaaaaaaaa", "name": 123},
    {"id": "bbbbbbbbbbbb", "name": None},
    {"id": "cccccccccccc"},                        # no name at all
    {"id": "dddddddddddd", "name": ["a"], "tags": [{}, "keep"]},
    {"id": "eeeeeeeeeeee", "name": "fine", "cores": float("inf")},
]


@pytest.fixture(scope="module")
def legacy(tmp_path_factory):
    """A data root seeded with damaged records, plus one healthy profile."""
    root = tmp_path_factory.mktemp("legacy")
    (root / "dashboard").mkdir(parents=True)
    for record in DAMAGED:
        (root / "dashboard" / f"{record['id']}.json").write_text(
            json.dumps(record), encoding="utf-8")
    ProfileStore(root).save(profile(name="healthy"))
    with TestClient(create_app(data_dir=str(root), start_scheduler=False),
                    raise_server_exceptions=False) as c:
        c.data_root = root
        yield c


@pytest.mark.parametrize("route", ["/api/profiles", "/api/profiles/health",
                                   "/api/profiles/collisions"])
def test_a_damaged_record_does_not_take_a_read_path_down(legacy, route):
    """One bad record used to blank the page for every profile, not just its
    own - `p.name.toLowerCase()` throws, and the listing renders as a unit."""
    response = legacy.get(route)

    assert response.status_code == 200, response.text


def test_every_listed_profile_has_the_display_fields_a_reader_dereferences(legacy):
    """The repair is a view, and it has to be complete: an absent `name` throws
    in the dashboard exactly as a numeric one does, so 'present' is the
    assertion, not 'not a number'."""
    listed = legacy.get("/api/profiles").json()

    assert len(listed) >= len(DAMAGED)
    for record in listed:
        assert isinstance(record["name"], str)
        assert all(isinstance(tag, str) for tag in record.get("tags", []))


def test_the_healthy_record_is_still_listed_alongside_the_damaged_ones(legacy):
    """Damage must be contained to the record that carries it."""
    listed = legacy.get("/api/profiles").json()

    assert [r for r in listed if r["name"] == "healthy"]


def test_the_healthy_record_is_still_editable_alongside_the_damaged_ones(legacy):
    """The write half of containment.

    `stores._claim_names` passed `stored_names()` straight into `name_clash`,
    which puts every stored name in a set - so one legacy record holding a list
    name made every API edit a 500, for every profile. Both write paths now go
    through the same repair.
    """
    listed = legacy.get("/api/profiles").json()
    healthy = [r for r in listed if r["name"] == "healthy"][0]

    edited = legacy.put(f"/api/profiles/{healthy['id']}",
                        json={**healthy, "notes": "still editable"})

    assert edited.status_code == 200


def test_a_damaged_stored_name_does_not_make_the_store_unwritable(tmp_path):
    """The uniqueness check reads every stored name into a `set`, so a legacy
    record holding `name: ["a"]` raised `TypeError: unhashable type: 'list'`
    out of `name_clash` - and the store then refused *every* new profile, not
    just that one. One unreadable record must not cost the whole store its
    write path.
    """
    store = ProfileStore(tmp_path)
    (tmp_path / "dashboard").mkdir(parents=True, exist_ok=True)
    (tmp_path / "dashboard" / "ffffffffffff.json").write_text(
        json.dumps({"id": "ffffffffffff", "name": ["a"]}), encoding="utf-8")

    saved = store.save(profile(name="new-arrival"))

    assert saved.name == "new-arrival"


def test_a_numeric_stored_name_still_counts_as_taken(tmp_path):
    """The repair is not a licence to duplicate. Every surface shows a stored
    `123` as `"123"`, so a new profile asking for that name is asking for one
    that is visibly in use, and `resolve("123")` would be ambiguous forever.
    """
    from numera.repo import NameTaken

    store = ProfileStore(tmp_path)
    (tmp_path / "dashboard").mkdir(parents=True, exist_ok=True)
    (tmp_path / "dashboard" / "ffffffffffff.json").write_text(
        json.dumps({"id": "ffffffffffff", "name": 123}), encoding="utf-8")

    with pytest.raises(NameTaken):
        store.save(profile(name="123"))


def test_the_damaged_bytes_on_disk_are_left_exactly_as_they_were(legacy):
    """Reading repairs the view, never the file. A read path that rewrote what
    it could not render would destroy the evidence of what went wrong - and
    would be a write, from a `GET`."""
    for record in DAMAGED:
        path = legacy.data_root / "dashboard" / f"{record['id']}.json"
        assert json.loads(path.read_text("utf-8")) == json.loads(
            json.dumps(record)), f"{record['id']} was rewritten by a reader"


def test_a_damaged_name_is_still_searchable_by_the_engine(tmp_path):
    """`find_by_name` is the engine's own lookup, and it compares against the
    stored value. A repair that only existed in the API's view would leave the
    CLI unable to name the profile the GUI shows."""
    store = ProfileStore(tmp_path)
    saved = store.save(profile(name="findable"))

    assert store.find_by_name("findable").id == saved.id
    assert store.find_by_name("nothing-here") is None


def test_the_repair_does_not_let_the_damage_be_written_back(legacy):
    """The read repair and permission to write are different questions, and
    conflating them is how a tolerant reader becomes a second write door: the
    dashboard reads the repaired view, submits it back on the next edit, and
    the damaged value is now the stored one."""
    listed = legacy.get("/api/profiles").json()
    damaged = next(r for r in listed if r["id"] == "aaaaaaaaaaaa")

    refused = legacy.put(f"/api/profiles/{damaged['id']}",
                         json={**damaged, "name": 123})

    assert 400 <= refused.status_code < 500
    assert "name" in refused.json()["detail"]
