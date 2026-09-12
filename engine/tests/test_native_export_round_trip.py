"""What `POST /api/profiles/import` does with Numera's own export file.

Export writes dashboard-shaped profiles; import ran every file through the
*vendor* importer, whose whole job is to be tolerant about formats it has never
seen. Fed its own output, that tolerance is destruction: `profile_from_export`
reads none of `seed`, `webglVendor`, `canvas`/`webrtc`/`audio`/`clientRects`/
`webglImage`/`mediaDevices`, `startupUrls`, `extensions`, `launchArgs`,
`bookmarks`, `dns`, `ipBased`, `clearOnStart` or `switchedOff`, and it reads
`folder` only to append it to the tag list - so the file a user exported came
back as a profile with a freshly generated fingerprint, no startup tabs, no
extensions and the wrong folder, reported as "Imported 1 profile".

So the native format is versioned and read as configuration, not guessed at:
every editable field survives, the id and the store's own bookkeeping are
deliberately re-minted, and anything that could *not* come back - an
unversioned file, a password the vault was holding when the file was written -
is said out loud rather than quietly imagined.

A configuration export is not a session backup: cookies, logins and local
storage are not in this file and nothing here pretends otherwise.
"""

import json

import pytest

from numera.convert import to_engine_profile
from numera.importers import NATIVE_KEY, NATIVE_VERSION
from numera.store import ProfileStore


@pytest.fixture
def store(tmp_path):
    return ProfileStore(tmp_path / "source")


@pytest.fixture
def elsewhere(tmp_path):
    """A second, empty install - where an export is normally taken."""
    return ProfileStore(tmp_path / "target")


def api(store, password=None):
    pytest.importorskip("fastapi")
    from fastapi.testclient import TestClient

    from numera.server import create_app

    return TestClient(create_app(data_dir=str(store.root), start_scheduler=False,
                                 password=password))


#: One sentinel in every field a user can edit. Anything the import path drops
#: or regenerates shows up as a changed value here, which is the whole point.
SENTINEL = {
    "name": "sentinel profile",
    "folder": "Client work",
    "os": "macOS",
    "engine": "camoufox",
    "browser": "Chrome",
    "browserVersion": "127.0.6533.120",
    "userAgent": ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) "
                  "AppleWebKit/537.36 (KHTML, like Gecko) "
                  "Chrome/127.0.6533.120 Safari/537.36"),
    "screen": "2560x1440",
    "cores": 12,
    "memory": 32,
    "webglVendor": "Google Inc. (Apple)",
    "webglRenderer": "ANGLE (Apple, ANGLE Metal Renderer: Apple M2, Unspecified)",
    "canvas": "Off",
    "webrtc": "Disabled",
    "audio": "Off",
    "fonts": "Real",
    "clientRects": "Off",
    "webglImage": "Off",
    "mediaDevices": "Real",
    "locale": "de-DE",
    "timezone": "Europe/Berlin",
    "geo": "Allow",
    "mediaVideo": 2,
    "mediaAudio": 3,
    "mediaSpeakers": 4,
    "fontList": "Sentinel Sans\nSentinel Serif",
    "osVersion": "14.5",
    "deviceModel": "MacBookPro18,3",
    "maxTouchPoints": 0,
    "devicePixelRatio": 2.0,
    "seed": 424242,
    "dnt": True,
    "notes": "sentinel notes",
    "tags": ["sentinel-tag"],
    "startupUrls": "https://example.test/search?q=red,blue\nhttps://second.test/",
    "extensions": "C:\\ext\\sentinel",
    "launchArgs": "--sentinel-flag",
    "bookmarks": "Sentinel | https://bookmarked.test/",
    "dns": "1.1.1.1",
    "ipBased": ["timezone", "geolocation"],
    "clearOnStart": ["cookies", "history"],
    "switchedOff": ["googleServices"],
    "proxy": {"type": "SOCKS5", "host": "gw.sentinel.test", "port": "1080",
              "user": "sentinel-user", "pass": "sentinel-pass", "country": "DE"},
}

#: Everything the native contract promises comes back unchanged.
PRESERVED = tuple(SENTINEL)


def exported(client, body=None) -> dict:
    """The file the dashboard's Export button writes for one saved profile."""
    answer = client.post("/api/profiles", json=dict(body or SENTINEL))
    assert answer.status_code == 200, answer.text
    listed = client.get("/api/profiles").json()
    return {NATIVE_KEY: NATIVE_VERSION, "profiles": listed}


def imported(client, file: dict):
    return client.post("/api/profiles/import",
                       json={"text": json.dumps(file)})


def test_the_sentinel_covers_every_field_a_client_may_write():
    """A new dashboard field fails here until the round-trip below exercises it.

    Otherwise the next control added to the editor is preserved by nothing and
    nothing says so - which is how this finding happened in the first place."""
    from numera.server.routes_profiles import client_fields

    # `status` and `lastActive` are the two client-visible fields the server
    # owns and re-mints; the bookkeeping test below is where they are asserted.
    assert set(client_fields()) - {"status", "lastActive"} == set(SENTINEL)


def test_every_editable_field_survives_the_round_trip(store, elsewhere):
    """The finding itself, field by field."""
    source = exported(api(store))
    original = source["profiles"][0]

    target = api(elsewhere)
    answer = imported(target, source)

    assert answer.status_code == 200, answer.text
    assert answer.json()["imported"] == 1
    landed = target.get("/api/profiles").json()[0]
    for key in PRESERVED:
        assert landed[key] == original[key], f"import changed {key!r}"


def test_the_fingerprint_that_lands_is_the_one_that_left(store, elsewhere):
    """The seed and every field it drives, compared as the engine sees them.

    Field equality above is the contract; this is what it is *for* - the
    browser this profile opens has to be the same device it was before."""
    source = exported(api(store))
    target = api(elsewhere)

    assert imported(target, source).status_code == 200
    landed = target.get("/api/profiles").json()[0]

    assert (to_engine_profile(landed).fingerprint
            == to_engine_profile(source["profiles"][0]).fingerprint)


def test_the_id_and_the_stores_bookkeeping_are_reminted(store, elsewhere):
    """Deliberately *not* preserved: the id is what the session directory,
    proxy assignments and revisions hang off, and a file cannot claim one."""
    source = exported(api(store))
    original = source["profiles"][0]
    target = api(elsewhere)

    assert imported(target, source).status_code == 200
    landed = target.get("/api/profiles").json()[0]

    assert landed["id"] != original["id"]
    assert landed["status"] == "stopped" and landed["lastActive"] == "never"
    assert landed["_rev"] == 1, "an imported profile inherited a revision"


def test_a_file_this_build_cannot_read_is_refused(store):
    client = api(store)
    source = exported(client)
    source[NATIVE_KEY] = NATIVE_VERSION + 1

    answer = imported(client, source)

    assert answer.status_code == 400, answer.text
    assert "newer" in answer.json()["detail"].lower()


def test_an_unversioned_native_export_is_refused_not_guessed_at(store, elsewhere):
    """The old files. Reading them through the vendor importer is what produced
    a confident, lossy import, so they are named and refused instead."""
    source = exported(api(store))
    legacy = source["profiles"]                    # a bare list, as 0.30 wrote

    answer = api(elsewhere).post("/api/profiles/import",
                                 json={"text": json.dumps(legacy)})

    assert answer.status_code == 400, answer.text
    detail = answer.json()["detail"]
    assert "Numera" in detail and "export" in detail.lower()
    assert api(elsewhere).get("/api/profiles").json() == [], \
        "a refused import still wrote profiles"


def test_a_vendor_export_still_imports(store):
    """The tolerant path is untouched: this is what it is for."""
    client = api(store)
    answer = client.post("/api/profiles/import", json={"text": json.dumps([
        {"name": "from gologin", "os": "Windows", "navigator": {
            "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/128.0.0.0",
            "resolution": "1920x1080", "hardwareConcurrency": 8},
         "proxy": {"mode": "http", "host": "1.2.3.4", "port": 8080}}])})

    assert answer.status_code == 200, answer.text
    assert answer.json()["format"] == "vendor"
    landed = client.get("/api/profiles").json()[0]
    assert landed["name"] == "from gologin"
    assert landed["proxy"]["host"] == "1.2.3.4"


def test_a_flat_vendor_export_is_not_mistaken_for_a_native_one(store):
    """The legacy detector must not refuse somebody's real vendor file.

    A few native markers are ordinary DOM property names, and a vendor export
    that happens to write them flat rather than nested would otherwise be
    turned away with a message about a format it was never in."""
    client = api(store)
    answer = client.post("/api/profiles/import", json={"text": json.dumps([
        {"name": "flat vendor export", "os": "Windows",
         "mediaDevices": "real", "maxTouchPoints": 0, "devicePixelRatio": 1,
         "user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/128.0.0.0"}])})

    assert answer.status_code == 200, answer.text
    assert answer.json()["format"] == "vendor"


def test_a_malformed_entry_refuses_the_whole_batch(store, elsewhere):
    """All or nothing, and the reason names the entry."""
    source = exported(api(store))
    source["profiles"].append({**source["profiles"][0],
                               "name": "broken", "tags": "not-a-list"})

    answer = imported(api(elsewhere), source)

    assert answer.status_code == 400, answer.text
    assert "profiles[1]" in answer.json()["detail"]
    assert api(elsewhere).get("/api/profiles").json() == [], \
        "half a malformed batch was written"


def test_profiles_is_not_a_list_of_objects(store):
    answer = api(store).post("/api/profiles/import", json={"text": json.dumps(
        {NATIVE_KEY: NATIVE_VERSION, "profiles": ["not an object"]})})

    assert answer.status_code == 400, answer.text


def test_an_unknown_engine_is_refused(store, elsewhere):
    source = exported(api(store))
    source["profiles"][0]["engine"] = "not-an-engine"

    answer = imported(api(elsewhere), source)

    assert answer.status_code == 422, answer.text
    assert api(elsewhere).get("/api/profiles").json() == []


def test_a_name_that_is_already_taken_refuses_the_import(store):
    """Same rule as every other write path: one name, one profile - and the
    file is refused whole rather than half-landing under renamed duplicates."""
    client = api(store)
    source = exported(client)

    answer = imported(client, source)

    assert answer.status_code == 409, answer.text
    assert "sentinel profile" in answer.json()["detail"]
    assert len(client.get("/api/profiles").json()) == 1


def test_a_password_the_vault_was_holding_is_reported_missing(store, elsewhere):
    """The export cannot contain what the process could not read, so the import
    must not imply it did. `pass: null` is exactly the listing's way of saying
    "withheld" - an absent password reads as `""`."""
    pytest.importorskip("cryptography")
    api(store).post("/api/profiles", json=dict(SENTINEL))
    store.enable_vault("master")
    locked = api(store)                       # no master password
    source = {NATIVE_KEY: NATIVE_VERSION, "profiles": locked.get("/api/profiles").json()}
    assert source["profiles"][0]["proxy"]["pass"] is None, \
        "the vault did not withhold the proxy password"

    target = api(elsewhere)
    answer = imported(target, source)

    assert answer.status_code == 200, answer.text
    issues = answer.json()["results"][0]["issues"]
    assert any("password" in i for i in issues), issues
    landed = target.get("/api/profiles").json()[0]
    # The rest of the proxy still comes across; only the secret is missing, and
    # the report above is what says so.
    assert landed["proxy"]["host"] == "gw.sentinel.test"
    assert not landed["proxy"]["pass"]


def test_a_readable_password_comes_across(store, elsewhere):
    source = exported(api(store))
    target = api(elsewhere)

    assert imported(target, source).status_code == 200
    assert target.get("/api/profiles").json()[0]["proxy"]["pass"] == "sentinel-pass"


def test_an_import_does_not_change_the_default_engine(store, elsewhere):
    """A file is not somebody choosing an engine for their next new profile.

    The vendor path already refuses to read its own fallback as a choice; a
    native export names an engine on *every* profile, so "last one wins" would
    silently reset the default on every import."""
    source = exported(api(store))
    target = api(elsewhere)
    assert target.put("/api/config", json={"default_engine": "cloak"}).status_code == 200
    before = target.get("/api/config").json()["default_engine"]
    assert source["profiles"][0]["engine"] != before, "the test proves nothing"

    assert imported(target, source).status_code == 200

    assert target.get("/api/config").json()["default_engine"] == before


def test_the_cli_importer_refuses_a_native_file_too(store, tmp_path):
    """Same rule, the other door.

    `numera import-from` reads the file through `load_export`, which knew
    nothing about the native format - so the CLI kept doing exactly what the
    API route was fixed for. `cmd_import_from` already turns a ValueError into
    'Couldn't read <file>: ...', so the refusal arrives as a message."""
    from numera import importers

    source = exported(api(store))
    for form in (source, source["profiles"]):          # versioned and legacy
        path = tmp_path / "export.json"
        path.write_text(json.dumps(form), encoding="utf-8")

        with pytest.raises(ValueError, match="Numera profile export"):
            importers.load_export(path)


def test_the_cli_importer_still_reads_a_vendor_file(tmp_path):
    from numera import importers

    path = tmp_path / "vendor.json"
    path.write_text(json.dumps([{"name": "from adspower", "os": "Windows"}]),
                    encoding="utf-8")

    assert importers.load_export(path) == [{"name": "from adspower", "os": "Windows"}]


def test_an_entry_naming_no_engine_takes_the_saved_default(store, elsewhere):
    source = exported(api(store))
    source["profiles"][0].pop("engine")
    target = api(elsewhere)
    assert target.put("/api/config", json={"default_engine": "cloak"}).status_code == 200
    default = target.get("/api/config").json()["default_engine"]

    assert imported(target, source).status_code == 200

    assert target.get("/api/profiles").json()[0]["engine"] == default
